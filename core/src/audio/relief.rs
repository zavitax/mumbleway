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

    /// The echo canceller's filter is halved, 1 024 taps to 512.
    ///
    /// **Shortened, never switched off.** Losing echo cancellation on a
    /// speakerphone is a howl, and the feedback guard that would cover for it
    /// is given up two rungs below this — so by the time anything could
    /// justify dropping the canceller there would be nothing left holding the
    /// loop open. Halving keeps the direct path, which is the loud part, and
    /// gives up about 10 ms of tail.
    ///
    /// Measured over a 138.8 s ride clip, per block, while actually
    /// cancelling:
    ///
    /// | taps | mean | p99 | share of 10 ms |
    /// |---|---|---|---|
    /// | 1 024 | 612 µs | 3 083 µs | 6.1% |
    /// | 512 | **131 µs** | **188 µs** | 1.3% |
    ///
    /// So the rung is worth **481 µs**, which puts it beside the feedback
    /// guard and RNNoise rather than among the rounding errors — and the p99
    /// improvement is the larger part of it.
    ///
    /// It sits above `NoPitch` because it is the smallest loss of the four
    /// audio rungs: the others each remove a whole stage, and this one leaves
    /// the stage working on a shorter path. The position is reasoned from that
    /// and from the measurement above; it has not been re-measured on a device
    /// in the ladder's own terms, which is what `chain_cost` on the phone
    /// would settle.
    ShortAec,

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
    /// The spectrum analyser stops easing its bars down; each bar sits where
    /// the newest frame puts it.
    ///
    /// **The cheapest display rung, so it is the first.** The decay is pure
    /// animation: any bar above its new value is redrawn every vsync until it
    /// arrives, so a panel with nothing happening in it still repaints on a
    /// phone that is missing audio deadlines. The reading is untouched — this
    /// gives up the *fall*, not the measurement, which is why it sits above
    /// [`Relief::NoAnalyser`] rather than replacing it.
    ///
    /// What it costs is small and real: a peak that used to linger long enough
    /// to see now goes with the frame that made it.
    NoAnalyserDecay,
    /// The spectrum analyser stops computing, in the core as well as on
    /// screen. The panel says why in its place.
    NoAnalyser,
    /// The classifier's top three classes stop being shown. The model still
    /// runs — `Auto` reads its verdict — so this is the display only.
    NoClassifierTop,
    /// The chain dots stop following the audio. Warnings and the rung keep
    /// updating, or this rung could hide the one after it.
    NoLiveDots,

    /// Speakers show only *that* they are talking, not how loudly.
    ///
    /// One meter per participant, each moving with every incoming frame, so a
    /// busy channel is several animated widgets at once. What is given up is
    /// the amount; who is speaking still shows, which is the part a rider is
    /// actually reading.
    ///
    /// **Last of the display rungs, because it is the only one on the main
    /// screen.** Everything above it lives in the diagnostics panel, which is
    /// open only when somebody has gone looking; this is visible to a rider
    /// who never opens that panel at all. So it goes after every diagnostic
    /// display and still before anything that changes what the far end hears.
    NoParticipantMeters,

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

    /// The enhancer swaps to the plain DeepFilterNet 3 — a third of the cost
    /// per frame, and harder on a quiet voice.
    ///
    /// **The rung that exists because the alternative was losing the enhancer
    /// altogether.** Everything above this has been given up and the device is
    /// still late; the only thing left was `EnhancerOff`, which costs the
    /// feature that turns 1.5 dB of speech-to-gap separation into 16. Measured
    /// against the low-latency model this ships, on three rides:
    ///
    /// | | DFN3-ll | plain |
    /// |---|---|---|
    /// | cost per frame, mean | 2.63 ms | **0.88 ms** |
    /// | p99 | 9.77 ms | **3.83 ms** |
    /// | separation, quiet ride | **4.6 dB** | 3.3 dB |
    /// | separation, road | 18.6 dB | **20.3 dB** |
    /// | cut from the vowel | −5.8 to −10.0 | −11.8 to −14.1 |
    ///
    /// So it is not a worse model, it is a **more aggressive** one: it takes 4
    /// to 6 dB more out of the speech, which wins where the background is loud
    /// enough to be worth removing and loses where it is not. A poor default —
    /// which is why it is not one — and far better than nothing, which is what
    /// the rung below offers.
    ///
    /// It also costs **20 ms** of algorithmic latency, because the plain model
    /// holds two frames of look-ahead where the low-latency one holds none.
    /// [`super::paydown`] bought 36 ms back, so a device on this rung is still
    /// ahead of where it was before that shipped.
    SimpleModel,

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
            Relief::NoPaydown => Relief::ShortAec,
            Relief::ShortAec => Relief::NoPitch,
            Relief::NoPitch => Relief::NoFeedback,
            Relief::NoFeedback => Relief::NoRnnoise,
            Relief::NoRnnoise => Relief::NoAnalyserDecay,
            Relief::NoAnalyserDecay => Relief::NoAnalyser,
            Relief::NoAnalyser => Relief::NoClassifierTop,
            Relief::NoClassifierTop => Relief::NoLiveDots,
            Relief::NoLiveDots => Relief::NoParticipantMeters,
            Relief::NoParticipantMeters => Relief::NoClassifier,
            Relief::NoClassifier => Relief::SimpleModel,
            Relief::SimpleModel => Relief::EnhancerOff,
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
            Relief::ShortAec => 4,
            Relief::NoPitch => 5,
            Relief::NoFeedback => 6,
            Relief::NoRnnoise => 7,
            Relief::NoAnalyserDecay => 8,
            Relief::NoAnalyser => 9,
            Relief::NoClassifierTop => 10,
            Relief::NoLiveDots => 11,
            Relief::NoParticipantMeters => 12,
            Relief::NoClassifier => 13,
            Relief::SimpleModel => 14,
            Relief::EnhancerOff => 15,
        }
    }

    /// The rung with this index, for carrying one across an atomic or the FFI.
    ///
    /// Round-trips with [`Self::index`]; a test asserts it for every rung, so
    /// inserting one in the middle cannot silently renumber a stored value.
    pub fn from_index(i: u8) -> Option<Relief> {
        let mut r = Relief::None;
        loop {
            if r.index() == i {
                return Some(r);
            }
            r = r.weaker()?;
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

    /// The enhancer runs the cheap model. See [`Relief::SimpleModel`].
    pub fn simple_model(self) -> bool {
        self >= Relief::SimpleModel
    }

    /// The analyser's bars stop easing down. See [`Relief::NoAnalyserDecay`].
    ///
    /// True once the analyser is switched off entirely as well, which is what
    /// `>=` buys: a rung that stops the whole display must not report that the
    /// animation is still running.
    pub fn skip_analyser_decay(self) -> bool {
        self >= Relief::NoAnalyserDecay
    }

    /// Speakers show only that they are talking, not how loudly. See
    /// [`Relief::NoParticipantMeters`].
    pub fn skip_participant_meters(self) -> bool {
        self >= Relief::NoParticipantMeters
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

    /// Whether the echo canceller should run its short filter.
    pub fn short_aec(self) -> bool {
        self >= Relief::ShortAec
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

/// Share of the whole device, above which the CPU is called saturated.
///
/// See [`crate::usage`] for what the number means: it is the device, not one
/// core, so 90 is "nine tenths of every core this phone has".
pub const CPU_BUSY_PERCENT: f32 = 90.0;

/// How long the CPU has to stay above [`CPU_BUSY_PERCENT`] before a rung goes.
///
/// Five seconds, which is far longer than the block deadline's one second on
/// purpose. **This is a backstop, not the main signal.** The deadline catches
/// the thing that actually hurts a rider — a block returned late — and it does
/// so within a second. Whole-process CPU is a slower, blunter statement about
/// the phone as a whole, and something that transient must not be allowed to
/// sell quality: a burst while the map redraws is not a device that cannot
/// cope.
pub const CPU_BUSY_SECONDS: f32 = 5.0;

// A third condition used to live here: any single core at or above 75% for
// five seconds cost a rung. **Removed as impractical**, and the reasoning that
// justified it is worth keeping so it is not reinvented.
//
// The argument was sound in the abstract — the capture worker is one thread, a
// thread runs on one core at a time, and a phone with eight cores can be
// comfortable overall while the one core that matters is pinned. The whole
// device rarely reaches 90%, so this was the condition that could actually
// fire.
//
// What it measures, though, is **the device's cores, not ours**: no platform
// here will attribute a process's CPU time to particular cores, so anything
// else on the machine pinning a core stepped this ladder down — permanently,
// since the ladder never climbs back. On a desktop that is not an edge case
// but the ordinary state. It fired on an i7-12700H at 30% overall and on a
// MacBook whose enhancer measured 1.90 ms against a 9 ms budget: three
// machines out of three, none of which was struggling.
//
// A condition that cannot tell "this device is too slow" from "this device is
// busy with something else" is not measuring what the ladder is for. The block
// deadline already answers within a second and is about *our* work, which is
// the thing being judged.

/// How far the echo canceller has been shortened *below* the bottom of the
/// ladder, to make the block fit.
///
/// # Why this is not a set of rungs
///
/// Every rung of [`Relief`] costs the same thing on every device: the pitch
/// search costs a pitch search. The canceller does not. It is 16 µs a block
/// while nobody is playing anything and 970 while the far end talks — measured
/// on the OPPO, `core/tests/aec_cost.rs` — so its cost is set by *the other end
/// of the call*, and a rung whose price depends on whether somebody else is
/// speaking is not a rung.
///
/// That asymmetry is also the fault this exists to fix. A phone that keeps up
/// through a monologue starts missing blocks the moment the conversation
/// becomes two-way, and the ladder answers by selling the enhancer, then the
/// pitch search, then RNNoise — none of which is what went over. So this is a
/// separate tail, entered only when the ladder has nothing left, and only when
/// the canceller is demonstrably the reason the block did not fit.
///
/// # Why it stops rather than reaching zero
///
/// [`Relief::ShortAec`] already argues it: losing echo cancellation on a
/// speakerphone is a howl, and the feedback guard that would cover for it is
/// given up long before this point, so by the bottom of the ladder there would
/// be nothing left holding the loop open. Shortening is not the same trade —
/// [`super::aec::Aligner`] has already pointed the filter at the direct
/// arrival, so what a shorter filter gives up is the *tail* of the echo, and
/// even 128 taps still cancels the loud part.
///
/// Priced off the measured line, ≈0.95 µs per tap per block. The canceller is
/// necessarily already at 512 taps here, because `ShortAec` is rung 4:
///
/// | cut | taps | covers | saves |
/// |---|---|---|---|
/// | `Taps384` | 384 | 8.0 ms | ~104 µs |
/// | `Taps256` | 256 | 5.3 ms | ~212 µs |
/// | `Taps128` | 128 | 2.7 ms | ~319 µs |
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum AecCut {
    /// The filter is whatever the rung says it should be.
    #[default]
    None,
    Taps384,
    Taps256,
    Taps128,
}

impl AecCut {
    /// The next cut down, or `None` at the floor.
    fn weaker(self) -> Option<AecCut> {
        Some(match self {
            AecCut::None => AecCut::Taps384,
            AecCut::Taps384 => AecCut::Taps256,
            AecCut::Taps256 => AecCut::Taps128,
            AecCut::Taps128 => return None,
        })
    }
}

/// What the ladder just gave up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// A rung of the ladder proper.
    Rung(Relief),
    /// A shorter echo filter, below the bottom of the ladder. See [`AecCut`].
    Aec(AecCut),
}

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
    /// The run's block and echo-canceller time, so the decision below the
    /// ladder is made on a hundred blocks rather than on whichever one happened
    /// to be the hundredth. Same reasoning as "read the mean, not the worst" in
    /// [`Relief`]'s own table.
    run_block_us: u64,
    run_echo_us: u64,
    /// Seconds the CPU has been continuously at or above the busy mark.
    busy_seconds: f32,
    cut: AecCut,
}

impl ReliefLadder {
    /// A ladder that begins part-way down, because the device was asked before
    /// the call started. See [`super::probe`].
    ///
    /// **Only ever a starting point.** The ladder still owns every decision
    /// after this one and still only ever falls, so a probe that was too
    /// generous is corrected by the same mechanism as before — it just costs a
    /// second of a real conversation instead of nothing. A probe that was too
    /// harsh costs quality that was affordable, which is why the budget it dials
    /// to has a millisecond of headroom rather than none.
    pub fn starting_at(level: Relief) -> Self {
        Self {
            level: (level != Relief::None).then_some(level),
            run_of_overruns: 0,
            run_block_us: 0,
            run_echo_us: 0,
            busy_seconds: 0.0,
            cut: AecCut::None,
        }
    }

    pub fn level(&self) -> Relief {
        self.level.unwrap_or(Relief::None)
    }

    /// How long the echo canceller's filter should be, in taps.
    ///
    /// One number with one owner. The rung halves it at [`Relief::ShortAec`]
    /// and the tail below the ladder shortens it further, and if those were two
    /// setters they could disagree about which of them last had the say.
    pub fn aec_taps(&self) -> usize {
        match self.cut {
            AecCut::None => {
                if self.level().short_aec() {
                    super::aec::DEFAULT_TAPS / 2
                } else {
                    super::aec::DEFAULT_TAPS
                }
            }
            AecCut::Taps384 => 384,
            AecCut::Taps256 => 256,
            AecCut::Taps128 => 128,
        }
    }

    /// How far the canceller has been cut below the ladder. `None` on almost
    /// every device.
    pub fn aec_cut(&self) -> AecCut {
        self.cut
    }

    /// Feeds one block's wall-clock cost in, and the echo canceller's share of
    /// it. Returns what was given up when something was, so the caller can say
    /// so once rather than every block.
    ///
    /// `echo_us` is [`super::timing::Stage::Echo`], and it is passed separately
    /// rather than read out of the timings because it is the one stage whose
    /// cost the ladder has to be able to attribute — see [`AecCut`].
    pub fn note_block(&mut self, us: u32, echo_us: u32) -> Option<Step> {
        if us <= BUDGET_US {
            self.forget_the_run();
            return None;
        }
        self.run_of_overruns += 1;
        self.run_block_us += us as u64;
        self.run_echo_us += echo_us as u64;
        if self.run_of_overruns < STEP_DOWN_AFTER {
            return None;
        }
        let (block, echo) = (self.run_block_us, self.run_echo_us);
        let blocks = self.run_of_overruns as u64;
        self.forget_the_run();
        match self.step() {
            Some(rung) => Some(Step::Rung(rung)),
            // Nothing left on the ladder. See [`AecCut`] for why the canceller
            // is not simply another rung.
            None => self
                .cut_the_canceller(block / blocks, echo / blocks)
                .map(Step::Aec),
        }
    }

    fn forget_the_run(&mut self) {
        self.run_of_overruns = 0;
        self.run_block_us = 0;
        self.run_echo_us = 0;
    }

    /// Shortens the echo filter, but only when it is why the block did not fit.
    ///
    /// **"Due to the AEC" made checkable:** would the block have fitted without
    /// the canceller at all? If it would not, the canceller is not what is
    /// costing the deadline, and shortening it would sell echo cancellation for
    /// nothing — which is the rule the whole ladder is built on and the reason
    /// the per-core CPU condition was removed.
    fn cut_the_canceller(&mut self, block_us: u64, echo_us: u64) -> Option<AecCut> {
        if block_us.saturating_sub(echo_us) > BUDGET_US as u64 {
            return None;
        }
        let next = self.cut.weaker()?;
        self.cut = next;
        Some(next)
    }

    /// Feeds a CPU reading in, with the time since the previous one.
    ///
    /// `percent` is a share of the **whole device**, from [`crate::usage`] —
    /// not of one core, which is what every platform API reports natively and
    /// what made the Android panel read 146%.
    ///
    /// **The second condition, and the weaker of the two.** A block returned
    /// late is a rider hearing a gap; a phone at 90% is a phone that might be
    /// about to cause one, and might equally be drawing a map. So it needs
    /// five seconds where the deadline needs one, and like the deadline a
    /// single reading below the mark forgives the run entirely — this measures
    /// a device that is *staying* saturated, not one that touched it.
    ///
    /// It can only ever cost a rung, never a cut to the canceller: a saturated
    /// CPU says nothing about *which* stage is holding the block, and the tail
    /// below the ladder exists precisely because that attribution is available
    /// for the deadline and not here.
    pub fn note_cpu(&mut self, percent: f32, elapsed_seconds: f32) -> Option<Step> {
        if !percent.is_finite() || percent < CPU_BUSY_PERCENT {
            self.busy_seconds = 0.0;
            return None;
        }
        // Guards a clock that went backwards or stood still: neither should
        // count towards five seconds of evidence.
        if elapsed_seconds > 0.0 {
            self.busy_seconds += elapsed_seconds;
        }
        if self.busy_seconds < CPU_BUSY_SECONDS {
            return None;
        }
        self.busy_seconds = 0.0;
        self.step().map(Step::Rung)
    }

    /// Gives up the next rung, if there is one left.
    fn step(&mut self) -> Option<Relief> {
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
            Relief::ShortAec,
            Relief::NoPitch,
            Relief::NoFeedback,
            Relief::NoRnnoise,
            Relief::NoAnalyserDecay,
            Relief::NoAnalyser,
            Relief::NoClassifierTop,
            Relief::NoLiveDots,
            Relief::NoParticipantMeters,
            Relief::NoClassifier,
            Relief::SimpleModel,
            Relief::EnhancerOff,
        ];
        for want in expected {
            for _ in 0..STEP_DOWN_AFTER - 1 {
                assert_eq!(ladder.note_block(BUDGET_US + 1, 0), None);
            }
            assert_eq!(ladder.note_block(BUDGET_US + 1, 0), Some(Step::Rung(want)));
        }
        // The bottom is the bottom — with the canceller costing nothing, which
        // is what stops the tail below it from firing. See the tests for that.
        for _ in 0..STEP_DOWN_AFTER * 2 {
            assert_eq!(ladder.note_block(BUDGET_US + 1, 0), None);
        }
        assert_eq!(ladder.level(), Relief::EnhancerOff);
        assert_eq!(ladder.aec_cut(), AecCut::None);
    }

    #[test]
    fn five_seconds_of_a_saturated_cpu_costs_a_rung() {
        let mut ladder = ReliefLadder::default();
        // Four seconds is not five, however it is sliced.
        for _ in 0..8 {
            assert_eq!(ladder.note_cpu(95.0, 0.5), None);
        }
        assert_eq!(ladder.level(), Relief::None);
        // The ninth half-second is still 4.5; the tenth reaches five.
        assert_eq!(ladder.note_cpu(95.0, 0.5), None);
        assert_eq!(
            ladder.note_cpu(95.0, 0.5),
            Some(Step::Rung(Relief::EnhancerReduced))
        );
    }

    #[test]
    fn a_cpu_below_the_mark_forgives_the_run() {
        // The same rule the deadline follows, and for the same reason: a phone
        // that touched 90% while a map redrew is not a phone that cannot cope,
        // and selling quality for it would be selling it for nothing.
        let mut ladder = ReliefLadder::default();
        for _ in 0..9 {
            assert_eq!(ladder.note_cpu(99.0, 0.5), None);
        }
        assert_eq!(ladder.note_cpu(12.0, 0.5), None);
        for _ in 0..9 {
            assert_eq!(ladder.note_cpu(99.0, 0.5), None);
        }
        assert_eq!(ladder.level(), Relief::None, "the run restarted");
    }

    #[test]
    fn a_stopped_clock_is_not_evidence() {
        // Five seconds of readings with no time between them is one reading.
        let mut ladder = ReliefLadder::default();
        for _ in 0..100 {
            assert_eq!(ladder.note_cpu(100.0, 0.0), None);
        }
        assert_eq!(ladder.level(), Relief::None);
    }

    #[test]
    fn the_cpu_condition_walks_the_same_ladder_and_stops_at_the_bottom() {
        // Not a separate ladder: the two conditions give up the same rungs in
        // the same order, so a device failing both does not skip any.
        let mut ladder = ReliefLadder::default();
        for _ in 0..40 {
            ladder.note_cpu(100.0, 5.0);
        }
        assert_eq!(ladder.level(), Relief::EnhancerOff);
    }

    #[test]
    fn one_block_inside_the_budget_forgives_the_run() {
        // A run, not a total: a scheduler hiccup must not cost quality on a
        // device that is coping.
        let mut ladder = ReliefLadder::default();
        for _ in 0..STEP_DOWN_AFTER * 10 {
            assert_eq!(ladder.note_block(BUDGET_US + 1, 0), None);
            assert_eq!(ladder.note_block(BUDGET_US, 0), None);
        }
        assert_eq!(ladder.level(), Relief::None);
    }

    /// Walks a ladder to its last rung, cheaply.
    fn at_the_bottom() -> ReliefLadder {
        let ladder = ReliefLadder::starting_at(Relief::EnhancerOff);
        assert_eq!(ladder.level().weaker(), None, "not the bottom any more");
        assert_eq!(ladder.aec_cut(), AecCut::None);
        ladder
    }

    /// Feeds a run of late blocks and returns whatever the last one cost.
    fn a_run_of(ladder: &mut ReliefLadder, block_us: u32, echo_us: u32) -> Option<Step> {
        let mut last = None;
        for _ in 0..STEP_DOWN_AFTER {
            last = ladder.note_block(block_us, echo_us);
        }
        last
    }

    #[test]
    fn below_the_bottom_the_canceller_is_shortened_a_step_at_a_time() {
        let mut ladder = at_the_bottom();
        // Late by 500 µs with the canceller costing 600: it would have fitted
        // without it, so it is the reason.
        for want in [AecCut::Taps384, AecCut::Taps256, AecCut::Taps128] {
            assert_eq!(
                a_run_of(&mut ladder, BUDGET_US + 500, 600),
                Some(Step::Aec(want))
            );
        }
        // 128 taps is the floor. Losing the canceller outright on a
        // speakerphone is a howl, and the feedback guard went eleven rungs ago.
        for _ in 0..STEP_DOWN_AFTER * 3 {
            assert_eq!(ladder.note_block(BUDGET_US + 500, 600), None);
        }
        assert_eq!(ladder.aec_cut(), AecCut::Taps128);
    }

    #[test]
    fn a_block_that_would_be_late_anyway_costs_the_canceller_nothing() {
        // **The condition that makes this defensible.** A device 5 ms over
        // budget with a canceller costing 600 µs is not late because of the
        // canceller, and shortening it would sell echo cancellation for
        // nothing — the same rule that removed the per-core CPU condition.
        let mut ladder = at_the_bottom();
        for _ in 0..STEP_DOWN_AFTER * 5 {
            assert_eq!(ladder.note_block(BUDGET_US + 5_000, 600), None);
        }
        assert_eq!(ladder.aec_cut(), AecCut::None);
    }

    #[test]
    fn the_canceller_is_not_cut_while_the_ladder_still_has_rungs() {
        // Rungs first, always. A device that has not yet given up the pitch
        // search has cheaper things to sell than the echo it is cancelling.
        let mut ladder = ReliefLadder::default();
        assert_eq!(
            a_run_of(&mut ladder, BUDGET_US + 500, 600),
            Some(Step::Rung(Relief::EnhancerReduced))
        );
        assert_eq!(ladder.aec_cut(), AecCut::None);
    }

    #[test]
    fn the_filter_length_is_the_rung_and_the_cut_resolved_into_one_number() {
        use super::super::aec::DEFAULT_TAPS;
        let mut ladder = ReliefLadder::default();
        assert_eq!(ladder.aec_taps(), DEFAULT_TAPS);

        // The rung halves it.
        ladder = ReliefLadder::starting_at(Relief::ShortAec);
        assert_eq!(ladder.aec_taps(), DEFAULT_TAPS / 2);
        // And every rung below it, since the ladder never climbs.
        ladder = ReliefLadder::starting_at(Relief::EnhancerOff);
        assert_eq!(ladder.aec_taps(), DEFAULT_TAPS / 2);

        // The tail takes it further.
        for want in [384, 256, 128] {
            a_run_of(&mut ladder, BUDGET_US + 500, 600);
            assert_eq!(ladder.aec_taps(), want);
        }
    }

    #[test]
    fn the_decision_is_made_on_the_run_rather_than_on_its_last_block() {
        // A hundred blocks that are late for other reasons, and one that is
        // late because of the canceller, is not evidence about the canceller.
        // The ladder reads means for the same reason its own table does.
        let mut ladder = at_the_bottom();
        for _ in 0..STEP_DOWN_AFTER - 1 {
            assert_eq!(ladder.note_block(BUDGET_US + 5_000, 10), None);
        }
        assert_eq!(ladder.note_block(BUDGET_US + 500, 600), None);
        assert_eq!(ladder.aec_cut(), AecCut::None);
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
