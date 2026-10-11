//! N matched impulses on an even beat, whatever heard them.
//!
//! **Extracted so the accelerometer and the microphone can share it**, which
//! is the half of tap detection that was never in doubt. What changed is the
//! front end: the accelerometer was always the fallback, chosen because iOS
//! has a single input route, and the original specification said so —
//!
//! > The tap literature's documented remedy for pocket false positives is
//! > audio first with the accelerometer confirming.
//!
//! — and flagged exactly where it would fail: at iOS's 100 Hz a tap's
//! high-frequency content is above Nyquist, so the ω² weighting the operator
//! depends on buys very little. In the field it bought nothing.
//!
//! Audio gives about forty times the resolution. Measured on two recordings
//! whose taps a rider confirmed were audible, a tap rises threefold between
//! consecutive milliseconds, stands ten times above the trailing second, and
//! **decays to a tenth within forty milliseconds** — forty samples of shape at
//! 48 kHz against four at 100 Hz.
//!
//! None of which changes what makes a *gesture*: N impulses, matched in
//! strength, evenly spaced. Road input arrives as uncorrelated singles or long
//! irregular trains, never as three matched strikes on a beat. That argument
//! is independent of the sensor, so this is the part both detectors use and
//! the part the existing tests already cover.

/// One impulse that passed whatever front end found it.
#[derive(Clone, Copy, Debug)]
pub struct Candidate {
    /// When it peaked, in milliseconds on the arrival clock.
    pub at_ms: u64,
    /// How far above the floor it reached, in dB.
    pub strength_db: f32,
}

/// A completed gesture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TapGesture {
    /// How many taps it was made of.
    pub taps: u8,
    /// When the last of them landed, on the arrival clock.
    pub at_ms: u64,
}

/// What happened to an offered candidate.
///
/// **Returned rather than written**, so the caller owns its own counters. The
/// two detectors report different things — one has a ψ floor, the other an
/// envelope — and a shared struct writing into a shared stats block would have
/// forced them to agree about fields neither needed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Banked {
    /// Not a tap of this gesture, and not the start of a new one either.
    Ignored,
    /// Banked, with the run now this long.
    Added { run: u8 },
    /// Banked, but it began a new run because it did not match the last in
    /// strength. The tap still landed and the rider should still hear it.
    RestartedOnMagnitude,
    /// The gesture is complete.
    Completed(TapGesture),
}

/// How unlike each other two taps of one gesture may be, as a ratio.
///
/// **Provisional.** Matched magnitude is nearly free and strong: road input
/// arrives as singles or irregular trains, not as pairs that resemble each
/// other.
const MAGNITUDE_TOLERANCE: f32 = 2.5;

/// How long the whole gesture may take, from its first tap to its last.
///
/// **One budget for the gesture rather than a limit on each gap**, which is
/// the rule as the rider stated it: three taps within at most three seconds.
/// It replaced a 600 ms per-gap window and an even-beat test, and the two are
/// not the same shape — a rider tapping a phone through a jacket pocket does
/// not keep time, and the beat test was rejecting the irregular middle tap and
/// starting the run again from there. A total budget cannot do that: it only
/// ever asks whether all three arrived in time.
///
/// Three seconds is also long enough to be a deliberate act. Nothing that
/// happens by accident on a bike produces three separate strikes inside three
/// seconds and then stops.
const GESTURE_WINDOW_MS: u32 = 3_000;

/// How long after a tap another one cannot be counted, in ms.
///
/// **Not a tuning knob — without it a single strike is a whole gesture.** A
/// tap on a phone's body rings, and the ringing crosses the detector's
/// threshold again on the way down, so one strike offers three candidates a
/// few tens of milliseconds apart. This is the dead time that makes a tap one
/// tap.
const GAP_MIN_MS: u32 = 80;

/// How many taps a gesture is.
///
/// **Fixed, where it used to be a setting offering two, three or four.** Two
/// carries no timing evidence at all and four is a chore; the setting existed
/// because nobody had measured which was right, and it was one more thing that
/// could disagree with what the detector was actually counting.
pub const TAPS_WANTED: u8 = 3;

/// The run of matched impulses in progress.
pub struct GesturePattern {
    pending: [Option<Candidate>; TAPS_WANTED as usize],
    pending_len: usize,
}

impl Default for GesturePattern {
    fn default() -> Self {
        Self::new()
    }
}

impl GesturePattern {
    pub fn new() -> Self {
        Self {
            pending: [None; TAPS_WANTED as usize],
            pending_len: 0,
        }
    }

    /// How many are banked right now.
    pub fn run(&self) -> u8 {
        self.pending_len as u8
    }

    pub fn reset(&mut self) {
        self.pending_len = 0;
    }

    pub fn offer(&mut self, c: Candidate) -> Banked {
        // **The budget is measured from the first tap, not the last.** Checked
        // before anything else, so a tap arriving late does not complete a
        // gesture that began four seconds ago — it begins a new one, which is
        // almost certainly what the rider meant by it.
        if self.pending_len > 0 {
            let first = self.pending[0].map(|f| f.at_ms).unwrap_or(c.at_ms);
            if c.at_ms.saturating_sub(first) > GESTURE_WINDOW_MS as u64 {
                self.pending_len = 0;
            }
        }

        if self.pending_len == 0 {
            self.pending[0] = Some(c);
            self.pending_len = 1;
            return Banked::Added { run: 1 };
        }

        let Some(last) = self.pending[self.pending_len - 1] else {
            return Banked::Ignored;
        };
        if c.at_ms.saturating_sub(last.at_ms) < GAP_MIN_MS as u64 {
            // The same strike still ringing, not a new tap. See `GAP_MIN_MS`.
            return Banked::Ignored;
        }

        // Matched magnitude. Two impulses of wildly different strength are a
        // road feature and a tap, not one gesture.
        let ratio = ratio_of(
            c.strength_db.abs().max(1.0),
            last.strength_db.abs().max(1.0),
        );
        if ratio > MAGNITUDE_TOLERANCE {
            self.pending[0] = Some(c);
            self.pending_len = 1;
            return Banked::RestartedOnMagnitude;
        }

        self.pending[self.pending_len] = Some(c);
        self.pending_len += 1;

        if self.pending_len >= TAPS_WANTED as usize {
            self.pending_len = 0;
            return Banked::Completed(TapGesture {
                taps: TAPS_WANTED,
                at_ms: c.at_ms,
            });
        }
        Banked::Added {
            run: self.pending_len as u8,
        }
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

    fn c(at_ms: u64, strength_db: f32) -> Candidate {
        Candidate { at_ms, strength_db }
    }

    #[test]
    fn three_matched_impulses_inside_the_window_are_a_gesture() {
        let mut p = GesturePattern::new();
        assert_eq!(p.offer(c(0, 40.0)), Banked::Added { run: 1 });
        assert_eq!(p.offer(c(200, 40.0)), Banked::Added { run: 2 });
        assert!(matches!(p.offer(c(400, 40.0)), Banked::Completed(_)));
    }

    #[test]
    fn an_impulse_that_does_not_match_begins_a_new_run() {
        let mut p = GesturePattern::new();
        p.offer(c(0, 40.0));
        // Ten times the strength is a road feature beside a tap, not a pair.
        assert_eq!(p.offer(c(200, 4.0)), Banked::RestartedOnMagnitude);
        assert_eq!(p.run(), 1, "it still banked, as the start of a new run");
    }

    /// **This used to be asserted the other way round.** The pattern required
    /// an even beat, so a rider whose middle tap landed late had the run
    /// restarted from it — two taps heard, nothing happening, which is exactly
    /// the complaint the gesture kept producing. Nobody taps a phone through a
    /// jacket in time.
    #[test]
    fn taps_do_not_have_to_be_evenly_spaced() {
        let mut p = GesturePattern::new();
        p.offer(c(0, 40.0));
        p.offer(c(150, 40.0));
        assert!(
            matches!(p.offer(c(1_400, 40.0)), Banked::Completed(_)),
            "an uneven three inside the window is still three taps"
        );
    }

    #[test]
    fn three_taps_spread_over_more_than_the_window_are_not_a_gesture() {
        let mut p = GesturePattern::new();
        p.offer(c(0, 40.0));
        p.offer(c(1_500, 40.0));
        assert_eq!(
            p.offer(c(3_400, 40.0)),
            Banked::Added { run: 1 },
            "past the budget, so it starts over rather than completing"
        );
    }

    /// The budget runs from the first tap, so a gesture cannot be assembled
    /// out of taps that were never close together — each pair being inside
    /// three seconds is not the same as all three being.
    #[test]
    fn the_window_is_measured_from_the_first_tap() {
        let mut p = GesturePattern::new();
        p.offer(c(0, 40.0));
        assert_eq!(p.offer(c(2_900, 40.0)), Banked::Added { run: 2 });
        assert_eq!(p.offer(c(5_000, 40.0)), Banked::Added { run: 1 });
    }

    #[test]
    fn a_second_strike_of_the_same_tap_is_ignored() {
        let mut p = GesturePattern::new();
        p.offer(c(0, 40.0));
        assert_eq!(p.offer(c(20, 40.0)), Banked::Ignored);
    }
}
