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

use super::codec::{Quality, VoiceEncoder, FRAME_SAMPLES};
use super::denoise::{CaptureProcessor, NoiseProfile, FRAME_SIZE, SAMPLE_RATE};
use super::dsp::interleaved_to_mono;
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

/// Shared state between the callbacks, the worker and the API layer.
pub struct AudioShared {
    /// Raw mono 48 kHz samples captured from the microphone.
    capture_queue: Mutex<VecDeque<f32>>,
    /// Mixed mono 48 kHz samples awaiting playback.
    playback_queue: Mutex<VecDeque<f32>>,
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
    /// Pre-rendered notification tones waiting to reach the output device.
    ///
    /// Rendering up front rather than synthesising in the worker keeps the cue
    /// intact even if the worker is busy, and makes a cue a single atomic
    /// action that cannot be half-played.
    cue_queue: Mutex<VecDeque<f32>>,

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
    /// Steady tone for checking the chosen output device.
    Test,
}

impl AudioCue {
    /// Segments as `(frequency_hz, milliseconds)`; frequency 0 is a gap.
    fn segments(self) -> &'static [(f32, u32)] {
        match self {
            // Falling, so it reads as "something went wrong" without thinking.
            AudioCue::Disconnected => &[(880.0, 140), (0.0, 40), (440.0, 240)],
            // Rising, the mirror image.
            AudioCue::Reconnected => &[(523.25, 120), (0.0, 30), (783.99, 220)],
            AudioCue::Test => &[(440.0, 600)],
        }
    }
}

/// Renders a cue to PCM at [`SAMPLE_RATE`].
///
/// Each segment is faded in and out over a few milliseconds; without that the
/// abrupt start and stop produce an audible click that is worse than the tone.
pub fn render_cue(cue: AudioCue) -> Vec<f32> {
    render_segments(cue.segments(), 0.22)
}

fn render_segments(segments: &[(f32, u32)], amplitude: f32) -> Vec<f32> {
    let mut out = Vec::new();
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
            let s = if freq <= 0.0 {
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
            speakers: Mutex::new(HashMap::new()),
            transmitting: AtomicBool::new(false),
            muted: AtomicBool::new(false),
            deafened: AtomicBool::new(false),
            input_level: AtomicU32::new(0),
            output_level: AtomicU32::new(0),
            speech_detected: AtomicBool::new(false),
            running: AtomicBool::new(true),
            input_gain_db: AtomicI32::new(0),
            output_volume_db: AtomicI32::new(0),
            monitor: AtomicBool::new(false),
            cue_queue: Mutex::new(VecDeque::new()),
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
            self.playback_queue.lock().clear();
        }
    }

    pub fn is_monitoring(&self) -> bool {
        self.monitor.load(Ordering::Relaxed)
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

    pub fn set_transmitting(&self, on: bool) {
        self.transmitting.store(on, Ordering::Relaxed);
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
    scratch.resize(FRAME_SAMPLES, 0.0);
    mixed.clear();
    mixed.resize(FRAME_SAMPLES, 0.0);

    let mut speakers = shared.speakers.lock();
    let mut active = 0usize;

    speakers.retain(|_, buf| {
        if buf.is_finished() {
            return false;
        }
        if !buf.ready() {
            return true;
        }
        if buf.pop(scratch).is_some() {
            for (m, s) in mixed.iter_mut().zip(scratch.iter()) {
                *m += *s;
            }
            active += 1;
        }
        true
    });
    drop(speakers);

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
                    pull.clear();
                    {
                        let mut q = play_shared.playback_queue.lock();
                        for _ in 0..want {
                            match q.pop_front() {
                                Some(s) => pull.push(s),
                                None => break,
                            }
                        }
                    }
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
                for frame in data.chunks_mut(out_channels.max(1)) {
                    let s = (pending.pop_front().unwrap_or(0.0) * vol).clamp(-1.0, 1.0);
                    peak = peak.max(s.abs());
                    for ch in frame.iter_mut() {
                        *ch = s;
                    }
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

            let analysis = processor.process(&mut block);
            shared.store_level(analysis.level_db);
            shared
                .speech_detected
                .store(analysis.speaking, Ordering::Relaxed);

            // Loopback monitoring: hear exactly what would be transmitted.
            if shared.is_monitoring() {
                let mut q = shared.playback_queue.lock();
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
                        sequence += 1;
                        was_transmitting = true;
                    }
                } else if was_transmitting {
                    // Send one terminator so the far end closes the stream
                    // immediately instead of waiting for its jitter buffer to
                    // time out.
                    if let Ok(packet) = encoder.encode(&vec![0.0; FRAME_SAMPLES]) {
                        on_frame(sequence, packet, true);
                    }
                    sequence = 0;
                    was_transmitting = false;
                }
                frame.clear();
            }
        }

        // --- notification cues ---------------------------------------------
        // Drained a block at a time so a long cue cannot overrun the playback
        // queue, and so voice keeps flowing underneath it.
        {
            let mut cues = shared.cue_queue.lock();
            if !cues.is_empty() {
                let mut q = shared.playback_queue.lock();
                let room = MAX_QUEUED_OUTPUT_SAMPLES.saturating_sub(q.len());
                let n = room.min(FRAME_SIZE).min(cues.len());
                for _ in 0..n {
                    if let Some(s) = cues.pop_front() {
                        q.push_back(s);
                    }
                }
                if n > 0 {
                    did_work = true;
                }
            }
        }

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
        shared.push_incoming(0, &packet(7, 0, frames[0].clone(), false));
        shared.push_incoming(0, &packet(7, 1, frames[1].clone(), true));

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
        s.playback_queue
            .lock()
            .extend(std::iter::repeat_n(0.4, 4800));

        s.set_monitor(false);
        assert!(!s.is_monitoring());
        assert!(s.playback_queue.lock().is_empty());
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
