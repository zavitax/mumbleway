//! A window of raw microphone audio, for a classifier that lives outside the
//! chain.
//!
//! Everything else the profile chooser uses is a level or a filter, and both
//! are measured *after* the suppressor whose behaviour they are supposed to
//! influence. `docs/MUSIC_GATE.md` is the record of six hand-built features
//! failing against that, and of two off-the-shelf models succeeding once they
//! were fed the microphone instead. This is what feeds them.
//!
//! # Why it is here rather than in `spectrum`
//!
//! The analyser answers "what does this block look like" and is wanted while
//! somebody is watching a panel. This answers "what has the last second
//! sounded like" and is wanted while `Auto` is chosen, which is normally with
//! the screen off. They arm separately for that reason.
//!
//! # 16 kHz, and by averaging threes
//!
//! YAMNet was built for 15 600 samples at 16 kHz — 0.975 s — and the capture
//! chain runs at 48 000, so the ratio is exactly three. The decimation is a
//! three-tap box average, which is a poor anti-alias filter and is deliberately
//! the same poor filter `tools/vad/yamnet_bench.py` used to produce every
//! number in `MUSIC_GATE.md`. Matching the measurement is worth more here than
//! improving on it: a better filter would make the app's input differ from the
//! input the decision was justified on, and nothing offline would notice.

/// Samples in one window: 0.975 s at 16 kHz, which is the input size the model
/// was built for and is not adjustable.
pub const WINDOW: usize = 15_600;

/// Input rate over model rate. Exactly three, and asserted rather than assumed
/// because a chain that ever ran at 44 100 would decimate to 14 700 and be
/// wrong in a way no test of this file would catch.
pub const DECIMATION: usize = 3;

/// One window of microphone audio, as the model wants it.
pub struct WaveformFrame {
    pub samples: [f32; WINDOW],
    /// Increments once per published window. A `seq` that has stopped moving is
    /// a stopped worker, which to a reader looks exactly like silence — the
    /// same trap the analyser carries this for.
    pub seq: u64,
}

/// Collects 48 kHz blocks into 16 kHz windows.
///
/// Owned by the capture worker, so filling it allocates nothing and locks
/// nothing. Only the finished window crosses into shared state.
pub struct WaveformTap {
    ring: Box<[f32; WINDOW]>,
    /// Where the next sample goes. The ring is only ever read as a whole, in
    /// order, so this is the rotation as well as the cursor.
    at: usize,
    /// How many samples have been written since the tap was reset, saturating
    /// at [`WINDOW`]. A partly filled ring is silence padded with a fragment of
    /// a ride, and the model would classify the padding.
    filled: usize,
    /// Carried between blocks so the decimation does not restart at every
    /// block boundary. A 480-sample block divides by three exactly, so this is
    /// zero in practice — and relying on that would break silently the first
    /// time the block size changed.
    pending: [f32; DECIMATION],
    pending_len: usize,
    seq: u64,
}

impl Default for WaveformTap {
    fn default() -> Self {
        Self::new()
    }
}

impl WaveformTap {
    pub fn new() -> Self {
        Self {
            ring: Box::new([0.0; WINDOW]),
            at: 0,
            filled: 0,
            pending: [0.0; DECIMATION],
            pending_len: 0,
            seq: 0,
        }
    }

    /// Adds one block of capture-rate audio.
    pub fn push(&mut self, block: &[f32]) {
        for &s in block {
            self.pending[self.pending_len] = s;
            self.pending_len += 1;
            if self.pending_len == DECIMATION {
                let mean = self.pending.iter().sum::<f32>() / DECIMATION as f32;
                self.ring[self.at] = mean;
                self.at = (self.at + 1) % WINDOW;
                self.filled = (self.filled + 1).min(WINDOW);
                self.pending_len = 0;
            }
        }
    }

    /// Whether a whole window has been collected.
    pub fn ready(&self) -> bool {
        self.filled >= WINDOW
    }

    /// Copies the window out, oldest sample first.
    ///
    /// Returns `None` until the ring has filled once. A short window would have
    /// to be padded, and the model would classify the padding as quiet — which
    /// is the answer that releases `Helmet`, so getting it wrong is not
    /// symmetric.
    pub fn frame(&mut self) -> Option<Box<WaveformFrame>> {
        if !self.ready() {
            return None;
        }
        self.seq += 1;
        let mut out = Box::new(WaveformFrame {
            samples: [0.0; WINDOW],
            seq: self.seq,
        });
        let (head, tail) = self.ring.split_at(self.at);
        out.samples[..tail.len()].copy_from_slice(tail);
        out.samples[tail.len()..].copy_from_slice(head);
        Some(out)
    }

    pub fn reset(&mut self) {
        self.at = 0;
        self.filled = 0;
        self.pending_len = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_window_is_not_offered_until_it_is_whole() {
        let mut tap = WaveformTap::new();
        // One short of a window's worth of input.
        tap.push(&vec![0.5; (WINDOW - 1) * DECIMATION]);
        assert!(!tap.ready());
        assert!(tap.frame().is_none());

        tap.push(&[0.5; DECIMATION]);
        assert!(tap.ready());
        assert!(tap.frame().is_some());
    }

    #[test]
    fn threes_are_averaged_and_the_window_reads_oldest_first() {
        let mut tap = WaveformTap::new();
        // A ramp, so every output sample is distinguishable from its
        // neighbours and an out-of-order window is obvious.
        let input: Vec<f32> = (0..WINDOW * DECIMATION).map(|i| i as f32).collect();
        tap.push(&input);
        let frame = tap.frame().unwrap();

        // First output is the mean of 0, 1, 2.
        assert!((frame.samples[0] - 1.0).abs() < 1e-6);
        // Last is the mean of the final three.
        let n = (WINDOW * DECIMATION - 1) as f32;
        assert!((frame.samples[WINDOW - 1] - (n - 1.0)).abs() < 1e-3);
        // And it is monotonic throughout, which only holds if the ring was
        // unrolled from the write cursor rather than read from index zero.
        for i in 1..WINDOW {
            assert!(
                frame.samples[i] > frame.samples[i - 1],
                "out of order at {i}"
            );
        }
    }

    #[test]
    fn the_window_slides_rather_than_starting_again() {
        // Two windows' worth in, and the second frame must be the *latest*
        // window, not a repeat of the first. A classifier fed a stale window
        // would hold Helmet on evidence from a minute ago.
        let mut tap = WaveformTap::new();
        tap.push(&vec![0.0; WINDOW * DECIMATION]);
        let first = tap.frame().unwrap();
        assert_eq!(first.samples[WINDOW - 1], 0.0);

        tap.push(&vec![1.0; WINDOW * DECIMATION]);
        let second = tap.frame().unwrap();
        assert_eq!(second.samples[0], 1.0, "the old window was still in there");
        assert_eq!(second.seq, first.seq + 1);
    }

    #[test]
    fn a_block_that_does_not_divide_by_three_keeps_its_remainder() {
        // 480 divides exactly, so this can only ever be exercised by a block
        // size that does not -- which is precisely when a tap that dropped the
        // remainder would start resampling at the wrong rate, silently.
        let mut tap = WaveformTap::new();
        for _ in 0..(WINDOW * DECIMATION / 7 + 7) {
            tap.push(&[1.0; 7]);
        }
        let frame = tap.frame().unwrap();
        for (i, s) in frame.samples.iter().enumerate() {
            assert!(
                (s - 1.0).abs() < 1e-6,
                "sample {i} was {s}, so input was lost"
            );
        }
    }
}
