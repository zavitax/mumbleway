//! Recognising a deliberate tap on the phone, from a pocket, on a motorcycle.
//!
//! **The thresholds here are provisional and are the whole of what the rig has
//! to settle.** Every constant below is marked, and none was chosen by
//! listening or by argument — they are starting points for
//! `tools/tap/`, which scores candidates against hand-labelled rides the way
//! `tools/vad/` scored the voice gate. The number that decides shippability is
//! **false arms per hour on held-out rides**, and nothing in this file can tell
//! you what it is.
//!
//! # Teager–Kaiser, and why
//!
//! ψ[x(n)] = x(n)² − x(n+1)·x(n−1). Two multiplies, no window, no state beyond
//! two samples, one sample of latency. For a sinusoid it evaluates to roughly
//! A²·sin²(ω), so it weights by **amplitude squared times frequency squared** —
//! which is the entire reason to prefer it to a plain derivative here. Engine
//! rumble, road undulation and a rider shifting their leg are low-frequency; a
//! tap is a broadband impulse. The ω² term does the discrimination a filter
//! would otherwise need, with no phase delay and nothing to tune.
//!
//! It is also the operator the literature validated for exactly this —
//! extracting tap events from noisy accelerometer signals — against discrete
//! wavelet transforms.
//!
//! **Its advantage is weakest where it is needed most**, and that has to be
//! said plainly: iOS delivers at 100 Hz, so a tap's high-frequency content is
//! above Nyquist and aliased. ψ still spikes on the step, but the frequency
//! weighting buys far less than it does at Android's 400. Whether tap-to-start
//! is viable on iOS at all is the first question for the rig.
//!
//! # The floor, not a threshold
//!
//! The level ψ has to clear **rides a tracked floor**, because the floor rises
//! with engine speed and road roughness exactly as the audio floor rises with
//! wind. A constant would be deaf at 120 km/h or hair-triggered at idle, which
//! is the same lesson [`NoiseFloorTracker`] was built for — so it is reused
//! rather than re-derived, and the cadence happens to line up: iOS sensors run
//! at 100 Hz, one sample per 10 ms, the same rate as an audio block.
//!
//! # What actually discriminates
//!
//! Taps are accepted on the front, the back and at an angle, because a phone in
//! a pocket shifts and keeps no orientation. So **no axis can be learned or
//! required**, and the per-axis sum below is what makes this
//! orientation-agnostic in the first place. That trades away what would
//! otherwise be the strongest single feature, and the load moves onto:
//!
//! 1. **Impulsiveness** — ψ above the floor, *and back below it within a
//!    maximum pulse width*. That "up and quickly down again" test is taken
//!    verbatim from the MEMS vendors' own tap detectors (ST's DT0101, NXP's
//!    AN3919) and it is what rejects large slow motions, which is most of what
//!    a thigh produces.
//! 2. **The pattern** — N candidates with **matched magnitudes** and, from
//!    three upwards, **consistent intervals**. Road input arrives as
//!    uncorrelated singles or long irregular trains, never as N matched
//!    impulses on an even beat. This is the primary discriminator, and it is
//!    why the rider chooses 2, 3 or 4: if single-impulse false alarms arrive at
//!    rate λ and the window is W, the chance of N landing in the right lattice
//!    falls roughly as (λW)^(N−1), so each extra tap cuts false starts
//!    multiplicatively rather than incrementally.
//! 3. **Direction, as a weighting and never a veto.** Gravity gives
//!    world-vertical in device coordinates; road shock arrives along it and a
//!    hand reaching to a thigh moves across it. But a bag sitting on top of the
//!    thigh is tapped downward and an angled tap splits its energy, so this
//!    only ever adjusts confidence.
//!
//! Two taps carry **no** interval evidence at all — one interval has nothing to
//! be compared against — which is why three is the defensible default once the
//! feature is switched on.

use super::dsp::NoiseFloorTracker;
use super::record::MotionSample;

/// Sub-windows of the floor tracker, matching the voice gate's ~1.5 s memory.
const FLOOR_SUB_BLOCKS: u32 = 25;

/// The lowest ψ the floor is allowed to believe in, in dB.
///
/// **This is not a tuning constant and it is not provisional.** It repairs a
/// design fault that made the detector useless and worse than useless, and the
/// reasoning is the sensor's, not anybody's taste.
///
/// [`NoiseFloorTracker`] is *minimum statistics*, and it was borrowed from the
/// voice gate, where the quantity it tracks is audio with dither and a real
/// noise floor under it. ψ is not like that. ψ[x] = x(n)² − x(n+1)·x(n−1) is
/// **identically zero whenever three consecutive samples are equal**, and a
/// quantised accelerometer lying still reports runs of equal samples as a
/// matter of course. So ψ is exactly 0 for much of any stationary stretch,
/// `10·log10(0 + 1e-12)` is −120 dB, and the minimum statistic parks in a
/// basement that no real signal ever visits.
///
/// From there two documented behaviours of the tracker finish the job:
/// *down is instant* (`dsp.rs`), so every recurrence of an exact zero yanks the
/// published floor straight back to the basement, while the climb out is capped
/// at `MAX_RISE_DB_PER_BLOCK` — 0.06 dB **per sample**, which is a rate meant
/// for 10 ms audio blocks and is slower still here — and only on the samples
/// where the candidate gate is not holding it.
///
/// The result, measured on a phone lying motionless: `ψ -68.0 dB ·
/// floor -117.0 dB · over 49.0 dB`. Everything is a candidate for ever, 1626
/// impulses were discarded as "too long" in 160 seconds, and four of them
/// lined up well enough to complete **gestures nobody performed** — about
/// ninety false arms an hour against a target of under one.
///
/// −70 dB is one quantisation step. A ±2 g accelerometer over 16 bits has an
/// LSB near 2.4e-4 m/s²; ψ for a one-LSB dither is about (2.4e-4)² ≈ 6e-8,
/// which is −72 dB. **Below one LSB there is no signal to measure**, so the
/// floor has no business going there, and a tap — which runs tens of dB above
/// this — is unaffected.
const QUANTISATION_DB: f32 = -70.0;

/// How far above the floor ψ must reach. **Provisional.**
const MARGIN_DB: f32 = 12.0;

/// The longest a candidate may stay above the floor and still be a tap.
///
/// **Provisional.** The vendors' "up and back down quickly" test: a thigh
/// produces large slow motions and this is what rejects them.
const MAX_PULSE_MS: u32 = 60;

/// Dead time after a candidate, to skip the mechanical ring.
///
/// **Provisional.** A phone in a padded bag rings for tens of milliseconds
/// after a strike, and without this the ring is counted as the next tap.
const LATENT_MS: u32 = 70;

/// The window in which the next tap of the gesture must arrive.
///
/// **Provisional.** Deliberate human tapping sits well inside this; the upper
/// bound is what stops two unrelated road impulses a second apart pairing up.
const GAP_MIN_MS: u32 = 80;
const GAP_MAX_MS: u32 = 400;

/// How unlike each other two taps of one gesture may be, as a ratio.
///
/// **Provisional.** Matched magnitude is nearly free and strong: road input
/// arrives as singles or irregular trains, not as pairs that resemble each
/// other.
const MAGNITUDE_TOLERANCE: f32 = 2.5;

/// How much the intervals of a gesture may differ, as a ratio.
///
/// **Provisional, and only meaningful from three taps up.**
const INTERVAL_TOLERANCE: f32 = 1.8;

/// One impulse that passed the pulse-width test.
#[derive(Clone, Copy, Debug)]
struct Candidate {
    /// When it peaked, in milliseconds on the arrival clock.
    at_ms: u64,
    /// Peak ψ above the floor, in dB.
    strength_db: f32,
}

/// What the detector is seeing, for the diagnostics panel and for the rig.
///
/// **Exists because the first field report was "taps are not detected" with no
/// way to tell which half was at fault** — no samples arriving at all, or
/// samples arriving and the thresholds rejecting them. Those need completely
/// different fixes and look identical from outside.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TapStats {
    /// Samples pushed since the detector was made. Zero means the sensors are
    /// not delivering, which is a platform problem and not a tuning one.
    pub samples: u64,
    /// Impulses that passed the pulse-width test.
    pub candidates: u64,
    /// Impulses rejected for staying above the floor **too long**.
    ///
    /// The single most useful number here. A tap through a padded bag rings,
    /// and if it rings past [`MAX_PULSE_MS`] every tap is thrown away and the
    /// feature looks dead while the signal is perfectly good.
    pub discarded_long: u64,
    /// Candidates dropped for not matching the one before them in magnitude.
    pub discarded_magnitude: u64,
    /// Candidates dropped for an interval unlike the previous one.
    pub discarded_interval: u64,
    /// Completed gestures.
    pub gestures: u64,
    /// The operator's output for the last sample, in dB.
    pub psi_db: f32,
    /// The tracked floor it is measured against.
    pub floor_db: f32,
    /// The peak of the candidate in progress, or the last one, above the floor.
    pub peak_db: f32,
    /// How many taps of a gesture are banked.
    pub pending: u8,
    /// Arrival stamps of the first and last samples, for working out the rate.
    pub first_us: u64,
    pub last_us: u64,
}

/// A completed gesture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TapGesture {
    /// How many taps it was made of.
    pub taps: u8,
    /// When the last of them landed, on the arrival clock.
    pub at_ms: u64,
}

/// Watches motion for a deliberate N-tap gesture.
///
/// Fed every sample; answers `Some` on the sample that completes one. Holds no
/// allocation after construction and does no work beyond a handful of
/// multiplies per sample, because it runs for the whole of a ride.
pub struct TapDetector {
    taps_wanted: u8,
    floor: NoiseFloorTracker,
    /// Three samples per axis, for the operator's own lookbehind.
    ///
    /// Per axis rather than on the magnitude, which is not the same operation:
    /// the magnitude is a square root of a sum, so taking ψ of it mixes the
    /// axes *before* the non-linearity and an impulse split across two of them
    /// partly cancels. Applying ψ per axis and summing afterwards keeps a tap
    /// arriving at an angle as loud as one square on a face, which is what
    /// supporting front, back and angled taps requires.
    history: [[f32; 3]; 3],
    seen: u8,
    /// Whether ψ is currently above the line, and when it crossed.
    above_since_ms: Option<u64>,
    peak_db: f32,
    /// The candidates of a gesture in progress. Four is the most ever wanted.
    pending: [Option<Candidate>; 4],
    pending_len: usize,
    /// When the dead time after the last candidate expires.
    latent_until_ms: u64,
    stats: TapStats,
}

impl TapDetector {
    pub fn new(taps_wanted: u8) -> Self {
        Self {
            taps_wanted: taps_wanted.clamp(2, 4),
            floor: NoiseFloorTracker::new(FLOOR_SUB_BLOCKS),
            history: [[0.0; 3]; 3],
            seen: 0,
            above_since_ms: None,
            peak_db: f32::NEG_INFINITY,
            pending: [None; 4],
            pending_len: 0,
            latent_until_ms: 0,
            stats: TapStats::default(),
        }
    }

    /// What it is seeing. Cheap; called once a second by the panel.
    pub fn stats(&self) -> TapStats {
        self.stats
    }

    /// Changes the gesture length, discarding anything half-performed.
    pub fn set_taps_wanted(&mut self, taps: u8) {
        let taps = taps.clamp(2, 4);
        if taps == self.taps_wanted {
            return;
        }
        self.taps_wanted = taps;
        self.pending_len = 0;
    }

    /// The tracked floor, in dB, for the diagnostics panel and the rig.
    pub fn floor_db(&self) -> f32 {
        self.floor.floor_db()
    }

    /// Teager–Kaiser summed across the axes.
    ///
    /// Summed rather than taken on the magnitude, so a tap arriving on any face
    /// counts the same — which is what lets the gesture be performed on the
    /// front, the back or at an angle.
    fn psi(&self) -> f32 {
        // x(n)² − x(n+1)·x(n−1) per axis, with each row holding
        // [n-1, n, n+1], then summed. Absolute values, because the operator is
        // an energy and a negative one is an artefact of a two-sample window
        // rather than a direction.
        (0..3)
            .map(|k| {
                let h = &self.history[k];
                (h[1] * h[1] - h[2] * h[0]).abs()
            })
            .sum()
    }

    pub fn push(&mut self, sample: &MotionSample) -> Option<TapGesture> {
        for k in 0..3 {
            self.history[k][0] = self.history[k][1];
            self.history[k][1] = self.history[k][2];
            self.history[k][2] = sample.accel[k];
        }
        if self.seen < 3 {
            self.seen += 1;
            return None;
        }

        self.stats.samples += 1;
        if self.stats.first_us == 0 {
            self.stats.first_us = sample.arrival_us;
        }
        self.stats.last_us = sample.arrival_us;

        let psi = self.psi();
        // dB on an arbitrary but consistent reference. The floor tracker works
        // in dB because minimum statistics over a logarithmic level is what it
        // was built for, and ψ spans orders of magnitude.
        //
        // **Clamped at the bottom, and that clamp is load-bearing** — see
        // [`QUANTISATION_DB`]. Without it the first field test of this detector
        // read `over 49.0 dB` on a phone lying perfectly still.
        let level_db = (10.0 * (psi + 1e-12).log10()).max(QUANTISATION_DB);
        let at_ms = sample.arrival_us / 1_000;

        // The floor must not learn the tap. `update_gated` already exists for
        // exactly this shape of problem — a held signal pulling the minimum up
        // onto itself — and "a candidate is in progress" is the gate.
        //
        // **But only while the run could still be a tap.** Gating on
        // `above_since_ms.is_some()` alone lets anything sustained above the
        // margin hold the floor down indefinitely, and the floor is the only
        // thing that would otherwise rise to meet it and end the condition.
        // That is a latch, and it is the shape of the fault [`QUANTISATION_DB`]
        // describes — on a bike it would arrive instead as engine vibration
        // clearing the margin and the detector never recovering for the rest of
        // the ride. Past [`MAX_PULSE_MS`] the run is by definition not a tap,
        // so the floor is allowed to learn it.
        let in_candidate = self
            .above_since_ms
            .is_some_and(|since| at_ms.saturating_sub(since) <= MAX_PULSE_MS as u64);
        let floor_db = self.floor.update_gated(level_db, in_candidate);
        let over = level_db - floor_db;
        self.stats.psi_db = level_db;
        self.stats.floor_db = floor_db;

        match self.above_since_ms {
            None => {
                if over >= MARGIN_DB {
                    self.above_since_ms = Some(at_ms);
                    self.peak_db = over;
                }
            }
            Some(since) => {
                self.peak_db = self.peak_db.max(over);
                if over >= MARGIN_DB {
                    // Still above. Too long above is a motion, not a tap.
                    if at_ms.saturating_sub(since) > MAX_PULSE_MS as u64 {
                        self.above_since_ms = None;
                        self.stats.peak_db = self.peak_db;
                        self.stats.discarded_long += 1;
                        self.peak_db = f32::NEG_INFINITY;
                    }
                } else {
                    // Came back down inside the pulse width: a candidate.
                    let strength_db = self.peak_db;
                    self.above_since_ms = None;
                    self.peak_db = f32::NEG_INFINITY;
                    self.stats.peak_db = strength_db;
                    if at_ms >= self.latent_until_ms {
                        self.latent_until_ms = at_ms + LATENT_MS as u64;
                        self.stats.candidates += 1;
                        return self.accept(Candidate {
                            at_ms: since,
                            strength_db,
                        });
                    }
                }
            }
        }
        None
    }

    /// Folds a candidate into the gesture in progress.
    fn accept(&mut self, c: Candidate) -> Option<TapGesture> {
        // Anything too old to be part of this gesture is dropped first, so a
        // candidate an hour later starts a gesture rather than completing one.
        if let Some(last) = self.pending[self.pending_len.saturating_sub(1)] {
            if self.pending_len > 0 && c.at_ms.saturating_sub(last.at_ms) > GAP_MAX_MS as u64 {
                self.pending_len = 0;
            }
        }

        if self.pending_len == 0 {
            self.pending[0] = Some(c);
            self.pending_len = 1;
            self.stats.pending = 1;
            return None;
        }

        let last = self.pending[self.pending_len - 1]?;
        let gap = c.at_ms.saturating_sub(last.at_ms);
        if gap < GAP_MIN_MS as u64 {
            // Too soon even for the dead time to have caught: treat it as part
            // of the same strike rather than a new tap.
            return None;
        }

        // Matched magnitude. Two impulses of wildly different strength are a
        // road feature and a tap, not one gesture.
        let ratio = ratio_of(
            c.strength_db.abs().max(1.0),
            last.strength_db.abs().max(1.0),
        );
        if ratio > MAGNITUDE_TOLERANCE {
            self.stats.discarded_magnitude += 1;
            self.pending[0] = Some(c);
            self.pending_len = 1;
            self.stats.pending = 1;
            return None;
        }

        // Consistent intervals, once there is more than one to compare. This
        // is the evidence two taps cannot carry at all.
        if self.pending_len >= 2 {
            let prev = self.pending[self.pending_len - 2]?;
            let previous_gap = last.at_ms.saturating_sub(prev.at_ms).max(1);
            if ratio_of(gap.max(1) as f32, previous_gap as f32) > INTERVAL_TOLERANCE {
                self.stats.discarded_interval += 1;
                self.pending[0] = Some(c);
                self.pending_len = 1;
                self.stats.pending = 1;
                return None;
            }
        }

        self.pending[self.pending_len] = Some(c);
        self.pending_len += 1;
        self.stats.pending = self.pending_len as u8;

        if self.pending_len >= self.taps_wanted as usize {
            self.pending_len = 0;
            self.stats.pending = 0;
            self.stats.gestures += 1;
            return Some(TapGesture {
                taps: self.taps_wanted,
                at_ms: c.at_ms,
            });
        }
        None
    }
}

/// The larger of two positives over the smaller, so order does not matter.
fn ratio_of(a: f32, b: f32) -> f32 {
    if a <= 0.0 || b <= 0.0 {
        return f32::INFINITY;
    }
    if a > b {
        a / b
    } else {
        b / a
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 100 Hz, the rate iOS gives and therefore the one that has to work.
    const STEP_US: u64 = 10_000;

    fn sample(i: u64, accel: f32) -> MotionSample {
        MotionSample {
            platform_ns: i * STEP_US * 1_000,
            arrival_us: i * STEP_US,
            accel: [accel, 0.0, 0.0],
            gravity: [0.0, 0.0, 9.81],
            rotation: [0.0; 3],
        }
    }

    /// Feeds quiet background, with an impulse wherever `taps_at` says.
    fn run(d: &mut TapDetector, samples: usize, taps_at: &[usize]) -> Vec<TapGesture> {
        let mut out = Vec::new();
        for i in 0..samples {
            // A small amount of steady background so the floor has something
            // to track; a true zero would make every ratio meaningless.
            let quiet = if i % 2 == 0 { 0.02 } else { -0.02 };
            let a = if taps_at.contains(&i) { 4.0 } else { quiet };
            if let Some(g) = d.push(&sample(i as u64, a)) {
                out.push(g);
            }
        }
        out
    }

    #[test]
    fn three_even_taps_are_a_gesture() {
        let mut d = TapDetector::new(3);
        // 150 ms apart at 100 Hz is every fifteenth sample — a comfortable
        // human tapping rhythm, and inside the gap window.
        let found = run(&mut d, 400, &[100, 115, 130]);
        assert_eq!(found.len(), 1, "expected one gesture, got {found:?}");
        assert_eq!(found[0].taps, 3);
    }

    #[test]
    fn two_taps_do_not_complete_a_three_tap_gesture() {
        let mut d = TapDetector::new(3);
        assert!(run(&mut d, 400, &[100, 115]).is_empty());
    }

    #[test]
    fn a_single_pothole_is_not_a_tap() {
        // The case that matters most, and the one a magnitude threshold alone
        // cannot answer: one sharp impulse looks exactly like one tap.
        let mut d = TapDetector::new(2);
        assert!(run(&mut d, 400, &[200]).is_empty());
    }

    #[test]
    fn impulses_too_far_apart_are_not_one_gesture() {
        // 800 ms at 100 Hz, well past the window. Two unrelated road features.
        let mut d = TapDetector::new(2);
        assert!(run(&mut d, 500, &[100, 180]).is_empty());
    }

    #[test]
    fn uneven_intervals_are_rejected_from_three_taps_up() {
        // The evidence two taps cannot carry: 150 ms then 350 ms is not a
        // rhythm anybody performed on purpose, though both gaps are inside the
        // window and all three impulses match in strength.
        let mut d = TapDetector::new(3);
        assert!(
            run(&mut d, 500, &[100, 115, 150]).is_empty(),
            "an irregular train completed a gesture"
        );
    }

    #[test]
    fn four_taps_need_all_four() {
        let mut d = TapDetector::new(4);
        assert!(run(&mut d, 500, &[100, 115, 130]).is_empty());
        let mut d = TapDetector::new(4);
        assert_eq!(run(&mut d, 500, &[100, 115, 130, 145]).len(), 1);
    }

    #[test]
    fn an_angled_tap_counts_as_much_as_a_square_one() {
        // The reason ψ is taken per axis and summed rather than on the
        // magnitude: a phone in a pocket keeps no orientation, so the same
        // strike arrives spread across two axes as often as square on one. On
        // the magnitude the two cases differ; here they must not.
        let mut square = TapDetector::new(3);
        let mut angled = TapDetector::new(3);
        let taps = [100usize, 115, 130];

        let mut square_found = 0;
        let mut angled_found = 0;
        for i in 0..400usize {
            let quiet = if i % 2 == 0 { 0.02 } else { -0.02 };
            let hit = taps.contains(&i);

            let s = MotionSample {
                accel: [if hit { 4.0 } else { quiet }, quiet, quiet],
                ..sample(i as u64, 0.0)
            };
            // The same strike at 45°, so the vector magnitude matches.
            let c = 4.0 / 2.0f32.sqrt();
            let a = MotionSample {
                accel: [
                    if hit { c } else { quiet },
                    if hit { c } else { quiet },
                    quiet,
                ],
                ..sample(i as u64, 0.0)
            };
            if square.push(&s).is_some() {
                square_found += 1;
            }
            if angled.push(&a).is_some() {
                angled_found += 1;
            }
        }

        assert_eq!(square_found, 1, "the square tap should be found");
        assert_eq!(
            angled_found, square_found,
            "an angled tap was treated differently from a square one"
        );
    }

    #[test]
    fn the_gesture_length_is_clamped_to_what_the_setting_offers() {
        assert_eq!(TapDetector::new(0).taps_wanted, 2);
        assert_eq!(TapDetector::new(99).taps_wanted, 4);
        let mut d = TapDetector::new(2);
        d.set_taps_wanted(7);
        assert_eq!(d.taps_wanted, 4);
    }

    /// A still phone reports **runs of identical samples**, so ψ is exactly 0.
    ///
    /// `run` above avoids that on purpose — its comment says a true zero
    /// "would make every ratio meaningless" — and that is why every test here
    /// passed while the detector was unusable on a phone lying on a table.
    /// This is the signal a quantised accelerometer actually produces: a
    /// constant, with the occasional one-LSB step. `fn run`'s alternating
    /// ±0.02 is not a quiet sensor, it is a 50 Hz tone.
    fn quantised_still(i: usize) -> f32 {
        const LSB: f32 = 2.4e-4;
        // A step every 40 samples and nothing in between: three equal samples
        // in a row give ψ = 0, which is the whole point.
        (i / 40) as f32 % 3.0 * LSB
    }

    #[test]
    fn a_motionless_phone_arms_nothing() {
        let mut d = TapDetector::new(3);
        for i in 0..4_000 {
            assert!(
                d.push(&sample(i as u64, quantised_still(i))).is_none(),
                "a gesture completed on a phone that was not moving"
            );
        }
        let s = d.stats();

        // The measured failure was `ψ -68.0 dB · floor -117.0 dB · over 49.0`.
        let over = s.psi_db - s.floor_db;
        assert!(
            over < MARGIN_DB,
            "a still phone sits {over:.1} dB over its own floor, so every \
             sample is a candidate (ψ {:.1}, floor {:.1})",
            s.psi_db,
            s.floor_db
        );
        assert_eq!(s.gestures, 0, "false gestures on a motionless phone");
        // 1626 in 160 s was the measurement. A handful while the floor first
        // settles is fine; a stream of them means the latch is back.
        assert!(
            s.discarded_long < 50,
            "{} impulses discarded as too long on a still phone — the floor is \
             latched below the signal again",
            s.discarded_long
        );
    }

    #[test]
    fn the_floor_recovers_from_a_sustained_shake() {
        // Engine vibration: well clear of the margin and lasting far longer
        // than any tap. The floor must rise to meet it rather than being held
        // down by a candidate that never resolves.
        let mut d = TapDetector::new(3);
        for i in 0..6_000 {
            let a = if i % 2 == 0 { 0.5 } else { -0.5 };
            d.push(&sample(i as u64, a));
        }
        let s = d.stats();
        let over = s.psi_db - s.floor_db;
        assert!(
            over < MARGIN_DB,
            "a steady shake still reads {over:.1} dB over the floor, so the \
             floor was frozen by it rather than learning it"
        );
    }

    #[test]
    fn a_tap_still_registers_once_the_floor_is_honest() {
        // The repair must not have bought quiet by going deaf: the same
        // gesture the other tests use has to survive a realistic still
        // background rather than the synthetic dither.
        let mut d = TapDetector::new(3);
        let mut gestures = 0;
        for i in 0..2_000 {
            let tap = [600usize, 620, 640].contains(&i);
            let a = if tap { 4.0 } else { quantised_still(i) };
            if d.push(&sample(i as u64, a)).is_some() {
                gestures += 1;
            }
        }
        assert_eq!(gestures, 1, "the three-tap gesture was not recognised");
    }
}
