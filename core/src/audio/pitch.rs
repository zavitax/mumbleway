//! Is this a voice, or is it merely loud?
//!
//! Every stage of the capture chain judges the microphone by **level**, and
//! level cannot separate the two things a rider needs separated. A gust is
//! loud. A row of idling motorcycles is loud. A rider speaking quietly inside
//! a helmet at 120 km/h is not much louder than either, and may be quieter.
//! Tune a level-based gate tight enough to reject the wind and it rejects the
//! rider too; tune it loose enough to pass the rider and the wind goes out on
//! the channel. There is no setting that does both, which is why the chain
//! fails in *both* directions at once rather than in one.
//!
//! So this measures something level cannot see: whether the block is periodic,
//! and periodic at a rate a human throat produces.
//!
//! # Why the range is the whole of it
//!
//! Plain "is it tonal" would be worse than useless here, because the loudest
//! thing a motorcyclist sits next to is a large single-cylinder engine, which
//! is *extremely* periodic. A tonality check calls that a voice with more
//! confidence than it calls a whisper one.
//!
//! What separates them is where the period sits. Human f0 runs roughly
//! 75–350 Hz. A motorcycle's firing fundamental is its crank speed divided by
//! the cylinder count — 30–60 Hz at idle, and even at 6000 rpm a twin fires at
//! 100 Hz with almost all its energy in the first two harmonics, not at a
//! pitch. Searching *only* 75–350 Hz rejects the engine by construction rather
//! than by a threshold somebody has to keep re-tuning: its period is outside
//! the range being looked at, so its correlation is never computed.
//!
//! Wind fails the same test for the opposite reason. Filtered noise has no
//! period anywhere, so nothing in the range correlates.
//!
//! # What it deliberately does not claim
//!
//! Unvoiced speech has no pitch at all. "s", "f", "sh", "th" and a whisper are
//! turbulence, and score near zero here — correctly, because they *are*
//! aperiodic. Anything using this as a gate has to understand that a low score
//! means "no evidence of voicing", not "evidence of no voice", and must never
//! be the only thing holding a transmission open. See the transmit decision in
//! [`super::denoise`], which requires this to *start* and does not require it
//! to continue.
//!
//! Music is not rejected by this and cannot be. It is harmonic, it sits inside
//! the human range, and it is modulated at close to speech rhythm. That is a
//! known and measured gap — see `core/tests/suppression.rs`.

/// Rate the search runs at.
///
/// 48 kHz divides by exactly 6, so decimation needs no resampler. Nothing is
/// lost: the highest pitch searched for is 350 Hz, a very long way below the
/// 4 kHz this leaves.
const RATE: usize = 8_000;
const DECIMATE: usize = 6;

/// 350 Hz, the top of the human range.
const MIN_LAG: usize = 23;
/// 75 Hz, the bottom of it.
const MAX_LAG: usize = 107;

/// Samples compared at each lag. 128 at 8 kHz is 16 ms, which spans at least
/// one period of even the lowest pitch searched for.
const WINDOW: usize = 128;

/// Enough history for the longest lag plus the window that follows it.
const HISTORY: usize = WINDOW + MAX_LAG;

/// Below this, a lag is periodic enough to stop looking and take it.
///
/// The other half of the octave remedy: a multiple of the true period matches
/// nearly as well as the period itself, so taking the plain best lands an
/// octave down often enough to matter. Taking the *first* lag that is good
/// enough takes the fundamental. 0.3 is the value the method was published
/// with and there is no reason here to differ.
const GOOD_ENOUGH: f32 = 0.30;

/// What the pitch search found in one block.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pitch {
    /// How strongly periodic the block is at a human pitch, 0..1.
    ///
    /// Not a probability and not calibrated against anything: it is the
    /// normalised autocorrelation at the best lag in range, which for clean
    /// voiced speech runs above 0.8, for a whisper or a fricative near 0.1,
    /// and for wind or an idling engine near zero.
    pub harmonicity: f32,
    /// The pitch that produced it, in Hz, or 0 when there is none worth
    /// reporting.
    pub f0_hz: f32,
}

impl Pitch {
    pub const NONE: Self = Self {
        harmonicity: 0.0,
        f0_hz: 0.0,
    };
}

/// Tracks pitch across blocks.
///
/// It has to be stateful. One 10 ms block is 80 samples once decimated, and
/// the longest lag being searched for is 107 — a block on its own is shorter
/// than the period it is looking for. The history is what makes the question
/// answerable at all.
pub struct PitchTracker {
    history: [f32; HISTORY],
    /// Samples of real audio in the history, so a fresh tracker does not
    /// report the periodicity of its own zeros.
    filled: usize,
}

impl Default for PitchTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl PitchTracker {
    pub fn new() -> Self {
        Self {
            history: [0.0; HISTORY],
            filled: 0,
        }
    }

    pub fn reset(&mut self) {
        self.history = [0.0; HISTORY];
        self.filled = 0;
    }

    /// Adds a block and reports what the history now looks like.
    ///
    /// `block` is at 48 kHz; any length is accepted, though the chain always
    /// hands over 10 ms.
    pub fn analyse(&mut self, block: &[f32]) -> Pitch {
        self.push(block);
        if self.filled < HISTORY {
            return Pitch::NONE;
        }

        // The most recent window, compared against itself one lag earlier.
        let recent = &self.history[HISTORY - WINDOW..];
        let energy: f32 = recent.iter().map(|s| s * s).sum();
        if energy <= 1e-9 {
            return Pitch::NONE;
        }

        // Squared difference at each lag, rather than a correlation.
        //
        // The first attempt here used plain normalised autocorrelation, and it
        // scored wind at 0.72 — as periodic as a voice. That is not a fault in
        // the wind generator: filtered noise is *smooth*, and a smooth signal
        // genuinely does resemble itself three milliseconds later. Correlation
        // asks "do these look alike", and at short lags on a low-passed signal
        // the answer is honestly yes.
        let mut diff = [0.0f32; MAX_LAG + 1];
        for (lag, d) in diff.iter_mut().enumerate().take(MAX_LAG + 1).skip(1) {
            let past = &self.history[HISTORY - WINDOW - lag..HISTORY - lag];
            let mut sum = 0.0f32;
            for i in 0..WINDOW {
                let e = recent[i] - past[i];
                sum += e * e;
            }
            *d = sum;
        }

        // Divided by the running mean of every shorter lag, which is what
        // separates the two cases. A periodic signal's difference collapses to
        // near zero at its period and nowhere else, so the ratio dives. Noise
        // that merely drifts has a difference that grows steadily with the lag
        // — it is a random walk — so its ratio sits flat near or above 1 at
        // every lag and never dives anywhere. The absolute values were similar;
        // the shapes are not, and this measures the shape.
        let mut running = 0.0f32;
        let mut ratio = [1.0f32; MAX_LAG + 1];
        for lag in 1..=MAX_LAG {
            running += diff[lag];
            if running > 1e-12 {
                ratio[lag] = diff[lag] * lag as f32 / running;
            }
        }

        // The first lag in range that is good enough, or the best if none is.
        let mut chosen = MIN_LAG;
        let mut lowest = f32::MAX;
        for (lag, r) in ratio.iter().enumerate().take(MAX_LAG + 1).skip(MIN_LAG) {
            if *r < lowest {
                lowest = *r;
                chosen = lag;
            }
            if *r < GOOD_ENOUGH {
                chosen = lag;
                lowest = *r;
                break;
            }
        }

        Pitch {
            harmonicity: (1.0 - lowest).clamp(0.0, 1.0),
            f0_hz: RATE as f32 / chosen as f32,
        }
    }

    /// Decimates a 48 kHz block into the history, oldest first.
    fn push(&mut self, block: &[f32]) {
        // A short moving average before dropping samples. Crude as an
        // anti-alias filter and entirely adequate: its first null sits at
        // 8 kHz, and everything this looks for is below 350 Hz, so the droop
        // it causes in between is on harmonics that are not being counted.
        let taken = block.len() / DECIMATE;
        if taken == 0 {
            return;
        }
        if taken >= HISTORY {
            // A block longer than the whole history: keep only its tail.
            for i in 0..HISTORY {
                let start = (taken - HISTORY + i) * DECIMATE;
                self.history[i] = mean(&block[start..start + DECIMATE]);
            }
            self.filled = HISTORY;
            return;
        }

        self.history.copy_within(taken.., 0);
        for i in 0..taken {
            let start = i * DECIMATE;
            self.history[HISTORY - taken + i] = mean(&block[start..start + DECIMATE]);
        }
        self.filled = (self.filled + taken).min(HISTORY);
    }
}

fn mean(x: &[f32]) -> f32 {
    x.iter().sum::<f32>() / x.len() as f32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::testsig;

    const BLOCK: usize = 480;

    /// Runs a signal through in 10 ms blocks and reports the median
    /// harmonicity, ignoring the blocks spent filling the history.
    fn median_harmonicity(signal: &[f32]) -> f32 {
        let mut tracker = PitchTracker::new();
        let mut scores = Vec::new();
        for (i, block) in signal.chunks_exact(BLOCK).enumerate() {
            let p = tracker.analyse(block);
            if i >= 5 {
                scores.push(p.harmonicity);
            }
        }
        scores.sort_by(|a, b| a.partial_cmp(b).unwrap());
        scores[scores.len() / 2]
    }

    fn median_f0(signal: &[f32]) -> f32 {
        let mut tracker = PitchTracker::new();
        let mut found = Vec::new();
        for (i, block) in signal.chunks_exact(BLOCK).enumerate() {
            let p = tracker.analyse(block);
            if i >= 5 && p.harmonicity > 0.6 {
                found.push(p.f0_hz);
            }
        }
        assert!(!found.is_empty(), "nothing was voiced enough to measure");
        found.sort_by(|a, b| a.partial_cmp(b).unwrap());
        found[found.len() / 2]
    }

    #[test]
    fn a_voice_scores_high() {
        for f0 in [95.0f32, 130.0, 210.0, 300.0] {
            let s = testsig::speech(48_000 * 2, f0, 0.4);
            let score = median_harmonicity(&s);
            assert!(score > 0.6, "speech at {f0} Hz scored only {score}");
        }
    }

    #[test]
    fn the_pitch_it_reports_is_the_pitch_that_is_there() {
        // The octave rule earns its place here: without it a good half of
        // these come back at half the true pitch, which would send the Helmet
        // comb filter to the wrong harmonics.
        for f0 in [95.0f32, 130.0, 210.0] {
            let s = testsig::speech(48_000 * 2, f0, 0.4);
            let measured = median_f0(&s);
            assert!(
                (measured - f0).abs() < f0 * 0.12,
                "a {f0} Hz voice was measured at {measured} Hz"
            );
        }
    }

    #[test]
    fn wind_scores_low() {
        // The case the whole thing exists for. Loud, broadband, and with no
        // period anywhere for the search to lock on to.
        for seed in [1u64, 40, 900] {
            let w = testsig::wind(48_000 * 2, 0.8, seed);
            let score = median_harmonicity(&w);
            assert!(score < 0.35, "wind (seed {seed}) scored {score}");
        }
    }

    #[test]
    fn an_engine_scores_low_however_periodic_it_is() {
        // The reason this is a *pitch-constrained* search and not a tonality
        // check. An idling single is more periodic than most speech; what
        // makes it not a voice is that its period is nowhere near a human one.
        for fundamental in [30.0f32, 42.0, 58.0] {
            let e = testsig::engine(48_000 * 2, fundamental, 0.8, 5);
            let score = median_harmonicity(&e);
            assert!(
                score < 0.5,
                "an engine firing at {fundamental} Hz scored {score}"
            );
        }
    }

    #[test]
    fn several_bikes_at_a_junction_score_low() {
        for seed in [2u64, 77] {
            let t = testsig::traffic(48_000 * 2, 0.8, seed);
            let score = median_harmonicity(&t);
            assert!(score < 0.5, "traffic (seed {seed}) scored {score}");
        }
    }

    #[test]
    fn a_voice_beats_the_noise_it_is_next_to() {
        // Absolute thresholds are a tuning decision that belongs to the
        // profile. What has to hold here, and what everything downstream
        // depends on, is the *ordering*: whatever threshold is picked, speech
        // must be on one side of it and weather on the other.
        let speech = median_harmonicity(&testsig::speech(48_000 * 2, 130.0, 0.4));
        let wind = median_harmonicity(&testsig::wind(48_000 * 2, 0.8, 3));
        let engine = median_harmonicity(&testsig::engine(48_000 * 2, 42.0, 0.8, 4));
        assert!(
            speech > wind + 0.25 && speech > engine + 0.25,
            "speech {speech}, wind {wind}, engine {engine}"
        );
    }

    #[test]
    fn a_voice_still_scores_over_wind_it_is_buried_in() {
        // The condition that matters on the road, and the one a level-based
        // decision cannot answer: the rider is talking *while* the wind is
        // blowing, not instead of it.
        let speech = testsig::speech(48_000 * 3, 130.0, 0.5);
        for snr_db in [6.0f32, 0.0] {
            let noisy = testsig::mix(&speech, &testsig::wind(speech.len(), 1.0, 11), snr_db);
            let score = median_harmonicity(&noisy);
            assert!(
                score > 0.35,
                "a voice at {snr_db} dB over wind scored only {score}"
            );
        }
    }

    #[test]
    fn silence_reports_nothing_rather_than_a_confident_guess() {
        let mut tracker = PitchTracker::new();
        let quiet = vec![0.0f32; BLOCK];
        for _ in 0..20 {
            assert_eq!(tracker.analyse(&quiet), Pitch::NONE);
        }
    }

    #[test]
    fn the_first_blocks_report_nothing_rather_than_the_periodicity_of_zeros() {
        // A fresh tracker's history is silence, and silence next to itself
        // correlates perfectly. Reporting that would open the gate on the
        // first breath of every connection.
        let mut tracker = PitchTracker::new();
        let s = testsig::speech(48_000, 130.0, 0.4);
        let first = tracker.analyse(&s[..BLOCK]);
        assert_eq!(first, Pitch::NONE);
    }
}
