//! WebRTC's AEC3, behind the same interface as the filter it replaces.
//!
//! # Why this is here
//!
//! [`super::aec::Nlms`] is a time-domain NLMS filter and it was measured on a
//! real room, on a real phone, and it removed **0.2 dB**. Build 123's decision
//! log said why: the echo was spread over 140 ms (440 at p90) and the filter
//! covers 21.3. `_WHY_NO_GROWTH` in that module records why it cannot simply be
//! lengthened — a single-power normalisation converges worse the further it
//! spans, which is why both production cancellers are frequency-domain.
//!
//! AEC3 is a partitioned block frequency-domain filter with per-bin
//! normalisation and its own delay estimator, and it is what every browser
//! ships. Measured on the same OPPO, 20 s a room (`tools/aec3bench`):
//!
//! ```text
//! room                         erle dB   mean us    p95 us
//! helmet, 20 ms tail              42.6       411       486
//! measured p10, 40 ms             44.6       409       483
//! measured median, 140 ms         38.4       408       481
//! measured p90, 440 ms            42.8       400       472
//! ```
//!
//! Against 970 µs and 0.0 dB for the filter it replaces. Less than half the
//! cost, forty decibels better, and it does not degrade as the tail lengthens.
//!
//! # What is being depended on, and the risk in it
//!
//! [Sonora](https://github.com/dignifiedquire/sonora) is a pure-Rust port of
//! WebRTC's AudioProcessing (M145), BSD-3-Clause, which this repository's
//! GPL-3.0-or-later can absorb. Pure Rust is the whole reason it is preferred
//! over WebRTC's own module: this project builds for iOS, macOS, Android,
//! Windows and Linux, and it has already lost an afternoon to one CMake policy
//! for Opus.
//!
//! **`sonora-aec3` was 0.2.0 and a fortnight old when this landed.** That is a
//! young dependency in a voice path, which is why [`super::aec::EchoCanceller`]
//! keeps the old filter reachable rather than deleting it: a bad build should
//! be comparable rather than arguable. If this has to be abandoned, the fallback
//! is the C++ extraction PulseAudio and PipeWire use, and the question there is
//! build times across five cross-compiled targets rather than whether it works.
//!
//! # The contract it needs, and where that came from
//!
//! Render and capture, **one 10 ms frame each, in lockstep, for ever**. The
//! reference could not supply that until `engine::record_played_silence`
//! existed: it was written only when a mixer produced something, so every pause
//! between the far end's phrases was cut out of it while the microphone ran on
//! continuously. That is a precondition of this file working at all.

use sonora::config::EchoCanceller as EchoConfig;
use sonora::{AudioProcessing, Config, StreamConfig};

use super::denoise::{FRAME_SIZE, SAMPLE_RATE};

/// How often to read the statistics, in blocks. Once a second.
///
/// They are not free — the call aggregates across the pipeline — and nothing
/// downstream needs them faster than the diagnostics panel redraws.
const STATS_EVERY: usize = 100;

/// How much echo path AEC3 covers, in milliseconds.
///
/// Reported rather than measured: the filter is partitioned across the whole
/// plausible range instead of aimed, so there is no single "span" the way there
/// is for a filter that has to be pointed somewhere. This is the default
/// configuration's reach, and it is here so the panel's *filter covers* reading
/// stays meaningful across both cancellers.
const SPAN_MS: f32 = 100.0;

pub struct Aec3 {
    apm: AudioProcessing,
    /// Scratch. The API writes its output rather than working in place, and
    /// the real-time path must not allocate.
    render_out: Vec<f32>,
    capture_out: Vec<f32>,
    enabled: bool,
    since_stats: usize,
    erle_db: f32,
    delay_ms: f32,
    /// Whether the delay estimator has produced anything at all.
    ///
    /// AEC3 reports no confidence, so this stands in for one: the panel's
    /// question is "has it found the echo", and a delay estimate existing is
    /// the honest answer to it. Reported as 0 or 1 rather than a fraction,
    /// because inventing a number between them would be inventing a
    /// measurement.
    located: bool,
}

impl Aec3 {
    pub fn new() -> Self {
        let cfg = Config {
            // The canceller and nothing else. This chain has its own
            // suppression, gate, AGC and limiter, all measured and tuned, and
            // turning AEC3's on would put two of each in series.
            echo_canceller: Some(EchoConfig::default()),
            ..Default::default()
        };
        let stream = StreamConfig::new(SAMPLE_RATE, 1);
        Self {
            apm: AudioProcessing::builder()
                .config(cfg)
                .capture_config(stream)
                .render_config(stream)
                .build(),
            render_out: vec![0.0; FRAME_SIZE],
            capture_out: vec![0.0; FRAME_SIZE],
            enabled: true,
            since_stats: 0,
            erle_db: 0.0,
            delay_ms: 0.0,
            located: false,
        }
    }

    pub fn set_enabled(&mut self, on: bool) {
        self.enabled = on;
        if !on {
            self.reset();
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Throws away everything learned about the path.
    ///
    /// A fresh instance rather than a reset method, because there is not one:
    /// AEC3 owns a filter, a delay estimator, a render buffer and several
    /// detectors, and rebuilding is the only way to be sure none of them kept
    /// an opinion about a room the rider has left.
    pub fn reset(&mut self) {
        let cfg = Config {
            echo_canceller: Some(EchoConfig::default()),
            ..Default::default()
        };
        let stream = StreamConfig::new(SAMPLE_RATE, 1);
        self.apm = AudioProcessing::builder()
            .config(cfg)
            .capture_config(stream)
            .render_config(stream)
            .build();
        self.since_stats = 0;
        self.erle_db = 0.0;
        self.delay_ms = 0.0;
        self.located = false;
    }

    pub fn erle_db(&self) -> f32 {
        self.erle_db
    }

    /// Where the echo was found, in milliseconds, and whether it was found.
    pub fn alignment(&self) -> (f32, f32) {
        (self.delay_ms, if self.located { 1.0 } else { 0.0 })
    }

    pub fn filter_span_ms(&self) -> f32 {
        SPAN_MS
    }

    /// Always zero.
    ///
    /// The old filter reports how far apart the arrivals are because it can
    /// only be in one place and the panel needs to say when a second echo is
    /// out of reach. AEC3 is partitioned across the whole range, so the
    /// question does not arise — and answering it with a made-up number would
    /// make the panel claim a measurement nothing took.
    pub fn measured_spread_ms(&self) -> f32 {
        0.0
    }

    /// Cancels echo from `mic` in place, using `reference` as what was played.
    ///
    /// Both must be exactly [`FRAME_SIZE`]. Unlike the filter this replaces,
    /// **a short or absent reference is not tolerated**: AEC3 keeps its own
    /// render buffer and expects one frame per frame, so handing it a partial
    /// one would desynchronise the buffer rather than simply cancel less.
    /// `engine::record_played_silence` is what guarantees the caller can.
    pub fn process(&mut self, mic: &mut [f32], reference: &[f32]) -> f32 {
        if !self.enabled || mic.len() != FRAME_SIZE || reference.len() != FRAME_SIZE {
            return 0.0;
        }

        if self
            .apm
            .process_render_f32(&[reference], &mut [&mut self.render_out])
            .is_err()
        {
            return self.erle_db;
        }
        if self
            .apm
            .process_capture_f32(&[mic], &mut [&mut self.capture_out])
            .is_err()
        {
            return self.erle_db;
        }
        mic.copy_from_slice(&self.capture_out);

        self.since_stats += 1;
        if self.since_stats >= STATS_EVERY {
            self.since_stats = 0;
            let s = self.apm.statistics();
            if let Some(v) = s.echo_return_loss_enhancement {
                self.erle_db = v as f32;
            }
            if let Some(v) = s.delay_ms {
                self.delay_ms = v as f32;
                self.located = true;
            }
        }
        self.erle_db
    }
}

impl Default for Aec3 {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn convolve(x: &[f32], h: &[f32]) -> Vec<f32> {
        let mut y = vec![0.0f32; x.len()];
        for (n, out) in y.iter_mut().enumerate() {
            let mut acc = 0.0f32;
            for (k, &hk) in h.iter().enumerate() {
                if n >= k {
                    acc += hk * x[n - k];
                }
            }
            *out = acc;
        }
        y
    }

    fn power(x: &[f32]) -> f64 {
        x.iter().map(|v| (*v as f64) * (*v as f64)).sum::<f64>() / x.len().max(1) as f64
    }

    /// The case the old filter cannot do, which is the whole reason this exists.
    ///
    /// A 30 ms playback delay and a 150 ms reverberation tail — a phone on a
    /// table, which is what the reported fault was. `super::super::aec`'s own
    /// tests use a 64-tap room, 1.3 ms, and that is why they all passed while a
    /// real room did not.
    #[test]
    fn it_cancels_a_room_the_old_filter_cannot_reach() {
        // Eight seconds, and the limit is the scaffolding rather than the
        // canceller: convolving against an 8 600-tap room is O(n·m) and is most
        // of this test's runtime. AEC3 converges in about two.
        let secs = 8;
        let len = SAMPLE_RATE as usize * secs;

        // Broadband, because an adaptive filter cannot identify a room from a
        // signal that only excites a few dozen frequencies — and *any* filter
        // scores perfectly on one that does. `tests/aec3_cost.rs` has the long
        // version of that mistake.
        let voiced = super::super::testsig::speech(len, 120.0, 0.30);
        let unvoiced = super::super::testsig::whisper(len, 0.30, 0x5EED);
        let far: Vec<f32> = voiced
            .iter()
            .zip(&unvoiced)
            .map(|(v, u)| v + u * 0.8)
            .collect();

        // 30 ms of playback path, then a tail decaying 60 dB over 150 ms.
        let delay = 30 * SAMPLE_RATE as usize / 1000;
        let tail = 150 * SAMPLE_RATE as usize / 1000;
        let mut h = vec![0.0f32; delay + tail];
        h[delay] = 0.6;
        let mut seed = 0x51ED_2701u32;
        for n in 0..tail {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let noise = (seed >> 8) as f32 / 8_388_608.0 - 1.0;
            h[delay + n] += 0.25 * noise * (-6.908 * n as f32 / tail as f32).exp();
        }
        let echo = convolve(&far, &h);

        let mut aec = Aec3::new();
        let measured_from = len - SAMPLE_RATE as usize * 3;
        let mut out = Vec::new();
        for i in 0..len / FRAME_SIZE {
            let r = &far[i * FRAME_SIZE..(i + 1) * FRAME_SIZE];
            let mut block = echo[i * FRAME_SIZE..(i + 1) * FRAME_SIZE].to_vec();
            aec.process(&mut block, r);
            if i * FRAME_SIZE >= measured_from {
                out.extend_from_slice(&block);
            }
        }

        let erle = 10.0 * (power(&echo[measured_from..]) / power(&out).max(1e-18)).log10();
        assert!(
            erle > 20.0,
            "a 150 ms room should be cancelled, got {erle:.1} dB \
             (the filter this replaces manages about 5)"
        );
    }

    /// Near-end speech survives when there is no echo to remove.
    ///
    /// **Asserted as a level, not sample by sample**, and the first draft of
    /// this test got that wrong: it compared each sample against the input and
    /// failed at 0.2393 on a signal of amplitude 0.2, which reads as the audio
    /// being destroyed and is not. At 48 kHz the WebRTC pipeline splits the
    /// band three ways and recombines it, so the output is a filtered version
    /// of the input rather than a copy of it, and a high-pass runs besides.
    /// None of that is audible; a sample-exact comparison cannot tell it from
    /// the thing that would be, which is the stage quietly gating a rider who
    /// is not echoing anybody.
    ///
    /// So: the level has to survive, and no stretch may fall silent.
    #[test]
    fn near_end_speech_survives_when_nothing_is_playing() {
        let mut aec = Aec3::new();
        let blocks = 60;
        let near = super::super::testsig::whisper(FRAME_SIZE * blocks, 0.2, 7);
        let silence = vec![0.0f32; FRAME_SIZE];

        let mut quietest = f64::MAX;
        let (mut in_acc, mut out_acc) = (0.0f64, 0.0f64);
        for i in 0..blocks {
            let want = &near[i * FRAME_SIZE..(i + 1) * FRAME_SIZE];
            let mut block = want.to_vec();
            aec.process(&mut block, &silence);
            // The first half second is the pipeline settling.
            if i > 50 {
                in_acc += power(want);
                out_acc += power(&block);
                quietest = quietest.min(power(&block));
            }
        }

        let kept_db = 10.0 * (out_acc / in_acc.max(1e-18)).log10();
        assert!(
            kept_db > -3.0,
            "near-end speech lost {:.1} dB with nothing playing to cancel",
            -kept_db
        );
        assert!(
            quietest > 1e-8,
            "a block went silent: the canceller is gating a rider it has no echo from"
        );
    }

    #[test]
    fn disabled_is_a_pass_through() {
        let mut aec = Aec3::new();
        aec.set_enabled(false);
        let reference = vec![0.3f32; FRAME_SIZE];
        let original = super::super::testsig::whisper(FRAME_SIZE, 0.2, 5);
        let mut block = original.clone();
        aec.process(&mut block, &reference);
        assert_eq!(block, original);
    }

    /// A frame that is not exactly one block is refused rather than mangled.
    ///
    /// AEC3 keeps its own render buffer, so a short frame does not mean "cancel
    /// less" — it means the render and capture streams stop describing the same
    /// instant, permanently.
    #[test]
    fn a_partial_frame_is_refused() {
        let mut aec = Aec3::new();
        let reference = vec![0.3f32; FRAME_SIZE / 2];
        let original = vec![0.2f32; FRAME_SIZE];
        let mut block = original.clone();
        aec.process(&mut block, &reference);
        assert_eq!(block, original, "a short reference must change nothing");
    }
}
