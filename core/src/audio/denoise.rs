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
use super::pitch::PitchTracker;

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
            // Not zero. Even with no suppression, voice activation still has
            // to decide whether anyone is talking, and a margin of zero means
            // anything at all above the noise floor counts — which is
            // everything, permanently.
            NoiseProfile::Off => 6.0,
            NoiseProfile::Light => 6.0,
            NoiseProfile::Standard => 8.0,
            NoiseProfile::Helmet => 10.0,
        }
    }

    /// Periodicity at which a block counts as unambiguously a voice.
    ///
    /// High, because what it buys is a relaxed SNR margin and a lifting of
    /// suppression, and both should happen only where there is no real doubt.
    /// Nothing [`super::pitch`] measures gets here by accident: wind, engines
    /// and traffic all sit below 0.5 in its tests, and clean speech above 0.85.
    fn voiced_threshold(self) -> f32 {
        match self {
            // Never used: with suppression off, level decides alone.
            NoiseProfile::Off => 2.0,
            NoiseProfile::Light => 0.80,
            NoiseProfile::Standard => 0.78,
            NoiseProfile::Helmet => 0.75,
        }
    }

    /// Periodicity below which a block may not *start* a transmission.
    ///
    /// Deliberately far below [`Self::voiced_threshold`], and it is a veto on
    /// opening rather than on continuing. Unvoiced speech is genuinely
    /// aperiodic — "s", "f", "sh" and a whisper all score near zero — so this
    /// must never be able to close a transmission that is already open, and
    /// must be low enough that a voiced syllable a moment earlier is what
    /// opened it.
    fn aperiodic_threshold(self) -> f32 {
        match self {
            NoiseProfile::Off => -1.0,
            NoiseProfile::Light => 0.20,
            NoiseProfile::Standard => 0.25,
            NoiseProfile::Helmet => 0.30,
        }
    }

    /// How many dB of SNR margin a strongly voiced block is forgiven.
    ///
    /// The margin exists because RNNoise's VAD fires on tonal backgrounds, and
    /// it is what a helmet at speed defeats: the wind raises the tracked floor
    /// until a rider's own voice cannot clear it. Periodicity at a human pitch
    /// is evidence of a voice that the level tests cannot see, so it is worth
    /// a few dB — not the whole margin, or a drone gets in. Bounded so that
    /// even at full relief the remaining margin is above zero.
    fn voiced_margin_relief(self) -> f32 {
        match self {
            NoiseProfile::Off => 0.0,
            NoiseProfile::Light => 3.0,
            NoiseProfile::Standard => 4.0,
            NoiseProfile::Helmet => 6.0,
        }
    }

    /// How much of the suppression to lift on a strongly voiced block.
    ///
    /// A rider complaining their voice sounds watery and gated is describing
    /// suppression applied at full strength to the speech as well as to the
    /// wind. On a block that is unambiguously a voice, less of it is needed:
    /// the voice is loud enough to be masking the noise on its own, which is
    /// what makes this free rather than a trade.
    fn voiced_relief(self) -> f32 {
        match self {
            NoiseProfile::Off => 0.0,
            NoiseProfile::Light => 0.15,
            NoiseProfile::Standard => 0.20,
            NoiseProfile::Helmet => 0.25,
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

    // --- what the decision was made of ------------------------------------
    //
    // The chain already computes all of this and then throws it away, which is
    // why "it cut me off" has never had an answer. Carried out so the
    // diagnostics panel can show which of the two conditions failed, rather
    // than only that the result was silence.
    /// Whether RNNoise thought this block was speech.
    pub vad_says_speech: bool,
    /// Whether the block cleared the SNR margin above the noise floor.
    pub snr_says_speech: bool,
    /// How periodic the block is at a human pitch, 0..1.
    ///
    /// See [`super::pitch`]. Low is "no evidence of voicing", which is the
    /// honest reading of a whisper as well as of wind — it is not evidence of
    /// no voice, and nothing downstream treats it as such.
    pub harmonicity: f32,
    /// The pitch that produced it, in Hz, or 0.
    pub f0_hz: f32,
    /// Whether the block was periodic enough to open a transmission by itself.
    pub pitch_says_speech: bool,
    /// Whether the noise gate is passing audio.
    ///
    /// Distinct from [`speaking`](Self::speaking): the gate ramps its gain, so
    /// it can still be open on a block the chain has decided is not speech.
    pub gate_open: bool,
    /// Gain the AGC is currently applying, in dB.
    pub agc_gain_db: f32,
    /// Whether the chain is still in its start-up hold and not to be trusted.
    pub warming_up: bool,
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
    /// The block as the gate saw it, kept for the diagnostics analyser.
    pre_gate: Vec<f32>,
    /// Whether the block is periodic at a human pitch — the one thing in this
    /// chain that is not a measure of level.
    pitch: PitchTracker,
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
            // Capacity now so the copy in `process` never reallocates.
            pre_gate: Vec::with_capacity(FRAME_SIZE),
            pitch: PitchTracker::new(),
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

        // 3. Ask whether this is periodic at a human pitch.
        //
        // On the denoised signal rather than the raw one: periodicity is what
        // is being measured, and leaving the wind in masks the very thing the
        // search is looking for. RNNoise has already removed most of it, and
        // it removes it *without* knowing whether a voice was there, so this
        // is not circular.
        let voice = self.pitch.analyse(&self.denoised);
        let pitch_says_speech = voice.harmonicity >= self.profile.voiced_threshold();

        // 4. Blend, so lighter profiles keep some natural room tone — and lift
        // some of the suppression back off on a block that is unambiguously a
        // voice. Full-strength suppression applied to speech is what "I sound
        // watery and gated" is a description of, and on a voiced block the
        // voice is masking the background anyway.
        // Only where the pitch search is *certain*, not scaled by whatever it
        // happened to measure. Lifting suppression in proportion to a middling
        // score means lifting it a little on everything, and "a little of the
        // raw signal on every block" is exactly the rumble this profile exists
        // to remove — `helmet_profile_crushes_engine_rumble` caught it doing
        // precisely that. The relief is for blocks that are unambiguously
        // speech or it is for nothing.
        let relief = if pitch_says_speech {
            self.profile.voiced_relief() * voice.harmonicity.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let mix = (self.profile.denoise_mix() * (1.0 - relief)).clamp(0.0, 1.0);
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

        // The SNR margin, relaxed when the block is unambiguously periodic at
        // a human pitch.
        //
        // Relaxed rather than bypassed, and that distinction was found the
        // hard way. An arm that let strong periodicity override the level
        // tests outright let a 55 Hz engine drone straight through: the Helmet
        // profile's 180 Hz high-pass strips the fundamental and what is left
        // is a clean 110 Hz tone, which is a perfectly good male pitch by
        // every measure in [`super::pitch`] — and RNNoise calls it speech with
        // high confidence too, exactly as the comment above warns.
        //
        // The thing that knows it is a machine is the noise floor tracker. A
        // drone is *steady*, so the floor climbs to meet it and the SNR never
        // clears however loud it gets. Keeping the margin in the decision
        // keeps that knowledge; removing it threw the one signal away that
        // could tell the difference.
        //
        // What the relief buys is the case it was added for. A rider at speed
        // does clear the wind — their voice adds energy on top of it — but by
        // less than the margin a helmet needs against a background that is
        // both loud and tonal. Today that costs them the whole sentence, and
        // no threshold on level can recover it because the wind moved the
        // threshold. Periodicity is evidence the level tests cannot see, so it
        // buys a few dB of margin and nothing more.
        let margin_db = self.profile.snr_margin_db()
            - if pitch_says_speech {
                self.profile.voiced_margin_relief()
            } else {
                0.0
            };
        let snr_says_speech = snr_db >= margin_db;
        // The decision, and the shape of it is the point.
        //
        // It used to be `vad && snr`, and a chain built on those two alone
        // must fail in *both* directions at once — which is what riders
        // reported. Both tests are measures of level, and level cannot
        // separate a rider talking quietly inside a helmet at speed from the
        // wind they are talking through: the two overlap. Tighten it and the
        // rider is cut off; loosen it and the weather goes out on the channel.
        // There is no setting that does both, so no amount of tuning was ever
        // going to fix it.
        //
        // Periodicity is a different axis, and adding it lets the decision be
        // *looser* about level and *stricter* about voice-likeness at the same
        // time. Three arms, deliberately asymmetric:
        let speech_now = if warming_up {
            false
        } else if self.profile == NoiseProfile::Off {
            // How hard the audio is cleaned and whether anyone is talking are
            // separate questions. Answering "always" here meant turning
            // suppression off also turned voice activation off, and the far end
            // saw an open microphone for the whole session.
            //
            // RNNoise is not running to be asked, so the level against the
            // tracked floor decides on its own.
            snr_says_speech
        } else if self.hangover > 0 {
            // Mid-sentence, inside the 200 ms hangover. Far less evidence is
            // needed to *keep* going than to start, because the quiet parts of
            // speech are genuinely quiet and genuinely aperiodic — an unvoiced
            // consonant has no pitch at all, and demanding one here would clip
            // every "s" and "f".
            //
            // Not loosened beyond that, and the reason is worth keeping: an
            // arm here that accepted the VAD alone kept a transmission open
            // for ever on an engine drone, because RNNoise never stops calling
            // it speech. Anything that can refresh the hangover indefinitely
            // has to be something a machine cannot supply.
            vad_says_speech && snr_says_speech
        } else {
            // Opening. Everything that was required before, and additionally
            // not clearly aperiodic — which is what a gust is, and what an
            // engine is, and what no voiced sound is. This is the arm that
            // stops the weather starting a transmission.
            vad_says_speech
                && snr_says_speech
                && voice.harmonicity >= self.profile.aperiodic_threshold()
        };

        if speech_now {
            self.hangover = 20; // ~200 ms
        } else {
            self.hangover = self.hangover.saturating_sub(1);
        }
        let voice_active = speech_now || self.hangover > 0;

        // 5. Gate. Feed it an artificially low level when the VAD disagrees, so
        // loud-but-not-speech blocks stay shut.
        //
        // Except with suppression off, where the audio must come through
        // untouched. Deciding not to transmit is not a licence to alter what
        // is transmitted when we do.
        let gate_level = if voice_active || self.profile == NoiseProfile::Off {
            level_db
        } else {
            -120.0
        };
        // Kept for the diagnostics analyser: this is the signal the gate is
        // about to judge, and the distance between it and the raw microphone is
        // everything the suppressor did.
        //
        // Copied unconditionally rather than behind a flag. A 480-sample memcpy
        // is a few hundred nanoseconds against a 10 ms budget, and a branch that
        // can be wrong — leaving the panel drawing a stale trace with no hint
        // that it has stopped updating — costs more than it saves.
        self.pre_gate.clear();
        self.pre_gate.extend_from_slice(block);

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
            // The margin actually in force, relief included, so the panel
            // shows the threshold the block was really judged against rather
            // than a nominal one it never used.
            activation_threshold_db: noise_floor_db + margin_db,
            speaking,
            erle_db,
            vad_says_speech,
            snr_says_speech,
            harmonicity: voice.harmonicity,
            f0_hz: voice.f0_hz,
            pitch_says_speech,
            gate_open,
            agc_gain_db: self.agc.gain_db(),
            warming_up,
        }
    }

    /// The signal as the noise gate saw it, from the most recent block.
    ///
    /// One of the three taps the diagnostics analyser draws. Empty until the
    /// first block has been processed.
    pub fn pre_gate(&self) -> &[f32] {
        &self.pre_gate
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::dsp::peak;

    #[test]
    fn a_quiet_room_never_keys_the_transmitter() {
        // Whether the audio is cleaned and whether anyone is talking are
        // separate questions. Answering the second with "always" whenever
        // suppression is off left the far end seeing an open microphone for
        // the entire session, which is both rude and a waste of the link.
        for profile in [
            NoiseProfile::Off,
            NoiseProfile::Light,
            NoiseProfile::Standard,
            NoiseProfile::Helmet,
        ] {
            let mut p = CaptureProcessor::new(profile);
            let mut keyed = 0;
            // Well past the warm-up hold, so the floor estimate has settled.
            for i in 0..200 {
                let mut block = white_noise(FRAME_SIZE, 0.004, i as u32);
                if p.process(&mut block).speaking {
                    keyed += 1;
                }
            }
            assert!(
                keyed < 20,
                "{profile:?}: room tone keyed {keyed}/200 blocks"
            );
        }
    }

    #[test]
    fn speech_still_keys_the_transmitter_with_suppression_off() {
        // The fix must not overshoot into never transmitting at all.
        let mut p = CaptureProcessor::new(NoiseProfile::Off);
        for i in 0..60 {
            let mut quiet = white_noise(FRAME_SIZE, 0.004, i as u32);
            p.process(&mut quiet);
        }

        let mut keyed = 0;
        for i in 0..60 {
            let mut block = white_noise(FRAME_SIZE, 0.25, 1_000 + i as u32);
            if p.process(&mut block).speaking {
                keyed += 1;
            }
        }
        assert!(keyed > 30, "speech keyed only {keyed}/60 blocks");
    }

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

    #[test]
    fn the_pre_gate_tap_is_what_the_gate_saw_and_not_what_came_out() {
        // The point of the tap. If it returned the post-gate block instead, the
        // analyser would draw two identical traces and the gate would look like
        // it was doing nothing — the exact opposite of what it is there to show.
        let mut p = CaptureProcessor::new(NoiseProfile::Standard);

        // Well past warm-up, and far below any plausible threshold, so the gate
        // is firmly shut and its output is near silence.
        let mut analysis = None;
        for i in 0..60 {
            let mut block = white_noise(FRAME_SIZE, 0.0005, 90 + i as u32);
            analysis = Some(p.process(&mut block));
        }
        let analysis = analysis.unwrap();

        assert!(!analysis.warming_up, "still warming up after 60 blocks");
        assert_eq!(p.pre_gate().len(), FRAME_SIZE);

        // Something was there before the gate.
        let before = rms(p.pre_gate());
        assert!(before > 0.0, "the tap captured nothing at all");
        assert!(before.is_finite());
    }

    #[test]
    fn the_analysis_says_which_of_the_two_conditions_failed() {
        // "It cut me off" is answerable only if the two halves of the decision
        // are reported separately. Steady noise must fail the SNR test — it
        // raises the floor with itself — and that must be visible as such.
        let mut p = CaptureProcessor::new(NoiseProfile::Standard);
        let mut last = None;
        for i in 0..80 {
            let mut block = white_noise(FRAME_SIZE, 0.02, 200 + i as u32);
            last = Some(p.process(&mut block));
        }
        let a = last.unwrap();

        assert!(!a.warming_up);
        assert!(
            !a.snr_says_speech,
            "steady noise cleared the SNR margin at {} dB over the floor",
            a.snr_db
        );
        assert!(!a.speaking, "steady noise was transmitted");
        // And the composite still agrees with its parts.
        assert!(!(a.vad_says_speech && a.snr_says_speech));
        assert!(a.agc_gain_db.is_finite());
    }
}
