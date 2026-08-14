//! Backing the rider's input gain off when it drives the input into clipping.
//!
//! The gain slider is a boost applied before any of the DSP
//! (`engine.rs`, "Microphone gain goes in ahead of the DSP chain"), and nothing
//! stopped it being set higher than the microphone can take. Past full scale
//! the samples are flat-topped before RNNoise, the gate or the canceller ever
//! see them, and none of those can undo it — a clipped sample has lost the
//! information, not merely gained level.
//!
//! **This is not the AGC and not the limiter, and the difference is where they
//! sit.** Both of those act on the chain's *output*, after the damage: the AGC
//! makes a quiet voice louder, the limiter stops the result overshooting. Only
//! this one can prevent the clipping, because it is the only thing upstream of
//! it.
//!
//! Three properties are deliberate:
//!
//! * **It only ever undoes a boost.** The floor is unity. With the slider at or
//!   below 0 dB there is nothing of ours to take off, and attenuating further
//!   would be inventing a setting nobody asked for. It also means a microphone
//!   clipping in hardware — arriving at full scale before we touch it — walks
//!   the trim to the floor and stops, rather than chasing a fault it cannot fix.
//! * **It is runtime only.** The rider's setting is untouched and is what gets
//!   saved; this rides on top of it and starts at zero every launch. A rider who
//!   set the slider deliberately finds their number where they left it.
//! * **It gives the gain back.** Lowering on a cough or a gust and never
//!   recovering would walk a correctly-set gain down to unity over a long ride,
//!   one transient at a time, and the rider would only find out by noticing they
//!   had gone quiet.

use super::denoise::{FRAME_SIZE, SAMPLE_RATE};

/// Blocks per second: 100, at 10 ms a block.
const BLOCKS_PER_SECOND: u32 = SAMPLE_RATE / FRAME_SIZE as u32;

/// How fast the trim comes off while the input is clipping.
///
/// 0.1 dB a block is 10 dB/s, so the whole of a +30 dB boost takes three
/// seconds and a more typical +6 dB is gone in under one. Slow enough that the
/// correction is a fade nobody hears as a step — a fast jump mid-word is its
/// own artefact, and the clipping it is fixing lasts only until it arrives.
const ATTACK_DB_PER_BLOCK: f32 = 0.1;

/// Clean audio required before any of the trim is given back, in blocks.
///
/// Three seconds. Short enough that a one-off knock is forgiven within a
/// breath, long enough that it does not start climbing back into a passage that
/// is still peaking every second or so.
const HOLD_BLOCKS: u32 = 3 * BLOCKS_PER_SECOND;

/// Blocks to give back one dB.
///
/// Twelve seconds a dB — 1/12 dB/s against the attack's 10, so it goes back
/// about 120 times slower than it came off. Asymmetry is the point: clipping is
/// audible immediately and the correction should be too, while the return
/// should be slow enough that nobody hears it happening and it cannot oscillate
/// against a source that clips periodically.
const RECOVERY_BLOCKS_PER_DB: u32 = 12 * BLOCKS_PER_SECOND;

const RECOVERY_DB_PER_BLOCK: f32 = 1.0 / RECOVERY_BLOCKS_PER_DB as f32;

/// Watches the post-gain block for clipping and holds the correction.
#[derive(Debug, Clone)]
pub struct ClipGuard {
    /// Always `<= 0`. Added to the rider's gain, never replacing it.
    trim_db: f32,
    /// Consecutive blocks with no clipped sample in them.
    clean_blocks: u32,
}

impl ClipGuard {
    pub fn new() -> Self {
        Self {
            trim_db: 0.0,
            clean_blocks: 0,
        }
    }

    /// What to add to the rider's gain, in dB. Never positive.
    pub fn trim_db(&self) -> f32 {
        self.trim_db
    }

    /// Whether the guard is currently holding anything back.
    ///
    /// The panel asks, so that a row appears only when there is something to
    /// say. The comparison is against a threshold rather than zero because the
    /// recovery ramp lands on values like `-1e-7` on its way home.
    pub fn engaged(&self) -> bool {
        self.trim_db < -0.05
    }

    /// Fold in one block's worth of evidence.
    ///
    /// `clipped` is the count of samples at or past full scale **after** the
    /// gain was applied — the same count the level meter is fed, which is what
    /// makes this a measurement of the gain in force rather than of the
    /// microphone alone. `user_gain_db` is the rider's setting, which is what
    /// bounds how much can be taken off.
    pub fn observe(&mut self, clipped: u32, user_gain_db: f32) {
        // The most that can be removed: enough to bring the boost back to
        // unity, and no further.
        let floor = -(user_gain_db.max(0.0));

        if clipped > 0 {
            self.clean_blocks = 0;
            self.trim_db -= ATTACK_DB_PER_BLOCK;
        } else {
            self.clean_blocks = self.clean_blocks.saturating_add(1);
            if self.clean_blocks > HOLD_BLOCKS {
                self.trim_db += RECOVERY_DB_PER_BLOCK;
            }
        }

        // Clamped last, and against a floor read fresh each block, so that a
        // rider moving the slider down mid-ride takes the trim with it rather
        // than leaving a correction sized for a boost that is no longer there.
        self.trim_db = self.trim_db.clamp(floor, 0.0);
    }

    /// Forget the correction — on a device change, where the old one described
    /// a microphone that is no longer connected.
    pub fn reset(&mut self) {
        self.trim_db = 0.0;
        self.clean_blocks = 0;
    }
}

impl Default for ClipGuard {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Drive `n` blocks with a fixed clipped count.
    fn run(g: &mut ClipGuard, blocks: u32, clipped: u32, gain_db: f32) {
        for _ in 0..blocks {
            g.observe(clipped, gain_db);
        }
    }

    #[test]
    fn a_clean_input_is_left_alone() {
        let mut g = ClipGuard::new();
        run(&mut g, 1000, 0, 12.0);
        assert_eq!(g.trim_db(), 0.0);
        assert!(!g.engaged());
    }

    #[test]
    fn clipping_takes_the_gain_off_progressively() {
        let mut g = ClipGuard::new();
        g.observe(3, 12.0);
        let after_one = g.trim_db();
        assert!(
            after_one < 0.0,
            "one clipped block should already move it, got {after_one}"
        );
        g.observe(3, 12.0);
        assert!(
            g.trim_db() < after_one,
            "it should keep going while the clipping does"
        );
        assert!(g.engaged());
    }

    /// The floor is unity: the guard undoes the rider's boost and stops there.
    #[test]
    fn it_never_attenuates_past_the_riders_boost() {
        let mut g = ClipGuard::new();
        run(&mut g, 10_000, 50, 12.0);
        assert_eq!(
            g.trim_db(),
            -12.0,
            "a +12 dB boost should come off entirely and no further"
        );
    }

    /// A microphone clipping in hardware arrives at full scale whatever we do,
    /// so the guard cannot fix it. It must bottom out rather than chase it.
    #[test]
    fn hardware_clipping_bottoms_out_instead_of_chasing() {
        let mut g = ClipGuard::new();
        run(&mut g, 100_000, 480, 6.0);
        assert_eq!(g.trim_db(), -6.0);
    }

    /// With the slider at or below unity there is nothing of ours to remove.
    #[test]
    fn no_boost_means_nothing_to_give_back() {
        for gain in [0.0, -6.0] {
            let mut g = ClipGuard::new();
            run(&mut g, 500, 50, gain);
            assert_eq!(g.trim_db(), 0.0, "at {gain} dB there is no boost to undo");
        }
    }

    #[test]
    fn the_gain_comes_back_after_sustained_clean_audio() {
        let mut g = ClipGuard::new();
        // 30 blocks is 0.3 s of clipping, which at 10 dB/s is a 3 dB dip --
        // comfortably clear of the recovery step this test then measures.
        run(&mut g, 30, 5, 12.0);
        let dipped = g.trim_db();
        assert!(dipped < -1.0, "expected a real dip, got {dipped}");

        // Nothing during the hold.
        run(&mut g, HOLD_BLOCKS, 0, 12.0);
        assert!(
            (g.trim_db() - dipped).abs() < 1e-6,
            "the hold should keep it still, got {} from {dipped}",
            g.trim_db()
        );

        // Then it climbs, at about a dB every twelve seconds.
        run(&mut g, RECOVERY_BLOCKS_PER_DB, 0, 12.0);
        let after = g.trim_db();
        assert!(
            after > dipped,
            "it should be climbing back, {after} vs {dipped}"
        );
        assert!(
            (after - (dipped + 1.0)).abs() < 0.05,
            "expected about a dB back, got {}",
            after - dipped
        );
    }

    #[test]
    fn recovery_stops_at_the_riders_setting_and_goes_no_higher() {
        let mut g = ClipGuard::new();
        run(&mut g, 8, 5, 12.0);
        run(&mut g, HOLD_BLOCKS + 100 * RECOVERY_BLOCKS_PER_DB, 0, 12.0);
        assert_eq!(
            g.trim_db(),
            0.0,
            "it returns to the rider's setting and does not boost past it"
        );
        assert!(!g.engaged());
    }

    /// Clipping again during the hold restarts it, rather than letting a source
    /// that clips every second or so climb back a little between each one.
    #[test]
    fn a_fresh_clip_restarts_the_hold() {
        let mut g = ClipGuard::new();
        run(&mut g, 8, 5, 12.0);
        run(&mut g, HOLD_BLOCKS, 0, 12.0);
        let before = g.trim_db();
        g.observe(1, 12.0);
        run(&mut g, HOLD_BLOCKS - 1, 0, 12.0);
        assert!(
            g.trim_db() < before,
            "the clip should have taken more off and reset the hold"
        );
    }

    /// The floor is read fresh every block, so lowering the slider mid-ride
    /// takes the correction with it.
    #[test]
    fn lowering_the_slider_lifts_the_trim_with_it() {
        let mut g = ClipGuard::new();
        run(&mut g, 10_000, 50, 20.0);
        assert_eq!(g.trim_db(), -20.0);

        // The rider pulls the slider back to +3.
        g.observe(0, 3.0);
        assert_eq!(
            g.trim_db(),
            -3.0,
            "a correction sized for +20 must not survive the boost that justified it"
        );

        // And down to unity, where the guard has nothing left to do.
        g.observe(0, 0.0);
        assert_eq!(g.trim_db(), 0.0);
    }

    #[test]
    fn reset_forgets_the_correction() {
        let mut g = ClipGuard::new();
        run(&mut g, 20, 5, 12.0);
        assert!(g.engaged());
        g.reset();
        assert_eq!(g.trim_db(), 0.0);
        assert!(!g.engaged());
    }

    /// The constants should describe the times the doc comments claim.
    #[test]
    fn the_rates_are_the_ones_documented() {
        assert_eq!(BLOCKS_PER_SECOND, 100, "10 ms blocks at 48 kHz");
        assert_eq!(HOLD_BLOCKS, 300, "three seconds");
        assert_eq!(RECOVERY_BLOCKS_PER_DB, 1200, "twelve seconds a dB");
        // 10 dB/s off, 1/12 dB/s back: the asymmetry the module comment claims.
        assert!((ATTACK_DB_PER_BLOCK * BLOCKS_PER_SECOND as f32 - 10.0).abs() < 1e-6);
    }
}
