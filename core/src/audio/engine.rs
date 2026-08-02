//! Real-time audio engine: cpal devices, DSP thread and mixing.
//!
//! Threading model:
//!
//! * The cpal **input callback** does nothing but copy samples into a queue. All
//!   the expensive work (resampling, RNNoise, Opus) happens on a worker thread,
//!   because overrunning an audio callback causes glitches on every platform.
//! * The **worker** pulls 10 ms blocks, runs the capture chain, packs two blocks
//!   into a 20 ms Opus frame and hands it to the session.
//! * Incoming packets are decoded per speaker by [`super::jitter::SpeakerBuffer`],
//!   mixed, and left in a queue that the cpal **output callback** drains.

use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use parking_lot::Mutex;
use std::collections::{HashMap, VecDeque};

use super::codec::{Quality, VoiceEncoder, FRAME_SAMPLES, SEQ_UNITS_PER_FRAME};
use super::denoise::{CaptureProcessor, NoiseProfile, FRAME_SIZE, SAMPLE_RATE};
use super::dsp::{interleaved_to_mono, Reverb};
use super::jitter::SpeakerBuffer;
use super::resample::Resampler;
use crate::error::{CoreError, Result};
use crate::net::audio_packet::VoicePacket;

/// When to actually send audio.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransmitMode {
    /// Open the mic automatically when speech is detected.
    VoiceActivity,
    /// Only while the user holds the key — the safest option on a bike, since
    /// it cannot be tripped by a gust or a passing lorry.
    PushToTalk,
    /// Always on.
    Continuous,
}

/// Caps the queues so a stalled consumer cannot grow memory without bound.
const MAX_QUEUED_INPUT_SAMPLES: usize = SAMPLE_RATE as usize; // 1 second
const MAX_QUEUED_OUTPUT_SAMPLES: usize = SAMPLE_RATE as usize / 2;

/// How long the room tail takes to fall 60 dB.
///
/// Short on purpose. Enough that a gated voice stops sounding like a switch
/// being thrown, not so much that speech smears into itself — intelligibility
/// matters more here than atmosphere.
const REVERB_DECAY_SECS: f32 = 0.28;

/// How much of the tail is mixed under the voice.
const REVERB_WET: f32 = 0.16;

/// Shared state between the callbacks, the worker and the API layer.
pub struct AudioShared {
    /// Raw mono 48 kHz samples captured from the microphone.
    capture_queue: Mutex<VecDeque<f32>>,
    /// Mixed mono 48 kHz samples awaiting playback.
    playback_queue: Mutex<VecDeque<f32>>,
    /// Loopback monitoring, kept apart from [`Self::playback_queue`].
    ///
    /// It has to bypass the echo reference. The canceller's whole premise is
    /// that the reference is a far-end signal, uncorrelated with whoever is
    /// talking into the microphone. Monitoring makes the reference a copy of
    /// the near-end talker, so the filter converges on subtracting the user's
    /// own voice — which is heard as the wanted signal vanishing and only the
    /// residual howl remaining.
    monitor_queue: Mutex<VecDeque<f32>>,
    /// Per-speaker decode buffers.
    ///
    /// Keyed by a *stream key*, not a bare session id: two servers hand out
    /// session ids independently, so with dual connections the same id can refer
    /// to different people. See [`stream_key`].
    speakers: Mutex<HashMap<u64, SpeakerBuffer>>,
    transmitting: AtomicBool,
    muted: AtomicBool,
    deafened: AtomicBool,
    /// Input level in dBFS * 100, for the UI meter.
    input_level: AtomicU32,
    /// Output level in dBFS * 100, so the UI can show playback activity.
    output_level: AtomicU32,
    /// Level voice activation currently opens at, in dBFS * 100. Tracks the
    /// background noise, so it is worth showing rather than a fixed number.
    activation_threshold: AtomicU32,
    noise_floor: AtomicU32,
    /// Whether the last processed block counted as speech.
    speech_detected: AtomicBool,
    running: AtomicBool,

    /// Microphone gain in dB * 100, applied before the DSP chain so the meter
    /// reflects what the pipeline actually sees.
    input_gain_db: AtomicI32,
    /// Playback attenuation in dB * 100.
    output_volume_db: AtomicI32,
    /// Loopback: route the processed microphone straight to playback so the
    /// user can hear exactly what the far end would hear.
    monitor: AtomicBool,
    /// Samples the output callback had to invent because the playback queue
    /// was dry, and samples the input callback threw away because the worker
    /// was behind.
    ///
    /// Choppy audio has several possible causes that sound identical, and
    /// these two counters separate them: an output underrun means nothing was
    /// ready to play, a capture drop means the microphone outran the DSP, and
    /// neither moving means the gaps are arriving already in the stream.
    underrun_samples: AtomicU64,
    capture_dropped_samples: AtomicU64,
    /// How many speakers the mixer last found streaming.
    ///
    /// The underrun counter is gated on this. An empty playback queue is the
    /// normal, correct state when nobody is talking — counting that as a
    /// dropout buries the real ones under hours of ordinary silence.
    active_speakers: AtomicU32,
    /// Frames of incoming audio invented by concealment, and decoded from real
    /// packets. Reported so a hiss can be attributed rather than argued about.
    concealed_frames: AtomicU64,
    decoded_frames: AtomicU64,
    /// Whether incoming speakers are levelled towards a common loudness.
    normalise_levels: AtomicBool,
    /// Whether a short room tail is added to incoming voices.
    reverb_enabled: AtomicBool,
    /// The room itself. Applied to the mixed speakers only, so it never
    /// touches a notification cue or the loopback monitor — a cue is a signal,
    /// not something anyone is meant to hear across a room.
    reverb: Mutex<Reverb>,
    /// Whether the echo canceller is active.
    ///
    /// Worth exposing rather than always-on: on a headset there is no acoustic
    /// path to cancel, so the filter can only subtract things it should not,
    /// and being able to take it out of the chain is the quickest way to find
    /// out whether it is the thing damaging the audio.
    echo_cancellation: AtomicBool,
    /// Pre-rendered notification tones waiting to reach the output device.
    ///
    /// Rendering up front rather than synthesising in the worker keeps the cue
    /// intact even if the worker is busy, and makes a cue a single atomic
    /// action that cannot be half-played.
    cue_queue: Mutex<VecDeque<f32>>,

    /// Copy of what was most recently handed to the output device, used as the
    /// echo canceller's reference.
    ///
    /// Filled by the output callback and drained by the worker at the same
    /// rate, so the queue's own depth approximates the device's output latency
    /// and the two stay roughly aligned. The adaptive filter absorbs whatever
    /// misalignment is left.
    echo_reference: Mutex<VecDeque<f32>>,

    /// Requested capture/playback devices, by name. `None` means system default.
    device_request: Mutex<(Option<String>, Option<String>)>,
    /// Bumped whenever `device_request` changes; the device thread watches this
    /// and rebuilds its streams, which is how devices switch without tearing
    /// down the sessions that feed this shared state.
    device_generation: AtomicU64,
}

/// A short tone played to signal something without needing the screen.
///
/// These exist because the app is normally in a rider's pocket or behind a
/// navigation app: a status change that is only visible is a status change that
/// gets missed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioCue {
    /// Falling two-tone — the connection dropped.
    Disconnected,
    /// Rising two-tone — the connection is back.
    Reconnected,
    /// Repeated soft double-beep while a connection is being established.
    Dialing,
    /// Someone else muted us.
    MutedByOther,
    /// Someone else unmuted us.
    UnmutedByOther,
    /// Someone else deafened us.
    DeafenedByOther,
    /// Someone else undeafened us.
    UndeafenedByOther,
    /// Push-to-talk pressed: a short crisp click, like keying a radio.
    TransmitStart,
    /// Push-to-talk released: the "roger beep" and squelch tail a walkie-talkie
    /// makes when you let go.
    TransmitEnd,
    /// Steady tone for checking the chosen output device.
    Test,
}

impl AudioCue {
    /// Segments as `(frequency_hz, milliseconds)`; frequency 0 is a gap.
    ///
    /// The vocabulary is consistent so it can be learned without a manual:
    /// falling means something was taken away, rising means it came back, and
    /// the number of tones tells you how big a deal it is — two for the
    /// microphone, three for hearing, which matters more.
    fn segments(self) -> &'static [(f32, u32)] {
        match self {
            // Falling, so it reads as "something went wrong" without thinking.
            AudioCue::Disconnected => &[(880.0, 140), (0.0, 40), (440.0, 240)],
            // Rising, the mirror image.
            AudioCue::Reconnected => &[(523.25, 120), (0.0, 30), (783.99, 220)],
            // Deliberately unobtrusive: this plays every connection attempt,
            // including the automatic retries during a bad stretch of road.
            AudioCue::Dialing => &[(587.33, 90), (0.0, 90), (587.33, 90)],

            AudioCue::MutedByOther => &[(659.25, 110), (0.0, 30), (440.0, 170)],
            AudioCue::UnmutedByOther => &[(440.0, 110), (0.0, 30), (659.25, 170)],
            AudioCue::DeafenedByOther => &[
                (659.25, 100),
                (0.0, 30),
                (523.25, 100),
                (0.0, 30),
                (392.0, 200),
            ],
            AudioCue::UndeafenedByOther => &[
                (392.0, 100),
                (0.0, 30),
                (523.25, 100),
                (0.0, 30),
                (659.25, 200),
            ],

            // Keying a radio: one short, bright click. Brief on purpose — this
            // plays before every transmission, so anything longer would be in
            // the way within a minute of use.
            AudioCue::TransmitStart => &[(1318.5, 40)],
            // Letting go: the "roger beep" followed by a squelch tail. The
            // negative frequency is the noise marker; see `render_segments`.
            AudioCue::TransmitEnd => &[(1046.5, 90), (0.0, 15), (-1.0, 45)],

            AudioCue::Test => &[(440.0, 600)],
        }
    }

    /// Peak level for this cue.
    ///
    /// The transmit cues are quieter than the rest: they fire constantly, and
    /// something you hear on every press has to sit under the conversation
    /// rather than on top of it.
    fn amplitude(self) -> f32 {
        match self {
            AudioCue::TransmitStart | AudioCue::TransmitEnd => 0.12,
            _ => 0.22,
        }
    }
}

/// Renders a cue to PCM at [`SAMPLE_RATE`].
///
/// Each segment is faded in and out over a few milliseconds; without that the
/// abrupt start and stop produce an audible click that is worse than the tone.
pub fn render_cue(cue: AudioCue) -> Vec<f32> {
    render_segments(cue.segments(), cue.amplitude())
}

/// Renders `(frequency_hz, milliseconds)` segments.
///
/// Frequency `0` is silence and a *negative* frequency is a burst of noise,
/// which is what gives the release cue its squelch tail — a pure tone alone
/// sounds like a doorbell rather than a radio.
fn render_segments(segments: &[(f32, u32)], amplitude: f32) -> Vec<f32> {
    let mut out = Vec::new();
    // Deterministic, so a cue sounds identical every time and the tests can
    // assert on it.
    let mut rng: u32 = 0x9E37_79B9;

    for &(freq, millis) in segments {
        let n = (SAMPLE_RATE as u64 * millis as u64 / 1000) as usize;
        if n == 0 {
            continue;
        }
        // ~5 ms of ramp at each end, but never more than a third of the segment.
        let ramp = ((SAMPLE_RATE as usize / 200).min(n / 3)).max(1);
        for i in 0..n {
            let env = if i < ramp {
                i as f32 / ramp as f32
            } else if i >= n - ramp {
                (n - i) as f32 / ramp as f32
            } else {
                1.0
            };
            let s = if freq < 0.0 {
                rng = rng.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                (rng >> 8) as f32 / 8_388_608.0 - 1.0
            } else if freq == 0.0 {
                0.0
            } else {
                (std::f32::consts::TAU * freq * i as f32 / SAMPLE_RATE as f32).sin()
            };
            out.push(s * env * amplitude);
        }
    }
    out
}

/// Bounds on the user-adjustable gains, in dB.
pub const MIN_INPUT_GAIN_DB: f32 = -20.0;
pub const MAX_INPUT_GAIN_DB: f32 = 30.0;
pub const MIN_OUTPUT_VOLUME_DB: f32 = -40.0;
pub const MAX_OUTPUT_VOLUME_DB: f32 = 10.0;

impl AudioShared {
    fn new() -> Self {
        Self {
            capture_queue: Mutex::new(VecDeque::with_capacity(MAX_QUEUED_INPUT_SAMPLES)),
            playback_queue: Mutex::new(VecDeque::with_capacity(MAX_QUEUED_OUTPUT_SAMPLES)),
            monitor_queue: Mutex::new(VecDeque::new()),
            speakers: Mutex::new(HashMap::new()),
            transmitting: AtomicBool::new(false),
            muted: AtomicBool::new(false),
            deafened: AtomicBool::new(false),
            input_level: AtomicU32::new(0),
            output_level: AtomicU32::new(0),
            activation_threshold: AtomicU32::new(0),
            noise_floor: AtomicU32::new(0),
            speech_detected: AtomicBool::new(false),
            running: AtomicBool::new(true),
            input_gain_db: AtomicI32::new(0),
            output_volume_db: AtomicI32::new(0),
            monitor: AtomicBool::new(false),
            echo_cancellation: AtomicBool::new(true),
            normalise_levels: AtomicBool::new(true),
            reverb_enabled: AtomicBool::new(true),
            reverb: Mutex::new(Reverb::new(REVERB_DECAY_SECS, REVERB_WET)),
            concealed_frames: AtomicU64::new(0),
            decoded_frames: AtomicU64::new(0),
            underrun_samples: AtomicU64::new(0),
            capture_dropped_samples: AtomicU64::new(0),
            active_speakers: AtomicU32::new(0),
            cue_queue: Mutex::new(VecDeque::new()),
            echo_reference: Mutex::new(VecDeque::new()),
            device_request: Mutex::new((None, None)),
            device_generation: AtomicU64::new(0),
        }
    }

    /// Sets microphone gain in dB, clamped to the supported range.
    pub fn set_input_gain_db(&self, db: f32) {
        let v = db.clamp(MIN_INPUT_GAIN_DB, MAX_INPUT_GAIN_DB);
        self.input_gain_db
            .store((v * 100.0) as i32, Ordering::Relaxed);
    }

    pub fn input_gain_db(&self) -> f32 {
        self.input_gain_db.load(Ordering::Relaxed) as f32 / 100.0
    }

    /// Sets playback volume in dB, clamped to the supported range.
    pub fn set_output_volume_db(&self, db: f32) {
        let v = db.clamp(MIN_OUTPUT_VOLUME_DB, MAX_OUTPUT_VOLUME_DB);
        self.output_volume_db
            .store((v * 100.0) as i32, Ordering::Relaxed);
    }

    pub fn output_volume_db(&self) -> f32 {
        self.output_volume_db.load(Ordering::Relaxed) as f32 / 100.0
    }

    /// Enables loopback monitoring, so the user hears their own processed voice.
    pub fn set_monitor(&self, on: bool) {
        self.monitor.store(on, Ordering::Relaxed);
        if !on {
            self.monitor_queue.lock().clear();
        }
    }

    pub fn is_monitoring(&self) -> bool {
        self.monitor.load(Ordering::Relaxed)
    }

    /// `(output underruns, capture drops)` in samples since start.
    pub fn glitch_counts(&self) -> (u64, u64) {
        (
            self.underrun_samples.load(Ordering::Relaxed),
            self.capture_dropped_samples.load(Ordering::Relaxed),
        )
    }

    pub fn reset_glitch_counts(&self) {
        self.underrun_samples.store(0, Ordering::Relaxed);
        self.capture_dropped_samples.store(0, Ordering::Relaxed);
    }

    /// Current level per speaker, keyed by [`stream_key`].
    ///
    /// Reported from the audio rather than the roster: the server never says
    /// who is talking, so it is only knowable from what actually arrives.
    pub fn speaker_levels(&self) -> Vec<(u64, f32)> {
        self.speakers
            .lock()
            .iter()
            .map(|(key, buf)| (*key, buf.level_db()))
            .collect()
    }

    /// `(invented, decoded)` frames of incoming audio.
    pub fn frame_counts(&self) -> (u64, u64) {
        (
            self.concealed_frames.load(Ordering::Relaxed),
            self.decoded_frames.load(Ordering::Relaxed),
        )
    }

    pub fn set_reverb(&self, on: bool) {
        let was = self.reverb_enabled.swap(on, Ordering::Relaxed);
        if was != on {
            // Otherwise switching it back on replays a tail from whatever was
            // being said when it was switched off.
            self.reverb.lock().reset();
        }
    }

    pub fn reverb_enabled(&self) -> bool {
        self.reverb_enabled.load(Ordering::Relaxed)
    }

    pub fn set_normalise_levels(&self, on: bool) {
        self.normalise_levels.store(on, Ordering::Relaxed);
    }

    pub fn normalise_levels_enabled(&self) -> bool {
        self.normalise_levels.load(Ordering::Relaxed)
    }

    pub fn set_echo_cancellation(&self, on: bool) {
        self.echo_cancellation.store(on, Ordering::Relaxed);
    }

    pub fn echo_cancellation_enabled(&self) -> bool {
        self.echo_cancellation.load(Ordering::Relaxed)
    }

    /// Queues a notification tone.
    ///
    /// Cues bypass the deafen flag deliberately: "the connection dropped" is
    /// exactly the thing a deafened user still needs to know.
    pub fn play_cue(&self, cue: AudioCue) {
        let pcm = render_cue(cue);
        let mut q = self.cue_queue.lock();
        // Replace rather than append: a flapping connection would otherwise
        // queue a backlog of tones that keeps playing long after it settles.
        q.clear();
        q.extend(pcm);
    }

    /// Queues a steady test tone of the given length.
    pub fn play_test_tone(&self, millis: u32) {
        let pcm = render_segments(&[(440.0, millis)], 0.22);
        let mut q = self.cue_queue.lock();
        q.clear();
        q.extend(pcm);
    }

    pub fn stop_test_tone(&self) {
        self.cue_queue.lock().clear();
    }

    pub fn test_tone_active(&self) -> bool {
        !self.cue_queue.lock().is_empty()
    }

    /// Playback level in dBFS.
    pub fn output_level_db(&self) -> f32 {
        self.output_level.load(Ordering::Relaxed) as f32 / 100.0 - 120.0
    }

    /// Requests different capture/playback devices. `None` selects the system
    /// default. Takes effect within a few hundred milliseconds.
    pub fn set_devices(&self, input: Option<String>, output: Option<String>) {
        *self.device_request.lock() = (input, output);
        self.device_generation.fetch_add(1, Ordering::Release);
    }

    pub fn devices(&self) -> (Option<String>, Option<String>) {
        self.device_request.lock().clone()
    }

    fn device_generation(&self) -> u64 {
        self.device_generation.load(Ordering::Acquire)
    }

    /// Keys or unkeys the microphone, with the radio-style confirmation cue.
    ///
    /// The cue lives here rather than in the UI so every route into
    /// transmitting sounds the same: the on-screen button, the floating
    /// overlay and a Bluetooth handlebar remote. It only fires on an actual
    /// change, so a remote that repeats while held stays quiet.
    ///
    /// This plays into the *playback* queue, so it is heard locally and never
    /// reaches the far end.
    pub fn set_transmitting(&self, on: bool) {
        let previous = self.transmitting.swap(on, Ordering::Relaxed);
        if previous != on {
            self.play_cue(if on {
                AudioCue::TransmitStart
            } else {
                AudioCue::TransmitEnd
            });
        }
    }

    pub fn set_muted(&self, on: bool) {
        self.muted.store(on, Ordering::Relaxed);
    }

    pub fn set_deafened(&self, on: bool) {
        self.deafened.store(on, Ordering::Relaxed);
        if on {
            self.playback_queue.lock().clear();
        }
    }

    pub fn is_muted(&self) -> bool {
        self.muted.load(Ordering::Relaxed)
    }

    pub fn is_deafened(&self) -> bool {
        self.deafened.load(Ordering::Relaxed)
    }

    /// Current input level in dBFS.
    pub fn input_level_db(&self) -> f32 {
        self.input_level.load(Ordering::Relaxed) as f32 / 100.0 - 120.0
    }

    pub fn speech_detected(&self) -> bool {
        self.speech_detected.load(Ordering::Relaxed)
    }

    fn store_level(&self, db: f32) {
        let v = ((db + 120.0).clamp(0.0, 120.0) * 100.0) as u32;
        self.input_level.store(v, Ordering::Relaxed);
    }

    fn store_output_level(&self, db: f32) {
        let v = ((db + 120.0).clamp(0.0, 120.0) * 100.0) as u32;
        self.output_level.store(v, Ordering::Relaxed);
    }

    fn store_threshold(&self, db: f32) {
        let v = ((db + 120.0).clamp(0.0, 120.0) * 100.0) as u32;
        self.activation_threshold.store(v, Ordering::Relaxed);
    }

    /// Level voice activation currently opens at, in dBFS.
    pub fn activation_threshold_db(&self) -> f32 {
        self.activation_threshold.load(Ordering::Relaxed) as f32 / 100.0 - 120.0
    }

    fn store_noise_floor(&self, db: f32) {
        let v = ((db + 120.0).clamp(0.0, 120.0) * 100.0) as u32;
        self.noise_floor.store(v, Ordering::Relaxed);
    }

    /// Tracked background noise level in dBFS.
    ///
    /// Reported separately from the activation threshold because the gap
    /// between the two is the margin the rider is actually tuning: on a
    /// motorcycle the floor moves with speed, and seeing only the threshold
    /// makes a rising floor look like a mis-set control.
    pub fn noise_floor_db(&self) -> f32 {
        self.noise_floor.load(Ordering::Relaxed) as f32 / 100.0 - 120.0
    }

    /// Queues a received voice packet for decoding and playback.
    ///
    /// `slot` identifies which server the packet came from, so that concurrent
    /// sessions cannot collide on session id.
    pub fn push_incoming(&self, slot: u16, packet: &VoicePacket) {
        if self.deafened.load(Ordering::Relaxed) {
            return;
        }
        let Some(session) = packet.session else {
            return;
        };
        let key = stream_key(slot, session);
        let mut speakers = self.speakers.lock();
        let buf = match speakers.entry(key) {
            std::collections::hash_map::Entry::Occupied(e) => e.into_mut(),
            std::collections::hash_map::Entry::Vacant(e) => match SpeakerBuffer::new() {
                Ok(b) => e.insert(b),
                Err(_) => return,
            },
        };
        buf.push(packet.sequence, packet.opus.clone(), packet.terminator);
    }

    pub fn stop(&self) {
        self.running.store(false, Ordering::Relaxed);
    }

    fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }
}

/// Combines a server slot and a session id into a globally unique speaker key.
#[inline]
pub fn stream_key(slot: u16, session: u32) -> u64 {
    ((slot as u64) << 32) | session as u64
}

/// Mixes one frame from every active speaker into the playback queue.
///
/// Split out so it can be tested without any audio hardware.
pub fn mix_speakers(shared: &AudioShared, scratch: &mut Vec<f32>, mixed: &mut Vec<f32>) {
    mixed.clear();

    let mut speakers = shared.speakers.lock();
    let mut active = 0usize;

    let normalise = shared.normalise_levels_enabled();
    speakers.retain(|_, buf| {
        if buf.is_finished() {
            return false;
        }
        buf.set_normalisation(normalise);
        if !buf.ready() {
            // Held but not played: a burst shorter than the target backlog
            // would otherwise wait for a transmission that already finished.
            buf.note_waiting();
            return true;
        }
        // Mix exactly what was decoded. Mixing the whole scratch buffer
        // regardless would append whatever the previous frame left in its tail
        // whenever a packet is shorter than the buffer, which is heard as a
        // hiss riding under everything.
        if let Some(n) = buf.pop(scratch) {
            if mixed.len() < n {
                mixed.resize(n, 0.0);
            }
            for (m, s) in mixed.iter_mut().zip(scratch[..n].iter()) {
                *m += *s;
            }
            active += 1;
        }
        true
    });

    // Anyone still holding frames is audio we owe the listener, whether or not
    // it was played this round. Counting only what was played would leave the
    // underrun meter blind to a buffer that is holding back — which is exactly
    // the failure worth catching.
    let expecting = speakers.values().filter(|b| b.buffered() > 0).count();
    let (invented, decoded) = speakers
        .values()
        .map(|b| b.frame_counts())
        .fold((0u64, 0u64), |(a, c), (x, y)| (a + x, c + y));
    shared.concealed_frames.store(invented, Ordering::Relaxed);
    shared.decoded_frames.store(decoded, Ordering::Relaxed);
    drop(speakers);

    shared
        .active_speakers
        .store(active.max(expecting) as u32, Ordering::Relaxed);
    if active == 0 {
        mixed.clear();
        return;
    }
    // Soft-limit the sum so several simultaneous speakers cannot clip.
    if active > 1 {
        let scale = 1.0 / (active as f32).sqrt();
        for s in mixed.iter_mut() {
            *s *= scale;
        }
    }
    for s in mixed.iter_mut() {
        *s = s.clamp(-1.0, 1.0);
    }

    // Applied to the mixed voices and nothing else: cues and the loopback
    // monitor join further downstream, and neither wants a room around it.
    if shared.reverb_enabled() {
        shared.reverb.lock().process(mixed);
    }

    let mut q = shared.playback_queue.lock();
    if q.len() + mixed.len() <= MAX_QUEUED_OUTPUT_SAMPLES {
        q.extend(mixed.iter().copied());
    }
}

/// Configuration for the engine.
#[derive(Debug, Clone)]
pub struct AudioConfig {
    pub noise_profile: NoiseProfile,
    pub quality: Quality,
    pub transmit_mode: TransmitMode,
    pub input_device: Option<String>,
    pub output_device: Option<String>,
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            noise_profile: NoiseProfile::Standard,
            quality: Quality::Balanced,
            transmit_mode: TransmitMode::VoiceActivity,
            input_device: None,
            output_device: None,
        }
    }
}

/// Collects up to `want` samples of what should be played, and records the
/// echo reference for the capture chain.
///
/// The ordering is the whole point, so it lives here rather than inline in the
/// output callback where it cannot be tested. Remote audio becomes the echo
/// reference; monitoring is mixed in afterwards and never referenced.
///
/// Referencing the monitor would hand the canceller the near-end talker as its
/// far-end signal, and an adaptive filter told that the person at the
/// microphone is the echo will duly learn to remove them — heard as one's own
/// voice disappearing under a howl, which is the opposite of a microphone test.
fn fill_output_block(shared: &AudioShared, want: usize, out: &mut Vec<f32>) {
    out.clear();
    {
        let mut queue = shared.playback_queue.lock();
        for _ in 0..want {
            match queue.pop_front() {
                Some(s) => out.push(s),
                None => break,
            }
        }
    }

    // Cues are mixed over the voice, never queued behind it.
    //
    // Appending them instead makes a cue *displace* speech rather than sound
    // over it: every cue pushes the voice already queued further into the
    // future, so latency grows by the length of each one and never comes back,
    // until the queue hits its cap and starts discarding audio. A cue that
    // repeats — the dialing one does, every few seconds — turns that into a
    // steady climb, heard as speech breaking up for reasons that have nothing
    // to do with the network.
    mix_in(&shared.cue_queue, want, out);

    // Taken after the cues, because they genuinely come out of the speaker and
    // genuinely echo back into the microphone, and at 48 kHz before resampling
    // so it matches what the capture chain sees.
    {
        let mut echo = shared.echo_reference.lock();
        if echo.len() + out.len() <= MAX_QUEUED_OUTPUT_SAMPLES {
            echo.extend(out.iter().copied());
        } else {
            // The worker has fallen behind; resync rather than feed the
            // canceller a stale reference.
            echo.clear();
        }
    }

    // Monitoring is mixed in last, after the reference has been taken, so the
    // canceller never sees the near-end talker as its own far-end signal.
    mix_in(&shared.monitor_queue, want, out);
}

/// Mixes up to `want` samples from `queue` over `out`, extending it when the
/// queue outruns it — a cue or the monitor has to be able to drive the output
/// on its own when nothing else is playing.
fn mix_in(queue: &Mutex<VecDeque<f32>>, want: usize, out: &mut Vec<f32>) {
    let mut queue = queue.lock();
    for i in 0..want {
        let Some(sample) = queue.pop_front() else {
            break;
        };
        match out.get_mut(i) {
            Some(existing) => *existing = (*existing + sample).clamp(-1.0, 1.0),
            None => out.push(sample.clamp(-1.0, 1.0)),
        }
    }
}

/// Lists the available input and output device names.
pub fn list_devices() -> (Vec<String>, Vec<String>) {
    let host = cpal::default_host();
    // cpal 0.18 exposes the device name through `Display` rather than a `name()`
    // method; `description()` carries the richer structured metadata.
    let inputs = host
        .input_devices()
        .map(|it| it.map(|d| d.to_string()).collect())
        .unwrap_or_default();
    let outputs = host
        .output_devices()
        .map(|it| it.map(|d| d.to_string()).collect())
        .unwrap_or_default();
    (inputs, outputs)
}

fn pick_input(host: &cpal::Host, name: Option<&str>) -> Option<cpal::Device> {
    if let Some(n) = name {
        if let Ok(mut it) = host.input_devices() {
            if let Some(d) = it.find(|d| d.to_string() == n) {
                return Some(d);
            }
        }
    }
    host.default_input_device()
}

fn pick_output(host: &cpal::Host, name: Option<&str>) -> Option<cpal::Device> {
    if let Some(n) = name {
        if let Ok(mut it) = host.output_devices() {
            if let Some(d) = it.find(|d| d.to_string() == n) {
                return Some(d);
            }
        }
    }
    host.default_output_device()
}

/// Handle to a running engine. Dropping it stops audio.
pub struct AudioEngine {
    shared: Arc<AudioShared>,
    /// Signals the device thread to tear down its streams.
    _thread: std::thread::JoinHandle<()>,
}

impl AudioEngine {
    pub fn shared(&self) -> Arc<AudioShared> {
        self.shared.clone()
    }

    /// Starts capture and playback.
    ///
    /// `on_frame` is called from the worker thread for every encoded 20 ms frame
    /// that should be transmitted.
    pub fn start<F>(config: AudioConfig, mut on_frame: F) -> Result<Self>
    where
        F: FnMut(u64, Vec<u8>, bool) + Send + 'static,
    {
        let shared = Arc::new(AudioShared::new());
        // Seed the request with the starting choice so `devices()` reports it.
        *shared.device_request.lock() = (config.input_device.clone(), config.output_device.clone());

        // cpal streams are not Send on every platform, so they are created and
        // owned by a dedicated thread that outlives them.
        let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<()>>();
        let dev_shared = shared.clone();
        let dev_config = config.clone();

        let thread = std::thread::Builder::new()
            .name("mumbleway-audio".into())
            .spawn(move || {
                let mut config = dev_config;
                let mut generation = dev_shared.device_generation();

                let mut streams = match build_streams(&config, &dev_shared) {
                    Ok(s) => {
                        let _ = ready_tx.send(Ok(()));
                        Some(s)
                    }
                    Err(e) => {
                        let _ = ready_tx.send(Err(e));
                        return;
                    }
                };

                while dev_shared.is_running() {
                    std::thread::sleep(std::time::Duration::from_millis(100));

                    let current = dev_shared.device_generation();
                    if current == generation {
                        continue;
                    }
                    generation = current;

                    let (input, output) = dev_shared.devices();
                    config.input_device = input;
                    config.output_device = output;

                    // Close the old streams before opening the new ones: some
                    // drivers (and most Bluetooth headsets) only allow one
                    // exclusive client at a time.
                    streams = None;
                    match build_streams(&config, &dev_shared) {
                        Ok(s) => streams = Some(s),
                        Err(e) => {
                            // Keep the engine alive with no device rather than
                            // killing the session; the user can pick another.
                            tracing::error!("could not switch audio device: {e}");
                        }
                    }
                }
                drop(streams);
            })
            .map_err(|e| CoreError::Audio(format!("spawning audio thread: {e}")))?;

        // Surface device errors to the caller rather than failing silently.
        match ready_rx.recv_timeout(std::time::Duration::from_secs(10)) {
            Ok(Ok(())) => {}
            Ok(Err(e)) => return Err(e),
            Err(_) => return Err(CoreError::Audio("audio device did not start".into())),
        }

        // Worker: capture DSP + encode, and playback mixing.
        let worker_shared = shared.clone();
        let worker_config = config.clone();
        std::thread::Builder::new()
            .name("mumbleway-dsp".into())
            .spawn(move || {
                run_worker(worker_config, worker_shared, &mut on_frame);
            })
            .map_err(|e| CoreError::Audio(format!("spawning DSP thread: {e}")))?;

        Ok(Self {
            shared,
            _thread: thread,
        })
    }
}

impl Drop for AudioEngine {
    fn drop(&mut self) {
        self.shared.stop();
    }
}

type Streams = (cpal::Stream, cpal::Stream);

fn build_streams(config: &AudioConfig, shared: &Arc<AudioShared>) -> Result<Streams> {
    let host = cpal::default_host();

    let input = pick_input(&host, config.input_device.as_deref())
        .ok_or_else(|| CoreError::Audio("no input device available".into()))?;
    let output = pick_output(&host, config.output_device.as_deref())
        .ok_or_else(|| CoreError::Audio("no output device available".into()))?;

    let in_cfg = input
        .default_input_config()
        .map_err(|e| CoreError::Audio(format!("input config: {e}")))?;
    let out_cfg = output
        .default_output_config()
        .map_err(|e| CoreError::Audio(format!("output config: {e}")))?;

    let in_rate = in_cfg.sample_rate();
    let in_channels = in_cfg.channels() as usize;
    let out_rate = out_cfg.sample_rate();
    let out_channels = out_cfg.channels() as usize;

    // --- input ------------------------------------------------------------
    let cap_shared = shared.clone();
    let mut in_resampler = Resampler::new(in_rate, SAMPLE_RATE);
    let mut mono_scratch: Vec<f32> = Vec::with_capacity(2048);
    let mut resampled: Vec<f32> = Vec::with_capacity(2048);

    let in_stream = input
        .build_input_stream(
            in_cfg.config(),
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                interleaved_to_mono(data, in_channels, &mut mono_scratch);
                resampled.clear();
                in_resampler.process(&mono_scratch, &mut resampled);

                let mut q = cap_shared.capture_queue.lock();
                // Drop the oldest audio rather than growing without bound if the
                // worker ever falls behind.
                if q.len() + resampled.len() > MAX_QUEUED_INPUT_SAMPLES {
                    let excess = q.len() + resampled.len() - MAX_QUEUED_INPUT_SAMPLES;
                    let drop_count = excess.min(q.len());
                    q.drain(..drop_count);
                    cap_shared
                        .capture_dropped_samples
                        .fetch_add(drop_count as u64, Ordering::Relaxed);
                }
                q.extend(resampled.iter().copied());
            },
            move |e| tracing::warn!("input stream error: {e}"),
            None,
        )
        .map_err(|e| CoreError::Audio(format!("building input stream: {e}")))?;

    // --- output -----------------------------------------------------------
    let play_shared = shared.clone();
    let mut out_resampler = Resampler::new(SAMPLE_RATE, out_rate);
    let mut pending: VecDeque<f32> = VecDeque::with_capacity(4096);
    let mut pull: Vec<f32> = Vec::with_capacity(2048);
    let mut converted: Vec<f32> = Vec::with_capacity(2048);

    let out_stream = output
        .build_output_stream(
            out_cfg.config(),
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                let frames_needed = data.len() / out_channels.max(1);

                // Top up the device-rate buffer from the 48 kHz mix.
                while pending.len() < frames_needed {
                    let want = 480;
                    fill_output_block(&play_shared, want, &mut pull);
                    if pull.is_empty() {
                        break; // nothing to play; the rest becomes silence
                    }

                    converted.clear();
                    out_resampler.process(&pull, &mut converted);
                    pending.extend(converted.iter().copied());
                }

                // Volume is applied here rather than in the mixer so it covers
                // everything the user might be listening to: voice, the
                // loopback monitor and the test tone alike.
                let vol = 10f32.powf(play_shared.output_volume_db() / 20.0);
                let mut peak = 0.0f32;
                let mut underruns = 0u64;
                for frame in data.chunks_mut(out_channels.max(1)) {
                    let raw = match pending.pop_front() {
                        Some(v) => v,
                        None => {
                            // Nothing ready: this is a real gap in the audio,
                            // not silence anyone asked for.
                            underruns += 1;
                            0.0
                        }
                    };
                    let s = (raw * vol).clamp(-1.0, 1.0);
                    peak = peak.max(s.abs());
                    for ch in frame.iter_mut() {
                        *ch = s;
                    }
                }
                // Only a gap if somebody was actually talking. Silence while
                // nobody is is not a dropout, and counting it hides the ones
                // that matter under hours of idle.
                if underruns > 0 && play_shared.active_speakers.load(Ordering::Relaxed) > 0 {
                    play_shared
                        .underrun_samples
                        .fetch_add(underruns, Ordering::Relaxed);
                }
                play_shared.store_output_level(super::dsp::to_dbfs(peak));
            },
            move |e| tracing::warn!("output stream error: {e}"),
            None,
        )
        .map_err(|e| CoreError::Audio(format!("building output stream: {e}")))?;

    in_stream
        .play()
        .map_err(|e| CoreError::Audio(format!("starting input: {e}")))?;
    out_stream
        .play()
        .map_err(|e| CoreError::Audio(format!("starting output: {e}")))?;

    Ok((in_stream, out_stream))
}

fn run_worker<F>(config: AudioConfig, shared: Arc<AudioShared>, on_frame: &mut F)
where
    F: FnMut(u64, Vec<u8>, bool) + Send + 'static,
{
    let mut processor = CaptureProcessor::new(config.noise_profile);
    let mut encoder = match VoiceEncoder::new(config.quality) {
        Ok(e) => e,
        Err(e) => {
            tracing::error!("cannot create Opus encoder: {e}");
            return;
        }
    };

    let mut block = vec![0.0f32; FRAME_SIZE];
    let mut echo_ref: Vec<f32> = Vec::with_capacity(FRAME_SIZE);
    let mut frame: Vec<f32> = Vec::with_capacity(FRAME_SAMPLES);
    let mut sequence: u64 = 0;
    let mut was_transmitting = false;

    let mut mix_scratch = Vec::new();
    let mut mixed = Vec::new();

    while shared.is_running() {
        let mut did_work = false;

        // --- capture path -------------------------------------------------
        loop {
            {
                let mut q = shared.capture_queue.lock();
                if q.len() < FRAME_SIZE {
                    break;
                }
                for slot in block.iter_mut() {
                    *slot = q.pop_front().unwrap_or(0.0);
                }
            }
            did_work = true;

            // Microphone gain goes in ahead of the DSP chain, so the level
            // meter, the gate and the far end all see the same signal.
            let gain = 10f32.powf(shared.input_gain_db() / 20.0);
            if (gain - 1.0).abs() > 1e-3 {
                for s in block.iter_mut() {
                    *s *= gain;
                }
            }

            // Take the matching stretch of what was played, so the canceller
            // has a reference for this block. Short-fill with silence rather
            // than stalling: a missing reference simply means nothing to cancel.
            echo_ref.clear();
            {
                let mut echo = shared.echo_reference.lock();
                for _ in 0..FRAME_SIZE {
                    match echo.pop_front() {
                        Some(s) => echo_ref.push(s),
                        None => break,
                    }
                }
            }
            echo_ref.resize(FRAME_SIZE, 0.0);

            // Picked up here rather than at construction so the switch takes
            // effect on the next block instead of the next restart.
            let want_aec = shared.echo_cancellation_enabled();
            if want_aec != processor.echo_cancellation_enabled() {
                processor.set_echo_cancellation(want_aec);
            }

            let analysis = processor.process_with_reference(&mut block, &echo_ref);
            shared.store_level(analysis.level_db);
            shared.store_threshold(analysis.activation_threshold_db);
            shared.store_noise_floor(analysis.noise_floor_db);
            shared
                .speech_detected
                .store(analysis.speaking, Ordering::Relaxed);

            // Loopback monitoring: hear exactly what would be transmitted.
            if shared.is_monitoring() {
                let mut q = shared.monitor_queue.lock();
                if q.len() + block.len() <= MAX_QUEUED_OUTPUT_SAMPLES {
                    q.extend(block.iter().copied());
                }
            }

            let allowed = !shared.is_muted()
                && match config.transmit_mode {
                    TransmitMode::Continuous => true,
                    TransmitMode::PushToTalk => shared.transmitting.load(Ordering::Relaxed),
                    TransmitMode::VoiceActivity => analysis.speaking,
                };

            frame.extend_from_slice(&block);
            if frame.len() >= FRAME_SAMPLES {
                if allowed {
                    if let Ok(packet) = encoder.encode(&frame[..FRAME_SAMPLES]) {
                        on_frame(sequence, packet, false);
                        sequence += SEQ_UNITS_PER_FRAME;
                        was_transmitting = true;
                    }
                } else if was_transmitting {
                    // Send one terminator so the far end closes the stream
                    // immediately instead of waiting for its jitter buffer to
                    // time out.
                    if let Ok(packet) = encoder.encode(&vec![0.0; FRAME_SAMPLES]) {
                        on_frame(sequence, packet, true);
                        sequence += SEQ_UNITS_PER_FRAME;
                    }
                    // The counter keeps climbing for the life of the session.
                    // Restarting it at zero puts every later burst *behind*
                    // the receiver's play head, and a jitter buffer discards
                    // what is behind its play head as arriving too late — so
                    // the opening of every utterance after the first is thrown
                    // away, and more of it the longer the session runs.
                    was_transmitting = false;
                }
                frame.clear();
            }
        }

        // Cues are not drained here. The output callback mixes them over the
        // voice as it plays, which is the only way a cue sounds *over* speech
        // rather than pushing it later.

        // --- playback path ------------------------------------------------
        let need_more = shared.playback_queue.lock().len() < FRAME_SAMPLES * 3;
        if need_more {
            mix_speakers(&shared, &mut mix_scratch, &mut mixed);
            did_work |= !mixed.is_empty();
        }

        if !did_work {
            // Nothing to do; sleep for well under one block so we never fall behind.
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::codec::{Quality, VoiceEncoder};

    fn encoded_frames(n: usize) -> Vec<Vec<u8>> {
        let mut enc = VoiceEncoder::new(Quality::Balanced).unwrap();
        (0..n)
            .map(|i| {
                let pcm: Vec<f32> = (0..FRAME_SAMPLES)
                    .map(|s| {
                        let t = (i * FRAME_SAMPLES + s) as f32 / 48000.0;
                        (2.0 * std::f32::consts::PI * 300.0 * t).sin() * 0.5
                    })
                    .collect();
                enc.encode(&pcm).unwrap()
            })
            .collect()
    }

    fn packet(session: u32, seq: u64, opus: Vec<u8>, term: bool) -> VoicePacket {
        VoicePacket {
            session: Some(session),
            target: 0,
            sequence: seq,
            opus,
            terminator: term,
            position: None,
        }
    }

    #[test]
    fn mixes_a_single_speaker_into_the_playback_queue() {
        let shared = AudioShared::new();
        for (i, f) in encoded_frames(6).into_iter().enumerate() {
            shared.push_incoming(0, &packet(1, i as u64, f, false));
        }
        let (mut a, mut b) = (Vec::new(), Vec::new());
        mix_speakers(&shared, &mut a, &mut b);

        let q = shared.playback_queue.lock();
        assert_eq!(q.len(), FRAME_SAMPLES, "one frame should have been mixed");
        assert!(q.iter().any(|s| s.abs() > 0.01), "mixed audio was silent");
    }

    #[test]
    fn mixes_two_speakers_without_clipping() {
        let shared = AudioShared::new();
        let frames = encoded_frames(6);
        for (i, f) in frames.iter().enumerate() {
            shared.push_incoming(0, &packet(1, i as u64, f.clone(), false));
            shared.push_incoming(0, &packet(2, i as u64, f.clone(), false));
        }
        let (mut a, mut b) = (Vec::new(), Vec::new());
        mix_speakers(&shared, &mut a, &mut b);

        let q = shared.playback_queue.lock();
        assert!(q.iter().all(|s| s.abs() <= 1.0), "mix clipped");
        assert!(q.iter().any(|s| s.abs() > 0.01), "mix was silent");
    }

    #[test]
    fn deafened_drops_incoming_audio() {
        let shared = AudioShared::new();
        shared.set_deafened(true);
        for (i, f) in encoded_frames(6).into_iter().enumerate() {
            shared.push_incoming(0, &packet(1, i as u64, f, false));
        }
        let (mut a, mut b) = (Vec::new(), Vec::new());
        mix_speakers(&shared, &mut a, &mut b);
        assert!(
            shared.playback_queue.lock().is_empty(),
            "deafened must not queue audio"
        );
    }

    #[test]
    fn finished_speakers_are_reaped() {
        let shared = AudioShared::new();
        let frames = encoded_frames(2);
        // Numbered in 10 ms units, as a conforming sender of 20 ms frames does.
        shared.push_incoming(0, &packet(7, 0, frames[0].clone(), false));
        shared.push_incoming(0, &packet(7, SEQ_UNITS_PER_FRAME, frames[1].clone(), true));

        let (mut a, mut b) = (Vec::new(), Vec::new());
        for _ in 0..6 {
            mix_speakers(&shared, &mut a, &mut b);
        }
        assert!(
            shared.speakers.lock().is_empty(),
            "a finished speaker should be dropped"
        );
    }

    #[test]
    fn playback_queue_is_bounded() {
        let shared = AudioShared::new();
        let frames = encoded_frames(1);
        // Push far more audio than could ever be consumed.
        for i in 0..5000u64 {
            shared.push_incoming(0, &packet(1, i, frames[0].clone(), false));
        }
        let (mut a, mut b) = (Vec::new(), Vec::new());
        for _ in 0..500 {
            mix_speakers(&shared, &mut a, &mut b);
        }
        assert!(
            shared.playback_queue.lock().len() <= MAX_QUEUED_OUTPUT_SAMPLES,
            "playback queue grew unbounded"
        );
    }

    #[test]
    fn level_meter_roundtrips_through_the_atomic() {
        let shared = AudioShared::new();
        for db in [-90.0f32, -60.0, -20.0, -3.0, 0.0] {
            shared.store_level(db);
            assert!(
                (shared.input_level_db() - db).abs() < 0.05,
                "level {db} came back as {}",
                shared.input_level_db()
            );
        }
    }

    #[test]
    fn noise_floor_and_threshold_are_reported_separately() {
        // The meters draw both, and the gap between them is the margin being
        // tuned. Sharing one atomic would collapse that gap to zero and make a
        // rising noise floor invisible.
        let shared = AudioShared::new();
        shared.store_noise_floor(-52.0);
        shared.store_threshold(-43.0);

        assert!((shared.noise_floor_db() - -52.0).abs() < 0.05);
        assert!((shared.activation_threshold_db() - -43.0).abs() < 0.05);

        // And the floor can move without dragging the threshold with it.
        shared.store_noise_floor(-31.0);
        assert!((shared.noise_floor_db() - -31.0).abs() < 0.05);
        assert!((shared.activation_threshold_db() - -43.0).abs() < 0.05);
    }

    #[test]
    fn mute_and_deafen_flags_are_independent() {
        let shared = AudioShared::new();
        assert!(!shared.is_muted() && !shared.is_deafened());
        shared.set_muted(true);
        assert!(shared.is_muted() && !shared.is_deafened());
        shared.set_deafened(true);
        assert!(shared.is_muted() && shared.is_deafened());
        shared.set_muted(false);
        assert!(!shared.is_muted() && shared.is_deafened());
    }

    #[test]
    fn packets_without_a_session_are_ignored() {
        let shared = AudioShared::new();
        let mut p = packet(1, 0, encoded_frames(1)[0].clone(), false);
        p.session = None;
        shared.push_incoming(0, &p);
        assert!(shared.speakers.lock().is_empty());
    }

    #[test]
    fn identical_session_ids_on_two_servers_stay_separate() {
        // The dual-server case: both servers happen to assign session id 1 to
        // different people. Their audio must not land in the same jitter buffer,
        // or the two streams would interleave into garbage.
        let shared = AudioShared::new();
        let frames = encoded_frames(4);
        for (i, f) in frames.iter().enumerate() {
            shared.push_incoming(0, &packet(1, i as u64, f.clone(), false));
            shared.push_incoming(1, &packet(1, i as u64, f.clone(), false));
        }
        assert_eq!(
            shared.speakers.lock().len(),
            2,
            "same session id on different servers must produce two speakers"
        );
    }

    #[test]
    fn gains_clamp_to_their_supported_range() {
        let s = AudioShared::new();
        assert_eq!(s.input_gain_db(), 0.0, "starts at unity");

        s.set_input_gain_db(12.5);
        assert!((s.input_gain_db() - 12.5).abs() < 0.02);

        // Beyond the range it must clamp, not wrap or distort wildly.
        s.set_input_gain_db(1000.0);
        assert_eq!(s.input_gain_db(), MAX_INPUT_GAIN_DB);
        s.set_input_gain_db(-1000.0);
        assert_eq!(s.input_gain_db(), MIN_INPUT_GAIN_DB);

        s.set_output_volume_db(-6.0);
        assert!((s.output_volume_db() + 6.0).abs() < 0.02);
        s.set_output_volume_db(999.0);
        assert_eq!(s.output_volume_db(), MAX_OUTPUT_VOLUME_DB);
        s.set_output_volume_db(-999.0);
        assert_eq!(s.output_volume_db(), MIN_OUTPUT_VOLUME_DB);
    }

    #[test]
    fn output_level_meter_roundtrips() {
        let s = AudioShared::new();
        for db in [-80.0f32, -24.0, -1.0] {
            s.store_output_level(db);
            assert!((s.output_level_db() - db).abs() < 0.05);
        }
    }

    #[test]
    fn disabling_monitor_flushes_pending_playback() {
        // Otherwise a burst of your own voice keeps playing after you switch
        // monitoring off.
        let s = AudioShared::new();
        s.set_monitor(true);
        assert!(s.is_monitoring());
        s.monitor_queue
            .lock()
            .extend(std::iter::repeat_n(0.4, 4800));

        s.set_monitor(false);
        assert!(!s.is_monitoring());
        assert!(s.monitor_queue.lock().is_empty());
    }

    #[test]
    fn a_cue_sounds_over_speech_instead_of_displacing_it() {
        // The bug this guards against does not sound like a cue problem: the
        // cue plays fine, and the *speech* breaks up, seemingly at random and
        // seemingly because of the network. Queued behind the voice, every cue
        // adds its whole length to the backlog and never gives it back.
        let shared = AudioShared::new();
        shared
            .playback_queue
            .lock()
            .extend(std::iter::repeat_n(0.2, 960));
        shared
            .cue_queue
            .lock()
            .extend(std::iter::repeat_n(0.5, 480));

        let mut out = Vec::new();
        fill_output_block(&shared, 480, &mut out);

        // The cue is heard on top of the speech in this very block...
        assert_eq!(out.len(), 480);
        assert!(
            out.iter().all(|s| (*s - 0.7).abs() < 1e-6),
            "cue did not mix"
        );

        // ...and the speech has advanced by exactly one block, not been pushed
        // 480 samples further into the future.
        assert_eq!(
            shared.playback_queue.lock().len(),
            480,
            "the cue displaced queued speech instead of mixing with it"
        );
        assert!(shared.cue_queue.lock().is_empty());
    }

    #[test]
    fn a_repeating_cue_does_not_grow_the_backlog() {
        // The dialing cue repeats for as long as a connection is being chased.
        // If each repeat added to the queue, latency would climb until the cap
        // started discarding audio.
        let shared = AudioShared::new();
        let mut out = Vec::new();
        for _ in 0..40 {
            shared
                .playback_queue
                .lock()
                .extend(std::iter::repeat_n(0.1, 480));
            shared
                .cue_queue
                .lock()
                .extend(std::iter::repeat_n(0.3, 480));
            fill_output_block(&shared, 480, &mut out);
        }
        assert!(
            shared.playback_queue.lock().len() <= 480,
            "backlog grew to {} samples",
            shared.playback_queue.lock().len()
        );
    }

    #[test]
    fn cues_are_referenced_because_they_really_are_played() {
        // Unlike the monitor, a cue leaves the speaker and echoes back, so the
        // canceller has to know about it.
        let shared = AudioShared::new();
        shared
            .cue_queue
            .lock()
            .extend(std::iter::repeat_n(0.4, 480));

        let mut out = Vec::new();
        fill_output_block(&shared, 480, &mut out);

        let echo = shared.echo_reference.lock();
        assert_eq!(echo.len(), 480);
        assert!(echo.iter().all(|s| (*s - 0.4).abs() < 1e-6));
    }

    #[test]
    fn monitored_audio_never_becomes_the_echo_reference() {
        // The bug this guards against is subtle and sounds like a hardware
        // fault: with monitoring referenced, the canceller is told the person
        // at the microphone is the echo, converges on removing them, and the
        // user hears a howl instead of themselves.
        let shared = AudioShared::new();
        shared.set_monitor(true);
        shared
            .monitor_queue
            .lock()
            .extend(std::iter::repeat_n(0.5, 480));

        let mut out = Vec::new();
        fill_output_block(&shared, 480, &mut out);

        assert_eq!(out.len(), 480, "monitoring must drive output on its own");
        assert!(out.iter().all(|s| (*s - 0.5).abs() < 1e-6));
        assert!(
            shared.echo_reference.lock().is_empty(),
            "the near-end talker must never be handed back as the far end"
        );
    }

    #[test]
    fn remote_audio_is_referenced_and_monitoring_mixes_on_top() {
        let shared = AudioShared::new();
        shared
            .playback_queue
            .lock()
            .extend(std::iter::repeat_n(0.25, 480));
        shared.set_monitor(true);
        shared
            .monitor_queue
            .lock()
            .extend(std::iter::repeat_n(0.5, 480));

        let mut out = Vec::new();
        fill_output_block(&shared, 480, &mut out);

        // Heard: both. Referenced: only what came from the far end.
        assert!(out.iter().all(|s| (*s - 0.75).abs() < 1e-6));
        let echo = shared.echo_reference.lock();
        assert_eq!(echo.len(), 480);
        assert!(echo.iter().all(|s| (*s - 0.25).abs() < 1e-6));
    }

    #[test]
    fn output_mixing_clamps_rather_than_wrapping() {
        let shared = AudioShared::new();
        shared
            .playback_queue
            .lock()
            .extend(std::iter::repeat_n(0.9, 480));
        shared.set_monitor(true);
        shared
            .monitor_queue
            .lock()
            .extend(std::iter::repeat_n(0.9, 480));

        let mut out = Vec::new();
        fill_output_block(&shared, 480, &mut out);
        assert!(out.iter().all(|s| *s <= 1.0 && *s >= -1.0));
    }

    #[test]
    fn monitoring_does_not_leave_remote_audio_behind() {
        // Switching the microphone test off must not silence people who are
        // actually talking, so the two queues have to stay separate.
        let s = AudioShared::new();
        s.playback_queue
            .lock()
            .extend(std::iter::repeat_n(0.3, 960));
        s.set_monitor(true);
        s.monitor_queue.lock().extend(std::iter::repeat_n(0.4, 960));

        s.set_monitor(false);
        assert!(s.monitor_queue.lock().is_empty());
        assert_eq!(
            s.playback_queue.lock().len(),
            960,
            "remote audio was dropped when monitoring stopped"
        );
    }

    #[test]
    fn test_tone_is_rendered_at_the_requested_length_and_can_be_cancelled() {
        let s = AudioShared::new();
        assert!(!s.test_tone_active());

        s.play_test_tone(500);
        assert!(s.test_tone_active());
        assert_eq!(
            s.cue_queue.lock().len(),
            SAMPLE_RATE as usize / 2,
            "500 ms at 48 kHz"
        );

        s.stop_test_tone();
        assert!(!s.test_tone_active());
    }

    #[test]
    fn drop_and_resume_cues_are_audibly_different() {
        // A rider identifies these by ear alone, so they must not be the same
        // sound: the drop falls in pitch and the resume rises.
        let drop = render_cue(AudioCue::Disconnected);
        let resume = render_cue(AudioCue::Reconnected);

        assert!(!drop.is_empty() && !resume.is_empty());
        assert_ne!(drop, resume, "cues must be distinguishable");

        // Compare the dominant pitch of the first and last segments by counting
        // zero crossings; falling versus rising is the whole point.
        let crossings = |s: &[f32]| {
            s.windows(2)
                .filter(|w| (w[0] < 0.0) != (w[1] < 0.0))
                .count()
        };
        let head = |s: &[f32]| crossings(&s[..s.len() / 4]);
        let tail = |s: &[f32]| crossings(&s[3 * s.len() / 4..]);

        assert!(
            head(&drop) > tail(&drop),
            "the drop cue should fall in pitch"
        );
        assert!(
            tail(&resume) > head(&resume),
            "the resume cue should rise in pitch"
        );
    }

    #[test]
    fn cues_stay_in_range_and_start_and_end_silently() {
        // Without the fade, the abrupt edges click louder than the tone itself.
        for cue in [
            AudioCue::Disconnected,
            AudioCue::Reconnected,
            AudioCue::Test,
        ] {
            let pcm = render_cue(cue);
            assert!(
                pcm.iter().all(|s| s.is_finite() && s.abs() <= 1.0),
                "{cue:?} out of range"
            );
            assert!(pcm[0].abs() < 0.02, "{cue:?} starts with a click");
            assert!(pcm[pcm.len() - 1].abs() < 0.02, "{cue:?} ends with a click");
        }
    }

    /// Every cue, so coverage cannot silently miss a newly added one.
    const ALL_CUES: [AudioCue; 9] = [
        AudioCue::Disconnected,
        AudioCue::Reconnected,
        AudioCue::Dialing,
        AudioCue::MutedByOther,
        AudioCue::UnmutedByOther,
        AudioCue::DeafenedByOther,
        AudioCue::UndeafenedByOther,
        AudioCue::TransmitStart,
        AudioCue::TransmitEnd,
    ];

    #[test]
    fn every_cue_is_in_range_and_free_of_clicks() {
        for cue in ALL_CUES {
            let pcm = render_cue(cue);
            assert!(!pcm.is_empty(), "{cue:?} rendered nothing");
            assert!(
                pcm.iter().all(|s| s.is_finite() && s.abs() <= 1.0),
                "{cue:?} out of range"
            );
            assert!(pcm[0].abs() < 0.02, "{cue:?} starts with a click");
            assert!(pcm[pcm.len() - 1].abs() < 0.02, "{cue:?} ends with a click");
        }
    }

    #[test]
    fn transmit_cues_are_short_enough_to_stay_out_of_the_way() {
        // These fire on every press. Anything long becomes intrusive within a
        // minute of riding, and delays the start of speech.
        let start = render_cue(AudioCue::TransmitStart);
        let end = render_cue(AudioCue::TransmitEnd);
        let ms = |n: usize| n as f32 * 1000.0 / SAMPLE_RATE as f32;

        assert!(
            ms(start.len()) <= 60.0,
            "press cue is {}ms",
            ms(start.len())
        );
        assert!(ms(end.len()) <= 200.0, "release cue is {}ms", ms(end.len()));
    }

    #[test]
    fn transmit_cues_sit_below_the_status_cues() {
        let peak = |c| render_cue(c).iter().fold(0.0f32, |m, s| m.max(s.abs()));
        assert!(
            peak(AudioCue::TransmitStart) < peak(AudioCue::Disconnected),
            "the press cue should be quieter than a status change"
        );
        assert!(peak(AudioCue::TransmitEnd) < peak(AudioCue::Disconnected));
    }

    #[test]
    fn the_release_cue_ends_in_a_squelch_tail() {
        // A pure tone alone sounds like a doorbell; the noise burst is what
        // makes it read as a radio.
        let pcm = render_cue(AudioCue::TransmitEnd);
        let tail = &pcm[pcm.len() * 3 / 4..];

        // Noise crosses zero far more often than a ~1 kHz tone over the same
        // span, which distinguishes the two without a spectrum analysis.
        let crossings = tail
            .windows(2)
            .filter(|w| (w[0] < 0.0) != (w[1] < 0.0))
            .count();
        let tone_crossings = 2.0 * 1046.5 * tail.len() as f32 / SAMPLE_RATE as f32;
        assert!(
            crossings as f32 > tone_crossings * 1.5,
            "tail looks tonal, not like noise ({crossings} crossings)"
        );
    }

    #[test]
    fn keying_the_microphone_plays_a_cue_only_on_a_change() {
        let s = AudioShared::new();

        s.set_transmitting(true);
        assert!(s.test_tone_active(), "no cue on key down");
        let keyed = s.cue_queue.lock().len();
        assert_eq!(keyed, render_cue(AudioCue::TransmitStart).len());

        // A remote that repeats while held must not retrigger it.
        s.set_transmitting(true);
        assert_eq!(
            s.cue_queue.lock().len(),
            keyed,
            "repeat retriggered the cue"
        );

        s.set_transmitting(false);
        assert_eq!(
            s.cue_queue.lock().len(),
            render_cue(AudioCue::TransmitEnd).len(),
            "release should play the roger beep"
        );
    }

    #[test]
    fn a_new_cue_replaces_any_pending_one() {
        // A flapping connection must not queue a backlog of tones that keeps
        // playing after it settles.
        let s = AudioShared::new();
        s.play_cue(AudioCue::Disconnected);
        let first = s.cue_queue.lock().len();
        s.play_cue(AudioCue::Reconnected);
        let second = s.cue_queue.lock().len();

        assert!(first > 0 && second > 0);
        assert_eq!(
            second,
            render_cue(AudioCue::Reconnected).len(),
            "the queue should hold only the newest cue"
        );
    }

    #[test]
    fn device_selection_bumps_the_generation_the_device_thread_watches() {
        let s = AudioShared::new();
        let before = s.device_generation();

        s.set_devices(Some("Headset".into()), None);
        assert_eq!(s.devices(), (Some("Headset".to_string()), None));
        assert!(
            s.device_generation() > before,
            "the device thread only rebuilds when the generation changes"
        );

        let mid = s.device_generation();
        s.set_devices(None, Some("Speakers".into()));
        assert_eq!(s.devices(), (None, Some("Speakers".to_string())));
        assert!(s.device_generation() > mid);
    }

    #[test]
    fn stream_keys_are_unique_per_slot() {
        assert_ne!(stream_key(0, 1), stream_key(1, 1));
        assert_ne!(stream_key(0, 1), stream_key(0, 2));
        assert_eq!(stream_key(0, 1), stream_key(0, 1));
        // A large session id must not bleed into the slot bits.
        assert_ne!(stream_key(0, u32::MAX), stream_key(1, 0));
    }
}
