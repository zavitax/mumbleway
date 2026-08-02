//! Noise suppression and the complete microphone processing chain.
//!
//! RNNoise (via the pure-Rust `nnnoiseless` port) does the heavy lifting on
//! broadband noise. It is a recurrent network trained on speech-plus-noise, so
//! unlike a spectral-subtraction gate it copes with the non-stationary noise a
//! helmet produces — wind buffeting, passing traffic, changing engine load.
//!
//! RNNoise alone is not enough at motorway speed, so it sits inside a chain:
//! rumble filter first (so the network is not asked to model 60 Hz wind energy
//! that a filter removes for free), then RNNoise, then a VAD-informed gate, then
//! AGC and a limiter.

use nnnoiseless::DenoiseState;

use super::aec::{EchoCanceller, DEFAULT_TAPS};
use super::dsp::{rms, to_dbfs, Agc, Limiter, NoiseFloorTracker, NoiseGate, RumbleFilter};

/// RNNoise works on fixed 10 ms blocks at 48 kHz.
pub const FRAME_SIZE: usize = DenoiseState::FRAME_SIZE;

/// Sample rate the whole pipeline runs at.
pub const SAMPLE_RATE: u32 = 48_000;

/// How hard to fight the environment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoiseProfile {
    /// No suppression; only a gentle DC/rumble cut.
    Off,
    /// Quiet indoor use.
    Light,
    /// General purpose.
    Standard,
    /// Motorcycle helmet at speed: steep rumble cut, full suppression,
    /// assertive gate and AGC.
    Helmet,
}

impl NoiseProfile {
    /// High-pass corner. Wind energy climbs steeply as speed rises, so the
    /// helmet profile sacrifices some low speech warmth for intelligibility.
    fn cutoff_hz(self) -> f32 {
        match self {
            NoiseProfile::Off => 60.0,
            NoiseProfile::Light => 90.0,
            NoiseProfile::Standard => 120.0,
            NoiseProfile::Helmet => 180.0,
        }
    }

    /// How much of the denoised signal to blend in, 0.0..=1.0. Blending below 1.0
    /// keeps a little of the original so quiet speech does not sound gated.
    fn denoise_mix(self) -> f32 {
        match self {
            NoiseProfile::Off => 0.0,
            NoiseProfile::Light => 0.6,
            NoiseProfile::Standard => 0.9,
            NoiseProfile::Helmet => 1.0,
        }
    }

    /// Gate thresholds (open, close) in dBFS.
    fn gate_db(self) -> (f32, f32) {
        match self {
            NoiseProfile::Off => (-90.0, -95.0),
            NoiseProfile::Light => (-52.0, -60.0),
            NoiseProfile::Standard => (-46.0, -54.0),
            NoiseProfile::Helmet => (-40.0, -48.0),
        }
    }

    /// Minimum RNNoise speech probability required to treat a block as speech.
    fn vad_threshold(self) -> f32 {
        match self {
            NoiseProfile::Off => 0.0,
            NoiseProfile::Light => 0.30,
            NoiseProfile::Standard => 0.50,
            NoiseProfile::Helmet => 0.65,
        }
    }

    fn agc_max_gain_db(self) -> f32 {
        match self {
            NoiseProfile::Off => 0.0,
            NoiseProfile::Light => 12.0,
            NoiseProfile::Standard => 18.0,
            NoiseProfile::Helmet => 24.0,
        }
    }

    /// How far above the tracked noise floor a block must sit to count as speech.
    ///
    /// This is the safeguard against RNNoise's VAD firing on engine harmonics.
    /// A helmet needs the widest margin because the background is both loud and
    /// tonal, which is exactly what fools the network.
    fn snr_margin_db(self) -> f32 {
        match self {
            NoiseProfile::Off => 0.0,
            NoiseProfile::Light => 6.0,
            NoiseProfile::Standard => 8.0,
            NoiseProfile::Helmet => 10.0,
        }
    }
}

/// Result of processing one 10 ms block.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BlockAnalysis {
    /// RNNoise speech probability, 0.0..=1.0.
    pub vad: f32,
    /// Post-suppression level in dBFS, for the input meter.
    pub level_db: f32,
    /// Estimated background noise level in dBFS.
    pub noise_floor_db: f32,
    /// How far the block sits above the noise floor.
    pub snr_db: f32,
    /// Level a block must reach before voice activation opens, in dBFS.
    ///
    /// This is the noise floor plus the profile's SNR margin, so it rises with
    /// the background — which is what makes it worth showing the user, rather
    /// than a fixed number that means nothing at 120 km/h.
    pub activation_threshold_db: f32,
    /// Whether this block should be transmitted.
    pub speaking: bool,
    /// Echo removed on this block, in dB.
    pub erle_db: f32,
}

/// The full microphone chain.
pub struct CaptureProcessor {
    profile: NoiseProfile,
    rumble: RumbleFilter,
    denoise: Box<DenoiseState<'static>>,
    /// Runs first: echo is speech, so neither the gate nor RNNoise will remove
    /// it, and everything downstream works better without it.
    aec: EchoCanceller,
    gate: NoiseGate,
    agc: Agc,
    limiter: Limiter,
    floor: NoiseFloorTracker,
    /// Scratch buffers, reused so the real-time path never allocates.
    scaled: Vec<f32>,
    denoised: Vec<f32>,
    /// Consecutive speech blocks, used to avoid clipping word onsets.
    hangover: u32,
    /// Blocks remaining before the chain is trusted; see [`WARMUP_BLOCKS`].
    warmup: u32,
}

/// RNNoise has internal lookahead, so its first frames come out near-silent
/// regardless of the input. Feeding those to the noise-floor tracker pins the
/// floor tens of dB too low and makes everything afterwards look like speech, so
/// we stay muted and hold the estimator back until the network is producing real
/// output. 15 blocks is 150 ms — imperceptible at connect time.
const WARMUP_BLOCKS: u32 = 15;

impl CaptureProcessor {
    pub fn new(profile: NoiseProfile) -> Self {
        let (open_db, close_db) = profile.gate_db();
        Self {
            profile,
            rumble: RumbleFilter::new(SAMPLE_RATE as f32, profile.cutoff_hz()),
            denoise: DenoiseState::new(),
            aec: EchoCanceller::new(DEFAULT_TAPS),
            // Hold for ~15 blocks (150 ms) so short pauses inside a sentence do
            // not chop the tail off words.
            gate: NoiseGate::new(open_db, close_db, 15),
            agc: Agc::new(-18.0, profile.agc_max_gain_db()),
            limiter: Limiter::new(0.98),
            // Six 0.25 s sub-windows: a ~1.5 s memory of the background level.
            floor: NoiseFloorTracker::new(25),
            warmup: WARMUP_BLOCKS,
            scaled: vec![0.0; FRAME_SIZE],
            denoised: vec![0.0; FRAME_SIZE],
            hangover: 0,
        }
    }

    pub fn profile(&self) -> NoiseProfile {
        self.profile
    }

    /// Swaps the profile, rebuilding only what depends on it.
    pub fn set_profile(&mut self, profile: NoiseProfile) {
        if profile == self.profile {
            return;
        }
        let (open_db, close_db) = profile.gate_db();
        self.profile = profile;
        self.rumble = RumbleFilter::new(SAMPLE_RATE as f32, profile.cutoff_hz());
        self.gate = NoiseGate::new(open_db, close_db, 15);
        self.agc = Agc::new(-18.0, profile.agc_max_gain_db());
    }

    pub fn reset(&mut self) {
        self.rumble.reset();
        self.gate.reset();
        self.agc.reset();
        self.limiter.reset();
        self.floor.reset();
        self.hangover = 0;
        self.warmup = WARMUP_BLOCKS;
    }

    /// Processes exactly [`FRAME_SIZE`] samples in place.
    ///
    /// Input and output are `-1.0..=1.0`; the i16 scaling RNNoise expects is
    /// handled internally.
    pub fn process(&mut self, block: &mut [f32]) -> BlockAnalysis {
        // No reference available means no echo to model; the canceller becomes
        // a pass-through of its own accord.
        self.process_with_reference(block, &[])
    }

    /// Enables or disables echo cancellation.
    pub fn set_echo_cancellation(&mut self, on: bool) {
        self.aec.set_enabled(on);
    }

    pub fn echo_cancellation_enabled(&self) -> bool {
        self.aec.is_enabled()
    }

    /// Processes one block, using `reference` — the audio recently sent to the
    /// speakers — to cancel echo. `reference` may be empty or shorter.
    pub fn process_with_reference(
        &mut self,
        block: &mut [f32],
        reference: &[f32],
    ) -> BlockAnalysis {
        debug_assert_eq!(block.len(), FRAME_SIZE);

        // 0. Remove what our own speakers are feeding back into the microphone.
        // This has to come first: echo is speech, so the gate and RNNoise both
        // pass it happily, and the AGC would then amplify it.
        let erle_db = if reference.is_empty() {
            0.0
        } else {
            self.aec.process(block, reference)
        };

        // 1. Strip wind and engine rumble before anything else sees it.
        self.rumble.process(block);

        // 2. RNNoise. It expects samples scaled to the i16 range, not -1..1.
        let vad = if self.profile == NoiseProfile::Off {
            self.denoised.copy_from_slice(block);
            // Without the network we have no speech probability, so fall back to
            // a level-based guess and let the gate decide.
            0.5
        } else {
            for (dst, src) in self.scaled.iter_mut().zip(block.iter()) {
                *dst = *src * 32768.0;
            }
            let vad = self.denoise.process_frame(&mut self.denoised, &self.scaled);
            for s in self.denoised.iter_mut() {
                *s /= 32768.0;
            }
            vad
        };

        // 3. Blend, so lighter profiles keep some natural room tone.
        let mix = self.profile.denoise_mix();
        for (dst, wet) in block.iter_mut().zip(self.denoised.iter()) {
            *dst = *dst * (1.0 - mix) + *wet * mix;
        }

        // 4. Decide whether this is speech.
        //
        // RNNoise's VAD alone is not sufficient: engine and wind noise are
        // harmonic enough that it reports probabilities above 0.8 on a steady
        // drone. So a block only counts as speech if the network agrees *and* it
        // rises clearly above the tracked background level. Steady noise raises
        // the floor with it and therefore never clears the SNR margin, however
        // loud it gets.
        let level_db = to_dbfs(rms(block));
        let warming_up = self.warmup > 0;
        if warming_up {
            self.warmup -= 1;
        }
        // Hold the estimator back until RNNoise is producing real output.
        let noise_floor_db = if warming_up {
            self.floor.floor_db()
        } else {
            self.floor.update(level_db)
        };
        let snr_db = level_db - noise_floor_db;

        let vad_says_speech = vad >= self.profile.vad_threshold();
        let snr_says_speech = snr_db >= self.profile.snr_margin_db();
        let speech_now = if warming_up {
            false
        } else if self.profile == NoiseProfile::Off {
            true
        } else {
            vad_says_speech && snr_says_speech
        };

        if speech_now {
            self.hangover = 20; // ~200 ms
        } else {
            self.hangover = self.hangover.saturating_sub(1);
        }
        let voice_active = speech_now || self.hangover > 0;

        // 5. Gate. Feed it an artificially low level when the VAD disagrees, so
        // loud-but-not-speech blocks stay shut.
        let gate_level = if voice_active { level_db } else { -120.0 };
        let gate_open = self.gate.process(block, gate_level);

        // 6. Level the result, then catch transients.
        let speaking = gate_open && voice_active;
        self.agc.process(block, level_db, speaking);
        self.limiter.process(block);

        BlockAnalysis {
            vad,
            level_db,
            noise_floor_db,
            snr_db,
            activation_threshold_db: noise_floor_db + self.profile.snr_margin_db(),
            speaking,
            erle_db,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::dsp::peak;

    fn white_noise(len: usize, amp: f32, seed: u32) -> Vec<f32> {
        // Deterministic LCG so tests are reproducible.
        let mut s = seed.wrapping_mul(2_654_435_761).wrapping_add(1);
        (0..len)
            .map(|_| {
                s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                ((s >> 8) as f32 / 8_388_608.0 - 1.0) * amp
            })
            .collect()
    }

    /// A continuous signal, generated in one piece.
    ///
    /// This must never be produced per-block: restarting the phase at each block
    /// boundary injects a step discontinuity, which is a broadband click rather
    /// than the low-frequency tone the test intends to measure.
    fn rumble(len: usize, amp: f32) -> Vec<f32> {
        // 55 Hz plus a harmonic, standing in for engine and wind noise.
        (0..len)
            .map(|i| {
                let t = i as f32 / SAMPLE_RATE as f32;
                let a = (2.0 * std::f32::consts::PI * 55.0 * t).sin();
                let b = (2.0 * std::f32::consts::PI * 110.0 * t).sin() * 0.5;
                (a + b) * amp
            })
            .collect()
    }

    #[test]
    fn helmet_profile_crushes_engine_rumble() {
        let mut p = CaptureProcessor::new(NoiseProfile::Helmet);

        // One continuous second of rumble, consumed block by block.
        let signal = rumble(FRAME_SIZE * 100, 0.4);
        let input_level = rms(&signal);
        let mut worst_out = 0.0f32;

        for (i, chunk) in signal.chunks_exact(FRAME_SIZE).enumerate() {
            let mut block = chunk.to_vec();
            p.process(&mut block);
            // Skip the first few blocks while the filters and gate settle.
            if i > 20 {
                worst_out = worst_out.max(rms(&block));
            }
        }

        assert!(
            worst_out < input_level * 0.02,
            "rumble leaked through: in {input_level}, out {worst_out}"
        );
    }

    /// Speech-like signal: a voiced pitch with formant-ish harmonics.
    fn speech(len: usize, offset: usize, amp: f32) -> Vec<f32> {
        (0..len)
            .map(|i| {
                let t = (i + offset) as f32 / SAMPLE_RATE as f32;
                let f0 = 130.0;
                let mut v = 0.0;
                for (h, w) in [(1.0, 0.5), (2.0, 0.9), (3.0, 0.7), (5.0, 0.4), (8.0, 0.2)] {
                    v += (2.0 * std::f32::consts::PI * f0 * h * t).sin() * w;
                }
                // Syllable-rate envelope so it is not a static drone.
                let env = 0.5 * (1.0 + (2.0 * std::f32::consts::PI * 3.5 * t).sin());
                v * env * amp
            })
            .collect()
    }

    #[test]
    fn speech_over_loud_engine_noise_is_still_transmitted() {
        // The counterpart to the rumble test: suppression must not be so
        // aggressive that the rider cannot be heard over the engine.
        let mut p = CaptureProcessor::new(NoiseProfile::Helmet);
        let n = FRAME_SIZE * 200;
        let bed = rumble(n, 0.3);
        let voice = speech(n, 0, 0.35);

        let mut speaking_blocks = 0;
        let mut total = 0;
        for (i, (b, v)) in bed
            .chunks_exact(FRAME_SIZE)
            .zip(voice.chunks_exact(FRAME_SIZE))
            .enumerate()
        {
            // First half is engine only; second half is engine plus speech.
            let speaking_half = i >= 100;
            let mut block: Vec<f32> = if speaking_half {
                b.iter().zip(v).map(|(x, y)| x + y).collect()
            } else {
                b.to_vec()
            };
            let a = p.process(&mut block);
            if speaking_half {
                total += 1;
                if a.speaking {
                    speaking_blocks += 1;
                }
            }
        }

        assert!(
            speaking_blocks * 100 / total > 40,
            "speech over engine noise was suppressed: only {speaking_blocks}/{total} blocks keyed"
        );
    }

    #[test]
    fn helmet_profile_does_not_transmit_steady_road_noise() {
        let mut p = CaptureProcessor::new(NoiseProfile::Helmet);
        let mut speaking_blocks = 0;
        for i in 0..150 {
            let mut block = white_noise(FRAME_SIZE, 0.05, i);
            if p.process(&mut block).speaking {
                speaking_blocks += 1;
            }
        }
        // A little leakage at the start is tolerable; sustained keying is not.
        assert!(
            speaking_blocks < 30,
            "road noise opened the gate for {speaking_blocks}/150 blocks"
        );
    }

    #[test]
    fn output_never_clips_even_on_extreme_input() {
        // Wind gusts and handling noise can massively overdrive a helmet mic.
        let mut p = CaptureProcessor::new(NoiseProfile::Helmet);
        for i in 0..60 {
            let mut block: Vec<f32> = white_noise(FRAME_SIZE, 4.0, i);
            p.process(&mut block);
            assert!(peak(&block) <= 1.0, "block {i} clipped at {}", peak(&block));
            assert!(
                block.iter().all(|s| s.is_finite()),
                "block {i} produced non-finite samples"
            );
        }
    }

    #[test]
    fn off_profile_passes_audio_through_largely_intact() {
        let mut p = CaptureProcessor::new(NoiseProfile::Off);
        // A 1 kHz tone should survive, since Off only removes rumble.
        let signal: Vec<f32> = (0..FRAME_SIZE * 40)
            .map(|i| (2.0 * std::f32::consts::PI * 1000.0 * i as f32 / 48000.0).sin() * 0.3)
            .collect();

        let mut worst = 0.0f32;
        for chunk in signal.chunks_exact(FRAME_SIZE) {
            let mut block = chunk.to_vec();
            p.process(&mut block);
            worst = worst.max(rms(&block));
        }
        assert!(
            worst > 0.1,
            "Off profile should not suppress speech-band tone"
        );
    }

    #[test]
    fn profiles_get_progressively_more_aggressive() {
        assert!(NoiseProfile::Helmet.cutoff_hz() > NoiseProfile::Standard.cutoff_hz());
        assert!(NoiseProfile::Standard.cutoff_hz() > NoiseProfile::Light.cutoff_hz());
        assert!(NoiseProfile::Helmet.vad_threshold() > NoiseProfile::Light.vad_threshold());
        assert!(NoiseProfile::Helmet.agc_max_gain_db() >= NoiseProfile::Standard.agc_max_gain_db());
    }

    #[test]
    fn switching_profiles_mid_stream_is_safe() {
        let mut p = CaptureProcessor::new(NoiseProfile::Standard);
        for i in 0..10 {
            let mut b = white_noise(FRAME_SIZE, 0.1, i);
            p.process(&mut b);
        }
        p.set_profile(NoiseProfile::Helmet);
        assert_eq!(p.profile(), NoiseProfile::Helmet);
        for i in 0..10 {
            let mut b = white_noise(FRAME_SIZE, 0.1, i);
            let a = p.process(&mut b);
            assert!(a.level_db.is_finite());
            assert!(b.iter().all(|s| s.is_finite()));
        }
        // Setting the same profile again must be a no-op, not a rebuild.
        p.set_profile(NoiseProfile::Helmet);
        assert_eq!(p.profile(), NoiseProfile::Helmet);
    }

    #[test]
    fn frame_size_matches_a_10ms_block_at_48k() {
        assert_eq!(FRAME_SIZE, 480);
        assert_eq!(SAMPLE_RATE, 48_000);
    }
}
