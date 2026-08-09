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
//!
//! The worker is woken by whoever gave it something to do rather than waking up
//! to look. It used to poll every 2 ms, which on a phone is five hundred
//! wakeups a second whether or not anybody was talking — enough on its own to
//! keep the CPU out of its deep idle states for as long as the app was open.
//! See [`AudioShared::signal_work`].

use std::sync::atomic::{
    AtomicBool, AtomicI32, AtomicU32, AtomicU64, AtomicU8, AtomicUsize, Ordering,
};
use std::sync::Arc;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use parking_lot::{Condvar, Mutex};
use std::collections::{HashMap, VecDeque};
use std::path::Path;
use std::time::Duration;

use super::codec::{Quality, VoiceEncoder, FRAME_SAMPLES, SEQ_UNITS_PER_FRAME};
use super::dehiss::{DehissMode, Expander, SpectralSubtractor};
use super::denoise::{CaptureProcessor, NoiseProfile, FRAME_SIZE, SAMPLE_RATE};
use super::dsp::{interleaved_to_mono, Reverb};
use super::feedback::{FeedbackGuard, FeedbackMode};
use super::jitter::{
    SpeakerBuffer, DEFAULT_TARGET_FRAMES, MAX_TARGET_FRAMES, MIN_TARGET_FRAMES, SILENT_DB,
};
use super::record::{DiagnosticRecorder, Recorded};
use super::resample::Resampler;
use super::spectrum::{SpectrumAnalyser, SpectrumFrame, TAP_PRE_GATE, TAP_RAW, TAP_SENT};
use super::waveform::{WaveformFrame, WaveformTap};

/// Where every stage of the capture chain stands, as of the last block.
///
/// Written on every block whether or not anybody is reading, unlike the
/// spectrum: it is a few dozen bytes behind an uncontended lock, and paying
/// that always is cheaper than the bug where the dots are stale because the
/// analyser happened to be disarmed. Only the transforms are worth gating.
///
/// Every field here is something the chain already worked out in order to
/// decide whether to open the microphone. None of it is computed for the
/// display's benefit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChainStatus {
    /// Still in the start-up hold; nothing below should be believed yet.
    pub warming_up: bool,
    /// RNNoise thought the last block was speech.
    pub vad_says_speech: bool,
    /// The last block cleared the SNR margin over the noise floor.
    pub snr_says_speech: bool,
    /// The noise gate is passing audio.
    pub gate_open: bool,
    /// Whether voice activation *would* open, whatever mode is actually set.
    ///
    /// Reported in every mode on purpose. A rider on push-to-talk who cannot
    /// be heard wants to know whether voice activation would have opened, and
    /// a rider on voice activation who is being cut off wants to see the moment
    /// it decides against them. This is the instantaneous decision, without the
    /// hold and fade envelope, which only runs in voice-activated mode.
    pub would_pass_voice_activated: bool,
    /// Audio actually went to the encoder on the last block.
    pub transmitting: bool,
    /// The microphone is muted, which overrules everything above.
    pub muted: bool,
    /// Echo removed on the last block, in dB.
    pub erle_db: f32,
    /// Gain the AGC is applying, in dB.
    pub agc_gain_db: f32,
    /// Post-suppression level, the noise floor under it, and the level a block
    /// has to reach to count as speech. All dBFS.
    pub level_db: f32,
    pub noise_floor_db: f32,
    pub activation_threshold_db: f32,
    /// Which suppression profile is in force, as its index.
    pub profile: u8,
    /// Which transmit mode is set, as its index.
    pub transmit_mode: u8,
    /// De-hiss and feedback-guard modes, as their indices.
    pub dehiss_mode: u8,
    pub feedback_mode: u8,
    /// The background classifier is holding `Helmet` in force.
    ///
    /// Published so the panel can say *why* the profile is what it is. Helmet
    /// arrived at by level and Helmet arrived at by the classifier look
    /// identical from outside, and a rider trying to work out whether the
    /// model is doing anything cannot tell them apart without this.
    pub music_hold: bool,
}

impl Default for ChainStatus {
    fn default() -> Self {
        Self {
            warming_up: true,
            vad_says_speech: false,
            snr_says_speech: false,
            gate_open: false,
            would_pass_voice_activated: false,
            transmitting: false,
            muted: false,
            erle_db: 0.0,
            agc_gain_db: 0.0,
            level_db: -120.0,
            noise_floor_db: -100.0,
            activation_threshold_db: -100.0,
            profile: 0,
            music_hold: false,
            transmit_mode: 0,
            dehiss_mode: 0,
            feedback_mode: 0,
        }
    }
}

/// Whether a diagnostic recording is running, and how it is doing.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RecordingState {
    pub active: bool,
    /// Blocks storage could not keep up with. Reported rather than hidden: a
    /// recording with gaps in it is still useful, and a recording with gaps
    /// nobody knows about is a measurement waiting to be wrong.
    pub dropped_blocks: u64,
    /// Where the files are being written, for the interface to offer to share.
    pub directory: String,
}

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

/// The shortest gap between two rebuilds forced by a stream reporting itself
/// dead. See where it is used for why there is a floor at all.
const REOPEN_INTERVAL: Duration = Duration::from_secs(1);

/// How long the capture stream may go without delivering a buffer before it is
/// treated as dead.
///
/// Also the device thread's idle wake interval, since that is what paces the
/// check. One second is many times the longest ordinary gap between callbacks
/// and short enough that the hole in a conversation is a stumble rather than a
/// dropped call.
const CAPTURE_WATCHDOG: Duration = Duration::from_secs(1);

/// How long the device thread sleeps when the devices are shut.
///
/// Every wake with nothing open is pure cost. The thread is woken by the
/// condvar whenever anything actually changes, so this is only a backstop
/// against a missed signal rather than how work is noticed.
const IDLE_WAKE: Duration = Duration::from_secs(30);

/// How long voice activation keeps sending at full level after the speech
/// detector drops.
///
/// A threshold lands mid-word. The unvoiced consonant that ends a sentence —
/// the "t" in "right", an "s", an "f" — carries a fraction of the energy of
/// the vowel before it and falls below the gate while the word is still being
/// said, so the far end hears the word truncated and waits for the rest.
///
/// **Sized so the listener gets 200 ms of audio after the detector drops, not
/// so the channel stays open for 200 ms.** Those are different numbers here,
/// and the difference is [`ONSET_LOOKAHEAD_SAMPLES`]: the envelope is applied
/// to audio 80 ms older than the decision driving it, so a 200 ms hold
/// delivers only 120 ms of speech past the last block the detector called
/// speech. The tail exists to carry a trailing consonant, and a consonant is
/// audio — so the audio is what gets measured, and the hold is 80 ms longer to
/// pay for the delay.
///
/// With [`VAD_FADE_SAMPLES`] the channel is held 280 ms and the far end hears
/// 200 ms, of which only the last 30 ms is below full level.
const VAD_HOLD_SAMPLES: usize = SAMPLE_RATE as usize * 250 / 1000;

/// And how long it then takes to reach silence.
///
/// A ramp rather than a second cliff: cutting a signal to zero in one sample
/// is a click, and a click at the end of every sentence is more noticeable
/// than the truncation this is fixing. Short, because its job is only to avoid
/// that click — a long ramp is not a gentler ending, it is speech sent at the
/// wrong level.
const VAD_FADE_SAMPLES: usize = SAMPLE_RATE as usize * 30 / 1000;

/// How far the transmit decision runs ahead of the audio it is applied to.
///
/// [`VAD_HOLD_SAMPLES`] and [`VAD_FADE_SAMPLES`] protect the *end* of a
/// phrase. Nothing protected the beginning, and the beginning is the harder
/// problem: a detector cannot decide a block is speech until it has the block,
/// so by the time the gate opens the sound that opened it has already been
/// discarded. What goes missing is precisely the quiet leading edge — a word
/// starting on "s", "f" or "h" arrives with its first consonant sheared off,
/// and "sixty" becomes "ixty".
///
/// No threshold fixes this, because the fault is not the threshold. It is that
/// the decision is causal and the sound is not. So the audio is delayed and
/// the decision is not: everything emitted is 80 ms old, and the gate opening
/// now opens on audio captured 80 ms ago.
///
/// 80 ms is enough for a leading fricative, which runs 50–100 ms. It is paid
/// as one-way latency and **only in voice-activated mode** — push-to-talk and
/// continuous have no threshold to be late for and are not delayed at all.
const ONSET_LOOKAHEAD_SAMPLES: usize = SAMPLE_RATE as usize * 80 / 1000;

/// Frames of incoming audio a loss measurement is taken over.
///
/// 100 frames is two seconds. Short enough to react while a rider is still in
/// the dead spot, long enough that the answer is a rate rather than a coin
/// flip — a single frame either arrived or it did not, and averaging those one
/// at a time measures nothing.
const LOSS_WINDOW_FRAMES: u64 = 100;

/// Protection is set in steps of this many percent.
const PROTECTION_STEP: u8 = 5;

/// Never protect less than this, however clean the link looks.
///
/// A mobile link that is losing nothing this second is not a link that will
/// keep losing nothing, and the cheapest moment to have bought protection is
/// before it was needed — the FEC copy travels in the *next* packet, so a
/// setting made after the loss starts protects nothing that was already lost.
const MIN_PROTECTION_PCT: u8 = 5;

/// And never more than this.
///
/// Past here the FEC copy is taking so many bits from the audio that the
/// frames which do arrive are worse than the ones being recovered. A link
/// losing 40% is not going to be rescued by spending more of it on redundancy.
const MAX_PROTECTION_PCT: u8 = 40;

/// Holds the audio back so the transmit decision can be made with hindsight.
///
/// See [`ONSET_LOOKAHEAD_SAMPLES`]. Split out from the worker so the claim it
/// makes is testable: the worker itself needs a sound card, and "the first
/// consonant survives" is not something to find out on a motorway.
#[derive(Default)]
pub struct OnsetDelay {
    ring: VecDeque<f32>,
}

impl OnsetDelay {
    pub fn new() -> Self {
        Self {
            ring: VecDeque::with_capacity(ONSET_LOOKAHEAD_SAMPLES + 2048),
        }
    }

    /// Replaces `block` with audio [`ONSET_LOOKAHEAD_SAMPLES`] older.
    ///
    /// Returns false while there is not yet anything old enough, in which case
    /// `block` is left alone and the caller must send nothing — passing the
    /// current block through instead would defeat the delay on exactly the
    /// first word after the mode was chosen, which is the one a rider notices.
    pub fn shift(&mut self, block: &mut [f32]) -> bool {
        self.ring.extend(block.iter().copied());
        if self.ring.len() < ONSET_LOOKAHEAD_SAMPLES + block.len() {
            return false;
        }
        for s in block.iter_mut() {
            *s = self.ring.pop_front().unwrap_or(0.0);
        }
        true
    }

    pub fn clear(&mut self) {
        self.ring.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.ring.is_empty()
    }
}

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

    /// Set when the worker has something to do, cleared when it takes it up.
    ///
    /// A flag rather than a bare condvar because the signal can arrive while
    /// the worker is still busy with the previous block. A notification sent
    /// then is simply lost, and the work it was announcing would wait for the
    /// timeout instead of being picked up on the next pass.
    work_pending: Mutex<bool>,
    work_ready: Condvar,

    /// Woken when [`Self::set_devices`] changes the requested devices.
    ///
    /// Guards nothing itself; [`Self::device_generation`] is still the value
    /// being watched. The mutex exists so the watcher can re-read that
    /// generation and go to sleep atomically, which is what stops a change
    /// landing in the gap between the two and being slept through.
    device_wake: Mutex<()>,
    device_changed: Condvar,

    /// Whether the capture and playback devices should be open at all.
    ///
    /// False until somebody asks. The engine used to open both the moment it
    /// started and hold them for the life of the process, which meant the
    /// microphone was live — and said so, in the status bar — from the first
    /// launch until the app was killed, whether or not there was anyone to
    /// talk to. It also pinned a Bluetooth headset to the hands-free profile,
    /// so music played through a helmet intercom dropped to telephone
    /// bandwidth for as long as this app was merely open.
    audio_wanted: AtomicBool,

    /// A stream has told us it is dead and wants the pair rebuilt.
    ///
    /// Set from cpal's error callback, cleared by the device thread when it
    /// acts. A flag as well as a generation bump so that a device reporting the
    /// same failure repeatedly asks once.
    reopen_pending: AtomicBool,

    /// Bumped by every capture callback, watched by the device thread.
    ///
    /// The only honest evidence that the microphone is still being read. A
    /// stream that has been taken away stops calling back, and on Android it
    /// does not reliably say so first — so silence here is the signal, and
    /// there is no other.
    capture_ticks: AtomicU64,

    /// Speakers whose audio is actually playing, as against still buffering.
    ///
    /// Kept apart from [`Self::active_speakers`], which counts anyone we owe
    /// audio to and is what decides whether the mixer needs waking. This one
    /// answers a narrower question — is there a stream mid-flow that ought to
    /// be producing sound right now — which is the only one a dropout counter
    /// may be gated on.
    playing_speakers: AtomicU32,

    /// The backlog every speaker buffer returns to, in 20 ms frames.
    ///
    /// Lives here rather than in each buffer because buffers come and go with
    /// the people talking, and a setting that only reached whoever happened to
    /// be speaking when it was changed would be no setting at all.
    jitter_target_frames: AtomicUsize,

    /// How the most recent open attempt went, for whoever asked for it.
    ///
    /// Opening a device is the one part of this that routinely fails for
    /// reasons the user can do something about — no microphone, permission
    /// refused, a headset claimed by another app — so the answer has to get
    /// back to the caller rather than into a log.
    open_outcome: Mutex<Option<std::result::Result<(), String>>>,
    open_settled: Condvar,

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
    /// Loss the receive side is currently seeing, in percent, smoothed.
    ///
    /// Fed to the *encoder*, which is a substitution that has to be stated
    /// rather than buried: this measures what is arriving here and spends bits
    /// protecting what is leaving. They are different directions of a
    /// different path, and nothing on the wire reports the one that matters —
    /// Mumble carries no receiver report, so a sender is never told what the
    /// far end failed to get.
    ///
    /// It is a good proxy for the reason it exists. The links that motivate
    /// adaptive protection are a rider in a dead spot, a cell handover, a
    /// congested tower — conditions of the *path*, which both directions
    /// share. It is a poor proxy for one-sided congestion, and there is no
    /// signal available that would be better.
    inbound_loss_pct: AtomicU32,
    /// Counter values at the last loss measurement, so a rate can be taken
    /// from totals that only ever climb.
    loss_mark: Mutex<(u64, u64)>,
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
    /// Which feedback guard is in use, as a [`FeedbackMode`] discriminant.
    feedback_mode: AtomicU8,
    /// 0 off, 1 expander, 2 spectral subtraction. Live-settable like the rest.
    dehiss_mode: AtomicU8,
    /// How the microphone opens, and how hard the suppressor works.
    ///
    /// Held here rather than read from the startup config, which is where they
    /// used to live: the capture loop copied the config once and nothing could
    /// reach it afterwards, so changing either in settings did nothing at all
    /// until the app was restarted.
    transmit_mode: AtomicU8,
    noise_profile: AtomicU8,

    /// Capture blocks the worker has processed, ever.
    ///
    /// Counted rather than timed, for the same reason the capture watchdog
    /// counts: a clock can jump — a phone sleeping, a time zone changing, NTP
    /// stepping — and a block count cannot.
    blocks_processed: AtomicU64,
    /// The block index up to which somebody wants a spectrum.
    ///
    /// The diagnostics analyser is the most expensive thing in the capture
    /// chain and it is worth nothing when nobody is looking, so it is armed by
    /// being *asked* rather than by being switched on.
    ///
    /// There is deliberately no matching "off". Every explicit off has a path
    /// that misses it — the diagnostics panel is never disposed, only slid off
    /// screen; the app can be backgrounded; the engine can be restarted
    /// underneath the interface — and a missed off leaves three transforms per
    /// block running in a rider's pocket for the rest of the session. Asking
    /// extends the arming by half a second; stop asking, and it lapses.
    spectrum_until: AtomicU64,
    /// The most recent analysis, if any has been produced.
    spectrum: Mutex<Option<SpectrumFrame>>,
    /// Armed the same way and for the same reasons as [`Self::spectrum_until`],
    /// for the raw window the background classifier eats.
    ///
    /// Separate from the spectrum's arming rather than sharing it: the two are
    /// wanted at different times. The panel wants bands while a rider is
    /// looking at it; the classifier wants samples while `Auto` is chosen,
    /// which is usually with the panel shut and the phone in a pocket. Sharing
    /// one flag would mean either the analyser ran for the classifier's sake or
    /// the classifier stopped when the panel closed.
    waveform_until: AtomicU64,
    /// The most recent window of raw microphone audio at 16 kHz.
    ///
    /// Boxed because it is 62 kB and this is swapped, not copied through, on
    /// the audio thread.
    waveform: Mutex<Option<Box<WaveformFrame>>>,
    /// The background classifier's last word, as a tri-state: 0 nothing has
    /// said anything, 1 clear, 2 loud and structured.
    ///
    /// Three states rather than a bool because "nobody is classifying" and
    /// "the classifier says it is quiet" must not be the same value. They lead
    /// to opposite behaviour, and a bool would have made the desktop build,
    /// where nothing ever classifies, permanently assert an all-clear.
    background_noisy: AtomicU8,
    /// Where every stage of the capture chain stands, as of the last block.
    chain: Mutex<ChainStatus>,

    /// Whether a diagnostic recording is running.
    ///
    /// Duplicated from `recorder.is_some()` on purpose. The worker asks this
    /// question on every block and the answer is no almost always, so the
    /// common path is one relaxed load; the lock is only taken by a block that
    /// is actually going to be written. The two are kept in step by writing
    /// this flag inside the same critical section that installs or removes the
    /// recorder, so a stale `true` costs at most one wasted lock and a stale
    /// `false` at most one missing block at the very start.
    recording: AtomicBool,
    /// The recording in progress, if any.
    ///
    /// Off by default and never started by anything but an explicit request:
    /// this writes the rider's microphone to storage, and the only acceptable
    /// default for that is off.
    recorder: Mutex<Option<DiagnosticRecorder>>,
    /// Pre-rendered notification tones waiting to reach the output device.
    ///
    /// Rendering up front rather than synthesising in the worker keeps the cue
    /// intact even if the worker is busy, and makes a cue a single atomic
    /// action that cannot be half-played.
    cue_queue: Mutex<VecDeque<f32>>,
    /// A recording being played back in the diagnostics panel.
    ///
    /// Its own queue rather than the cue one: a cue clears whatever is there
    /// so a flapping connection cannot stack up tones, and doing that to a
    /// preview would cut it off every time something beeped.
    preview_queue: Mutex<VecDeque<f32>>,

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
    /// Someone joined the channel.
    ParticipantJoined,
    /// Someone left it.
    ParticipantLeft,
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

            // Quieter and shorter than the connection cues, and deliberately
            // so: on a busy channel these fire often, and anything with the
            // weight of "you have been disconnected" would wear out fast.
            // Rising for an arrival, falling for a departure, which is the
            // same grammar the connection cues use.
            AudioCue::ParticipantJoined => &[(587.33, 55), (0.0, 20), (880.0, 75)],
            AudioCue::ParticipantLeft => &[(880.0, 55), (0.0, 20), (587.33, 75)],

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
            // Someone coming or going is worth noticing, not worth
            // interrupting a sentence for, and on a busy channel it happens
            // often.
            AudioCue::ParticipantJoined | AudioCue::ParticipantLeft => 0.14,
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
            work_pending: Mutex::new(false),
            work_ready: Condvar::new(),
            device_wake: Mutex::new(()),
            device_changed: Condvar::new(),
            audio_wanted: AtomicBool::new(false),
            reopen_pending: AtomicBool::new(false),
            capture_ticks: AtomicU64::new(0),
            playing_speakers: AtomicU32::new(0),
            jitter_target_frames: AtomicUsize::new(DEFAULT_TARGET_FRAMES),
            open_outcome: Mutex::new(None),
            open_settled: Condvar::new(),
            input_gain_db: AtomicI32::new(0),
            output_volume_db: AtomicI32::new(0),
            monitor: AtomicBool::new(false),
            echo_cancellation: AtomicBool::new(true),
            feedback_mode: AtomicU8::new(0),
            dehiss_mode: AtomicU8::new(0),
            transmit_mode: AtomicU8::new(0),
            noise_profile: AtomicU8::new(0),
            blocks_processed: AtomicU64::new(0),
            spectrum_until: AtomicU64::new(0),
            spectrum: Mutex::new(None),
            waveform_until: AtomicU64::new(0),
            waveform: Mutex::new(None),
            background_noisy: AtomicU8::new(0),
            chain: Mutex::new(ChainStatus::default()),
            recording: AtomicBool::new(false),
            recorder: Mutex::new(None),
            normalise_levels: AtomicBool::new(true),
            reverb_enabled: AtomicBool::new(true),
            reverb: Mutex::new(Reverb::new(REVERB_DECAY_SECS, REVERB_WET)),
            concealed_frames: AtomicU64::new(0),
            decoded_frames: AtomicU64::new(0),
            inbound_loss_pct: AtomicU32::new(0),
            loss_mark: Mutex::new((0, 0)),
            underrun_samples: AtomicU64::new(0),
            capture_dropped_samples: AtomicU64::new(0),
            active_speakers: AtomicU32::new(0),
            cue_queue: Mutex::new(VecDeque::new()),
            preview_queue: Mutex::new(VecDeque::new()),
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

    /// `(gaps concealed, deepest jitter buffer in frames)` across speakers.
    ///
    /// The buffer depth is worth surfacing: it grows itself when the link is
    /// jittery, so a number well above the default says the network is
    /// struggling even while the audio still sounds fine.
    pub fn loss_summary(&self) -> (u64, usize) {
        let speakers = self.speakers.lock();
        let losses = speakers.values().map(|b| b.loss_events()).sum();
        let depth = speakers
            .values()
            .map(|b| b.target_frames())
            .max()
            .unwrap_or(0);
        (losses, depth)
    }

    /// Updates the inbound loss estimate from the cumulative frame counters.
    ///
    /// Deliberately not every mixer round. A round hands out one frame, so the
    /// instantaneous rate is 0% or 100% and an average of those is a coin-flip
    /// estimate that would have the encoder reconfiguring itself constantly.
    /// Nothing is measured until a window's worth of frames has gone by.
    fn note_inbound_loss(&self, invented: u64, decoded: u64) {
        let mut mark = self.loss_mark.lock();
        let dropped = invented.saturating_sub(mark.0);
        let played = decoded.saturating_sub(mark.1);
        let total = dropped + played;
        if total < LOSS_WINDOW_FRAMES {
            return;
        }
        *mark = (invented, decoded);
        drop(mark);

        let observed = dropped as f32 / total as f32 * 100.0;
        let previous = self.inbound_loss_pct.load(Ordering::Relaxed) as f32;
        // Rises quickly and falls slowly. Under-protecting the moment a link
        // goes bad costs words; over-protecting for a few seconds after it
        // recovers costs a little quality nobody notices.
        let glide = if observed > previous { 0.5 } else { 0.15 };
        let next = previous + (observed - previous) * glide;
        self.inbound_loss_pct
            .store(next.round().clamp(0.0, 100.0) as u32, Ordering::Relaxed);
    }

    /// Loss the receive side is currently seeing, in percent.
    pub fn inbound_loss_percent(&self) -> u8 {
        self.inbound_loss_pct.load(Ordering::Relaxed).min(100) as u8
    }

    /// How much loss the encoder should be spending bits to protect against.
    ///
    /// Quantised, and with a floor. Quantised because a wobbling estimate
    /// would otherwise reconfigure the encoder every block for a difference of
    /// one percent. Floored because a mobile link that is losing nothing this
    /// second is not a link that will keep losing nothing, and the cheapest
    /// moment to have bought protection is before it was needed.
    pub fn protection_percent(&self) -> u8 {
        let loss = self.inbound_loss_percent();
        let stepped = (loss.saturating_add(PROTECTION_STEP / 2) / PROTECTION_STEP)
            .saturating_mul(PROTECTION_STEP);
        stepped.clamp(MIN_PROTECTION_PCT, MAX_PROTECTION_PCT)
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

    /// How much audio to hold before playing it, in milliseconds.
    ///
    /// Stored in frames, because that is the unit the buffers work in and
    /// rounding once here beats rounding on every mixer pass.
    pub fn set_jitter_buffer_ms(&self, ms: u32) {
        let frames = ((ms as usize + 10) / 20).clamp(MIN_TARGET_FRAMES, MAX_TARGET_FRAMES);
        self.jitter_target_frames.store(frames, Ordering::Relaxed);
    }

    pub fn jitter_buffer_ms(&self) -> u32 {
        (self.jitter_target_frames.load(Ordering::Relaxed) * 20) as u32
    }

    pub fn normalise_levels_enabled(&self) -> bool {
        self.normalise_levels.load(Ordering::Relaxed)
    }

    pub fn set_transmit_mode(&self, mode: TransmitMode) {
        self.transmit_mode.store(
            match mode {
                TransmitMode::VoiceActivity => 0u8,
                TransmitMode::PushToTalk => 1,
                TransmitMode::Continuous => 2,
            },
            Ordering::Relaxed,
        );
    }

    pub fn transmit_mode(&self) -> TransmitMode {
        match self.transmit_mode.load(Ordering::Relaxed) {
            1 => TransmitMode::PushToTalk,
            2 => TransmitMode::Continuous,
            _ => TransmitMode::VoiceActivity,
        }
    }

    /// How long an ask keeps the analyser running, in capture blocks.
    ///
    /// 50 blocks is half a second. Long enough that a caller polling at any
    /// sane rate never sees a gap, short enough that a caller which stops —
    /// for any reason, including ones nobody thought of — is not paid for.
    pub const SPECTRUM_ARM_BLOCKS: u64 = 50;

    /// How long an ask keeps the classifier's window being collected.
    ///
    /// 500 blocks is five seconds, ten times the analyser's, because the
    /// reader polls on the model's cadence rather than the screen's. Collecting
    /// is cheap — a mean of three and a store, per sample — so the cost of
    /// being generous here is small, and the cost of being mean is a window
    /// that is never whole when the model asks for it.
    pub const WAVEFORM_ARM_BLOCKS: u64 = 500;

    /// Asks for the analyser to keep running, and returns the latest frame.
    ///
    /// The ask and the read are one call on purpose. Two calls would let a
    /// caller read without asking, and a spectrum that is being read but not
    /// armed goes stale silently — on screen, indistinguishable from silence.
    pub fn take_spectrum(&self) -> Option<SpectrumFrame> {
        let now = self.blocks_processed.load(Ordering::Relaxed);
        self.spectrum_until
            .store(now + Self::SPECTRUM_ARM_BLOCKS, Ordering::Relaxed);
        // Never blocks the caller: this runs on whatever thread the interface
        // asked from, and the worker must not be able to stall it.
        self.spectrum.try_lock().and_then(|f| *f)
    }

    /// Whether the analyser should run for the block at `index`.
    pub fn spectrum_wanted(&self, index: u64) -> bool {
        self.spectrum_until.load(Ordering::Relaxed) > index
    }

    /// Asks for the classifier's window to keep being collected, and returns
    /// the latest one.
    ///
    /// Armed for longer than the spectrum — 5 seconds against half a second —
    /// because the reader's cadence is the model's, and running a neural
    /// network more often than the thing it describes changes would be the
    /// battery cost this design exists to avoid. Long enough that a poll every
    /// few seconds keeps it alive; short enough that a screen going off stops
    /// it well inside a ride.
    pub fn take_waveform(&self) -> Option<Box<WaveformFrame>> {
        let now = self.blocks_processed.load(Ordering::Relaxed);
        self.waveform_until
            .store(now + Self::WAVEFORM_ARM_BLOCKS, Ordering::Relaxed);
        self.waveform.try_lock().and_then(|mut f| f.take())
    }

    /// Whether the classifier's window should be collected for block `index`.
    pub fn waveform_wanted(&self, index: u64) -> bool {
        self.waveform_until.load(Ordering::Relaxed) > index
    }

    /// Publishes a window. Dropped rather than waited for, like the spectrum.
    pub fn publish_waveform(&self, frame: Box<WaveformFrame>) {
        if let Some(mut slot) = self.waveform.try_lock() {
            *slot = Some(frame);
        }
    }

    /// What the background classifier last concluded, and whether anything has
    /// concluded anything at all.
    ///
    /// `None` until something sets it, which is not the same as "the background
    /// is clear": on a desktop, or with the setting off, nothing ever will, and
    /// a chain that read that absence as an all-clear would be acting on a
    /// measurement nobody made.
    pub fn background_noisy(&self) -> Option<bool> {
        match self.background_noisy.load(Ordering::Relaxed) {
            0 => None,
            1 => Some(false),
            _ => Some(true),
        }
    }

    /// Sets it, from outside the audio thread.
    pub fn set_background_noisy(&self, noisy: bool) {
        self.background_noisy
            .store(if noisy { 2 } else { 1 }, Ordering::Relaxed);
    }

    /// Forgets it, when the classifier stops running.
    ///
    /// Called when the setting is turned off or `Auto` is deselected, so the
    /// chain goes back to deciding on its own rather than on a verdict that
    /// has stopped being updated. A stale `true` would pin `Helmet` for the
    /// rest of the session.
    pub fn clear_background_noisy(&self) {
        self.background_noisy.store(0, Ordering::Relaxed);
    }

    /// Publishes a frame. Silently drops it if the reader holds the lock,
    /// which costs one frame at 33 Hz and never costs the worker a wait.
    pub fn publish_spectrum(&self, frame: SpectrumFrame) {
        if let Some(mut slot) = self.spectrum.try_lock() {
            *slot = Some(frame);
        }
    }

    /// Advances the block counter and returns the index of the block about to
    /// be processed.
    pub fn next_block_index(&self) -> u64 {
        self.blocks_processed.fetch_add(1, Ordering::Relaxed)
    }

    /// Where the capture chain stood as of the last block.
    ///
    /// Free to ask for, and always current: unlike the spectrum this is written
    /// every block whether or not anybody is looking.
    pub fn chain_status(&self) -> ChainStatus {
        self.chain.try_lock().map(|c| *c).unwrap_or_default()
    }

    /// Publishes where the chain stands. Called once per block by the worker.
    pub fn publish_chain_status(&self, status: ChainStatus) {
        if let Some(mut slot) = self.chain.try_lock() {
            *slot = status;
        }
    }

    /// Starts writing capture and decisions into `dir`.
    ///
    /// Starting one while another runs stops the first, rather than refusing:
    /// the rider's mental model is a switch, and a switch that silently does
    /// nothing because of state they cannot see is worse than a restart.
    pub fn start_diagnostic_recording(&self, dir: &Path, tag: &str) -> std::io::Result<()> {
        let recorder = DiagnosticRecorder::start(dir, tag, SAMPLE_RATE)?;
        let mut slot = self.recorder.lock();
        // Dropping the old one here flushes and joins it, inside the lock, so
        // two sessions can never be writing the same directory at once.
        *slot = Some(recorder);
        self.recording.store(true, Ordering::Relaxed);
        Ok(())
    }

    /// Stops the recording and waits for the last file to be closed.
    ///
    /// Returns how many blocks storage could not keep up with, which is the
    /// only thing about the result that cannot be read off the files.
    pub fn stop_diagnostic_recording(&self) -> u64 {
        // Cleared first: the worker checks this without the lock, and clearing
        // it first means the worst case is a block that finds no recorder,
        // rather than one that blocks behind the flush below.
        self.recording.store(false, Ordering::Relaxed);
        let mut slot = self.recorder.lock();
        match slot.take() {
            Some(rec) => {
                let dropped = rec.dropped_blocks();
                drop(rec); // flushes and joins the writer thread
                dropped
            }
            None => 0,
        }
    }

    pub fn is_diagnostic_recording(&self) -> bool {
        self.recording.load(Ordering::Relaxed)
    }

    /// Where the recording is and how it is doing, for the diagnostics panel.
    pub fn diagnostic_recording_state(&self) -> RecordingState {
        match self.recorder.try_lock() {
            Some(slot) => match slot.as_ref() {
                Some(rec) => RecordingState {
                    active: true,
                    dropped_blocks: rec.dropped_blocks(),
                    directory: rec.directory().to_string_lossy().into_owned(),
                },
                None => RecordingState::default(),
            },
            // Contended only while a session is being started or stopped.
            // Reporting the flag alone for one frame beats blocking the
            // interface thread behind a file flush.
            None => RecordingState {
                active: self.is_diagnostic_recording(),
                ..RecordingState::default()
            },
        }
    }

    /// Hands one block to the recorder, if one is running.
    fn record_block(&self, block: Recorded) {
        if !self.recording.load(Ordering::Relaxed) {
            return;
        }
        if let Some(slot) = self.recorder.try_lock() {
            if let Some(rec) = slot.as_ref() {
                rec.push(block);
            }
        }
    }

    pub fn set_noise_profile(&self, profile: NoiseProfile) {
        self.noise_profile.store(
            match profile {
                NoiseProfile::Off => 0u8,
                NoiseProfile::Light => 1,
                NoiseProfile::Standard => 2,
                NoiseProfile::Helmet => 3,
                NoiseProfile::Auto => 4,
            },
            Ordering::Relaxed,
        );
    }

    pub fn noise_profile(&self) -> NoiseProfile {
        match self.noise_profile.load(Ordering::Relaxed) {
            1 => NoiseProfile::Light,
            2 => NoiseProfile::Standard,
            3 => NoiseProfile::Helmet,
            4 => NoiseProfile::Auto,
            _ => NoiseProfile::Off,
        }
    }

    /// Which de-hissing method the capture loop should apply, if any.
    pub fn set_dehiss_mode(&self, mode: DehissMode) {
        self.dehiss_mode.store(
            match mode {
                DehissMode::Off => 0,
                DehissMode::Expander => 1,
                DehissMode::Spectral => 2,
            },
            Ordering::Relaxed,
        );
    }

    pub fn dehiss_mode(&self) -> u8 {
        self.dehiss_mode.load(Ordering::Relaxed)
    }

    pub fn set_feedback_mode(&self, mode: FeedbackMode) {
        let code = match mode {
            FeedbackMode::Off => 0u8,
            FeedbackMode::Duck => 1,
            FeedbackMode::HowlGuard => 2,
            FeedbackMode::Residual => 3,
        };
        self.feedback_mode.store(code, Ordering::Relaxed);
    }

    pub fn feedback_mode(&self) -> FeedbackMode {
        match self.feedback_mode.load(Ordering::Relaxed) {
            1 => FeedbackMode::Duck,
            2 => FeedbackMode::HowlGuard,
            3 => FeedbackMode::Residual,
            _ => FeedbackMode::Off,
        }
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

    /// Hands the output a stretch of a recording being previewed.
    ///
    /// The transport lives on the Dart side, which is the point: previewing a
    /// file means reading it, and reading it must not happen anywhere near the
    /// audio thread. What crosses is decoded samples and nothing else, so this
    /// end is a queue and a cap.
    ///
    /// Returns what it accepted. A caller that keeps pushing past the cap is
    /// running ahead of the speaker rather than feeding it, and needs to be
    /// told so rather than have its audio silently dropped — the position
    /// readout is derived from what went in against what is left.
    pub fn preview_push(&self, samples: &[f32]) -> usize {
        let mut q = self.preview_queue.lock();
        let room = MAX_QUEUED_OUTPUT_SAMPLES.saturating_sub(q.len());
        let take = room.min(samples.len());
        q.extend(samples[..take].iter().copied());
        take
    }

    /// How much is still waiting to be heard. Position is what was pushed
    /// minus this, which is the only honest way to say where the playhead is:
    /// the queue drains at the speaker's rate, not at a timer's.
    pub fn preview_queued(&self) -> usize {
        self.preview_queue.lock().len()
    }

    /// Stop, and seek — both are this, since seeking is throwing away what was
    /// queued for somewhere else and refilling from the new position.
    pub fn preview_clear(&self) {
        self.preview_queue.lock().clear();
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
        // Bumped with the wake lock held so the watcher cannot be part-way
        // through deciding to sleep: see [`Self::await_device_change`].
        let _guard = self.device_wake.lock();
        self.device_generation.fetch_add(1, Ordering::Release);
        self.device_changed.notify_all();
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
        drop(speakers);
        // Somebody started talking. Nothing else would wake the mixer for the
        // first packet of a burst: the output callback only asks for more when
        // it is already draining something.
        self.signal_work();
    }

    pub fn stop(&self) {
        self.running.store(false, Ordering::Relaxed);
        // Both threads are asleep on a condvar by design, so setting the flag
        // alone leaves them there until their timeout expires. Waking them is
        // what makes shutdown prompt rather than eventual.
        self.signal_work();
        let _guard = self.device_wake.lock();
        self.device_changed.notify_all();
    }

    fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }

    /// Tells the DSP worker there is something waiting for it.
    ///
    /// Called from the cpal callbacks, which are real-time threads: this takes
    /// an uncontended lock and sets a bool, which is the same order of cost as
    /// the queue lock they already hold on either side of it.
    fn signal_work(&self) {
        *self.work_pending.lock() = true;
        self.work_ready.notify_one();
    }

    /// Blocks the worker until there is work, or `timeout` elapses.
    ///
    /// The timeout is a backstop, not the mechanism. Everything that produces
    /// work signals, so under normal running this returns because it was woken;
    /// the deadline only decides how long a missed signal could ever cost, and
    /// keeps [`Self::is_running`] being re-read on an idle engine.
    fn await_work(&self, timeout: Duration) {
        let mut pending = self.work_pending.lock();
        if !*pending {
            self.work_ready.wait_for(&mut pending, timeout);
        }
        *pending = false;
    }

    pub fn audio_wanted(&self) -> bool {
        self.audio_wanted.load(Ordering::Acquire)
    }

    /// Asks for the devices to be opened or closed.
    ///
    /// Returns at once; the work happens on the device thread, which owns the
    /// streams because cpal's are not `Send` everywhere. Wait on
    /// [`Self::await_open`] for the answer when turning them on.
    ///
    /// Rides on the device generation rather than introducing a second thing
    /// to watch: from the thread's side "which devices" and "any devices at
    /// all" are the same question, asked whenever that counter moves.
    pub fn set_audio_wanted(&self, wanted: bool) {
        let previous = self.audio_wanted.swap(wanted, Ordering::AcqRel);

        // A deliberate open or close is not a recovery, whatever a stream said
        // on its way down. Left set, a straggling error from the last close
        // would put the second-long recovery pause in front of the next call's
        // microphone — a rider pressing talk and losing the start of a sentence
        // to a wait that exists to protect a driver from being hammered.
        self.reopen_pending.store(false, Ordering::Release);

        if wanted {
            // Asking again after a refusal is a retry; asking again after a
            // success is a no-op. Treating both as "no change" would mean a
            // rider whose headset was claimed by another app the first time
            // could never get the microphone back without restarting.
            let already_open = previous && matches!(*self.open_outcome.lock(), Some(Ok(())));
            if already_open {
                return;
            }
            // Cleared before the request, not after the answer: otherwise the
            // caller can be handed the verdict on the *previous* open.
            *self.open_outcome.lock() = None;
        } else if !previous {
            return;
        }

        let _guard = self.device_wake.lock();
        self.device_generation.fetch_add(1, Ordering::Release);
        self.device_changed.notify_all();
    }

    /// Asks for the streams to be rebuilt because one of them has died.
    ///
    /// cpal reports a stream that has been taken away — a headset unplugged, a
    /// route changed under us, another app claiming the device — through the
    /// error callback and then goes quiet forever. Nothing polls the stream
    /// afterwards, so without this the microphone simply stops: the meter sits
    /// at silence, the far end hears nothing, and the app has no idea anything
    /// is wrong. Android is where it bites, because moving between the floating
    /// window and the activity is enough to change the routing.
    ///
    /// Called from cpal's error thread, never from a data callback, and only
    /// when the stream is already gone — so the lock taken here cannot delay
    /// audio that is still flowing.
    pub fn request_reopen(&self) {
        // A stream that errors on the way down, after the devices were given
        // back on purpose, is not a fault to recover from.
        if !self.audio_wanted() {
            return;
        }
        if self.reopen_pending.swap(true, Ordering::AcqRel) {
            return;
        }
        let _guard = self.device_wake.lock();
        self.device_generation.fetch_add(1, Ordering::Release);
        self.device_changed.notify_all();
    }

    /// Whether this pass of the device thread is a recovery, clearing the flag.
    fn take_reopen_request(&self) -> bool {
        self.reopen_pending.swap(false, Ordering::AcqRel)
    }

    /// Waits for the pending open to succeed or fail.
    ///
    /// Timing out is reported as a failure rather than as success, because the
    /// caller is about to join a conversation and "the microphone did not open
    /// in ten seconds" is the same news to a rider as "it did not open".
    pub fn await_open(&self, timeout: Duration) -> std::result::Result<(), String> {
        let mut outcome = self.open_outcome.lock();
        if outcome.is_none() {
            self.open_settled.wait_for(&mut outcome, timeout);
        }
        match outcome.clone() {
            Some(result) => result,
            None => Err("the audio device did not open in time".into()),
        }
    }

    fn publish_open(&self, result: std::result::Result<(), String>) {
        *self.open_outcome.lock() = Some(result);
        self.open_settled.notify_all();
    }

    /// Throws away everything in flight when the devices close.
    ///
    /// Without this the queues keep whatever was captured at the moment of
    /// closing, and the next call opens with a fragment of the last one still
    /// in them — heard as a bark of stale audio at the far end. The meters are
    /// reset for the same reason: a bar frozen at the last level anybody spoke
    /// at reads as a live microphone.
    fn discard_in_flight(&self) {
        self.capture_queue.lock().clear();
        self.playback_queue.lock().clear();
        self.monitor_queue.lock().clear();
        self.echo_reference.lock().clear();
        self.speakers.lock().clear();
        // Clearing the speakers resets their frame counters to nothing, so a
        // mark taken against the old ones would make the next window's counts
        // go backwards and read as a link that had suddenly become perfect.
        *self.loss_mark.lock() = (0, 0);
        self.inbound_loss_pct.store(0, Ordering::Relaxed);
        self.active_speakers.store(0, Ordering::Relaxed);
        self.playing_speakers.store(0, Ordering::Relaxed);
        self.speech_detected.store(false, Ordering::Relaxed);
        self.store_level(SILENT_DB);
        self.store_output_level(SILENT_DB);
        self.store_threshold(SILENT_DB);
        self.store_noise_floor(SILENT_DB);
    }

    /// Blocks the device watcher until the requested devices change.
    ///
    /// `seen` is the generation the caller has already acted on. Re-reading it
    /// under the lock is what makes this race-free: a [`Self::set_devices`]
    /// arriving now is either already visible here, or is still waiting for the
    /// lock this releases on the way into the wait.
    fn await_device_change(&self, seen: u64, timeout: Duration) {
        let mut guard = self.device_wake.lock();
        if self.device_generation() != seen || !self.is_running() {
            return;
        }
        self.device_changed.wait_for(&mut guard, timeout);
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
    let base_target = shared.jitter_target_frames.load(Ordering::Relaxed);
    speakers.retain(|_, buf| {
        if buf.is_finished() {
            return false;
        }
        buf.set_normalisation(normalise);
        buf.set_base_target(base_target);
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
    // Separately: how many are actually mid-stream, as against still filling.
    //
    // This is what the dropout counter is gated on, and the distinction is the
    // whole of it. A buffer that is filling produces no audio on purpose — for
    // the length of its target backlog, at the start of every utterance — and
    // counting that as a gap made the meter grow *faster* the deeper the
    // buffer was set, which is precisely backwards from what it is read for.
    let playing = speakers.values().filter(|b| b.is_playing()).count();
    let (invented, decoded) = speakers
        .values()
        .map(|b| b.frame_counts())
        .fold((0u64, 0u64), |(a, c), (x, y)| (a + x, c + y));
    shared.concealed_frames.store(invented, Ordering::Relaxed);
    shared.decoded_frames.store(decoded, Ordering::Relaxed);
    shared.note_inbound_loss(invented, decoded);
    drop(speakers);

    shared
        .active_speakers
        .store(active.max(expecting) as u32, Ordering::Relaxed);
    shared
        .playing_speakers
        .store(playing as u32, Ordering::Relaxed);
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

    // Beside the cues and before the echo reference is taken: a preview really
    // does come out of the speaker, so the canceller has to know about it or
    // playing a recording back through a helmet would train the canceller on a
    // signal it never sees again.
    mix_in(&shared.preview_queue, want, out);

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
        // Seeded from the starting configuration, since these now live here
        // rather than being read out of the config by the capture loop.
        shared.set_transmit_mode(config.transmit_mode);
        shared.set_noise_profile(config.noise_profile);
        // Seed the request with the starting choice so `devices()` reports it.
        *shared.device_request.lock() = (config.input_device.clone(), config.output_device.clone());

        // cpal streams are not Send on every platform, so they are created and
        // owned by a dedicated thread that outlives them.
        //
        // Nothing is opened here. The thread starts with the devices shut and
        // waits to be asked, which is what makes the microphone a thing this
        // app holds during a conversation rather than for as long as it is
        // installed. See [`AudioShared::set_audio_wanted`].
        let dev_shared = shared.clone();
        let dev_config = config.clone();

        let thread = std::thread::Builder::new()
            .name("mumbleway-audio".into())
            .spawn(move || {
                let mut config = dev_config;
                let mut streams: Option<Streams> = None;
                // Deliberately not the current generation: the first pass has
                // to run so that a request arriving before the thread was
                // scheduled is not slept through.
                let mut applied = u64::MAX;
                let mut opened_at: Option<std::time::Instant> = None;
                let mut last_ticks = 0u64;

                while dev_shared.is_running() {
                    let generation = dev_shared.device_generation();
                    if generation != applied {
                        applied = generation;

                        // A rebuild asked for by a dying stream can be asked
                        // for again the instant the new one opens, if whatever
                        // killed the first is still there. Once a second is
                        // quick enough for a headset being swapped mid-ride and
                        // slow enough that a device which refuses to stay open
                        // cannot spin this thread — or, worse, hammer a driver
                        // that is already in trouble. Only recoveries wait; a
                        // rider changing the device in a dropdown is answered
                        // immediately, which is the case anyone is watching.
                        if dev_shared.take_reopen_request() {
                            if let Some(at) = opened_at {
                                if let Some(left) = REOPEN_INTERVAL.checked_sub(at.elapsed()) {
                                    std::thread::sleep(left);
                                }
                            }
                        }

                        let (input, output) = dev_shared.devices();
                        config.input_device = input;
                        config.output_device = output;

                        // Always closed first, whether this is a shutdown or a
                        // change of device: some drivers, and most Bluetooth
                        // headsets, allow only one exclusive client.
                        streams = None;

                        if dev_shared.audio_wanted() {
                            match build_streams(&config, &dev_shared) {
                                Ok(s) => {
                                    tracing::info!("audio streams open");
                                    streams = Some(s);
                                    opened_at = Some(std::time::Instant::now());
                                    dev_shared.publish_open(Ok(()));
                                }
                                Err(e) => {
                                    // Reported rather than fatal: the engine
                                    // stays up with no device, so the user can
                                    // choose another instead of restarting.
                                    tracing::error!("audio device refused to open: {e}");
                                    dev_shared.publish_open(Err(e.to_string()));
                                }
                            }
                        } else {
                            tracing::info!("audio streams closed");
                            opened_at = None;
                            dev_shared.discard_in_flight();
                        }
                    }

                    // Is the microphone still actually being read?
                    //
                    // The error callback is the polite way to be told a stream
                    // has gone, and it is not always used: on Android a
                    // capture stream taken away by a routing change can simply
                    // stop calling back, saying nothing. That leaves the app
                    // holding a stream object that will never produce another
                    // sample, a meter at silence, and a rider being heard by
                    // nobody with nothing anywhere to say why.
                    //
                    // So the callbacks themselves are the evidence. They come
                    // every few milliseconds whether or not anyone is
                    // speaking — silence is still samples — so a whole second
                    // without one means the stream is gone, not that the rider
                    // is quiet.
                    if streams.is_some() && dev_shared.audio_wanted() {
                        let ticks = dev_shared.capture_ticks.load(Ordering::Relaxed);
                        let settled = opened_at
                            .map(|at| at.elapsed() >= CAPTURE_WATCHDOG)
                            .unwrap_or(false);
                        if settled && ticks == last_ticks {
                            tracing::warn!("capture stream stopped delivering audio; rebuilding");
                            dev_shared.request_reopen();
                        }
                        last_ticks = ticks;
                    }

                    // Woken by `set_devices` and `set_audio_wanted`, not by a
                    // clock. This thread exists to notice a value that changes
                    // when the user taps a dropdown or joins a server, and it
                    // used to look ten times a second for the entire life of
                    // the app to catch it. The timeout is what paces the
                    // watchdog above; nothing else needs a tick — and with the
                    // devices shut there is nothing to watch, so it goes back
                    // to sleeping properly rather than waking every second for
                    // the life of an app that is not in a call.
                    let idle = if dev_shared.audio_wanted() {
                        CAPTURE_WATCHDOG
                    } else {
                        IDLE_WAKE
                    };
                    dev_shared.await_device_change(applied, idle);
                }
                tracing::info!("audio thread stopping, closing its streams");
                drop(streams);
            })
            .map_err(|e| CoreError::Audio(format!("spawning audio thread: {e}")))?;

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

    // What the hardware actually offered, by name and format.
    //
    // Two separate faults have now turned on exactly these numbers — a device
    // reporting zero input channels, and a phone opening a route that captures
    // nothing — and in both cases the app could only say that audio had failed,
    // not what it had been handed. A device name here also settles which of
    // several possible microphones a helmet headset actually attached to.
    tracing::info!(
        "input '{}' at {} Hz, {} ch; output '{}' at {} Hz, {} ch",
        input,
        in_rate,
        in_channels,
        output,
        out_rate,
        out_channels,
    );

    // --- input ------------------------------------------------------------
    let cap_shared = shared.clone();
    let mut in_resampler = Resampler::new(in_rate, SAMPLE_RATE);
    let mut mono_scratch: Vec<f32> = Vec::with_capacity(2048);
    let mut resampled: Vec<f32> = Vec::with_capacity(2048);

    let in_stream = input
        .build_input_stream(
            in_cfg.config(),
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                // Proof of life for the watchdog. A counter rather than a
                // clock: this runs on the audio thread every few milliseconds
                // for the whole of a call, and reading the time there is work
                // the watchdog can do for itself, off the hot path.
                cap_shared.capture_ticks.fetch_add(1, Ordering::Relaxed);

                interleaved_to_mono(data, in_channels, &mut mono_scratch);
                resampled.clear();
                in_resampler.process(&mono_scratch, &mut resampled);

                let ready = {
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
                    q.len() >= FRAME_SIZE
                };
                // Only once there is a whole block to work on. A device period
                // shorter than 10 ms would otherwise wake the worker for a
                // partial block it is going to put straight back.
                if ready {
                    cap_shared.signal_work();
                }
            },
            {
                // A dead capture stream is the failure with no symptom: the
                // callback simply stops being called, so the level sits at
                // silence and everything downstream carries on as if the rider
                // were merely not speaking. Ask for the pair to be rebuilt.
                let err_shared = shared.clone();
                move |e| {
                    tracing::warn!("input stream error: {e}");
                    err_shared.request_reopen();
                }
            },
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
                // Only a gap if somebody's audio was actually playing.
                //
                // Not "somebody is tracked": that includes a buffer still
                // filling to its target, which produces no audio deliberately
                // and does so at the start of every single utterance. Counting
                // that made this number rise with the buffer depth — so a
                // rider following it would keep increasing the setting, adding
                // delay, and watching the very reading they were chasing get
                // worse.
                if underruns > 0 && play_shared.playing_speakers.load(Ordering::Relaxed) > 0 {
                    play_shared
                        .underrun_samples
                        .fetch_add(underruns, Ordering::Relaxed);
                }
                play_shared.store_output_level(super::dsp::to_dbfs(peak));

                // Ask for more only while somebody is actually streaming.
                // An empty queue is the normal state when nobody is talking,
                // and treating that as work to be done would put the wakeups
                // straight back at one per device period, for a mixer that has
                // nothing to mix. The first packet of a burst is covered by
                // `push_incoming` instead.
                if play_shared.active_speakers.load(Ordering::Relaxed) > 0
                    && play_shared.playback_queue.lock().len() < FRAME_SAMPLES * 3
                {
                    play_shared.signal_work();
                }
            },
            {
                // Both streams are rebuilt together, so either one dying is
                // enough to ask. Losing playback is at least audible, but it is
                // the same fault and the same cure.
                let err_shared = shared.clone();
                move |e| {
                    tracing::warn!("output stream error: {e}");
                    err_shared.request_reopen();
                }
            },
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
    let mut guard = FeedbackGuard::default();
    // Both are built whatever the setting, so switching between them mid-call
    // costs nothing and neither has to learn from scratch on every change.
    let mut expander = Expander::standard();
    let mut subtractor = SpectralSubtractor::new();
    // Allocated once, like everything else this thread touches. It does no work
    // at all unless the diagnostics panel is asking for frames.
    let mut analyser = SpectrumAnalyser::new();
    let mut spectrum = SpectrumFrame::default();
    // Likewise allocated once and idle unless something is asking. 62 kB of
    // ring, filled with a mean of three per sample.
    let mut waveform = WaveformTap::new();
    let mut waveform_at = 0u64;
    let mut encoder = match VoiceEncoder::new(config.quality) {
        Ok(e) => e,
        Err(e) => {
            tracing::error!("cannot create Opus encoder: {e}");
            return;
        }
    };

    // What the encoder was last told to protect against; see the poll below.
    // Starts at the value VoiceEncoder::new sets, so the first poll only calls
    // through if the link has already said something different.
    let mut protection: u8 = 10;

    let mut block = vec![0.0f32; FRAME_SIZE];
    let mut echo_ref: Vec<f32> = Vec::with_capacity(FRAME_SIZE);
    let mut frame: Vec<f32> = Vec::with_capacity(FRAME_SAMPLES);
    let mut sequence: u64 = 0;
    let mut was_transmitting = false;
    // Voice activation's release envelope; see where they are spent.
    let mut hold_left: usize = 0;
    let mut fade_left: usize = 0;
    // Voice activation's *attack* protection; see [`ONSET_LOOKAHEAD_SAMPLES`].
    let mut onset_delay = OnsetDelay::new();

    // Recorded blocks waiting to learn whether they were transmitted. See the
    // release point below for why they cannot be written where they are taken.
    let mut pending_record: VecDeque<Recorded> = VecDeque::new();
    let mut pending_samples: usize = 0;

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

            // One index per block, taken before any of the work, so the
            // analyser's arming and its cadence are decided from the same
            // number the rest of this iteration uses.
            let block_index = shared.next_block_index();
            let analysing = shared.spectrum_wanted(block_index);
            if analysing {
                analyser.push(TAP_RAW, &block);
            }

            // The classifier's window, from the same place the analyser's raw
            // trace comes from: after input gain and before anything else. It
            // has to be the microphone rather than the suppressor's output,
            // because disagreeing with the suppressor is the entire reason a
            // model is here — see `docs/MUSIC_GATE.md`.
            if shared.waveform_wanted(block_index) {
                waveform.push(&block);
                // Published about once a second. The reader wants a window
                // every few seconds and copying 62 kB more often than that is
                // work with nobody to read it.
                if waveform.ready() && block_index >= waveform_at + 100 {
                    if let Some(frame) = waveform.frame() {
                        shared.publish_waveform(frame);
                        waveform_at = block_index;
                    }
                }
            } else if waveform.ready() {
                // Nobody is asking any more. Dropped rather than kept, so a
                // reader that comes back in an hour is not handed an hour-old
                // window as if it were now.
                waveform.reset();
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
            let want_profile = shared.noise_profile();
            if want_profile != processor.profile() {
                processor.set_profile(want_profile);
            }

            // The classifier's verdict, likewise polled rather than pushed, so
            // the audio thread never waits on the thread running a model.
            // `None` — nobody is classifying — reads as "clear", which is the
            // right default: it leaves `Auto` deciding exactly as it did
            // before any of this existed.
            processor.set_background_noisy(shared.background_noisy().unwrap_or(false));

            let want_aec = shared.echo_cancellation_enabled();
            if want_aec != processor.echo_cancellation_enabled() {
                processor.set_echo_cancellation(want_aec);
            }

            // Taken *before* the chain touches it, and this ordering is the
            // whole point of the feature. Recording the output would record
            // what the suppression already decided, and the question being
            // asked is whether those decisions are right — which cannot be
            // answered from audio the decisions have already been applied to.
            // Everything downstream can be reproduced from this block and the
            // log line beside it; nothing can reproduce the block itself.
            //
            // The copy is only made while a rider has explicitly turned
            // recording on, and this is the worker rather than the device
            // callback, so an allocation here cannot miss a hardware deadline.
            let raw = if shared.is_diagnostic_recording() {
                Some(block.clone())
            } else {
                None
            };

            let analysis = processor.process_with_reference(&mut block, &echo_ref);
            if analysing {
                analyser.push(TAP_PRE_GATE, processor.pre_gate());
            }

            if raw.is_none() && !pending_record.is_empty() {
                // Recording stopped. The blocks still waiting have no answer
                // coming and never will, so they go rather than being written
                // with a guess.
                pending_record.clear();
                pending_samples = 0;
            }
            if let Some(samples) = raw {
                pending_samples += samples.len();
                pending_record.push_back(Recorded {
                    // Filled in once this block's audio has reached the
                    // transmit decision, which is not this iteration.
                    transmitting: false,
                    samples,
                    speaking: analysis.speaking,
                    gate_open: analysis.gate_open,
                    vad: analysis.vad,
                    snr_db: analysis.snr_db,
                    level_db: analysis.level_db,
                    floor_db: analysis.noise_floor_db,
                    harmonicity: analysis.harmonicity,
                    modulation: analysis.modulation,
                });
            }

            // After the canceller, on whatever survived it, and with the same
            // reference: the guard's whole job is the part that could not be
            // modelled and subtracted.
            guard.set_mode(shared.feedback_mode());
            guard.process(&mut block, &echo_ref);

            // De-hissing last of the reductions, and before the level is
            // published: everything above it either removes a correlated signal
            // or ducks, and what is left over is the stationary floor this is
            // for. Placed ahead of the meters so the rider sees the level that
            // will actually be transmitted.
            match DehissMode::from_index(shared.dehiss_mode()) {
                DehissMode::Off => {}
                DehissMode::Expander => {
                    expander.process(&mut block, analysis.level_db, analysis.noise_floor_db);
                }
                DehissMode::Spectral => {
                    // Learning is gated on the detector rather than on a timer:
                    // a spectrum learned while somebody is talking has their
                    // voice subtracted out of everything that follows.
                    subtractor.process(&mut block, analysis.speaking);
                }
            }

            shared.store_level(analysis.level_db);
            shared.store_threshold(analysis.activation_threshold_db);
            shared.store_noise_floor(analysis.noise_floor_db);
            shared
                .speech_detected
                .store(analysis.speaking, Ordering::Relaxed);

            let mode = shared.transmit_mode();
            let open = match mode {
                TransmitMode::Continuous => true,
                TransmitMode::PushToTalk => shared.transmitting.load(Ordering::Relaxed),
                TransmitMode::VoiceActivity => analysis.speaking,
            };

            // Hand the envelope below audio from 80 ms ago, so a gate opening
            // on this block opens on the sound that led into it rather than on
            // whatever is left by the time the detector has made up its mind.
            //
            // Only in voice-activated mode. The other two modes have no
            // threshold to be late for, and delaying them would buy nothing
            // and cost 80 ms of every conversation.
            let mut priming = false;
            if mode == TransmitMode::VoiceActivity {
                priming = !onset_delay.shift(&mut block);
            } else if !onset_delay.is_empty() {
                onset_delay.clear();
            }

            let allowed = if priming {
                false
            } else if shared.is_muted() {
                // Mute is immediate and absolute. Fading out of it would send
                // the tail of whatever the rider muted themselves to stop
                // sending, which is the one thing mute must never do.
                hold_left = 0;
                fade_left = 0;
                false
            } else if mode == TransmitMode::VoiceActivity {
                // Voice activation cuts on a threshold, and a threshold lands
                // mid-word: the quiet consonant that ends a sentence sits below
                // it, so "right" arrives as "righ" and the listener is left
                // waiting for a word that was sent. So the gate does not slam.
                // It stays fully open for a while after the level drops, then
                // closes over a ramp — long enough to carry a word's tail,
                // short enough that a rider who stops talking is not still
                // broadcasting a second later.
                if open {
                    hold_left = VAD_HOLD_SAMPLES;
                    fade_left = VAD_FADE_SAMPLES;
                }
                let mut sending = false;
                for s in block.iter_mut() {
                    let gain = if open {
                        1.0
                    } else if hold_left > 0 {
                        hold_left -= 1;
                        1.0
                    } else if fade_left > 0 {
                        fade_left -= 1;
                        // Linear to zero. The ramp is applied per sample rather
                        // than per block so it cannot be heard as a staircase.
                        fade_left as f32 / VAD_FADE_SAMPLES as f32
                    } else {
                        0.0
                    };
                    if gain > 0.0 {
                        sending = true;
                    }
                    *s *= gain;
                }
                sending
            } else {
                open
            };

            // One recorded block released per iteration, stamped with the
            // decision that was actually made about *its* audio.
            //
            // In voice-activated mode that decision is eight blocks late: the
            // look-ahead hands the envelope audio from 80 ms earlier, so what
            // was just decided applies to what was captured 80 ms ago. Writing
            // `allowed` against the block that produced it would put every
            // boundary in the log 80 ms early — the same 80 ms the look-ahead
            // exists to recover, so the error would look exactly like the fix
            // working. The queue holds each block back by precisely the delay
            // its audio was subject to.
            //
            // The condition mirrors `OnsetDelay::shift`, and is on samples
            // rather than a block count so it stays right if the capture block
            // size ever changes.
            let ready = mode != TransmitMode::VoiceActivity
                || pending_record
                    .front()
                    .is_some_and(|f| pending_samples >= ONSET_LOOKAHEAD_SAMPLES + f.samples.len());
            if ready {
                if let Some(mut entry) = pending_record.pop_front() {
                    pending_samples -= entry.samples.len();
                    entry.transmitting = allowed;
                    shared.record_block(entry);
                }
            }

            // Everything the chain worked out on the way to that decision,
            // published so the diagnostics panel can say which stage stopped
            // the rider being heard rather than only that they were not.
            //
            // Unconditional. It is a few dozen bytes behind an uncontended
            // lock, and paying it always is cheaper than the class of bug where
            // the dots are stale because the analyser happened to be disarmed.
            shared.publish_chain_status(ChainStatus {
                warming_up: analysis.warming_up,
                vad_says_speech: analysis.vad_says_speech,
                snr_says_speech: analysis.snr_says_speech,
                gate_open: analysis.gate_open,
                would_pass_voice_activated: analysis.speaking,
                transmitting: allowed,
                muted: shared.is_muted(),
                erle_db: analysis.erle_db,
                agc_gain_db: analysis.agc_gain_db,
                level_db: analysis.level_db,
                noise_floor_db: analysis.noise_floor_db,
                activation_threshold_db: analysis.activation_threshold_db,
                // What is in force, not what was asked for. With Auto selected
                // the two differ, and the one worth showing is the one the
                // audio actually went through — a rider who cannot see where
                // Auto landed cannot tell a bad choice from a bad chain.
                profile: analysis.effective_profile as u8,
                transmit_mode: mode as u8,
                dehiss_mode: shared.dehiss_mode(),
                feedback_mode: shared.feedback_mode() as u8,
                music_hold: processor.music_hold_active(),
            });

            // Loopback monitoring: hear exactly what would be transmitted.
            //
            // Below the gate rather than above it, so the release envelope is
            // in what the rider hears. The microphone test is the one place
            // anybody can judge whether the tail is long enough, and a monitor
            // that bypassed it would be answering a different question.
            if shared.is_monitoring() {
                let mut q = shared.monitor_queue.lock();
                if q.len() + block.len() <= MAX_QUEUED_OUTPUT_SAMPLES {
                    q.extend(block.iter().copied());
                }
            }

            // The last tap, and the only one that has to be taken whether or
            // not this block is going anywhere. A flat sent trace beside two
            // moving ones is the single most useful thing this display shows —
            // it is the gate closing, visible — and taking the tap only when
            // transmitting would draw the last thing that *was* sent instead,
            // frozen, which looks like the analyser has hung.
            //
            // Pre-Opus: this is what the encoder is handed, not what comes out
            // of it.
            if analysing {
                analyser.push(TAP_SENT, &block);
                if analyser.due(block_index) {
                    analyser.analyse(&mut spectrum, allowed);
                    shared.publish_spectrum(spectrum);
                }
            }

            frame.extend_from_slice(&block);
            if frame.len() >= FRAME_SAMPLES {
                // Match the encoder's protection to what the link is doing.
                //
                // It was set to 10% once, at construction, and never touched
                // again — which is the wrong number twice over. On a clean
                // link it spends bits on a recovery nobody collects, making
                // every packet slightly worse for nothing; under a bridge it
                // spends 10% of protection against 40% of loss, which is not
                // enough to matter. See `protection_percent` for what the
                // number means and `inbound_loss_pct` for the substitution it
                // rests on.
                let want = shared.protection_percent();
                if want != protection && encoder.set_packet_loss_perc(want).is_ok() {
                    protection = want;
                }
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
            // Nothing to do. Woken by whoever produces the next block rather
            // than by a timer, so an idle engine costs no wakeups at all.
            //
            // The deadline is a backstop against a signal this has not thought
            // of, and is well under one block so that even then nothing falls
            // behind. It is not what the loop runs on.
            shared.await_work(Duration::from_millis(20));
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

    #[test]
    fn nothing_is_recorded_until_a_rider_asks_for_it() {
        // The default matters more than the feature: this writes a microphone
        // to storage, and it must be impossible to reach that state by
        // omission. A fresh engine records nothing and names no directory.
        let shared = AudioShared::new();
        assert!(!shared.is_diagnostic_recording());
        assert_eq!(
            shared.diagnostic_recording_state(),
            RecordingState::default()
        );

        // And a block handed over while off must not open anything, which is
        // the failure that would otherwise only show up as a file appearing on
        // a rider's phone.
        shared.record_block(Recorded {
            samples: vec![0.1; FRAME_SIZE],
            transmitting: true,
            speaking: true,
            gate_open: true,
            vad: 0.9,
            snr_db: 10.0,
            level_db: -20.0,
            floor_db: -40.0,
            harmonicity: 0.8,
            modulation: 0.5,
        });
        assert!(!shared.is_diagnostic_recording());
    }

    #[test]
    fn a_session_writes_what_it_was_given_and_closes_when_stopped() {
        let dir = std::env::temp_dir().join(format!("mw-engine-rec-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let shared = AudioShared::new();
        shared.start_diagnostic_recording(&dir, "ride").unwrap();
        assert!(shared.is_diagnostic_recording());
        let state = shared.diagnostic_recording_state();
        assert!(state.active);
        assert!(
            !state.directory.is_empty(),
            "the rider has to be able to find the files"
        );

        for i in 0..5 {
            shared.record_block(Recorded {
                samples: vec![0.25; FRAME_SIZE],
                transmitting: i % 2 == 0,
                speaking: i % 2 == 0,
                gate_open: i % 2 == 0,
                vad: 0.7,
                snr_db: 9.0,
                level_db: -25.0,
                floor_db: -45.0,
                harmonicity: 0.6,
                modulation: 0.4,
            });
        }

        // Stopping has to flush and close, not merely stop appending: the next
        // thing that happens is a rider sharing the file, and a file still
        // held open by a writer thread shares as a truncated one.
        shared.stop_diagnostic_recording();
        assert!(!shared.is_diagnostic_recording());

        let pcm = std::fs::read(dir.join("ride-000.s16")).unwrap();
        assert_eq!(pcm.len(), 5 * FRAME_SIZE * 2);
        let log = std::fs::read_to_string(dir.join("ride-000.csv")).unwrap();
        let rows = log.lines().filter(|l| !l.starts_with('#')).count();
        assert_eq!(rows, 6, "a header and one line per block");

        // Stopping twice is what happens when the interface and a teardown
        // path both do the tidy-up, and it must not panic.
        shared.stop_diagnostic_recording();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn protection_follows_the_loss_the_link_is_actually_showing() {
        let shared = AudioShared::new();
        // A clean link still buys the floor: the FEC copy rides in the *next*
        // packet, so protection bought after the loss starts protects nothing
        // that was already lost.
        shared.note_inbound_loss(0, LOSS_WINDOW_FRAMES);
        assert_eq!(shared.protection_percent(), MIN_PROTECTION_PCT);

        // Now a bad stretch. Several windows, because the estimate glides.
        let mut invented = 0u64;
        let mut decoded = LOSS_WINDOW_FRAMES;
        for _ in 0..10 {
            invented += LOSS_WINDOW_FRAMES / 3;
            decoded += LOSS_WINDOW_FRAMES * 2 / 3;
            shared.note_inbound_loss(invented, decoded);
        }
        let bad = shared.protection_percent();
        assert!(
            bad >= 25,
            "a third of the frames concealed and protection only reached {bad}%"
        );

        // And it comes back down once the link does, or every call after a
        // bad minute pays for a link that recovered.
        for _ in 0..40 {
            decoded += LOSS_WINDOW_FRAMES;
            shared.note_inbound_loss(invented, decoded);
        }
        assert_eq!(shared.protection_percent(), MIN_PROTECTION_PCT);
    }

    #[test]
    fn protection_is_bounded_at_both_ends() {
        let shared = AudioShared::new();
        // A link losing nearly everything: past the ceiling the FEC copy takes
        // so many bits from the audio that the frames which do arrive are
        // worse than the ones being recovered.
        let mut invented = 0u64;
        for _ in 0..20 {
            invented += LOSS_WINDOW_FRAMES;
            shared.note_inbound_loss(invented, 0);
        }
        assert_eq!(shared.protection_percent(), MAX_PROTECTION_PCT);
    }

    #[test]
    fn a_single_frame_does_not_move_the_estimate() {
        // One frame either arrived or it did not, so measured alone it reads
        // as 0% or 100% loss. The mixer hands out one frame per round, so
        // without a window the encoder would be reconfigured on a coin flip
        // fifty times a second.
        let shared = AudioShared::new();
        for i in 1..LOSS_WINDOW_FRAMES {
            shared.note_inbound_loss(i, 0);
            assert_eq!(
                shared.inbound_loss_percent(),
                0,
                "the estimate moved after only {i} frames"
            );
        }
    }

    #[test]
    fn a_reconnect_does_not_read_as_a_perfect_link() {
        // discard_in_flight drops the speakers, and their counters with them.
        // A mark left over from the old ones makes the next window's deltas
        // saturate to zero, which reads as a link that suddenly stopped losing
        // anything at the exact moment it was re-established.
        let shared = AudioShared::new();
        let mut invented = 0u64;
        let mut decoded = 0u64;
        for _ in 0..10 {
            invented += LOSS_WINDOW_FRAMES / 2;
            decoded += LOSS_WINDOW_FRAMES / 2;
            shared.note_inbound_loss(invented, decoded);
        }
        assert!(shared.inbound_loss_percent() > 20);

        shared.discard_in_flight();
        assert_eq!(shared.inbound_loss_percent(), 0);
        // And the very next window is measured against zero, not against the
        // totals from before the reconnect.
        shared.note_inbound_loss(LOSS_WINDOW_FRAMES / 2, LOSS_WINDOW_FRAMES / 2);
        assert!(
            shared.inbound_loss_percent() > 0,
            "the first window after a reconnect was swallowed"
        );
    }

    #[test]
    fn the_look_ahead_hands_back_audio_from_before_the_decision() {
        // The mechanism, stated as arithmetic: sample n out is sample
        // n - ONSET_LOOKAHEAD_SAMPLES in. Everything else this does rests on
        // that being exactly true, off by not even one sample — an off-by-one
        // here would be inaudible and would quietly halve the protection.
        let mut delay = OnsetDelay::new();
        let mut produced: Vec<f32> = Vec::new();
        let total = ONSET_LOOKAHEAD_SAMPLES * 3;
        let source: Vec<f32> = (0..total).map(|i| i as f32).collect();

        for chunk in source.chunks(FRAME_SIZE) {
            let mut block = chunk.to_vec();
            if delay.shift(&mut block) {
                produced.extend_from_slice(&block);
            }
        }

        assert!(!produced.is_empty(), "nothing came out at all");
        for (i, s) in produced.iter().enumerate() {
            assert_eq!(*s, i as f32, "sample {i} was not delayed by the look-ahead");
        }
        // And what it held back is the delay itself, not a rounding of it.
        assert_eq!(
            total - produced.len(),
            ONSET_LOOKAHEAD_SAMPLES,
            "held back the wrong amount"
        );
    }

    #[test]
    fn a_quiet_onset_survives_a_detector_that_only_hears_the_vowel() {
        // The fault, reproduced. A word starting on "s" or "f" leads with
        // 60 ms of quiet turbulence before the vowel arrives, and no detector
        // can decide that turbulence is speech until it has heard it — by
        // which time, without a delay, it has been discarded and the far end
        // hears "ixty" for "sixty".
        //
        // Modelled with the smallest thing that shows it: a quiet lead-in, a
        // loud body, and a detector that fires only on the loud part.
        const LEAD: usize = FRAME_SIZE * 6; // 60 ms of leading consonant
        let mut source = vec![0.0f32; FRAME_SIZE * 4]; // silence first
        source.extend(std::iter::repeat_n(0.05f32, LEAD)); // the "s"
        source.extend(std::iter::repeat_n(0.8f32, FRAME_SIZE * 20)); // the vowel

        let mut delay = OnsetDelay::new();
        let mut sent: Vec<f32> = Vec::new();
        for chunk in source.chunks(FRAME_SIZE) {
            // The decision is made on the block as captured, before the delay.
            let loud = crate::audio::dsp::rms(chunk) > 0.5;
            let mut block = chunk.to_vec();
            if !delay.shift(&mut block) {
                continue;
            }
            if loud {
                sent.extend_from_slice(&block);
            }
        }

        // Everything sent while the detector was firing, and the leading
        // consonant has to be in there: 80 ms of look-ahead against 60 ms of
        // lead-in means all of it, plus a little of the silence before.
        let quiet_lead = sent.iter().filter(|s| (**s - 0.05).abs() < 1e-6).count();
        assert!(
            quiet_lead >= LEAD,
            "only {quiet_lead} of {LEAD} samples of the leading consonant were sent"
        );
    }

    #[test]
    fn the_tail_after_a_sentence_is_200_ms_with_a_30_ms_fade() {
        // Three constants that only mean anything together, which is the whole
        // reason for asserting them. The envelope runs on audio that the
        // look-ahead has already delayed, so what a listener hears after the
        // detector drops is the hold and fade *minus* that delay. Lengthen the
        // look-ahead alone and the tail silently shortens by as much; this
        // fails when that happens, which is the only warning there would be.
        let ms = |n: usize| n * 1000 / SAMPLE_RATE as usize;
        let delivered = VAD_HOLD_SAMPLES + VAD_FADE_SAMPLES - ONSET_LOOKAHEAD_SAMPLES;
        assert_eq!(ms(delivered), 200, "audio sent after the detector drops");
        assert_eq!(ms(VAD_FADE_SAMPLES), 30, "linear fade at the very end");
    }

    #[test]
    fn nothing_is_sent_while_the_look_ahead_is_still_filling() {
        // Passing the current block through while the ring fills would defeat
        // the delay on exactly the first word after voice activation is
        // chosen, which is the one a rider is listening for when they test it.
        let mut delay = OnsetDelay::new();
        let mut block = vec![0.5f32; FRAME_SIZE];
        let blocks_to_fill = ONSET_LOOKAHEAD_SAMPLES / FRAME_SIZE;
        for i in 0..blocks_to_fill {
            assert!(
                !delay.shift(&mut block),
                "block {i} should still be filling"
            );
        }
        assert!(delay.shift(&mut block), "should be running by now");
    }

    #[test]
    fn a_deep_backlog_reaches_the_listener_faster_than_real_time() {
        // The unit tests around SpeakerBuffer prove it hands out shorter
        // frames when it is holding too much. That is only half the claim, and
        // the other half lives here: nothing anywhere sets a playback *rate*.
        // The mixer is paced by how empty the playback queue is, and the queue
        // is drained by the output device at the speed of the world — so
        // producing fewer samples from the same packet is the entire mechanism
        // by which a backlog is caught up. If that link were ever broken the
        // buffer would still shorten its frames and the listener would still
        // be four seconds behind, with every unit test passing.
        let shared = AudioShared::new();
        // Four seconds arriving at once, as it does at the far side of a
        // tunnel. Numbered in tens of milliseconds, which is Mumble's unit.
        for (i, f) in encoded_frames(200).into_iter().enumerate() {
            shared.push_incoming(0, &packet(1, i as u64 * 2, f, false));
        }

        let (mut scratch, mut mixed) = (Vec::new(), Vec::new());
        let rounds = 100usize;
        for _ in 0..rounds {
            // The worker keeps mixing while the queue is short.
            for _ in 0..16 {
                if shared.playback_queue.lock().len() >= FRAME_SAMPLES * 3 {
                    break;
                }
                mix_speakers(&shared, &mut scratch, &mut mixed);
                if mixed.is_empty() {
                    break;
                }
            }
            // And the device takes one period, whatever is there.
            let mut q = shared.playback_queue.lock();
            for _ in 0..FRAME_SAMPLES {
                if q.pop_front().is_none() {
                    break;
                }
            }
        }

        let (invented, decoded) = shared
            .speakers
            .lock()
            .values()
            .map(|b| b.frame_counts())
            .fold((0u64, 0u64), |(a, c), (x, y)| (a + x, c + y));
        assert_eq!(invented, 0, "a complete stream should invent nothing");
        assert!(
            decoded as f32 > rounds as f32 * 1.3,
            "{rounds} periods of real time played only {decoded} frames of a backlog"
        );
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
    fn the_chain_status_is_free_to_ask_for_and_starts_out_untrusted() {
        // Unlike the spectrum, this costs nothing and is always current, so
        // reading it must never arm anything.
        let shared = AudioShared::new();
        let before = shared.chain_status();

        // Nothing has been processed, so it must say so rather than claim a
        // confident "not speaking" that a reader would draw as a red dot.
        assert!(before.warming_up);
        assert!(!before.transmitting);
        assert!(
            !shared.spectrum_wanted(0),
            "asking for the chain status armed the analyser"
        );

        let published = ChainStatus {
            warming_up: false,
            would_pass_voice_activated: true,
            transmitting: false,
            level_db: -22.0,
            ..ChainStatus::default()
        };
        shared.publish_chain_status(published);

        let got = shared.chain_status();
        // The case worth being able to see: voice activation would have opened,
        // and the rider still was not transmitted — so the mode is what stopped
        // them, not the noise.
        assert!(got.would_pass_voice_activated);
        assert!(!got.transmitting);
        assert!(!got.warming_up);
        assert_eq!(got, published);
    }

    #[test]
    fn asking_for_a_spectrum_arms_it_and_not_asking_disarms_it() {
        // The whole design of this feature. The diagnostics panel is never
        // disposed — it is only slid off screen — so there is no reliable
        // moment at which to switch the analyser off. It is armed by being
        // asked, and lapses on its own.
        let shared = AudioShared::new();

        // Nobody has asked: the worker must not do the work.
        assert!(!shared.spectrum_wanted(0));

        shared.take_spectrum();
        assert!(shared.spectrum_wanted(0), "asking did not arm it");
        assert!(
            shared.spectrum_wanted(AudioShared::SPECTRUM_ARM_BLOCKS - 1),
            "it lapsed before the half second was up"
        );
        assert!(
            !shared.spectrum_wanted(AudioShared::SPECTRUM_ARM_BLOCKS),
            "it did not lapse — a caller that stops asking would be paid for ever"
        );
    }

    #[test]
    fn the_arming_window_moves_with_the_blocks_already_processed() {
        // Arming is relative to now, not to zero. Getting this wrong would give
        // a long-running session an ask that had already expired before it was
        // made, and the panel would show nothing with no way to tell why.
        let shared = AudioShared::new();
        for _ in 0..1_000 {
            shared.next_block_index();
        }
        let now = shared.next_block_index() + 1;

        shared.take_spectrum();
        assert!(shared.spectrum_wanted(now));
        assert!(!shared.spectrum_wanted(now + AudioShared::SPECTRUM_ARM_BLOCKS));
    }

    #[test]
    fn the_classifier_window_arms_and_lapses_the_same_way() {
        // Same argument as the spectrum, and a stronger one: this feeds a
        // neural network, so a tap left collecting for a caller that stopped
        // asking is battery spent on a model nobody is running.
        let shared = AudioShared::new();
        assert!(!shared.waveform_wanted(0));

        shared.take_waveform();
        assert!(shared.waveform_wanted(0), "asking did not arm it");
        assert!(
            shared.waveform_wanted(AudioShared::WAVEFORM_ARM_BLOCKS - 1),
            "it lapsed before the five seconds were up"
        );
        assert!(
            !shared.waveform_wanted(AudioShared::WAVEFORM_ARM_BLOCKS),
            "it did not lapse"
        );
    }

    #[test]
    fn nothing_classifying_is_not_the_same_as_a_clear_background() {
        // The distinction the tri-state exists for. On desktop, or with the
        // setting off, nothing ever sets this — and a chain that read the
        // absence as an all-clear would be acting on a measurement nobody
        // made. It would also make the value unclearable, so turning the
        // classifier off would leave its last verdict in force for ever.
        let shared = AudioShared::new();
        assert_eq!(shared.background_noisy(), None);

        shared.set_background_noisy(false);
        assert_eq!(shared.background_noisy(), Some(false));

        shared.set_background_noisy(true);
        assert_eq!(shared.background_noisy(), Some(true));

        shared.clear_background_noisy();
        assert_eq!(shared.background_noisy(), None, "a stale verdict survived");
    }

    #[test]
    fn a_window_is_only_handed_out_once() {
        // Taken rather than copied: two readers polling would otherwise both
        // classify the same second of audio, and the second verdict would look
        // like confirmation of the first while being the same measurement.
        let shared = AudioShared::new();
        assert!(shared.take_waveform().is_none());

        shared.publish_waveform(Box::new(crate::audio::waveform::WaveformFrame {
            samples: [0.0; crate::audio::waveform::WINDOW],
            seq: 1,
        }));
        assert!(shared.take_waveform().is_some());
        assert!(
            shared.take_waveform().is_none(),
            "the window was handed out twice"
        );
    }

    #[test]
    fn block_indices_are_handed_out_once_each() {
        let shared = AudioShared::new();
        let a = shared.next_block_index();
        let b = shared.next_block_index();
        let c = shared.next_block_index();
        assert_eq!((a, b, c), (0, 1, 2));
    }

    #[test]
    fn a_published_frame_is_what_comes_back() {
        let shared = AudioShared::new();
        assert!(
            shared.take_spectrum().is_none(),
            "a frame appeared from nowhere"
        );

        let frame = SpectrumFrame {
            seq: 42,
            harmonicity: 0.75,
            ..SpectrumFrame::default()
        };
        shared.publish_spectrum(frame);

        let got = shared.take_spectrum().expect("nothing published");
        assert_eq!(got.seq, 42);
        assert!((got.harmonicity - 0.75).abs() < 1e-6);
    }

    #[test]
    fn mixes_a_single_speaker_into_the_playback_queue() {
        let shared = AudioShared::new();
        // Enough to clear the target backlog, counted off it rather than
        // written out: the default is a tuning decision and this test is about
        // mixing, not about its value.
        for (i, f) in encoded_frames(DEFAULT_TARGET_FRAMES + 1)
            .into_iter()
            .enumerate()
        {
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
        let frames = encoded_frames(DEFAULT_TARGET_FRAMES + 1);
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
    fn arrival_and_departure_cues_are_short_quiet_and_mirrored() {
        // These fire whenever anyone comes or goes, so on a busy channel they
        // are heard constantly. Anything with the weight of a disconnection
        // tone would wear out within a ride.
        let join = render_cue(AudioCue::ParticipantJoined);
        let leave = render_cue(AudioCue::ParticipantLeft);
        let drop = render_cue(AudioCue::Disconnected);

        for pcm in [&join, &leave] {
            let millis = pcm.len() * 1000 / SAMPLE_RATE as usize;
            assert!(millis <= 200, "cue runs for {millis} ms");
            assert!(
                super::super::dsp::peak(pcm) < super::super::dsp::peak(&drop),
                "an arrival should not be as loud as losing the connection"
            );
        }

        // Rising for an arrival and falling for a departure, the same grammar
        // the connection cues use, so neither has to be learned separately.
        let rises = |c: AudioCue| {
            let s = c.segments();
            s.first().unwrap().0 < s.last().unwrap().0
        };
        assert!(rises(AudioCue::ParticipantJoined));
        assert!(!rises(AudioCue::ParticipantLeft));
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

    /// Long enough that a wait which is genuinely running to its deadline
    /// cannot be mistaken for one that was woken, short enough that a broken
    /// signal path fails the test in a moment rather than stalling the suite.
    const NEVER: Duration = Duration::from_secs(30);

    #[test]
    fn the_worker_is_woken_by_incoming_audio_rather_than_by_a_timer() {
        let shared = Arc::new(AudioShared::new());

        // Nothing waiting: this must not return early, or the wait is not
        // actually waiting and the loop is a spin by another name.
        let start = std::time::Instant::now();
        shared.await_work(Duration::from_millis(30));
        assert!(
            start.elapsed() >= Duration::from_millis(25),
            "an idle worker should sleep to its deadline, slept {:?}",
            start.elapsed()
        );

        let waker = shared.clone();
        let handle = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(20));
            let mut enc = VoiceEncoder::new(Quality::Balanced).unwrap();
            let opus = enc.encode(&vec![0.1; FRAME_SAMPLES]).unwrap();
            waker.push_incoming(0, &packet(1, 0, opus, false));
        });

        let start = std::time::Instant::now();
        shared.await_work(NEVER);
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "the first packet of a burst has to wake the mixer itself; \
             nothing else asks for it"
        );
        handle.join().unwrap();
    }

    #[test]
    fn a_signal_arriving_while_the_worker_is_busy_is_not_lost() {
        let shared = AudioShared::new();
        // Sent while nobody is waiting, which is what happens whenever a block
        // lands mid-pass. Without the pending flag the notification would go
        // nowhere and this block would wait for the backstop instead.
        shared.signal_work();

        let start = std::time::Instant::now();
        shared.await_work(NEVER);
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "work signalled before the wait began was dropped"
        );

        // And it is consumed, not sticky: a second wait has nothing to take up.
        let start = std::time::Instant::now();
        shared.await_work(Duration::from_millis(30));
        assert!(
            start.elapsed() >= Duration::from_millis(25),
            "the pending flag was never cleared, so the worker will spin"
        );
    }

    #[test]
    fn the_device_watcher_sleeps_until_the_selection_changes() {
        let shared = Arc::new(AudioShared::new());
        let generation = shared.device_generation();

        let start = std::time::Instant::now();
        shared.await_device_change(generation, Duration::from_millis(30));
        assert!(
            start.elapsed() >= Duration::from_millis(25),
            "the watcher polled instead of waiting"
        );

        let waker = shared.clone();
        let handle = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(20));
            waker.set_devices(Some("Headset".into()), None);
        });

        let start = std::time::Instant::now();
        shared.await_device_change(generation, NEVER);
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "changing the device must wake the thread that rebuilds the streams"
        );
        handle.join().unwrap();
    }

    #[test]
    fn a_change_that_lands_before_the_wait_does_not_sleep_through_it() {
        let shared = AudioShared::new();
        let generation = shared.device_generation();
        shared.set_devices(Some("Headset".into()), None);

        // The race this closes: the watcher reads the generation, a change
        // lands, and only then does it go to sleep — on a notification that
        // has already been sent.
        let start = std::time::Instant::now();
        shared.await_device_change(generation, NEVER);
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "a device change was slept through"
        );
    }

    #[test]
    fn stopping_wakes_both_threads_instead_of_leaving_them_on_their_deadlines() {
        let shared = Arc::new(AudioShared::new());
        let generation = shared.device_generation();

        let stopper = shared.clone();
        let handle = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(20));
            stopper.stop();
        });

        let start = std::time::Instant::now();
        shared.await_work(NEVER);
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "the DSP thread would outlive the engine by its whole timeout"
        );

        // Already stopped by now, so this must refuse to wait at all rather
        // than blocking on a notification that will never come again.
        let start = std::time::Instant::now();
        shared.await_device_change(generation, NEVER);
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "the device thread never noticed the engine had stopped"
        );
        handle.join().unwrap();
    }

    #[test]
    fn the_devices_start_shut() {
        let shared = AudioShared::new();
        assert!(
            !shared.audio_wanted(),
            "the microphone must not be open before anybody asks for it"
        );
    }

    #[test]
    fn asking_for_audio_wakes_the_thread_that_owns_the_streams() {
        let shared = Arc::new(AudioShared::new());
        let generation = shared.device_generation();

        let waker = shared.clone();
        let handle = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(20));
            waker.set_audio_wanted(true);
        });

        let start = std::time::Instant::now();
        shared.await_device_change(generation, NEVER);
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "the device thread slept through a request to open the microphone"
        );
        assert!(shared.audio_wanted());
        handle.join().unwrap();
    }

    #[test]
    fn a_dead_stream_asks_for_the_devices_to_be_rebuilt() {
        let shared = AudioShared::new();

        // Nothing is open, so a stream reporting itself gone is a straggler
        // from a close the user asked for, not a fault to chase.
        let idle = shared.device_generation();
        shared.request_reopen();
        assert_eq!(
            shared.device_generation(),
            idle,
            "a shut engine tried to reopen devices nobody had asked for"
        );

        shared.set_audio_wanted(true);
        let open = shared.device_generation();

        shared.request_reopen();
        assert!(
            shared.device_generation() > open,
            "a dead stream left the microphone shut with nothing to reopen it"
        );

        // The same failure reported again and again — which is what a device
        // that has gone actually does — must not queue a rebuild per report.
        let asked = shared.device_generation();
        shared.request_reopen();
        shared.request_reopen();
        assert_eq!(shared.device_generation(), asked);

        assert!(shared.take_reopen_request());
        assert!(
            !shared.take_reopen_request(),
            "the request outlived the pass that acted on it"
        );
    }

    #[test]
    fn a_filling_buffer_is_not_counted_as_a_dropout() {
        // The reading that sent a rider the wrong way: every utterance begins
        // with the buffer filling to its target, producing no audio on
        // purpose, and that was counted as a gap — so raising the buffer, the
        // one control offered for the problem, made the number worse.
        let shared = AudioShared::new();
        let frames = encoded_frames(4);

        // Fewer than the target, so the buffer is still filling.
        for (i, f) in frames.into_iter().enumerate() {
            shared.push_incoming(0, &packet(1, i as u64, f, false));
        }
        let (mut a, mut b) = (Vec::new(), Vec::new());
        mix_speakers(&shared, &mut a, &mut b);

        assert!(
            shared.active_speakers.load(Ordering::Relaxed) > 0,
            "audio is owed, so the mixer must still be woken for it"
        );
        assert_eq!(
            shared.playing_speakers.load(Ordering::Relaxed),
            0,
            "a buffer that has not started playing was counted as playing"
        );
    }

    #[test]
    fn a_playing_buffer_is_counted() {
        // Once it is mid-stream, silence really is a hole, and the counter has
        // to see it. Enough frames to clear the target and start.
        let shared = AudioShared::new();
        for (i, f) in encoded_frames(DEFAULT_TARGET_FRAMES + 2)
            .into_iter()
            .enumerate()
        {
            shared.push_incoming(0, &packet(1, i as u64, f, false));
        }
        let (mut a, mut b) = (Vec::new(), Vec::new());
        mix_speakers(&shared, &mut a, &mut b);

        assert!(
            shared.playing_speakers.load(Ordering::Relaxed) > 0,
            "a stream that is playing was invisible to the dropout counter"
        );
    }

    #[test]
    fn a_silent_capture_stream_is_noticed_without_being_told() {
        // The Android failure this exists for: the stream is taken away and
        // never says so, it simply stops calling back. The counter is the only
        // evidence, so this is the whole of the watchdog's reasoning.
        let shared = AudioShared::new();
        shared.set_audio_wanted(true);

        // A stream that is delivering. Callbacks arrive every few
        // milliseconds, whether or not anybody is speaking — silence is still
        // samples — so movement here means alive.
        let before = shared.capture_ticks.load(Ordering::Relaxed);
        shared.capture_ticks.fetch_add(1, Ordering::Relaxed);
        assert_ne!(
            shared.capture_ticks.load(Ordering::Relaxed),
            before,
            "a live stream must look different from a dead one"
        );

        // And one that has stopped. Two reads with nothing in between is what
        // the device thread sees a second apart.
        let stalled = shared.capture_ticks.load(Ordering::Relaxed);
        assert_eq!(shared.capture_ticks.load(Ordering::Relaxed), stalled);

        // Which it answers by asking for the pair to be rebuilt.
        let generation = shared.device_generation();
        shared.request_reopen();
        assert!(
            shared.device_generation() > generation,
            "a stalled microphone was left shut with nothing to reopen it"
        );
    }

    #[test]
    fn deliberately_closing_the_devices_clears_a_pending_recovery() {
        let shared = AudioShared::new();
        shared.set_audio_wanted(true);
        shared.request_reopen();

        // Giving the devices back on purpose settles whatever the dying
        // streams said on the way down. Left standing, the next call would
        // wait out the recovery pause before its microphone opened.
        shared.set_audio_wanted(false);
        assert!(!shared.take_reopen_request());
    }

    #[test]
    fn the_caller_is_told_whether_the_device_opened() {
        let shared = Arc::new(AudioShared::new());
        shared.set_audio_wanted(true);

        let opener = shared.clone();
        let handle = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(20));
            opener.publish_open(Err("no input device available".into()));
        });

        // The reason has to reach the caller: this is the one failure here a
        // rider can act on, and it happens as they try to join a server.
        assert_eq!(
            shared.await_open(NEVER),
            Err("no input device available".to_string())
        );
        handle.join().unwrap();
    }

    #[test]
    fn a_device_that_never_answers_is_a_failure_not_a_success() {
        let shared = AudioShared::new();
        shared.set_audio_wanted(true);
        assert!(
            shared.await_open(Duration::from_millis(30)).is_err(),
            "a silent timeout must not read as an open microphone"
        );
    }

    #[test]
    fn asking_again_after_a_refusal_retries() {
        let shared = AudioShared::new();

        shared.set_audio_wanted(true);
        shared.publish_open(Err("device busy".into()));
        let after_failure = shared.device_generation();

        // The headset has been given up by whatever had it. Asking again has
        // to actually try again rather than hand back the old refusal.
        shared.set_audio_wanted(true);
        assert!(
            shared.device_generation() > after_failure,
            "a retry after a refused open was swallowed"
        );

        shared.publish_open(Ok(()));
        assert_eq!(shared.await_open(NEVER), Ok(()));

        // And once it is open, asking again does nothing: this runs on every
        // connect, and rebuilding a working stream would cut the audio.
        let settled = shared.device_generation();
        shared.set_audio_wanted(true);
        assert_eq!(
            shared.device_generation(),
            settled,
            "an already-open device was needlessly reopened"
        );
    }

    #[test]
    fn closing_the_devices_throws_away_what_was_in_flight() {
        let shared = AudioShared::new();
        shared
            .capture_queue
            .lock()
            .extend(std::iter::repeat_n(0.5, 480));
        shared
            .playback_queue
            .lock()
            .extend(std::iter::repeat_n(0.5, 480));
        shared
            .echo_reference
            .lock()
            .extend(std::iter::repeat_n(0.5, 480));
        for (i, f) in encoded_frames(3).into_iter().enumerate() {
            shared.push_incoming(0, &packet(1, i as u64, f, false));
        }
        shared.store_level(-12.0);
        shared.speech_detected.store(true, Ordering::Relaxed);

        shared.discard_in_flight();

        // Otherwise the next call opens with a fragment of the last one still
        // queued, and the far end hears a bark of stale audio.
        assert!(shared.capture_queue.lock().is_empty());
        assert!(shared.playback_queue.lock().is_empty());
        assert!(shared.echo_reference.lock().is_empty());
        assert!(shared.speakers.lock().is_empty());
        // And a bar frozen at the last level anybody spoke at reads as a live
        // microphone, which is precisely the claim this change exists to stop
        // the app making.
        assert_eq!(shared.input_level_db(), SILENT_DB);
        assert!(!shared.speech_detected());
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
