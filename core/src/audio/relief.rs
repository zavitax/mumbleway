//! Giving up chain stages, in order, to meet the block deadline.
//!
//! # Why there is a second ladder
//!
//! The enhancer has one of its own ([`super::deepfilter::Effort`]) and it is
//! the big lever: on the phone this was built for, the enhancer is **88% of a
//! block** and everything else together is 0.78 ms. So this ladder cannot
//! rescue a device on its own, and it is not meant to.
//!
//! **It is currency for keeping the enhancer one rung higher.** At `ErbOnly`
//! the block measures 8.12–8.37 ms worst against a 10 ms budget — inside, and
//! not comfortably. Spending a few tenths of a millisecond of stages that
//! measurably contribute little buys the headroom to hold `Reduced`, and the
//! enhancer does more for intelligibility per millisecond than anything else
//! in the chain.
//!
//! # The order is measured, not guessed
//!
//! From the first unclipped ride, scoring how well each feature separates
//! transmitted blocks from untransmitted ones (Mann-Whitney AUC, 0.5 being a
//! coin toss):
//!
//! | Feature | AUC | What that means here |
//! |---|---|---|
//! | `level_db` | **0.878** | the gate's best single input, and it is free |
//! | `snr_db` | 0.779 | derived from the level and the floor |
//! | `vad` (RNNoise) | 0.767 | *worse* than the level it would be dropped for |
//! | `harmonicity` (pitch) | **0.564** | barely better than a coin toss |
//!
//! **The enhancer bends before anything else breaks, and is switched off last
//! of all** — see [`Relief`] for the two costs side by side. Between those two
//! extremes the cheap stages go in order of quality given up rather than
//! milliseconds gained, and those turn out to be almost opposite. Measured on
//! the OPPO A3s:
//!
//! | Rung | Saves | AUC it costs |
//! |---|---|---|
//! | pitch search | **0.052 ms** | 0.564 |
//! | feedback guard | 0.194 ms | — (nothing, on a headset) |
//! | RNNoise | **0.295 ms** | 0.767 |
//!
//! So the pitch search goes first because it is the cheapest quality to sell,
//! **not** because it is expensive — an earlier draft of this file claimed it
//! was "the second most expensive thing in suppression", which was read off
//! the code and is wrong: it is the least expensive thing on the ladder, at
//! 13% of suppression against RNNoise's 76%. Selling it buys almost nothing,
//! and that is the point of selling it first.
//!
//! RNNoise goes late but not last, because DeepFilterNet now sits in front of
//! it and has already removed most of what RNNoise existed to remove — its
//! remaining unique contribution is a VAD that the level beats.
//!
//! All three together are **0.54 ms**. That is the whole budget of this
//! ladder, against an enhancer rung worth 1.9 ms, so nothing here rescues a
//! device on its own — see above.
//!
//! # What is deliberately not on the ladder
//!
//! - **The rumble filter and speech band.** Nearly free, and *everything*
//!   downstream measures through them — the floor tracker, the gate, the AGC.
//!   Dropping them would not lose filtering, it would corrupt every threshold.
//! - **The limiter**, which is what stops clipped audio reaching the wire.
//! - **The input peak counter**, which is the instrument that cost an evening.
//! - **The gate itself**, which is the feature.
//! - **Opus.** A fixed cost for a fixed frame size, already the cheapest
//!   speech configuration, and it carries hand-written NEON where `tract` does
//!   not.

/// One rung of the whole-chain ladder.
///
/// # The enhancer bends before anything else breaks
///
/// **Its first two rungs come first, and its last one comes last.** That looks
/// odd beside "the enhancer is 88% of the block" until the two costs are put
/// side by side:
///
/// | Rung | Buys | Costs |
/// |---|---|---|
/// | enhancer `Reduced` | **1.9 ms** | ~nothing; on voice over music it measured *better* than full |
/// | enhancer `ErbOnly` | 0.6 ms, and halves the tail | up to 4.9 dB of separation on a quiet ride |
/// | pitch, feedback, RNNoise | **0.54 ms together** | a coin-toss feature, headset insurance, and a VAD the level beats |
/// | enhancer `Bypassed` | 4.3 ms | all of it — the thing that turns 1.5 dB into 16 |
///
/// So bending the enhancer is both the cheapest quality and the largest
/// saving, twice over, before a single other stage is touched. And switching
/// it off entirely is the largest loss on the chain by a wide margin, so it is
/// the last thing tried rather than the fourth — every other stage is worth
/// spending to avoid it.
///
/// # What each rung costs a whole block
///
/// Measured on the OPPO A3s with `MW_RELIEF` on `core/tests/chain_cost.rs`,
/// over a clip that is 70% speech, against a 10 ms deadline:
///
/// | Rung | Given up | Block mean | Worst |
/// |---|---|---|---|
/// | 0 | — | 8.27 ms | 13.63 |
/// | 1 | enhancer `Reduced` | 6.18 ms | 11.37 |
/// | 2 | enhancer `ErbOnly` | 5.51 ms | **7.76** |
/// | 3 | + pitch | 5.50 ms | — |
/// | 4 | + feedback | 5.30 ms | 7.90 |
/// | 5 | + RNNoise | 4.80 ms | 7.64 |
/// | 6 | enhancer off | 0.25 ms | 1.06 |
///
/// **Rung 2 is where the worst block first fits**, which is the whole argument
/// for this order: the enhancer's two bends carry the device inside the
/// deadline on their own, and the three cheap stages below are the margin that
/// keeps it there — 0.71 ms of it — rather than the thing that gets it there.
///
/// Read the mean, not the worst. A single late block moves the worst column by
/// milliseconds (rung 3 shows 10.83 in one run and nothing unusual in the
/// next); the mean is stable across runs and is what the ladder reacts to over
/// a hundred blocks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Relief {
    /// Everything runs.
    None,
    /// The enhancer drops to `Effort::Reduced`.
    EnhancerReduced,
    /// The enhancer drops to `Effort::ErbOnly`.
    EnhancerLight,

    /// The look-ahead stops being paid down — see [`super::paydown`].
    ///
    /// **First of the cheap rungs, because it is the only one that costs no
    /// audio at all.** Every other rung on this ladder removes something from
    /// the signal or from what the gate knows about it. This one changes
    /// nothing a listener can hear: the samples are identical, and what
    /// degrades is the one-way delay, from a mean of 124 ms back to a flat
    /// 160 ms — which is exactly what shipped before the pay-down existed. A
    /// device that cannot afford a pitch search per block gets the latency it
    /// always had rather than something worse.
    NoPaydown,

    /// The pitch search is skipped. AUC 0.564.
    NoPitch,
    /// The feedback guard is skipped. It does nothing on a headset, where
    /// there is no acoustic path from the speaker back to the microphone.
    NoFeedback,
    /// RNNoise is skipped; the gate runs on level and SNR alone.
    NoRnnoise,

    // --- the diagnostics panel's own costs --------------------------------
    //
    // **These take nothing at all from the audio.** They are the last things
    // given up before the enhancer because the panel is only expensive while
    // it is *open*, and a rider with it open is deliberately looking at
    // something — so it earns its keep right up to the point where the
    // alternative is losing the enhancer altogether. After that it does not.
    //
    // Measured with the engine running on the emulator, the open panel is
    // about 45 percentage points of CPU against 103 for the chain alone. Most
    // of that is the analyser: three FFTs per third block in the core and a
    // continuously repainting `CustomPaint` above it. The two rungs after it
    // are much smaller and are here for completeness rather than for the
    // milliseconds.
    /// The spectrum analyser stops computing, in the core as well as on
    /// screen. The panel says why in its place.
    NoAnalyser,
    /// The classifier's top three classes stop being shown. The model still
    /// runs — `Auto` reads its verdict — so this is the display only.
    NoClassifierTop,
    /// The chain dots stop following the audio. Warnings and the rung keep
    /// updating, or this rung could hide the one after it.
    NoLiveDots,

    /// The background classifier stops running and the profile is pinned to
    /// whatever is in force.
    ///
    /// **The largest single saving on this ladder other than the enhancer, and
    /// it was not measurable until a device reported it.** The panel's own
    /// figure from the OPPO A3s is **50 ms per check**, ranging 42–61 — against
    /// a chain whose whole block budget is 10 ms. It does not run per block, so
    /// it is not 5 ms of every block; it is a 50 ms stall on one thread
    /// whenever it fires, on a phone that is already missing deadlines.
    ///
    /// [`Relief::NoClassifierTop`] above only stops *displaying* the top three;
    /// the model still ran, because `Auto` reads its verdict. This stops the
    /// inference. What is lost is real: `Auto` can no longer notice that the
    /// background became music or a motorway, so the profile stops adapting and
    /// holds wherever it was. That is a worse loss than any panel rung and a
    /// smaller one than losing the enhancer, which is where it sits.
    NoClassifier,

    /// The enhancer is bypassed. The last resort, and the bottom.
    EnhancerOff,
}

impl Relief {
    /// The next rung down, or `None` at the bottom.
    pub fn weaker(self) -> Option<Relief> {
        Some(match self {
            Relief::None => Relief::EnhancerReduced,
            Relief::EnhancerReduced => Relief::EnhancerLight,
            Relief::EnhancerLight => Relief::NoPaydown,
            Relief::NoPaydown => Relief::NoPitch,
            Relief::NoPitch => Relief::NoFeedback,
            Relief::NoFeedback => Relief::NoRnnoise,
            Relief::NoRnnoise => Relief::NoAnalyser,
            Relief::NoAnalyser => Relief::NoClassifierTop,
            Relief::NoClassifierTop => Relief::NoLiveDots,
            Relief::NoLiveDots => Relief::NoClassifier,
            Relief::NoClassifier => Relief::EnhancerOff,
            Relief::EnhancerOff => return None,
        })
    }

    /// How far down the ladder this is, for the panel and the log.
    pub fn index(self) -> u8 {
        match self {
            Relief::None => 0,
            Relief::EnhancerReduced => 1,
            Relief::EnhancerLight => 2,
            Relief::NoPaydown => 3,
            Relief::NoPitch => 4,
            Relief::NoFeedback => 5,
            Relief::NoRnnoise => 6,
            Relief::NoAnalyser => 7,
            Relief::NoClassifierTop => 8,
            Relief::NoLiveDots => 9,
            Relief::NoClassifier => 10,
            Relief::EnhancerOff => 11,
        }
    }

    /// The look-ahead is no longer paid down; the delay is flat at
    /// [`super::paydown::FALLBACK_MS`].
    pub fn skip_paydown(self) -> bool {
        self >= Relief::NoPaydown
    }

    /// The background classifier no longer runs at all, and the profile is
    /// pinned. Distinct from [`Self::skip_classifier_top`], which only stops
    /// the display.
    pub fn skip_classifier(self) -> bool {
        self >= Relief::NoClassifier
    }

    /// The spectrum analyser has been given up — in the core, not only on
    /// screen. Three transforms a block stop being computed at all.
    pub fn skip_analyser(self) -> bool {
        self >= Relief::NoAnalyser
    }

    /// The classifier's top three classes are no longer shown.
    pub fn skip_classifier_top(self) -> bool {
        self >= Relief::NoClassifierTop
    }

    /// The chain dots no longer follow the audio.
    pub fn skip_live_dots(self) -> bool {
        self >= Relief::NoLiveDots
    }

    pub fn skip_pitch(self) -> bool {
        self >= Relief::NoPitch
    }

    pub fn skip_feedback(self) -> bool {
        self >= Relief::NoFeedback
    }

    pub fn skip_rnnoise(self) -> bool {
        self >= Relief::NoRnnoise
    }

    /// How many rungs the enhancer should have given up by now.
    ///
    /// It holds at `ErbOnly` across the three cheap rungs — those exist
    /// precisely so it does not have to go further.
    pub fn enhancer_rungs(self) -> u8 {
        match self {
            Relief::None => 0,
            Relief::EnhancerReduced => 1,
            Relief::EnhancerOff => 3,
            // Everything between holds the enhancer at `ErbOnly`. That is what
            // all of these rungs exist to buy.
            _ => 2,
        }
    }
}

/// Consecutive blocks over the deadline before a rung is given up.
///
/// One second's worth, matching the enhancer's own guard: long enough that a
/// scheduler hiccup costs nothing, short enough that a device which cannot
/// manage is not allowed to ruin a whole ride.
pub const STEP_DOWN_AFTER: u32 = 100;

/// The deadline a whole block has to be returned in, in microseconds.
pub const BUDGET_US: u32 = 10_000;

/// Tracks the ladder and decides when to give up the next rung.
///
/// **Driven by the whole block, not by any one stage.** The enhancer used to
/// judge itself on its own stopwatch, which is how a model measured at 6.2 ms
/// against a 10 ms budget came to switch itself off: it carried the only clock
/// in the chain, so it was the only stage that could be blamed for a late
/// block. The deadline belongs to the block, so the decision does too.
#[derive(Debug, Default)]
pub struct ReliefLadder {
    level: Option<Relief>,
    run_of_overruns: u32,
}

impl ReliefLadder {
    pub fn level(&self) -> Relief {
        self.level.unwrap_or(Relief::None)
    }

    /// Feeds one block's wall-clock cost in. Returns the new rung when it
    /// stepped, so the caller can say so once rather than every block.
    pub fn note_block(&mut self, us: u32) -> Option<Relief> {
        if us <= BUDGET_US {
            self.run_of_overruns = 0;
            return None;
        }
        self.run_of_overruns += 1;
        if self.run_of_overruns < STEP_DOWN_AFTER {
            return None;
        }
        self.run_of_overruns = 0;
        let next = self.level().weaker()?;
        self.level = Some(next);
        Some(next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_walks_down_one_rung_at_a_time_and_stops_at_the_bottom() {
        let mut ladder = ReliefLadder::default();
        assert_eq!(ladder.level(), Relief::None);

        // The enhancer bends first, the cheap stages go next, and switching
        // the enhancer off is the last resort.
        let expected = [
            Relief::EnhancerReduced,
            Relief::EnhancerLight,
            Relief::NoPaydown,
            Relief::NoPitch,
            Relief::NoFeedback,
            Relief::NoRnnoise,
            Relief::NoAnalyser,
            Relief::NoClassifierTop,
            Relief::NoLiveDots,
            Relief::NoClassifier,
            Relief::EnhancerOff,
        ];
        for want in expected {
            for _ in 0..STEP_DOWN_AFTER - 1 {
                assert_eq!(ladder.note_block(BUDGET_US + 1), None);
            }
            assert_eq!(ladder.note_block(BUDGET_US + 1), Some(want));
        }
        // The bottom is the bottom.
        for _ in 0..STEP_DOWN_AFTER * 2 {
            assert_eq!(ladder.note_block(BUDGET_US + 1), None);
        }
        assert_eq!(ladder.level(), Relief::EnhancerOff);
    }

    #[test]
    fn one_block_inside_the_budget_forgives_the_run() {
        // A run, not a total: a scheduler hiccup must not cost quality on a
        // device that is coping.
        let mut ladder = ReliefLadder::default();
        for _ in 0..STEP_DOWN_AFTER * 10 {
            assert_eq!(ladder.note_block(BUDGET_US + 1), None);
            assert_eq!(ladder.note_block(BUDGET_US), None);
        }
        assert_eq!(ladder.level(), Relief::None);
    }

    #[test]
    fn each_rung_gives_up_everything_the_ones_above_it_did() {
        // The panel draws a stage as disabled from these, so a rung that
        // forgot one would show a stage as running while it was not.
        let mut previous = Relief::None;
        let mut rung = Relief::None;
        while let Some(next) = rung.weaker() {
            assert!(next.skip_pitch() >= previous.skip_pitch());
            assert!(next.skip_feedback() >= previous.skip_feedback());
            assert!(next.skip_rnnoise() >= previous.skip_rnnoise());
            assert!(next.skip_analyser() >= previous.skip_analyser());
            assert!(next.skip_classifier_top() >= previous.skip_classifier_top());
            assert!(next.skip_live_dots() >= previous.skip_live_dots());
            assert!(next.enhancer_rungs() >= previous.enhancer_rungs());
            assert_eq!(next.index(), previous.index() + 1);
            previous = next;
            rung = next;
        }
        assert_eq!(rung, Relief::EnhancerOff);
        assert_eq!(rung.enhancer_rungs(), 3);
    }
}
