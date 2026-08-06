//! Playing a backlog off faster than it arrived, without sounding like it.
//!
//! A tunnel, a lift, a dead spot behind a lorry: the link stops, packets queue
//! up somewhere between here and the sender, and then all of them arrive at
//! once. The jitter buffer is then holding seconds of speech, and every one of
//! those seconds is latency the rider carries for the rest of the
//! conversation — they answer a question the other person asked four seconds
//! ago, and by then it has been asked again.
//!
//! There are three ways to get rid of it and only one of them is acceptable.
//! Dropping the backlog throws away the words. Playing it at ordinary speed
//! keeps the delay for ever. Resampling it — reading the samples out faster —
//! shortens it, and raises the pitch by the same factor, which at anything
//! approaching 2× turns everyone into a cartoon.
//!
//! So: remove whole pitch periods and cross-fade over the join. A voiced sound
//! is very nearly the same waveform repeated at the pitch period, so deleting
//! one period and fading between the two sides of the cut leaves the pitch
//! exactly where it was, the formants where they were, and the sound shorter.
//! That is the same family of trick as the "accelerate" mode in WebRTC's
//! NetEq, and it is what makes a catch-up sound like someone talking quickly
//! rather than someone inhaling helium.
//!
//! # What it costs
//!
//! Unvoiced sounds have no period to remove, so the fade there joins two
//! uncorrelated pieces of noise and loses roughly 3 dB across its length. A
//! fricative is a few tens of milliseconds of broadband hiss and the fade is
//! five, so this is heard as nothing at all.
//!
//! The cross-fade is linear rather than equal-power, deliberately. Equal power
//! is right when the two sides are uncorrelated; these are pitch-aligned and
//! so add coherently, where linear is right and equal-power would bulge.
//! Getting this backwards is audible as a pulse of loudness on every removal,
//! at whatever rate the removals happen — a flutter.

/// Highest pitch looked for, as a period in samples at 48 kHz. 400 Hz.
const MIN_PERIOD: usize = 120;

/// Lowest pitch looked for. 100 Hz, which reaches most adult male speech.
///
/// It cannot simply be extended downwards: a removal needs two periods of
/// signal to fade between, so the longest period searched for is half the
/// block, and a 20 ms block is 960 samples.
const MAX_PERIOD: usize = 480;

/// Used when the block is too short to search, or has no periodicity worth
/// finding — silence, or a fricative. 200 Hz: middling, and it divides a 20 ms
/// block exactly, so removals land evenly rather than leaving a remainder.
const DEFAULT_PERIOD: usize = 240;

/// Decimation for the coarse period search.
///
/// The search is the only part of this with a cost worth thinking about, and
/// it runs on the mixer thread. Searching at a quarter rate and then refining
/// at full rate over a few samples finds the same period for a sixteenth of
/// the multiplies.
const COARSE: usize = 4;

/// Enough for a 60 ms block, the longest frame Opus offers, at quarter rate.
const MAX_COARSE: usize = 2_880 / COARSE;

/// Removes pitch periods from a block so it plays in less time than it took.
pub struct TimeCompressor {
    /// Samples still owed to the requested speed, carried between blocks.
    ///
    /// Speed is asked for as a ratio and paid in whole pitch periods, which
    /// almost never divide the block evenly. Carrying the remainder is what
    /// makes the average come out at the ratio asked for instead of at
    /// whatever the pitch happened to allow.
    debt: f32,
    /// Scratch for the decimated search. Sized once; never grows on the
    /// mixer thread.
    coarse: [f32; MAX_COARSE],
}

impl Default for TimeCompressor {
    fn default() -> Self {
        Self::new()
    }
}

impl TimeCompressor {
    pub fn new() -> Self {
        Self {
            debt: 0.0,
            coarse: [0.0; MAX_COARSE],
        }
    }

    pub fn reset(&mut self) {
        self.debt = 0.0;
    }

    /// Compresses `buf[..n]` in place at `speed`, returning what is left.
    ///
    /// `speed` is a ratio: 1.0 changes nothing, 2.0 asks for the block in half
    /// the time. Whether it gets there depends on the pitch — see [`Self::debt`]
    /// — but it never overshoots, and the samples it returns are always a
    /// prefix of `buf`.
    pub fn process(&mut self, buf: &mut [f32], n: usize, speed: f32) -> usize {
        let n = n.min(buf.len());
        if speed <= 1.001 || n < MIN_PERIOD * 2 {
            // Nothing to do, and no debt to carry: a speed of 1 means the
            // caller has caught up, and owing samples from a catch-up that
            // finished would make the next one start by rushing.
            self.debt = 0.0;
            return n;
        }

        // Bounded, because the pitch may not allow the speed asked for. With a
        // 200 Hz period a 20 ms block can give up 400 of its 960 samples and no
        // more, which is 1.7× however hard it is pushed. Without a bound the
        // shortfall would accumulate for the whole catch-up and then be spent
        // in one burst afterwards, racing through a sentence that had already
        // arrived on time.
        self.debt = (self.debt + (speed - 1.0) * n as f32).min(n as f32);

        let period = self.period(&buf[..n]);
        let mut read = 0usize;
        let mut write = 0usize;
        while read < n {
            if self.debt >= period as f32 && read + 2 * period <= n {
                // Fade from the samples before the cut to the samples one
                // period later, so the waveform continues in phase and a
                // period's worth of time disappears.
                for i in 0..period {
                    let w = i as f32 / period as f32;
                    // `write <= read` always holds, so this reads each source
                    // sample before anything overwrites it.
                    buf[write + i] = buf[read + i] * (1.0 - w) + buf[read + period + i] * w;
                }
                write += period;
                read += 2 * period;
                self.debt -= period as f32;
            } else {
                buf[write] = buf[read];
                write += 1;
                read += 1;
            }
        }
        write
    }

    /// The pitch period of a block, in samples.
    ///
    /// Normalised correlation rather than a bare dot product: unnormalised,
    /// the shortest lag wins on any signal that is getting louder, because it
    /// overlaps the loudest part of the window.
    fn period(&mut self, x: &[f32]) -> usize {
        let len = x.len();
        if len < MAX_PERIOD * 2 {
            return DEFAULT_PERIOD.min(len / 2).max(1);
        }
        // The window is what is left once the longest lag has been allowed
        // for, so every lag in the range compares the same number of samples
        // and their scores are comparable.
        let window = len - MAX_PERIOD;

        let clen = (len / COARSE).min(MAX_COARSE);
        for i in 0..clen {
            let start = i * COARSE;
            let mut sum = 0.0;
            for s in &x[start..start + COARSE] {
                sum += *s;
            }
            self.coarse[i] = sum / COARSE as f32;
        }

        let cwindow = window / COARSE;
        let mut best = DEFAULT_PERIOD / COARSE;
        let mut best_score = f32::MIN;
        for lag in (MIN_PERIOD / COARSE)..=(MAX_PERIOD / COARSE) {
            if lag + cwindow > clen {
                break;
            }
            let score = correlation(&self.coarse[..cwindow], &self.coarse[lag..lag + cwindow]);
            if score > best_score {
                best_score = score;
                best = lag;
            }
        }

        // Refine at full rate. The coarse search is only accurate to four
        // samples, and four samples of phase error at the cut is a step in the
        // waveform that the fade smears rather than removes — a click.
        let centre = best * COARSE;
        let lo = centre.saturating_sub(COARSE).max(MIN_PERIOD);
        let hi = (centre + COARSE).min(MAX_PERIOD);
        let mut refined = centre.clamp(MIN_PERIOD, MAX_PERIOD);
        let mut refined_score = f32::MIN;
        for lag in lo..=hi {
            let score = correlation(&x[..window], &x[lag..lag + window]);
            if score > refined_score {
                refined_score = score;
                refined = lag;
            }
        }
        refined
    }
}

/// Pearson-style correlation of two equal-length windows, ignoring the mean.
fn correlation(a: &[f32], b: &[f32]) -> f32 {
    let mut num = 0.0f32;
    let mut ea = 0.0f32;
    let mut eb = 0.0f32;
    for i in 0..a.len() {
        num += a[i] * b[i];
        ea += a[i] * a[i];
        eb += b[i] * b[i];
    }
    if ea <= 1e-12 || eb <= 1e-12 {
        return 0.0;
    }
    num / (ea * eb).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: f32 = 48_000.0;
    const BLOCK: usize = 960;

    fn tone(hz: f32, n: usize, phase: f32) -> Vec<f32> {
        (0..n)
            .map(|i| (2.0 * std::f32::consts::PI * hz * i as f32 / RATE + phase).sin() * 0.5)
            .collect()
    }

    /// Dominant frequency by counting rising zero crossings.
    ///
    /// Crude, and exactly strong enough for the question being asked, which is
    /// whether the pitch moved by anything like the speed factor.
    fn pitch_hz(x: &[f32]) -> f32 {
        let mut crossings = 0;
        for i in 1..x.len() {
            if x[i - 1] <= 0.0 && x[i] > 0.0 {
                crossings += 1;
            }
        }
        crossings as f32 * RATE / x.len() as f32
    }

    #[test]
    fn a_speed_of_one_changes_nothing() {
        let mut c = TimeCompressor::new();
        let mut buf = tone(200.0, BLOCK, 0.0);
        let before = buf.clone();
        let n = c.process(&mut buf, BLOCK, 1.0);
        assert_eq!(n, BLOCK);
        assert_eq!(buf, before);
    }

    #[test]
    fn the_pitch_does_not_move() {
        // The whole reason this exists rather than a resampler. Reading the
        // same samples out faster would double the frequency; removing whole
        // periods leaves it alone.
        let mut c = TimeCompressor::new();
        let mut out = Vec::new();
        let source = tone(200.0, BLOCK * 20, 0.0);
        for block in source.chunks(BLOCK) {
            let mut buf = block.to_vec();
            let n = c.process(&mut buf, block.len(), 2.0);
            out.extend_from_slice(&buf[..n]);
        }

        let before = pitch_hz(&source);
        let after = pitch_hz(&out);
        assert!(
            (after - before).abs() < 10.0,
            "pitch moved from {before} Hz to {after} Hz"
        );
        assert!(
            out.len() < source.len(),
            "nothing was removed: {} of {}",
            out.len(),
            source.len()
        );
    }

    #[test]
    fn the_average_speed_is_the_speed_asked_for() {
        // Within what the pitch allows. A 200 Hz period is 240 samples and a
        // block is 960, so two removals fit exactly and 2x is reachable.
        let mut c = TimeCompressor::new();
        let source = tone(200.0, BLOCK * 30, 0.0);
        let mut produced = 0usize;
        for block in source.chunks(BLOCK) {
            let mut buf = block.to_vec();
            produced += c.process(&mut buf, block.len(), 2.0);
        }
        let speed = source.len() as f32 / produced as f32;
        assert!(
            (speed - 2.0).abs() < 0.15,
            "asked for 2x and got {speed:.3}x"
        );
    }

    #[test]
    fn a_gentler_speed_removes_proportionally_less() {
        let mut c = TimeCompressor::new();
        let source = tone(180.0, BLOCK * 30, 0.0);
        let mut produced = 0usize;
        for block in source.chunks(BLOCK) {
            let mut buf = block.to_vec();
            produced += c.process(&mut buf, block.len(), 1.25);
        }
        let speed = source.len() as f32 / produced as f32;
        assert!(
            (speed - 1.25).abs() < 0.1,
            "asked for 1.25x and got {speed:.3}x"
        );
    }

    #[test]
    fn the_join_does_not_click() {
        // A removal that cuts mid-period leaves a step in the waveform, which
        // is heard as a click on every removal — a tick at the pitch of the
        // catch-up rather than of the voice. The period search and the fade
        // exist to prevent it, and a step is easy to see: no sample-to-sample
        // jump much larger than the signal itself already makes.
        let source = tone(120.0, BLOCK * 10, 0.0);
        let largest_natural = source
            .windows(2)
            .map(|w| (w[1] - w[0]).abs())
            .fold(0.0f32, f32::max);

        let mut c = TimeCompressor::new();
        let mut out = Vec::new();
        for block in source.chunks(BLOCK) {
            let mut buf = block.to_vec();
            let n = c.process(&mut buf, block.len(), 2.0);
            out.extend_from_slice(&buf[..n]);
        }

        let largest = out
            .windows(2)
            .map(|w| (w[1] - w[0]).abs())
            .fold(0.0f32, f32::max);
        assert!(
            largest < largest_natural * 1.5,
            "a step of {largest} against {largest_natural} in the source"
        );
    }

    #[test]
    fn noise_survives_being_compressed() {
        // No period to find, so the fade joins two uncorrelated pieces. It
        // must still produce finite audio at roughly the level it was given,
        // rather than the hole a badly weighted fade would leave.
        let mut rng = 12_345u32;
        let source: Vec<f32> = (0..BLOCK * 10)
            .map(|_| {
                rng = rng.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                (rng >> 8) as f32 / 8_388_608.0 - 1.0
            })
            .collect();

        let mut c = TimeCompressor::new();
        let mut out = Vec::new();
        for block in source.chunks(BLOCK) {
            let mut buf = block.to_vec();
            let n = c.process(&mut buf, block.len(), 2.0);
            out.extend_from_slice(&buf[..n]);
        }

        assert!(out.iter().all(|s| s.is_finite()));
        let level = |x: &[f32]| (x.iter().map(|s| s * s).sum::<f32>() / x.len() as f32).sqrt();
        let ratio = level(&out) / level(&source);
        assert!(
            (0.5..1.5).contains(&ratio),
            "noise came out at {ratio:.2} of the level it went in at"
        );
    }

    #[test]
    fn a_block_too_short_to_cut_is_left_alone() {
        let mut c = TimeCompressor::new();
        let mut buf = tone(200.0, 100, 0.0);
        assert_eq!(c.process(&mut buf, 100, 2.0), 100);
    }
}
