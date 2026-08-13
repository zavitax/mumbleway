//! Where a capture block's time goes.
//!
//! # Why this exists
//!
//! The enhancer switched itself off on a low-end Android phone, and the only
//! evidence was its own stopwatch: a hundred consecutive frames over 10 ms.
//! Measured standalone on that same phone the enhancer runs at a **6.23 ms
//! mean with not one frame over budget**, so the model is not what breaks the
//! deadline — something else in the block is, and the enhancer is simply the
//! one stage that was holding a stopwatch.
//!
//! That is the same mistake as the level meter: a number taken at one point in
//! a chain was used to draw a conclusion about the whole of it. Every stage now
//! carries a clock.
//!
//! # What these numbers are, and are not
//!
//! **Wall clock, not CPU time.** A stage that is descheduled by the operating
//! system mid-block is charged for the wait, because from the deadline's point
//! of view that time is gone either way. On a four-core A53 running a Flutter
//! UI, an audio callback and this worker, that is a real and large effect — so
//! a stage looking expensive here means "this is where the block was when the
//! time went", not "this code is slow".
//!
//! [`StageTimings::block`] is what disentangles it. It times the whole
//! iteration, so the difference between it and the sum of the stages is work
//! that is not attributed to any of them plus scheduling. If the stages add up
//! and the total is much bigger, the worker is being starved rather than
//! running slowly.
//!
//! [`StageTimings::backlog`] is the other half: how much audio was already
//! waiting when the block started. A backlog that grows is the definition of a
//! chain that cannot keep up, and it says so before anything is dropped.

use std::time::Instant;

/// The stages of the capture chain worth telling apart.
///
/// Deliberately coarse. A breakdown finer than this would be measuring the
/// clock as much as the code — `Instant::now()` is tens of nanoseconds and
/// these stages are tens of microseconds and up — and it would not change any
/// decision available: each of these can be turned off, made cheaper, or
/// moved, and the ones below them cannot be told apart from outside.
/// Each variant is a **contiguous span of the worker's loop**, in the order it
/// runs, so eight splits tile a block exactly and nothing falls between two
/// stages without being charged to one of them.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Stage {
    /// Everything before the audio is altered: input gain, the peak and level
    /// measurements, the raw spectrum and classifier taps, fetching the echo
    /// reference, and the diagnostic recorder's copy.
    ///
    /// Worth its own line because most of it is optional — the taps only run
    /// while a panel is open and the copy only while recording is on — so this
    /// is the stage a rider can shrink by closing something.
    Input = 0,
    /// Echo cancellation — `CaptureProcessor::cancel_echo`.
    ///
    /// **First, ahead of the enhancer**, because an adaptive filter cannot
    /// learn a room through a neural mask. `engine.rs` carries the argument.
    ///
    /// **Its own line because its cost depends on what the far end is doing,
    /// and nothing else in the chain does.** Measured on the OPPO it is 16 µs
    /// while nobody is playing anything and 970 µs while somebody is talking,
    /// so a phone that keeps up all the way through a monologue can miss the
    /// deadline the moment the conversation becomes one. Folded in with six
    /// stages of near-fixed cost, that reads as a block that went late for no
    /// reason — and the ladder cannot sell what it cannot see.
    Echo,
    /// DeepFilterNet.
    Enhancer,
    /// The filters, RNNoise, the pitch search, the gate, the AGC and the
    /// limiter — `CaptureProcessor::suppress`.
    Suppression,
    /// The howl guard.
    Feedback,
    /// The expander or the spectral subtractor.
    Dehiss,
    /// The onset delay, the transmit envelope, writing the decision log and
    /// the sent tap.
    Transmit,
    /// Opus.
    Encode,
}

pub const STAGES: usize = 8;

/// In [`Stage`] order. Used by the offline harness; the app localises its own.
pub const STAGE_NAMES: [&str; STAGES] = [
    "input",
    "echo",
    "enhancer",
    "suppression",
    "feedback",
    "de-hiss",
    "transmit",
    "encode",
];

/// A stopwatch that reports each split and starts again.
///
/// One `Instant::now()` per stage boundary rather than two, so the gaps
/// between stages are charged to somebody rather than vanishing.
pub struct Lap(Instant);

impl Lap {
    pub fn new() -> Self {
        Self(Instant::now())
    }

    /// Microseconds since the last split, and restart.
    pub fn split(&mut self) -> u32 {
        let now = Instant::now();
        let us = now.duration_since(self.0).as_micros().min(u32::MAX as u128) as u32;
        self.0 = now;
        us
    }
}

impl Default for Lap {
    fn default() -> Self {
        Self::new()
    }
}

/// Running cost per stage, resettable from the panel.
///
/// Sums and a count rather than a decaying average: a mean over a known number
/// of blocks is a number somebody can check by hand, and the panel already has
/// a Reset for choosing the window.
#[derive(Debug, Clone, Copy, Default)]
pub struct StageTimings {
    total_us: [u64; STAGES],
    worst_us: [u32; STAGES],
    block_total_us: u64,
    block_worst_us: u32,
    backlog_total_ms: f64,
    backlog_worst_ms: f32,
    blocks: u64,
}

impl StageTimings {
    /// Charges a stage for the time it just took.
    pub fn record(&mut self, stage: Stage, us: u32) {
        let i = stage as usize;
        self.total_us[i] += us as u64;
        if us > self.worst_us[i] {
            self.worst_us[i] = us;
        }
    }

    /// Closes a block: the whole iteration, and what was queued when it began.
    pub fn block(&mut self, us: u32, backlog_ms: f32) {
        self.block_total_us += us as u64;
        if us > self.block_worst_us {
            self.block_worst_us = us;
        }
        self.backlog_total_ms += backlog_ms as f64;
        if backlog_ms > self.backlog_worst_ms {
            self.backlog_worst_ms = backlog_ms;
        }
        self.blocks += 1;
    }

    pub fn blocks(&self) -> u64 {
        self.blocks
    }

    /// Mean microseconds for a stage over the blocks counted so far.
    pub fn mean_us(&self, stage: Stage) -> f32 {
        if self.blocks == 0 {
            return 0.0;
        }
        self.total_us[stage as usize] as f32 / self.blocks as f32
    }

    pub fn worst_us(&self, stage: Stage) -> u32 {
        self.worst_us[stage as usize]
    }

    /// Mean and worst for the whole iteration, in microseconds.
    pub fn block_mean_us(&self) -> f32 {
        if self.blocks == 0 {
            return 0.0;
        }
        self.block_total_us as f32 / self.blocks as f32
    }

    pub fn block_worst_us(&self) -> u32 {
        self.block_worst_us
    }

    /// How much audio was waiting when a block started, in milliseconds.
    ///
    /// **The number that says whether the chain is keeping up.** Everything
    /// else here is a cost; this is the consequence. A mean near zero with a
    /// small worst is a chain with headroom whatever the stages cost, and a
    /// backlog that climbs is one that has already lost, before a single
    /// sample has been dropped.
    pub fn backlog_mean_ms(&self) -> f32 {
        if self.blocks == 0 {
            return 0.0;
        }
        (self.backlog_total_ms / self.blocks as f64) as f32
    }

    pub fn backlog_worst_ms(&self) -> f32 {
        self.backlog_worst_ms
    }

    /// What the stages did not account for, on average, in microseconds.
    ///
    /// Scheduling, the queue handling, and anything without a clock on it.
    /// Large here means the worker is being interrupted rather than working
    /// slowly, and no amount of making a stage cheaper will help.
    pub fn unattributed_us(&self) -> f32 {
        let stages: u64 = self.total_us.iter().sum();
        if self.blocks == 0 {
            return 0.0;
        }
        (self.block_total_us.saturating_sub(stages)) as f32 / self.blocks as f32
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stage_reports_its_mean_and_its_worst() {
        let mut t = StageTimings::default();
        for us in [100u32, 300, 200] {
            t.record(Stage::Suppression, us);
            t.block(us, 0.0);
        }
        assert_eq!(t.blocks(), 3);
        assert!((t.mean_us(Stage::Suppression) - 200.0).abs() < 0.01);
        assert_eq!(t.worst_us(Stage::Suppression), 300);
    }

    #[test]
    fn what_the_stages_do_not_account_for_is_reported_rather_than_hidden() {
        // The block took 1000 us and the stages own 400 of it. The other 600
        // is scheduling, and it is the difference between "this code is slow"
        // and "this phone is busy" — which call for opposite fixes.
        let mut t = StageTimings::default();
        t.record(Stage::Enhancer, 300);
        t.record(Stage::Encode, 100);
        t.block(1000, 0.0);
        assert!((t.unattributed_us() - 600.0).abs() < 0.01);
    }

    #[test]
    fn a_reset_forgets_everything_including_the_worst() {
        let mut t = StageTimings::default();
        t.record(Stage::Enhancer, 9999);
        t.block(9999, 12.0);
        t.reset();
        assert_eq!(t.blocks(), 0);
        assert_eq!(t.worst_us(Stage::Enhancer), 0);
        assert_eq!(t.backlog_worst_ms(), 0.0);
        assert_eq!(t.block_worst_us(), 0);
    }

    #[test]
    fn nothing_divides_by_zero_before_the_first_block() {
        let t = StageTimings::default();
        assert_eq!(t.mean_us(Stage::Enhancer), 0.0);
        assert_eq!(t.block_mean_us(), 0.0);
        assert_eq!(t.backlog_mean_ms(), 0.0);
        assert_eq!(t.unattributed_us(), 0.0);
    }

    #[test]
    fn the_lap_charges_the_gaps_between_stages_to_somebody() {
        // One clock read per boundary means consecutive splits tile the block
        // with no holes. If this ever became two reads per stage, the time
        // between them would silently leave the accounting.
        let mut lap = Lap::new();
        std::thread::sleep(std::time::Duration::from_millis(2));
        let first = lap.split();
        std::thread::sleep(std::time::Duration::from_millis(2));
        let second = lap.split();
        assert!(first >= 1000, "first split was {first} us");
        assert!(second >= 1000, "second split was {second} us");
    }
}
