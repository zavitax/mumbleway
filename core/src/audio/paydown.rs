//! A look-ahead that is long at the start of a phrase and short by the middle.
//!
//! # Why the delay exists at all
//!
//! Voice activation cannot decide a block is speech until it has the block, so
//! the sound that opens the gate has already gone by. The chain answers that by
//! delaying the audio and not the decision. `tools/vad/onset_lead.py` measured
//! what real openings need, walking back from each to its own local background
//! across 69 openings on three rides:
//!
//! | Look-ahead | Openings fully covered |
//! |---|---|
//! | 80 ms | 89.9% |
//! | 160 ms | 94.2% |
//! | 240 ms | 95.7% |
//! | 320 ms | 98.6% |
//!
//! And the cost of buying more of that column is one-way latency on every
//! transmission, for ever, to protect the first tenth of a second — while p50
//! of the lead distribution is 10 ms. Nine openings in ten pay the full delay
//! and need almost none of it.
//!
//! # Paying it down
//!
//! So hold a long look-ahead, transmit the whole pre-roll when the gate opens,
//! and then transmit *slightly faster than real time* until the backlog drains
//! to a floor: emit one block while consuming a little more than one, by
//! removing whole pitch periods. The first word is intact because the pre-roll
//! went out; the steady state is short because the debt was repaid.
//!
//! [`super::stretch::TimeCompressor`] is that primitive, already used on the
//! receive side to play off a jitter backlog, and already tested.
//!
//! # The design doc asked for 400 ms and the measurement refused it
//!
//! `docs/ONSET_LATENCY.md` proposed a 400 ms look-ahead, enough to cover the
//! whole distribution. **It was written before transmit runs had been
//! measured**, and the run length is what decides whether a debt can be repaid
//! — repayment happens *during* a transmission, so a phrase that ends first
//! ends still owing. Measured over the corpus, p50 of a transmit run is 1.52 s.
//!
//! `tools/vad/paydown.py`, mean one-way delay weighted per transmitted block:
//!
//! | Configuration | Mean latency | Onset coverage |
//! |---|---|---|
//! | 160 ms flat — what shipped | 160 ms | 94.2% |
//! | 400 ms, repay 5% | **294 ms** | ~99% |
//! | 400 ms, repay 10% | **238 ms** | ~99% |
//! | **240 ms, repay 10%** | **124 ms** | **95.7%** |
//! | 320 ms, repay 20% | 131 ms | 98.6% |
//!
//! At an inaudible repay rate the 400 ms proposal is **worse than doing
//! nothing**: 5% clears only 76 ms of a 340 ms debt inside a median phrase, so
//! nearly all of it is still being carried when the phrase ends.
//!
//! 240 ms at 10% is better than what shipped on *both* axes — 36 ms less mean
//! latency and 1.5 points more onset coverage — at a rate the design doc
//! already calls comfortable. That is what this ships.
//!
//! 320 ms at 20% buys another 2.9 points of coverage for 7 ms, and is left
//! unshipped deliberately: 20% is a 1.2× speed-up through the first second of
//! every phrase, and nobody has listened to it yet. The listen sheet's chain
//! playback is how to judge it, and it is a two-constant change.
//!
//! # What it must not disturb
//!
//! **The diagnostic recorder.** The `.s16` has to stay the microphone at real
//! time or the corpus is silently time-warped, which is the same class of fault
//! as recording the enhancer's output — `core/src/audio/record.rs` exists
//! because that has happened once. The recorder's tap is upstream of this and
//! this stage never runs on it.
//!
//! **The echo canceller.** `docs/ONSET_LATENCY.md` flagged this as the thing
//! that could sink the idea: the reference is what was played, and a near end
//! that is time-scaled against it would make the canceller adapt on a moving
//! timebase. It is not a problem *here*, and the reason is placement rather
//! than luck — the canceller runs inside `CaptureProcessor::process_with_
//! reference`, several stages upstream of the delay line, so it never sees a
//! compressed sample. Moving this earlier in `run_worker` would make the
//! warning apply again.

use std::collections::VecDeque;

use super::stretch::TimeCompressor;

const SAMPLE_RATE: usize = 48_000;

/// How far ahead of the audio the transmit decision runs at the start of a
/// phrase. See the table above.
pub const LOOKAHEAD_MS: usize = 240;

/// Where the delay settles once the debt is repaid.
///
/// Not zero: some slack absorbs a late block without a dropout, and a delay
/// line held at exactly nothing is a dropout generator on a phone that hiccups.
pub const FLOOR_MS: usize = 60;

/// What the delay becomes when the performance ladder switches the pay-down
/// off — see [`Paydown::set_enabled`].
///
/// **Exactly what shipped before this existed**, which is the right thing for a
/// device that cannot afford the compressor: it loses the improvement rather
/// than getting something worse than it had. Holding the full 240 ms without
/// repaying would raise a slow phone's latency above where it started.
pub const FALLBACK_MS: usize = 160;

/// How much faster than real time to transmit while in debt.
///
/// 1.10, which the design doc calls comfortable and which the receive side
/// exceeds by a wide margin — `stretch.rs` plays off a jitter backlog at up to
/// 2×. Pitch periods are removed whole, so this is a change of duration and not
/// of pitch.
const REPAY_SPEED: f32 = 1.10;

const LOOKAHEAD_SAMPLES: usize = SAMPLE_RATE * LOOKAHEAD_MS / 1000;
const FLOOR_SAMPLES: usize = SAMPLE_RATE * FLOOR_MS / 1000;
const FALLBACK_SAMPLES: usize = SAMPLE_RATE * FALLBACK_MS / 1000;

/// Holds the audio back so the transmit decision can be made with hindsight,
/// and gives the delay back once the phrase is under way.
pub struct Paydown {
    ring: VecDeque<f32>,
    comp: TimeCompressor,
    /// Reused across blocks. The audio thread does not allocate.
    scratch: Vec<f32>,
    /// Latches once the ring has first filled. Without it the readiness test
    /// would be re-asked every block and, since repaying deliberately empties
    /// the ring back towards the floor, would never be true again after the
    /// first phrase.
    primed: bool,
    enabled: bool,
}

impl Default for Paydown {
    fn default() -> Self {
        Self::new()
    }
}

impl Paydown {
    pub fn new() -> Self {
        Self {
            ring: VecDeque::with_capacity(LOOKAHEAD_SAMPLES + 2048),
            comp: TimeCompressor::new(),
            scratch: Vec::with_capacity(2048),
            primed: false,
            enabled: true,
        }
    }

    /// Turns the pay-down off, leaving a plain delay of [`FALLBACK_MS`].
    ///
    /// A rung on the performance ladder: the compressor searches for a pitch
    /// period every block it repays, and a phone that cannot afford that should
    /// give up the improvement rather than the call. Re-priming is deliberate —
    /// the fill target changes with the mode, so the ring has to reach the new
    /// one before anything is emitted against it.
    pub fn set_enabled(&mut self, on: bool) {
        if on != self.enabled {
            self.enabled = on;
            self.primed = false;
            self.comp.reset();
        }
    }

    /// How far behind real time the transmitted audio currently is, in ms.
    /// For the diagnostics panel: this is the number the whole stage moves.
    pub fn held_ms(&self) -> u32 {
        (self.ring.len() * 1000 / SAMPLE_RATE) as u32
    }

    /// The same, in samples.
    ///
    /// The diagnostic recorder needs this: it pairs each recorded block with
    /// the decision made about that block's audio, which means holding it back
    /// by however much the delay line is holding *now*. That used to be a
    /// constant and is not any more.
    pub fn held_samples(&self) -> usize {
        self.ring.len()
    }

    fn fill_target(&self) -> usize {
        if self.enabled {
            LOOKAHEAD_SAMPLES
        } else {
            FALLBACK_SAMPLES
        }
    }

    /// Replaces `block` with older audio, repaying the debt as it goes.
    ///
    /// Returns false while there is not yet anything old enough, in which case
    /// `block` is left alone and the caller must send nothing — passing the
    /// current block through instead would defeat the delay on exactly the
    /// first word after the mode was chosen, which is the one a rider notices.
    pub fn shift(&mut self, block: &mut [f32]) -> bool {
        self.ring.extend(block.iter().copied());

        if !self.primed {
            if self.ring.len() < self.fill_target() + block.len() {
                return false;
            }
            self.primed = true;
        }
        if self.ring.len() < block.len() {
            return false;
        }

        // Repay, on the oldest audio in the ring — which is the audio about to
        // be emitted, and the only audio it would be correct to shorten.
        if self.enabled && self.ring.len() > FLOOR_SAMPLES + block.len() {
            let n = block.len();
            self.scratch.clear();
            for _ in 0..n {
                self.scratch.push(self.ring.pop_front().unwrap_or(0.0));
            }
            let kept = self.comp.process(&mut self.scratch, n, REPAY_SPEED);
            // Back to the front in order, which means pushing in reverse.
            for s in self.scratch[..kept].iter().rev() {
                self.ring.push_front(*s);
            }
        } else {
            // Not in debt, so nothing is owed to the next catch-up either.
            self.comp.reset();
        }

        if self.ring.len() < block.len() {
            // The compressor cannot take more than it was given, so this is
            // unreachable — but emitting a short block would desynchronise the
            // encoder, and going quiet for one block will not.
            return false;
        }
        for s in block.iter_mut() {
            *s = self.ring.pop_front().unwrap_or(0.0);
        }
        true
    }

    pub fn clear(&mut self) {
        self.ring.clear();
        self.primed = false;
        self.comp.reset();
    }

    pub fn is_empty(&self) -> bool {
        self.ring.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BLOCK: usize = 480;

    /// A block of something the pitch search can find a period in. Silence has
    /// no period and would measure the fallback rather than the mechanism.
    fn voiced(at: usize) -> Vec<f32> {
        (0..BLOCK)
            .map(|i| {
                let t = (at + i) as f32 / SAMPLE_RATE as f32;
                // 200 Hz with a couple of harmonics, which is a voice-shaped
                // thing to ask a pitch detector about.
                0.3 * (t * 200.0 * std::f32::consts::TAU).sin()
                    + 0.15 * (t * 400.0 * std::f32::consts::TAU).sin()
            })
            .collect()
    }

    #[test]
    fn nothing_comes_out_until_the_look_ahead_is_covered() {
        let mut p = Paydown::new();
        let mut sent = 0;
        for i in 0..LOOKAHEAD_SAMPLES / BLOCK {
            let mut b = voiced(i * BLOCK);
            if p.shift(&mut b) {
                sent += 1;
            }
        }
        assert_eq!(sent, 0, "audio went out before the look-ahead was covered");
        assert!(p.held_ms() >= (LOOKAHEAD_MS as u32) - 10);
    }

    #[test]
    fn the_debt_is_repaid_down_to_the_floor_and_then_left_alone() {
        let mut p = Paydown::new();
        // Fill, then run for four seconds of blocks.
        let mut at = 0;
        let mut first = None;
        for i in 0..400 {
            let mut b = voiced(at);
            at += BLOCK;
            if p.shift(&mut b) && first.is_none() {
                first = Some(i);
            }
        }
        let held = p.held_ms();
        assert!(
            held <= FLOOR_MS as u32 + 20,
            "the debt never came down: {held} ms held"
        );
        // And it does not keep going: the floor is a floor, not a target to
        // undershoot, or the delay line stops absorbing a late block.
        assert!(
            held + 20 >= FLOOR_MS as u32,
            "repaid past the floor: {held} ms held"
        );
    }

    #[test]
    fn switched_off_it_is_a_plain_delay_at_the_old_length() {
        let mut p = Paydown::new();
        p.set_enabled(false);
        let mut at = 0;
        for _ in 0..400 {
            let mut b = voiced(at);
            at += BLOCK;
            p.shift(&mut b);
        }
        let held = p.held_ms();
        assert!(
            (held as i64 - FALLBACK_MS as i64).abs() <= 10,
            "a disabled pay-down should hold exactly {FALLBACK_MS} ms, held {held}"
        );
    }

    #[test]
    fn what_comes_out_is_what_went_in_when_there_is_no_debt() {
        // With the pay-down off, the stage is a pure delay and must not alter a
        // sample — the encoder's input has to be the chain's output.
        let mut p = Paydown::new();
        p.set_enabled(false);
        let mut at = 0;
        let mut fed: Vec<f32> = Vec::new();
        let mut got: Vec<f32> = Vec::new();
        for _ in 0..40 {
            let mut b = voiced(at);
            at += BLOCK;
            fed.extend_from_slice(&b);
            if p.shift(&mut b) {
                got.extend_from_slice(&b);
            }
        }
        assert!(!got.is_empty());
        assert_eq!(
            got[..],
            fed[..got.len()],
            "a delay with no debt must be sample-exact"
        );
    }

    #[test]
    fn clearing_re_primes() {
        let mut p = Paydown::new();
        let mut at = 0;
        for _ in 0..100 {
            let mut b = voiced(at);
            at += BLOCK;
            p.shift(&mut b);
        }
        p.clear();
        assert!(p.is_empty());
        let mut b = voiced(at);
        assert!(
            !p.shift(&mut b),
            "it emitted before re-filling after a clear"
        );
    }
}
