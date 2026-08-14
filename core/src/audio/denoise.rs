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
use super::dsp::{
    rms, to_dbfs, Agc, Biquad, Limiter, NoiseFloorTracker, NoiseGate, RumbleFilter, SpeechBand,
};
use super::modulation::ModulationTracker;
use super::pitch::{Pitch, PitchTracker};

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
    /// Chooses between the four above from what the microphone is hearing.
    ///
    /// A fifth option rather than a replacement, and never itself in force:
    /// see [`CaptureProcessor::effective_profile`]. A rider who sets `Helmet`
    /// and then stops for coffee is over-suppressed indoors; one who sets
    /// `Light` at home and then rides is under-suppressed at 120 km/h, which
    /// is the worse half — and neither notices until somebody tells them.
    ///
    /// `Off` is never chosen. It is a diagnostic setting, not a condition.
    ///
    /// It has no numbers of its own — it is a rule for choosing, not a
    /// setting. Every parameter table below therefore shares `Standard`'s row
    /// with it, which is a fallback that should never be reached:
    /// [`CaptureProcessor`] resolves the choice once and keeps a profile that
    /// is never `Auto` as the one actually in force.
    Auto,
}

impl NoiseProfile {
    /// High-pass corner. Wind energy climbs steeply as speed rises, so the
    /// helmet profile sacrifices some low speech warmth for intelligibility.
    fn cutoff_hz(self) -> f32 {
        match self {
            NoiseProfile::Off => 60.0,
            NoiseProfile::Light => 90.0,
            NoiseProfile::Standard | NoiseProfile::Auto => 120.0,
            NoiseProfile::Helmet => 180.0,
        }
    }

    /// Low-pass corner, or `None` to leave the top of the band open.
    ///
    /// Consonants reach 6–7 kHz and carry a large share of intelligibility —
    /// "s" against "f" is decided up there and nowhere else — so these do not
    /// go lower than 6.5 even in a helmet. What is above them is not speech:
    /// wind hiss, tyre roar, chain noise, and the top octave of a helmet's own
    /// turbulence, all of which the level meter and the floor tracker were
    /// otherwise counting as signal.
    ///
    /// `Off` keeps nothing at all, because `Off` means untouched.
    ///
    /// These started at 8.5 / 7.5 / 6.5 kHz, which is the textbook answer and
    /// cost more than it was worth. `a_whisper_is_not_thrown_away` dropped
    /// from passing to 13.2% the moment the filter went in, and switching it
    /// off for `Standard` put it straight back — so the corner, not the
    /// suppression, was removing the speech. A whisper is broadband
    /// turbulence with real energy well above 8 kHz; low-passing it takes away
    /// the signal, the level, and with it the rider.
    ///
    /// Against a measured cost, the benefit has to be measured too, and it is
    /// not: `noise_alone_transmits_nothing` was already at zero before this
    /// existed. So the two profiles that are not fighting a gale keep a much
    /// gentler corner, and only `Helmet` — where the hiss above the voice is
    /// genuinely loud and the rider has accepted a trade by choosing it —
    /// keeps the aggressive one.
    fn low_cutoff_hz(self) -> Option<f32> {
        match self {
            NoiseProfile::Off => None,
            NoiseProfile::Light => Some(12_000.0),
            NoiseProfile::Standard | NoiseProfile::Auto => Some(10_000.0),
            NoiseProfile::Helmet => Some(6_500.0),
        }
    }

    /// How much of the denoised signal to blend in, 0.0..=1.0. Blending below 1.0
    /// keeps a little of the original so quiet speech does not sound gated.
    fn denoise_mix(self) -> f32 {
        match self {
            NoiseProfile::Off => 0.0,
            NoiseProfile::Light => 0.6,
            NoiseProfile::Standard | NoiseProfile::Auto => 0.9,
            NoiseProfile::Helmet => 1.0,
        }
    }

    /// Gate thresholds (open, close) in dBFS.
    ///
    /// These used to climb with the profile — Helmet the highest at -40 —
    /// which is backwards, and measurably so. The gate sees the signal *after*
    /// suppression, and Helmet suppresses hardest: on real helmet audio its
    /// output averages -45 dBFS, five decibels below the threshold it then has
    /// to clear. The profile built for the worst conditions was the one whose
    /// gate was hardest to open, and the two compounded.
    ///
    /// It cost 31% of the rider's speech. Of blocks hand-labelled as speech,
    /// 90% cleared the SNR margin and only 69% got through the gate, so a
    /// third of what the chain had already decided to send was thrown away
    /// afterwards by a threshold nobody had matched to the suppression in
    /// front of it.
    ///
    /// It was lowered on that reasoning and put back, because the reasoning
    /// was wrong in a way only the measurement showed. The gate is fed -120 dB
    /// on any block the transmit decision rejected, so its 69% is not its own
    /// threshold talking — it is the decision upstream, seen through the gate.
    /// Dropping the thresholds eighteen decibels moved recall by less than two
    /// points and put 2% of synthetic noise on the wire, because for the
    /// blocks it did affect the gate had been catching false positives the
    /// decision let past.
    ///
    /// **And then DeepFilterNet went in front of the whole chain, and the
    /// premise the revert rested on stopped being true.** The enhancer takes 7
    /// to 11 dB out of the speech before any of this sees it, so the levels
    /// these thresholds were matched to are gone. Measured with the enhancer
    /// in place on a real unclipped ride, counting only the blocks the
    /// transmit decision *accepted* — so the confound above is excluded by
    /// construction — `core/tests/profile_recall.rs` gives:
    ///
    /// | Profile | Gate vetoed | Level it judged | Old threshold |
    /// |---|---|---|---|
    /// | `Light` | 3.3% | −44.6 dBFS | −52 |
    /// | `Standard` | 45.4% | −45.7 dBFS | −46 |
    /// | `Helmet` | **67.9%** | **−51.1 dBFS** | **−40** |
    ///
    /// Helmet was judging accepted speech eleven decibels below the bar it had
    /// to clear, and throwing away two thirds of it. That is the "cuts into
    /// words" a rider hears, and it is not the transmit decision — the
    /// decision had already said yes.
    ///
    /// # Why there is no longer a number here worth reading
    ///
    /// A rider listening through the chain reported Helmet cutting into words.
    /// `core/tests/profile_recall.rs` said why, from the chain's own state and
    /// no labels at all: **the gate was throwing away most of the blocks the
    /// transmit decision had accepted**, because the level it judged them by
    /// no longer resembled the threshold it judged them against.
    ///
    /// | Ride | Profile | Decision accepted | Gate vetoed | Level it judged |
    /// |---|---|---|---|---|
    /// | music | `Light` | 45.1% | 3.3% | −44.6 dBFS |
    /// | music | `Standard` | 40.0% | 45.4% | −45.7 dBFS |
    /// | music | `Helmet` | 42.1% | **67.9%** | **−51.1 dBFS** |
    /// | road | `Helmet` | 26.6% | **98.6%** | **−69.9 dBFS** |
    ///
    /// The thresholds climbed with the profile while the output level fell,
    /// and DeepFilterNet then took a further 7 to 11 dB out of the speech in
    /// front of all of it. **A fixed dBFS threshold cannot survive that** — the
    /// same Helmet profile measures −51.1 dBFS on one ride and −69.9 on
    /// another, so no single number is right for both, and lowering them all
    /// (tried, measured) still left Helmet vetoing 47% on the second ride.
    ///
    /// So the gate is anchored to the tracked noise floor instead, by
    /// [`Self::gate_margin_db`], and asks the only question it can answer
    /// without being recalibrated every time something upstream changes: **is
    /// this above the background?** These remain only as the value before the
    /// first block sets a real one, and for `Off`, where nothing is suppressed
    /// and nothing is tracked.
    ///
    /// **What this cost, and who decided it was worth paying.** Removing the
    /// veto takes Helmet from 63.2% recall to 97.9% and its precision from
    /// 99.9% to 44% against the recording's own `speaking` column — on a
    /// voice-over-music ride, that difference is music going out with the
    /// words. Those figures cannot adjudicate it: the labels were written by
    /// this code, so the old settings scoring 99.9% against them is close to
    /// tautological. The rider listened to the transmitted-only playback and
    /// judged the leakage acceptable against the words being cut. That is a
    /// label this chain did not write, and it is the one this rests on.
    fn gate_db(self) -> (f32, f32) {
        match self {
            NoiseProfile::Off => (-90.0, -95.0),
            // Overwritten from the floor on every block, before the gate is
            // asked anything.
            _ => (-52.0, -60.0),
        }
    }

    /// How far above the tracked noise floor the gate opens, in dB.
    ///
    /// Below [`Self::snr_margin_db`] on purpose, and the gap is the design:
    /// the transmit decision uses the wider margin to decide whether a phrase
    /// *starts*, and the gate uses this narrower one to decide whether audio
    /// is still passing. A gate at the same margin would shut on the quiet
    /// middle of a word that the decision's own hangover was holding open —
    /// which is the fault above, arrived at from the other side.
    fn gate_margin_db(self) -> f32 {
        match self {
            NoiseProfile::Off => 0.0,
            NoiseProfile::Light => 0.0,
            NoiseProfile::Standard | NoiseProfile::Auto => 2.0,
            NoiseProfile::Helmet => 4.0,
        }
    }

    /// Minimum RNNoise speech probability required to treat a block as speech.
    fn vad_threshold(self) -> f32 {
        match self {
            NoiseProfile::Off => 0.0,
            NoiseProfile::Light => 0.30,
            NoiseProfile::Standard | NoiseProfile::Auto => 0.50,
            NoiseProfile::Helmet => 0.65,
        }
    }

    fn agc_max_gain_db(self) -> f32 {
        match self {
            NoiseProfile::Off => 0.0,
            NoiseProfile::Light => 12.0,
            NoiseProfile::Standard | NoiseProfile::Auto => 18.0,
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
            NoiseProfile::Standard | NoiseProfile::Auto => 8.0,
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
            NoiseProfile::Standard | NoiseProfile::Auto => 0.78,
            NoiseProfile::Helmet => 0.75,
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
    /// How much of the last second's loudness is moving at a talking rate.
    ///
    /// See [`super::modulation`]. Published, not acted on: it is a candidate
    /// discriminator being scored against hand-labelled recordings before it
    /// is allowed anywhere near the transmit decision, which is the order the
    /// pitch measure should have gone in.
    pub modulation: f32,
    /// Whether the noise gate is passing audio.
    ///
    /// Distinct from [`speaking`](Self::speaking): the gate ramps its gain, so
    /// it can still be open on a block the chain has decided is not speech.
    pub gate_open: bool,
    /// Gain the AGC is currently applying, in dB.
    pub agc_gain_db: f32,
    /// Whether the chain is still in its start-up hold and not to be trusted.
    pub warming_up: bool,
    /// The profile actually in force, which for [`NoiseProfile::Auto`] is
    /// whichever of the other four it has settled on.
    pub effective_profile: NoiseProfile,
}

/// The full microphone chain.
pub struct CaptureProcessor {
    /// What the rider asked for, which may be [`NoiseProfile::Auto`].
    profile: NoiseProfile,
    /// What is actually in force. Never `Auto`.
    effective: NoiseProfile,
    /// Splits the block's energy at 300 Hz, for the `Auto` decision.
    ///
    /// Run on the signal *before* the rumble filter, which is the only place
    /// the question can be asked: the rumble filter's own corner is 180 Hz in
    /// a helmet, so measuring after it would be measuring the filter that the
    /// answer chooses. The chooser would then see whatever it last decided.
    tilt: Biquad,
    /// Blocks since `Auto` last changed its mind.
    auto_dwell: u32,
    /// Blocks the level has continuously wanted something lighter than what is
    /// in force.
    ///
    /// Only ever gates going *lighter*, and by how far. See
    /// [`AUTO_COOLDOWN_STANDARD_BLOCKS`].
    auto_calm: u32,
    /// The classifier's last word: the background is loud and structured.
    ///
    /// Set from outside the chain, because the thing that decides it is a
    /// neural model running well away from the audio thread. Stale by up to a
    /// few seconds by construction and that is fine — what it describes
    /// changes over tens of seconds, and [`Self::music_hold`] is what turns it
    /// into a decision.
    background_noisy: bool,
    /// Blocks of Helmet still owed to the classifier. See
    /// [`MUSIC_HOLD_BLOCKS`].
    music_hold: u32,
    /// The classifier's last word on whether a voice is present, or `None`
    /// when nothing is classifying. See [`CaptureProcessor::set_classifier_voice`].
    classifier_voice: Option<bool>,
    /// The background level *before* the chain touches it.
    ///
    /// Separate from [`Self::floor`], which tracks the signal after
    /// suppression and is the right measure for the transmit decision and the
    /// wrong one for this. Choosing a profile from the post-suppression floor
    /// is a feedback loop pointing the wrong way: a helmet at speed with
    /// `Helmet` in force is *quiet* by that measure precisely because the
    /// profile is working, so Auto reads it as a quiet room and backs off,
    /// which lets the noise back in. It settles somewhere too gentle, or
    /// oscillates. Both showed up the first time this was run.
    auto_floor: NoiseFloorTracker,
    rumble: RumbleFilter,
    /// Closes the top of the band. `None` with suppression off.
    band: Option<SpeechBand>,
    denoise: Box<DenoiseState<'static>>,
    /// Runs first: echo is speech, so neither the gate nor RNNoise will remove
    /// it, and everything downstream works better without it.
    aec: EchoCanceller,
    /// What [`CaptureProcessor::cancel_echo`] last measured, waiting to be put
    /// in the analysis by [`CaptureProcessor::suppress`].
    ///
    /// A field rather than an argument because the two are separate calls only
    /// so that the worker can time them separately, and threading a number
    /// through the split would let a caller hand `suppress` an ERLE from a
    /// different block.
    erle_db: f32,
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
    /// Stages the performance ladder has given up. Set from the worker, which
    /// owns the ladder because the deadline belongs to the whole block rather
    /// than to any one stage in it. See [`super::relief`].
    skip_pitch: bool,
    skip_rnnoise: bool,
    /// Whether the recent loudness is moving at a talking rate. Measured and
    /// published; nothing is decided by it.
    modulation: ModulationTracker,
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

/// Where `Auto` splits the spectrum to ask "is this rumble or is it a room".
///
/// 300 Hz. Below it sits wind, engine and road; above it sits nearly all of
/// the speech that carries a word. The *share* below is what distinguishes a
/// quiet room from a helmet at speed far better than the level alone does — a
/// loud living room and a motorway are similar in dBFS and nothing alike in
/// where the energy is.
const AUTO_TILT_HZ: f32 = 300.0;

/// Blocks `Auto` must stay put before it may change its mind. 5 s.
///
/// Long, on purpose. The cost of switching is not the arithmetic, it is that
/// the rumble filter, the band filter, the gate and the AGC are all rebuilt
/// and each starts from nothing — audible as a moment of unevenness. A
/// chooser that flapped at every junction would spend the ride doing that.
const AUTO_DWELL_BLOCKS: u32 = 500;

/// Hysteresis on the noise floor thresholds, in dB.
///
/// Without it a floor sitting exactly on a boundary switches back and forth
/// every dwell period for as long as the rider stays there, which is precisely
/// what a steady speed produces.
const AUTO_HYSTERESIS_DB: f32 = 4.0;

/// How long the level must keep asking for something lighter before `Auto`
/// dials down, per step.
///
/// **Asymmetric on purpose, and the asymmetry is the whole point.** Going more
/// aggressive costs some naturalness; going lighter costs whatever is in the
/// room going out on the wire, and `Light` was measured to suppress music
/// barely at all — 73.3% of blocks transmitted against `Helmet`'s 13.8%. So
/// the further down a step goes the longer it waits: fifteen seconds to leave
/// `Helmet` for `Standard`, a minute to reach `Light`.
///
/// The counter is "how long has the level wanted something lighter than what
/// is in force", not "how long has it been silent". An earlier version
/// required the floor below −55 dB for *any* dial-down, which made the step
/// from `Helmet` to `Standard` unreachable in a room the level itself called
/// `Standard` — the counter could only run in conditions that already implied
/// `Light`.
///
/// What the old rule bought was protection against a measured inversion: `Auto`
/// chooses on level, so *quieter* music landed in a lighter profile, and the
/// same clip attenuated 10 dB moved from `Helmet` to `Standard` with its
/// transmitted share going **up**, 28.7% to 47.2%. That job now belongs to the
/// classifier, which detects the music itself and holds `Helmet` — a real
/// measurement instead of a level heuristic. **With the classifier switched
/// off, or on a platform that cannot run it, this protection is weaker than it
/// was.** See `docs/MUSIC_GATE.md`.
const AUTO_COOLDOWN_STANDARD_BLOCKS: u32 = 1_500;
const AUTO_COOLDOWN_LIGHT_BLOCKS: u32 = 6_000;

/// How long the level has to keep wanting lighter before `Auto` moves to `to`.
fn cooldown_blocks(to: NoiseProfile) -> u32 {
    match to {
        NoiseProfile::Light => AUTO_COOLDOWN_LIGHT_BLOCKS,
        _ => AUTO_COOLDOWN_STANDARD_BLOCKS,
    }
}

/// One profile lighter, and no further.
///
/// Dialling down is a staircase, not a jump. A room that goes straight from a
/// motorway to silence still passes through `Standard` on the way to `Light`,
/// serving each step its own cooldown — fifteen seconds and then a minute — so
/// the minute is spent in `Standard` rather than in `Helmet`. Going *up* is not
/// stepped: escalating is the cheap direction and waiting to climb it is how a
/// rider gets caught out.
fn one_step_lighter(from: NoiseProfile) -> NoiseProfile {
    match from {
        NoiseProfile::Helmet => NoiseProfile::Standard,
        // `Off` is never chosen by `Auto`, so `Light` is the bottom.
        _ => NoiseProfile::Light,
    }
}

/// How long `Helmet` is held after the classifier last said the background was
/// loud and structured. 15 seconds.
///
/// A minimum rather than an exact span. Once it expires the dial-down cooldown
/// still applies, so leaving `Helmet` additionally needs fifteen seconds of the
/// level itself asking — the two compose, and the effect is that a rider who
/// pulls up at lights keeps the profile that was working until the road has
/// actually gone quiet.
///
/// The hold is the whole reason a classifier is safe to use here. A single
/// inference is wrong sometimes; to *release* Helmet the classifier has to be
/// wrong continuously for fifteen seconds, and to *take* it, right once. That
/// asymmetry matches the cost: over-suppressing indoors sounds slightly
/// synthetic, under-suppressing at 120 km/h loses the rider.
const MUSIC_HOLD_BLOCKS: u32 = 1_500;

/// How hard a profile works, for comparing two of them.
///
/// `Off` is not reachable from `Auto` and sorts lowest so the ordering is
/// total; the enum's own order is not this, and relying on it would be a
/// silent bug the first time a variant moved.
fn aggressiveness(p: NoiseProfile) -> u8 {
    match p {
        NoiseProfile::Off => 0,
        NoiseProfile::Light => 1,
        NoiseProfile::Standard => 2,
        NoiseProfile::Helmet => 3,
        NoiseProfile::Auto => 2,
    }
}

impl CaptureProcessor {
    pub fn new(profile: NoiseProfile) -> Self {
        let (open_db, close_db) = profile.gate_db();
        // Auto starts at Standard rather than at either extreme: whichever way
        // the first few seconds land, the distance to walk is one step.
        let effective = if profile == NoiseProfile::Auto {
            NoiseProfile::Standard
        } else {
            profile
        };
        Self {
            profile,
            effective,
            tilt: Biquad::low_pass(SAMPLE_RATE as f32, AUTO_TILT_HZ, 0.707),
            auto_dwell: 0,
            auto_calm: 0,
            background_noisy: false,
            music_hold: 0,
            classifier_voice: None,
            // Slower than the transmit-side tracker. This one is deciding what
            // kind of place the rider is in, which changes over minutes, not
            // whether the current block is speech.
            auto_floor: NoiseFloorTracker::new(100),
            rumble: RumbleFilter::new(SAMPLE_RATE as f32, effective.cutoff_hz()),
            band: effective
                .low_cutoff_hz()
                .map(|hz| SpeechBand::new(SAMPLE_RATE as f32, hz)),
            denoise: DenoiseState::new(),
            aec: EchoCanceller::new(DEFAULT_TAPS),
            erle_db: 0.0,
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
            skip_pitch: false,
            skip_rnnoise: false,
            modulation: ModulationTracker::new(),
            hangover: 0,
        }
    }

    pub fn profile(&self) -> NoiseProfile {
        self.profile
    }

    /// The profile actually in force, which for [`NoiseProfile::Auto`] is
    /// whichever of the other four it has settled on.
    ///
    /// Published rather than kept private because a rider who cannot see where
    /// Auto has landed has no way to tell a bad choice from a bad chain, and
    /// the first thing they will do is turn Auto off and never turn it on
    /// again.
    pub fn effective_profile(&self) -> NoiseProfile {
        self.effective
    }

    /// Tells the chain what the background classifier last concluded.
    ///
    /// A supporting vote for `Helmet` and nothing else: it never reaches the
    /// transmit decision, and it cannot pick a *lighter* profile. Being wrong
    /// about a profile costs some naturalness; being wrong at the gate cuts a
    /// rider off mid-sentence, and no classifier is going near that.
    ///
    /// It also has no effect at all unless the rider has chosen `Auto`. A
    /// profile someone picked by hand is an instruction, not a suggestion.
    pub fn set_background_noisy(&mut self, noisy: bool) {
        self.background_noisy = noisy;
    }

    /// Tells the chain whether the classifier can currently hear a voice.
    ///
    /// `Some(true)` for speech or singing, `Some(false)` for neither, and
    /// **`None` when nothing is classifying at all** — which is not the same as
    /// "no voice" and must not be treated as one. The classifier runs only
    /// under `Auto`, only where the model exists, and only every couple of
    /// seconds; the chain falls back to its own opinion whenever it is absent,
    /// rather than to silence.
    ///
    /// Unlike [`Self::set_background_noisy`] this *does* reach the gate,
    /// indirectly: it decides whether the noise floor may climb, and the gate
    /// opens relative to that floor. It can only ever hold the floor **down**,
    /// so the failure it can cause is a gate that opens too easily — never one
    /// that closes on a rider mid-sentence, which is the failure that matters.
    pub fn set_classifier_voice(&mut self, voice: Option<bool>) {
        self.classifier_voice = voice;
    }

    /// Whether the classifier's hold is currently keeping `Helmet` in force.
    ///
    /// For the diagnostics panel, so the decision is visible rather than
    /// inferred from a profile name that has several possible causes.
    pub fn music_hold_active(&self) -> bool {
        self.music_hold > 0
    }

    /// Swaps the profile, rebuilding only what depends on it.
    /// Tells the chain which stages the performance ladder has given up.
    ///
    /// Only ever called with more skipped than before — the ladder never
    /// climbs — but nothing here depends on that, so a caller that unset one
    /// would simply get the stage back.
    pub fn set_relief(&mut self, skip_pitch: bool, skip_rnnoise: bool) {
        self.skip_pitch = skip_pitch;
        self.skip_rnnoise = skip_rnnoise;
    }

    pub fn set_profile(&mut self, profile: NoiseProfile) {
        if profile == self.profile {
            return;
        }
        self.profile = profile;
        self.auto_dwell = 0;
        self.auto_calm = 0;
        let effective = if profile == NoiseProfile::Auto {
            // Keep whatever is in force and let the chooser move from there.
            // Snapping to a default would throw away a correct answer the
            // moment a rider switched Auto on, which is the moment they are
            // listening hardest.
            self.effective
        } else {
            profile
        };
        self.apply_effective(effective);
    }

    /// Puts a resolved profile in force, rebuilding only what depends on it.
    fn apply_effective(&mut self, profile: NoiseProfile) {
        debug_assert_ne!(profile, NoiseProfile::Auto, "Auto is never in force");
        if profile == self.effective {
            return;
        }
        let (open_db, close_db) = profile.gate_db();
        self.effective = profile;
        self.rumble = RumbleFilter::new(SAMPLE_RATE as f32, profile.cutoff_hz());
        self.band = profile
            .low_cutoff_hz()
            .map(|hz| SpeechBand::new(SAMPLE_RATE as f32, hz));
        self.gate = NoiseGate::new(open_db, close_db, 15);
        self.agc = Agc::new(-18.0, profile.agc_max_gain_db());
    }

    /// Share of the block's energy below [`AUTO_TILT_HZ`], 0..1.
    fn low_share(&mut self, block: &[f32]) -> f32 {
        if self.profile != NoiseProfile::Auto {
            // Nothing reads it, and the filter would run on every block of
            // every call for a number nobody asked for.
            return 0.0;
        }
        let mut low = 0.0f32;
        let mut total = 0.0f32;
        for s in block {
            let l = self.tilt.process(*s);
            low += l * l;
            total += *s * *s;
        }
        if total <= 1e-12 {
            return 0.0;
        }
        (low / total).clamp(0.0, 1.0)
    }

    /// Lets `Auto` reconsider, at most once every [`AUTO_DWELL_BLOCKS`].
    ///
    /// `floor_db` is [`Self::auto_floor`] — the background *before* the chain
    /// touches it — and passing the post-suppression floor here instead is the
    /// bug this was written with. See that field.
    ///
    /// The thresholds are a starting point and are not yet measured against a
    /// real bike. They say so here rather than only in a commit message,
    /// because whoever finds them wrong will be reading this and not that.
    fn reconsider(&mut self, floor_db: f32, low_share: f32) {
        if self.profile != NoiseProfile::Auto {
            return;
        }
        // Hysteresis applied in the direction that resists *leaving* whatever
        // is in force, so a floor sitting on a boundary stays put instead of
        // switching back and forth every five seconds for the whole ride.
        let bias = |limit: f32, quieter: NoiseProfile| -> f32 {
            if self.effective == quieter {
                limit + AUTO_HYSTERESIS_DB
            } else {
                limit - AUTO_HYSTERESIS_DB
            }
        };

        // What the level alone would choose, computed every block rather than
        // only when the dwell lets us act. The cooldown below counts how long
        // the room has wanted something lighter, and that has to be a property
        // of the room rather than of how often we happen to look at it.
        let by_level = if floor_db < bias(-55.0, NoiseProfile::Light) && low_share < 0.35 {
            NoiseProfile::Light
        } else if floor_db < bias(-40.0, NoiseProfile::Standard) {
            NoiseProfile::Standard
        } else {
            NoiseProfile::Helmet
        };

        if aggressiveness(by_level) < aggressiveness(self.effective) {
            self.auto_calm = self.auto_calm.saturating_add(1);
        } else {
            self.auto_calm = 0;
        }

        // Likewise every block. The classifier speaks every few seconds and
        // the hold is what turns its last word into a span of time.
        if self.background_noisy {
            self.music_hold = MUSIC_HOLD_BLOCKS;
        } else {
            self.music_hold = self.music_hold.saturating_sub(1);
        }

        // Escalating on the classifier does not wait for the dwell.
        //
        // The dwell exists to stop `Auto` flapping when a *level* sits on a
        // threshold, and this is not a level on a threshold — it is a model
        // saying the background changed. Making a rider who has just pulled
        // onto a motorway wait five more seconds for the profile that suits it
        // would give away the promptness the classifier was added for. Coming
        // back down is a different matter and stays slow, below.
        if self.music_hold > 0 && self.effective != NoiseProfile::Helmet {
            self.auto_dwell = 0;
            self.auto_calm = 0;
            self.apply_effective(NoiseProfile::Helmet);
            return;
        }

        self.auto_dwell = self.auto_dwell.saturating_add(1);
        if self.auto_dwell < AUTO_DWELL_BLOCKS {
            return;
        }

        // Nothing takes Helmet away while the classifier's hold is running,
        // whatever the level says. This is the second half of the 15 s rule:
        // the first half took the profile, this one keeps it.
        let want = if self.music_hold > 0 {
            NoiseProfile::Helmet
        } else {
            by_level
        };

        // Dialling down goes one step at a time, and each step has its own
        // patience: fifteen seconds to leave Helmet for Standard, then a full
        // minute in Standard before Light. Light barely suppresses anything,
        // so arriving there wrongly is the expensive mistake and it is the one
        // worth being slow about.
        let want = if aggressiveness(want) < aggressiveness(self.effective) {
            let step = one_step_lighter(self.effective);
            if self.auto_calm < cooldown_blocks(step) {
                self.effective
            } else {
                step
            }
        } else {
            want
        };

        if want != self.effective {
            self.auto_dwell = 0;
            // A profile change is a fresh start for the quiet run. Otherwise
            // one calm stretch could be spent twice, stepping down two profiles
            // on the strength of a single quiet period.
            self.auto_calm = 0;
            self.apply_effective(want);
        }
    }

    pub fn reset(&mut self) {
        self.rumble.reset();
        if let Some(band) = self.band.as_mut() {
            band.reset();
        }
        self.pitch.reset();
        self.modulation.reset();
        self.gate.reset();
        self.agc.reset();
        self.limiter.reset();
        self.floor.reset();
        self.auto_floor.reset();
        self.tilt.reset();
        self.hangover = 0;
        self.warmup = WARMUP_BLOCKS;
        // The classifier's hold does not survive the devices closing. It is a
        // claim about a place, and reopening the microphone is the one moment
        // we have no idea whether the rider is still in it.
        self.music_hold = 0;
        self.background_noisy = false;
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

    /// Sets the echo canceller's filter length, for the ladder.
    pub fn set_aec_taps(&mut self, taps: usize) {
        self.aec.set_taps(taps);
    }

    /// How long that filter is now.
    pub fn aec_taps(&self) -> usize {
        self.aec.taps()
    }

    /// Whether AEC3 is the canceller running, rather than the time-domain
    /// filter. For the decision log and the panel.
    pub fn aec_is_aec3(&self) -> bool {
        self.aec.is_aec3()
    }

    /// Whether the filter is shorter than its full length — the panel's
    /// "the ladder has been at this" flag.
    pub fn short_aec(&self) -> bool {
        self.aec.taps() < DEFAULT_TAPS
    }

    /// What the echo canceller has worked out about the path: where the echo
    /// is, how sure it is, how wide the arrivals are spread, and how much of
    /// that the filter covers. All in milliseconds except the confidence.
    ///
    /// For the diagnostics panel. A canceller that is doing nothing and one
    /// with nothing to do look identical from outside, and these four numbers
    /// are what tell them apart.
    pub fn aec_alignment(&self) -> (f32, f32, f32, f32) {
        let (lag_ms, confidence) = self.aec.alignment();
        (
            lag_ms,
            confidence,
            self.aec.measured_spread_ms(),
            self.aec.filter_span_ms(),
        )
    }

    /// Processes one block, using `reference` — the audio recently sent to the
    /// speakers — to cancel echo. `reference` may be empty or shorter.
    pub fn process_with_reference(
        &mut self,
        block: &mut [f32],
        reference: &[f32],
    ) -> BlockAnalysis {
        self.cancel_echo(block, reference);
        self.suppress(block)
    }

    /// Step 0 alone: removes what our own speakers are feeding back into the
    /// microphone. Returns the ERLE in dB.
    ///
    /// **It has to come first.** Echo is speech, so the gate and RNNoise both
    /// pass it happily and the AGC would then amplify it.
    ///
    /// Split out from [`Self::process_with_reference`] so the capture worker
    /// can hold a stopwatch over it — see [`super::timing::Stage::Echo`] for
    /// why this one stage is worth timing on its own. Callers that do not care
    /// go on using `process_with_reference`, which is these two in order.
    ///
    /// The ERLE is kept until the next call rather than returned into the
    /// analysis directly, so [`Self::suppress`] reports the figure for the
    /// block it is actually holding and the two halves cannot come apart.
    pub fn cancel_echo(&mut self, block: &mut [f32], reference: &[f32]) -> f32 {
        self.erle_db = if reference.is_empty() {
            0.0
        } else {
            self.aec.process(block, reference)
        };
        self.erle_db
    }

    /// Everything after the echo canceller: the filters, RNNoise, the pitch
    /// search, the gate, the AGC and the limiter.
    ///
    /// Expects [`Self::cancel_echo`] to have run on this same block, or not at
    /// all — the ERLE it reports is whatever that call last measured.
    pub fn suppress(&mut self, block: &mut [f32]) -> BlockAnalysis {
        debug_assert_eq!(block.len(), FRAME_SIZE);
        let erle_db = self.erle_db;

        // 0b. Where the energy sits, measured before any filter shapes it.
        //
        // Has to be here. The rumble filter's corner is 180 Hz in a helmet, so
        // a measurement taken after it would be measuring the filter that the
        // measurement chooses — the chooser would see its own last decision
        // and keep confirming it.
        let low_share = self.low_share(block);
        let raw_floor_db = if self.profile == NoiseProfile::Auto {
            self.auto_floor.update(to_dbfs(rms(block)))
        } else {
            0.0
        };

        // 1. Strip wind and engine rumble before anything else sees it, and
        // close the top of the band while we are here.
        //
        // Both before the measurements rather than on the way to the encoder,
        // and that placement is the whole value. Filtering later would take
        // the noise off the wire but leave it driving the level meter, the
        // noise floor tracker, the gate that compares one against the other,
        // and the AGC deciding how much a quiet block needs lifting. Hiss
        // above the voice would go on moving the thresholds that decide
        // whether the rider is heard.
        self.rumble.process(block);
        if let Some(band) = self.band.as_mut() {
            band.process(block);
        }

        // 2. RNNoise. It expects samples scaled to the i16 range, not -1..1.
        //
        // `skip_rnnoise` is the performance ladder giving it up, and takes the
        // same path as `Off` — which is why that path is worth having had
        // already. What is lost is the network's VAD, and the loss is smaller
        // than it looks: measured on the first unclipped ride it separates
        // transmitted blocks from untransmitted ones at AUC 0.767, against
        // 0.878 for the level the gate also has. The better feature survives.
        let vad = if self.effective == NoiseProfile::Off || self.skip_rnnoise {
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
        //
        // The first *stage* the performance ladder gives up, once the enhancer
        // has already bent as far as it will without being switched off. It
        // goes before the others because it is the cheapest quality to sell:
        // measured on the first unclipped ride, harmonicity separates
        // transmitted blocks from untransmitted ones at AUC 0.564, barely
        // better than a coin toss.
        //
        // It is not sold because it is expensive. On the OPPO it costs
        // **0.052 ms**, 13% of this function against RNNoise's 76%, so giving
        // it up buys very little — which is exactly why it goes first.
        let voice = if self.skip_pitch {
            Pitch::NONE
        } else {
            self.pitch.analyse(&self.denoised)
        };
        // `Pitch::NONE` scores zero, which would read as "certainly not
        // speech" and withhold the voiced relief below on every block. That is
        // the correct reading of *no information* here: the relief exists to
        // lift suppression where a voice is unambiguous, and without the
        // search nothing is unambiguous.
        let pitch_says_speech =
            !self.skip_pitch && voice.harmonicity >= self.effective.voiced_threshold();

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
        let mix = self.effective.denoise_mix();
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
        // **Is something speaking or singing right now?** If so the floor must
        // not climb, because a minimum statistic over a held phrase measures
        // the phrase. See `NoiseFloorTracker::update_gated`.
        //
        // The classifier is the authority when it is running: it is asked what
        // the sound *is*, and it answers `Speech` and `Singing` as well as
        // `Music`. But it only runs under `Auto`, only on platforms with the
        // model, and only every two seconds — so it cannot be the only source.
        //
        // Failing that, the chain's own two floor-independent opinions: RNNoise
        // and the pitch search. **Deliberately not the SNR term** that the
        // transmit decision also uses. SNR is measured against the floor, so
        // feeding it back in to decide whether the floor may move is a loop
        // that latches: a floor that has climbed suppresses the SNR, which
        // says "not speech", which lets it climb further.
        //
        // Either source being wrong is survivable in the direction it errs.
        // A false "voice" holds the floor low and lets some noise through,
        // which the suppressor ahead of this is there to deal with; a false
        // "no voice" is the behaviour that was already being reported.
        let voice_now = match self.classifier_voice {
            Some(said) => said,
            None => vad >= self.effective.vad_threshold() && pitch_says_speech,
        };

        // Hold the estimator back until RNNoise is producing real output.
        let noise_floor_db = if warming_up {
            self.floor.floor_db()
        } else {
            self.floor.update_gated(level_db, voice_now)
        };
        let snr_db = level_db - noise_floor_db;
        // Fed the post-suppression level, which is the envelope a listener
        // would hear rather than the one the microphone saw.
        let modulation = self.modulation.push(level_db);

        // Let Auto reconsider, from the level of the room rather than the
        // level of what the chain left of it. After the warm-up only: RNNoise
        // takes a moment to produce real output and a floor measured through
        // that is tens of dB too low.
        if !warming_up {
            self.reconsider(raw_floor_db, low_share);
        }

        let vad_says_speech = vad >= self.effective.vad_threshold();

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
        // clears however loud it gets.
        let margin_db = self.effective.snr_margin_db();
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
        } else if self.effective == NoiseProfile::Off {
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
            // Opening.
            //
            // This arm briefly also required the block not be clearly
            // aperiodic, on the argument that a gust is aperiodic and a voice
            // is not. Real helmet audio says otherwise and the veto has been
            // taken out again — see the note on `pitch` below and the
            // measurement in `core/tests/road.rs`. It was rejecting 42% of
            // labelled speech against 47% of everything else, which is not a
            // discriminator, it is a coin weighted slightly against the rider.
            // RNNoise's opinion is required only where the level is ambiguous.
            //
            // Measured, not assumed. Of the blocks a rider hand-labelled as
            // their own speech inside a helmet at speed, the VAD fired on 40%.
            // The SNR margin passed 90% of the same blocks, and by the
            // threshold-free comparison in `core/tests/road.rs` the SNR is
            // also the better feature outright — 0.77 against the VAD's 0.67,
            // where 0.50 is a coin.
            //
            // So the weaker signal was holding a veto over the stronger one,
            // and it was the binding constraint on the whole chain: loosening
            // the SNR margin changed nothing, loosening the gate changed
            // nothing, because neither was what stopped the words. Lowering
            // the VAD threshold barely helped either — the probability itself
            // is near zero on those blocks, so no threshold recovers them.
            //
            // The VAD keeps its say where it is worth having: near the floor,
            // where "loud enough" and "speech" genuinely come apart, and where
            // the drone that motivates the margin lives. Well above the floor
            // it is overruled, because a block that clears the tracked
            // background by that much is either speech or something the rider
            // would want sent anyway.
            // The override needs a second opinion, and the first attempt
            // without one was a disaster worth recording. Overruling the VAD
            // on level alone put 44-76% of synthetic engine and traffic noise
            // on the wire. The VAD's low hit rate on speech made it look like
            // the weak link; what it actually is is the only thing in the
            // chain that recognises an engine, and the SNR margin cannot help
            // because a lumpy engine note fluctuates well clear of its own
            // tracked floor. A summary statistic said the VAD was the worse
            // feature and hid that its value is concentrated exactly where the
            // other one is blind.
            //
            // So the override also asks whether the loudness has been moving
            // at a talking rate. Speech is syllables at three to eight a
            // second; an engine at throttle and tyre roar are not, whatever
            // their level does. See [`super::modulation`].
            // ...and it does not work either, so the veto stays. On real audio
            // the override recovered nothing at all — recall stayed at 59.1%,
            // because the speech blocks the VAD misses are also the ones whose
            // envelope does not look like syllables — while still leaking
            // 12-41% of synthetic engine and traffic. Worst of both.
            //
            // The conclusion is not that the VAD is good. It is that no
            // combination of the numbers this chain currently computes can
            // raise recall without opening the channel to engines, and three
            // features have now been tried: periodicity, level, and syllabic
            // modulation. Recall is bought elsewhere — see the gate below.
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
        //
        // Except with suppression off, where the audio must come through
        // untouched. Deciding not to transmit is not a licence to alter what
        // is transmitted when we do.
        let gate_level = if voice_active || self.effective == NoiseProfile::Off {
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

        // The threshold follows the background, every block.
        //
        // It used to be a fixed dBFS number per profile, and that cannot work
        // in a chain whose own output level is what it judges: the level moves
        // with the profile, and DeepFilterNet moved it again by another 7 to
        // 11 dB. Measured on two real rides, the same Helmet profile produced
        // −51.1 dBFS on one and −69.9 on the other against a threshold of −40
        // for both — throwing away 67.9% and 98.6% of the blocks the transmit
        // decision had already accepted. That is what "it cuts into words"
        // was, and it was not the decision doing it.
        //
        // `Off` keeps its fixed pair: nothing is suppressed there, the floor
        // tracker is measuring the room rather than the chain's residue, and
        // the gate must not interfere with audio that is meant to be untouched.
        if self.effective != NoiseProfile::Off {
            let open_db = noise_floor_db + self.effective.gate_margin_db();
            self.gate.open_db = open_db;
            self.gate.close_db = open_db - 8.0;
        }

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
            modulation,
            gate_open,
            agc_gain_db: self.agc.gain_db(),
            warming_up,
            effective_profile: self.effective,
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

    /// A steady tone at `hz`, generated continuously.
    fn tone(len: usize, hz: f32, amp: f32) -> Vec<f32> {
        (0..len)
            .map(|i| {
                let t = i as f32 / SAMPLE_RATE as f32;
                (2.0 * std::f32::consts::PI * hz * t).sin() * amp
            })
            .collect()
    }

    /// What survives the band filter alone, as a fraction of what went in.
    fn through_band(profile: NoiseProfile, hz: f32) -> f32 {
        let Some(cutoff) = profile.low_cutoff_hz() else {
            return 1.0;
        };
        let mut band = SpeechBand::new(SAMPLE_RATE as f32, cutoff);
        let mut signal = tone(SAMPLE_RATE as usize / 4, hz, 0.5);
        let before = rms(&signal);
        band.process(&mut signal);
        // Skip the filter's own start-up transient.
        rms(&signal[2_000..]) / before
    }

    #[test]
    fn the_band_filter_keeps_consonants_and_drops_what_is_above_them() {
        // The corner is a compromise with a wrong answer on each side. Too
        // high and the hiss it was meant to remove stays in, still counted by
        // the level meter and the floor tracker and still moving the
        // thresholds the transmit decision is made against. Too low and it
        // takes the consonants with it: "s" against "f" is decided between 4
        // and 7 kHz and nowhere else, so a filter that reaches down there does
        // not muffle speech, it makes words genuinely ambiguous.
        for profile in [
            NoiseProfile::Light,
            NoiseProfile::Standard,
            NoiseProfile::Helmet,
        ] {
            let consonant = through_band(profile, 5_000.0);
            assert!(
                consonant > 0.7,
                "{profile:?} took {:.0}% off a 5 kHz consonant",
                (1.0 - consonant) * 100.0
            );

            // Measured against each profile's own corner rather than at one
            // fixed frequency, because the corners deliberately differ by
            // nearly an octave. A fixed frequency would be asserting where the
            // corners are — a tuning decision, and one that has already moved
            // once on evidence — instead of that the filter has the slope a
            // 4th-order Butterworth has.
            let corner = profile.low_cutoff_hz().unwrap();
            let hiss = through_band(profile, corner * 1.8);
            assert!(
                hiss < 0.15,
                "{profile:?} left {:.0}% of what sits an octave above its {corner} Hz corner",
                hiss * 100.0
            );
        }
    }

    #[test]
    fn suppression_off_leaves_the_top_of_the_band_alone() {
        // Off means untouched. It is the setting a rider reaches for to find
        // out whether the chain is what is wrong, and a filter that stayed in
        // circuit would make that answer useless.
        assert_eq!(NoiseProfile::Off.low_cutoff_hz(), None);
        let mut p = CaptureProcessor::new(NoiseProfile::Off);
        let signal = tone(FRAME_SIZE * 40, 12_000.0, 0.3);
        let mut worst = 0.0f32;
        for (i, chunk) in signal.chunks_exact(FRAME_SIZE).enumerate() {
            let mut block = chunk.to_vec();
            p.process(&mut block);
            if i > 20 {
                worst = worst.max(rms(&block));
            }
        }
        assert!(
            worst > rms(&signal) * 0.5,
            "12 kHz was filtered out with suppression off"
        );
    }

    #[test]
    fn the_band_filter_takes_hiss_out_of_the_level_the_gate_judges() {
        // Why this sits before the measurements rather than on the way to the
        // encoder. Hiss above the voice is not merely wasted bandwidth: it is
        // counted by the level meter and by the noise floor tracker, so it
        // moves the threshold the rider's voice then has to clear.
        let hiss = tone(FRAME_SIZE * 60, 16_000.0, 0.35);

        let mut p = CaptureProcessor::new(NoiseProfile::Helmet);
        let mut level = -120.0f32;
        for (i, chunk) in hiss.chunks_exact(FRAME_SIZE).enumerate() {
            let mut block = chunk.to_vec();
            let a = p.process(&mut block);
            if i > 30 {
                level = level.max(a.level_db);
            }
        }
        assert!(
            level < -50.0,
            "13 kHz hiss still reached the level meter at {level} dBFS"
        );
    }

    /// Runs a signal through and reports where Auto settled.
    fn auto_lands_on(signal: &[f32]) -> NoiseProfile {
        let mut p = CaptureProcessor::new(NoiseProfile::Auto);
        for chunk in signal.chunks_exact(FRAME_SIZE) {
            let mut block = chunk.to_vec();
            p.process(&mut block);
        }
        p.effective_profile()
    }

    #[test]
    fn auto_is_never_itself_the_profile_in_force() {
        // The invariant the whole design rests on. Auto has no numbers of its
        // own — every parameter table shares Standard's row with it purely to
        // stay exhaustive — so an Auto that reached the chain would be running
        // on a fallback nobody chose.
        let mut p = CaptureProcessor::new(NoiseProfile::Auto);
        assert_ne!(p.effective_profile(), NoiseProfile::Auto);

        let noisy = crate::audio::testsig::wind(SAMPLE_RATE as usize * 12, 0.5, 3);
        for chunk in noisy.chunks_exact(FRAME_SIZE) {
            let mut block = chunk.to_vec();
            p.process(&mut block);
            assert_ne!(p.effective_profile(), NoiseProfile::Auto);
        }
    }

    #[test]
    fn auto_reaches_for_the_helmet_when_it_hears_one() {
        // Loud and bottom-heavy: wind and engine, which is what a helmet at
        // speed sounds like and nothing else does.
        let len = SAMPLE_RATE as usize * 14;
        let mut roar = crate::audio::testsig::wind(len, 0.7, 5);
        for (r, e) in roar
            .iter_mut()
            .zip(crate::audio::testsig::engine(len, 45.0, 0.6, 6))
        {
            *r = (*r + e).clamp(-1.0, 1.0);
        }
        assert_eq!(auto_lands_on(&roar), NoiseProfile::Helmet);
    }

    #[test]
    fn auto_stays_gentle_in_a_quiet_room() {
        // Quiet, and with its energy where a room's is rather than where a
        // motorway's is. Choosing Helmet here would put a 180 Hz high-pass and
        // full suppression on somebody sitting at a desk.
        let len = SAMPLE_RATE as usize * 14;
        let quiet = crate::audio::testsig::white(len, 0.0015, 9);
        assert_ne!(auto_lands_on(&quiet), NoiseProfile::Helmet);
    }

    #[test]
    fn auto_does_not_flap() {
        // Switching rebuilds the rumble filter, the band filter, the gate and
        // the AGC, and each restarts from nothing — audible as a moment of
        // unevenness. A chooser that changed its mind at every junction would
        // spend the ride doing that, which is worse than any single wrong
        // choice it might be avoiding.
        //
        // Driven at a level deliberately near a boundary, which is exactly
        // where a chooser without hysteresis oscillates.
        let len = SAMPLE_RATE as usize * 40;
        let borderline = crate::audio::testsig::wind(len, 0.02, 11);

        let mut p = CaptureProcessor::new(NoiseProfile::Auto);
        let mut changes = 0;
        let mut last = p.effective_profile();
        for chunk in borderline.chunks_exact(FRAME_SIZE) {
            let mut block = chunk.to_vec();
            p.process(&mut block);
            if p.effective_profile() != last {
                changes += 1;
                last = p.effective_profile();
            }
        }
        assert!(
            changes <= 2,
            "Auto changed its mind {changes} times in 40 seconds"
        );
    }

    /// Feeds `seconds` of a quiet room, with the classifier saying what it is
    /// told to, and reports where Auto ended up and how long the hold ran.
    fn with_classifier(seconds: usize, noisy: impl Fn(usize) -> bool) -> (NoiseProfile, bool) {
        let len = SAMPLE_RATE as usize * seconds;
        let quiet = crate::audio::testsig::white(len, 0.0015, 9);
        let mut p = CaptureProcessor::new(NoiseProfile::Auto);
        for (i, chunk) in quiet.chunks_exact(FRAME_SIZE).enumerate() {
            p.set_background_noisy(noisy(i));
            let mut block = chunk.to_vec();
            p.process(&mut block);
        }
        (p.effective_profile(), p.music_hold_active())
    }

    /// Starts on `from`, then feeds `seconds` of a quiet room and reports where
    /// `Auto` ended up.
    ///
    /// The room is quiet enough that the level wants `Light` throughout, so
    /// what the result measures is purely how long the cooldowns made it wait.
    fn dials_down_to(from: NoiseProfile, seconds: usize) -> NoiseProfile {
        let len = SAMPLE_RATE as usize * seconds;
        let quiet = crate::audio::testsig::white(len, 0.0005, 21);
        let mut p = CaptureProcessor::new(from);
        p.set_profile(NoiseProfile::Auto);
        for chunk in quiet.chunks_exact(FRAME_SIZE) {
            let mut block = chunk.to_vec();
            p.process(&mut block);
        }
        p.effective_profile()
    }

    #[test]
    fn leaving_helmet_waits_fifteen_seconds() {
        // Below the cooldown it must still be Helmet; above it, Standard. Not
        // Light: that step has its own, longer wait, and skipping straight
        // there would mean one quiet stretch spent twice.
        assert_eq!(
            dials_down_to(NoiseProfile::Helmet, 12),
            NoiseProfile::Helmet
        );
        assert_eq!(
            dials_down_to(NoiseProfile::Helmet, 25),
            NoiseProfile::Standard,
            "fifteen seconds of quiet did not get us out of Helmet"
        );
    }

    #[test]
    fn reaching_light_takes_a_full_minute() {
        // The expensive mistake. `Light` barely suppresses anything -- 73.3% of
        // music transmitted against Helmet's 13.8% -- so arriving there wrongly
        // is the one worth being slow about.
        assert_ne!(
            dials_down_to(NoiseProfile::Standard, 45),
            NoiseProfile::Light,
            "reached Light well inside its minute"
        );
        assert_eq!(
            dials_down_to(NoiseProfile::Standard, 75),
            NoiseProfile::Light,
            "a minute of quiet still did not reach Light"
        );
    }

    #[test]
    fn the_cooldown_counts_the_room_wanting_lighter_not_silence() {
        // The bug the per-step rule replaced. The old counter only ran while
        // the floor was below -55 dB, which is the bar `Light` itself has to
        // clear -- so the step from Helmet to Standard could only be taken in
        // conditions that already implied Light, and in an ordinary room it
        // was unreachable. This asserts the middle case works: a room the
        // level calls `Standard` must be able to leave `Helmet`.
        let len = SAMPLE_RATE as usize * 25;
        // Loud enough that the floor sits above the Light bar, quiet enough
        // that it sits below the Helmet one.
        let room = crate::audio::testsig::white(len, 0.006, 23);
        let mut p = CaptureProcessor::new(NoiseProfile::Helmet);
        p.set_profile(NoiseProfile::Auto);
        for chunk in room.chunks_exact(FRAME_SIZE) {
            let mut block = chunk.to_vec();
            p.process(&mut block);
        }
        assert_ne!(
            p.effective_profile(),
            NoiseProfile::Helmet,
            "stuck in Helmet in a room the level does not call a helmet"
        );
    }

    #[test]
    fn the_classifier_takes_helmet_without_waiting_for_the_dwell() {
        // Under a second of a quiet room, which on level alone is the last
        // place Auto would choose Helmet. The dwell is 5 s and would have made
        // a rider joining a motorway wait it out; escalating is the half of
        // this that should be prompt.
        let (profile, holding) = with_classifier(1, |_| true);
        assert_eq!(profile, NoiseProfile::Helmet);
        assert!(holding);
    }

    #[test]
    fn helmet_is_held_for_fifteen_seconds_after_the_music_stops() {
        // The rule as asked for. Ten seconds of noisy, then silence from the
        // classifier: at +10 s the hold is still running, and it is still
        // running at +14 s.
        let blocks_per_second = SAMPLE_RATE as usize / FRAME_SIZE;
        let stop = 10 * blocks_per_second;

        let (profile, holding) = with_classifier(20, |i| i < stop);
        assert_eq!(profile, NoiseProfile::Helmet, "let go far too early");
        assert!(holding, "the hold should still have five seconds to run");

        // And it does expire: at +16 s the hold is done. The profile may well
        // still be Helmet, because the calm ratchet needs its own 15 s of
        // quiet before anything is allowed to lighten -- the two compose, and
        // that is the intended floor rather than an accident.
        let (_, still_holding) = with_classifier(26, |i| i < stop);
        assert!(!still_holding, "the hold outlived its fifteen seconds");
    }

    #[test]
    fn the_classifier_cannot_choose_a_lighter_profile() {
        // It is a supporting vote for Helmet and nothing else. A classifier
        // that could talk Auto *down* would be able to strip suppression off a
        // rider at speed on the strength of one wrong inference, which is the
        // failure this is not allowed to have.
        let len = SAMPLE_RATE as usize * 14;
        let mut roar = crate::audio::testsig::wind(len, 0.7, 5);
        for (r, e) in roar
            .iter_mut()
            .zip(crate::audio::testsig::engine(len, 45.0, 0.6, 6))
        {
            *r = (*r + e).clamp(-1.0, 1.0);
        }

        let mut p = CaptureProcessor::new(NoiseProfile::Auto);
        for chunk in roar.chunks_exact(FRAME_SIZE) {
            p.set_background_noisy(false); // "all clear", continuously
            let mut block = chunk.to_vec();
            p.process(&mut block);
        }
        assert_eq!(
            p.effective_profile(),
            NoiseProfile::Helmet,
            "a quiet verdict overrode a loud road"
        );
    }

    #[test]
    fn the_classifier_is_ignored_unless_auto_is_chosen() {
        // A profile someone picked by hand is an instruction. `reconsider`
        // returns early for every setting but Auto, and this holds it to that
        // now that something else can call for Helmet.
        let mut p = CaptureProcessor::new(NoiseProfile::Light);
        let len = SAMPLE_RATE as usize * 3;
        let quiet = crate::audio::testsig::white(len, 0.0015, 9);
        for chunk in quiet.chunks_exact(FRAME_SIZE) {
            p.set_background_noisy(true);
            let mut block = chunk.to_vec();
            p.process(&mut block);
        }
        assert_eq!(p.effective_profile(), NoiseProfile::Light);
        assert!(!p.music_hold_active());
    }

    #[test]
    fn choosing_auto_keeps_what_was_already_in_force() {
        // A rider switching Auto on is listening at that moment. Snapping to a
        // default would throw away a correct answer precisely then.
        let mut p = CaptureProcessor::new(NoiseProfile::Helmet);
        assert_eq!(p.effective_profile(), NoiseProfile::Helmet);
        p.set_profile(NoiseProfile::Auto);
        assert_eq!(p.effective_profile(), NoiseProfile::Helmet);
    }

    #[test]
    fn the_tilt_filter_does_not_run_unless_auto_is_selected() {
        // It is a biquad over every sample of every block, for a number that
        // only Auto reads.
        let mut p = CaptureProcessor::new(NoiseProfile::Helmet);
        let signal = tone(FRAME_SIZE, 100.0, 0.5);
        assert_eq!(p.low_share(&signal), 0.0);

        let mut p = CaptureProcessor::new(NoiseProfile::Auto);
        assert!(
            p.low_share(&signal) > 0.5,
            "a 100 Hz tone should be almost entirely below the 300 Hz split"
        );
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
