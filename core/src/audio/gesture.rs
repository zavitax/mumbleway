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
    /// The same, for a beat the run could not accept.
    RestartedOnInterval,
    /// The gesture is complete.
    Completed(TapGesture),
}

/// How unlike each other two taps of one gesture may be, as a ratio.
///
/// **Provisional.** Matched magnitude is nearly free and strong: road input
/// arrives as singles or irregular trains, not as pairs that resemble each
/// other.
const MAGNITUDE_TOLERANCE: f32 = 2.5;

/// How much the intervals of a gesture may differ, as a ratio.
///
/// **Provisional, and only meaningful from three taps up.** Two taps give one
/// interval, which has nothing to be compared against.
const INTERVAL_TOLERANCE: f32 = 1.8;

/// The window in which the next tap of the gesture must arrive.
///
/// **Measured.** 400 ms cut off real gestures: tapping a phone held in the
/// hand came out around 200–330 ms between taps, but a deliberate, spaced-out
/// three on a desk ran to 570.
const GAP_MIN_MS: u32 = 80;
const GAP_MAX_MS: u32 = 600;

/// The run of matched impulses in progress.
pub struct GesturePattern {
    taps_wanted: u8,
    /// Four is the most ever wanted, so there is nothing to allocate.
    pending: [Option<Candidate>; 4],
    pending_len: usize,
}

impl GesturePattern {
    pub fn new(taps_wanted: u8) -> Self {
        Self {
            taps_wanted: taps_wanted.clamp(2, 4),
            pending: [None; 4],
            pending_len: 0,
        }
    }

    pub fn taps_wanted(&self) -> u8 {
        self.taps_wanted
    }

    /// How many are banked right now.
    pub fn run(&self) -> u8 {
        self.pending_len as u8
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

    pub fn reset(&mut self) {
        self.pending_len = 0;
    }

    pub fn offer(&mut self, c: Candidate) -> Banked {
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
            return Banked::Added { run: 1 };
        }

        let Some(last) = self.pending[self.pending_len - 1] else {
            return Banked::Ignored;
        };
        let gap = c.at_ms.saturating_sub(last.at_ms);
        if gap < GAP_MIN_MS as u64 {
            // Too soon even for the dead time to have caught: part of the same
            // strike rather than a new tap.
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

        // Consistent intervals, once there is more than one to compare. This
        // is the evidence two taps cannot carry at all.
        if self.pending_len >= 2 {
            let Some(prev) = self.pending[self.pending_len - 2] else {
                return Banked::Ignored;
            };
            let previous_gap = last.at_ms.saturating_sub(prev.at_ms).max(1);
            if ratio_of(gap.max(1) as f32, previous_gap as f32) > INTERVAL_TOLERANCE {
                self.pending[0] = Some(c);
                self.pending_len = 1;
                return Banked::RestartedOnInterval;
            }
        }

        self.pending[self.pending_len] = Some(c);
        self.pending_len += 1;

        if self.pending_len >= self.taps_wanted as usize {
            self.pending_len = 0;
            return Banked::Completed(TapGesture {
                taps: self.taps_wanted,
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
    fn three_matched_impulses_on_a_beat_are_a_gesture() {
        let mut p = GesturePattern::new(3);
        assert_eq!(p.offer(c(0, 40.0)), Banked::Added { run: 1 });
        assert_eq!(p.offer(c(200, 40.0)), Banked::Added { run: 2 });
        assert!(matches!(p.offer(c(400, 40.0)), Banked::Completed(_)));
    }

    #[test]
    fn an_impulse_that_does_not_match_begins_a_new_run() {
        let mut p = GesturePattern::new(3);
        p.offer(c(0, 40.0));
        // Ten times the strength is a road feature beside a tap, not a pair.
        assert_eq!(p.offer(c(200, 4.0)), Banked::RestartedOnMagnitude);
        assert_eq!(p.run(), 1, "it still banked, as the start of a new run");
    }

    #[test]
    fn an_off_beat_impulse_begins_a_new_run() {
        let mut p = GesturePattern::new(3);
        p.offer(c(0, 40.0));
        p.offer(c(150, 40.0));
        assert_eq!(p.offer(c(550, 40.0)), Banked::RestartedOnInterval);
    }

    #[test]
    fn impulses_too_far_apart_are_not_one_gesture() {
        let mut p = GesturePattern::new(2);
        p.offer(c(0, 40.0));
        assert_eq!(
            p.offer(c(1_000, 40.0)),
            Banked::Added { run: 1 },
            "past the window, so it starts over rather than completing"
        );
    }

    #[test]
    fn a_second_strike_of_the_same_tap_is_ignored() {
        let mut p = GesturePattern::new(3);
        p.offer(c(0, 40.0));
        assert_eq!(p.offer(c(20, 40.0)), Banked::Ignored);
    }
}
