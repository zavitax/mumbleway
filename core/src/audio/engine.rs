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

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
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
    /// Whether the last processed block counted as speech.
    speech_detected: AtomicBool,
    running: AtomicBool,
}

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
            speech_detected: AtomicBool::new(false),
            running: AtomicBool::new(true),
        }
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

        // cpal streams are not Send on every platform, so they are created and
        // owned by a dedicated thread that outlives them.
        let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<()>>();
        let dev_shared = shared.clone();
        let dev_config = config.clone();

        let thread = std::thread::Builder::new()
            .name("mumbleway-audio".into())
            .spawn(move || {
                let built = build_streams(&dev_config, &dev_shared);
                match built {
                    Ok((in_stream, out_stream)) => {
                        let _ = ready_tx.send(Ok(()));
                        // Hold the streams open until asked to stop.
                        while dev_shared.is_running() {
                            std::thread::sleep(std::time::Duration::from_millis(100));
                        }
                        drop(in_stream);
                        drop(out_stream);
                    }
                    Err(e) => {
                        let _ = ready_tx.send(Err(e));
                    }
                }
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

                for frame in data.chunks_mut(out_channels.max(1)) {
                    let s = pending.pop_front().unwrap_or(0.0);
                    for ch in frame.iter_mut() {
                        *ch = s;
                    }
                }
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

            let analysis = processor.process(&mut block);
            shared.store_level(analysis.level_db);
            shared
                .speech_detected
                .store(analysis.speaking, Ordering::Relaxed);

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
    fn stream_keys_are_unique_per_slot() {
        assert_ne!(stream_key(0, 1), stream_key(1, 1));
        assert_ne!(stream_key(0, 1), stream_key(0, 2));
        assert_eq!(stream_key(0, 1), stream_key(0, 1));
        // A large session id must not bleed into the slot bits.
        assert_ne!(stream_key(0, u32::MAX), stream_key(1, 0));
    }
}
