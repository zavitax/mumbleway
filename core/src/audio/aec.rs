//! Acoustic echo cancellation.
//!
//! In a helmet the speakers sit centimetres from the microphone, so whatever
//! the other rider says comes straight back to them a few milliseconds later.
//! The gate cannot fix this — the echo *is* speech, and RNNoise will happily
//! pass it through.
//!
//! This is a normalised least-mean-squares adaptive filter: it learns the
//! impulse response from the speaker to the microphone and subtracts its
//! prediction of the echo from the captured signal.
//!
//! ```text
//! mic = near_end_speech + echo(reference)
//! estimate = w · reference_history
//! output = mic - estimate          (and w adapts to shrink output)
//! ```
//!
//! The filter is only correct while the far end alone is talking. When both
//! talk at once ("double talk") the near-end voice looks like error the filter
//! should cancel, and adapting on it makes the filter diverge — so adaptation
//! freezes whenever near-end speech is likely.

/// Filter length in taps.
///
/// 512 taps at 48 kHz models ~10.7 ms of echo path, which comfortably covers a
/// helmet or headset where the speaker is close to the microphone. Room-scale
/// echo would need far more, but that is not the target here and a long filter
/// costs real CPU on every 10 ms block.
pub const DEFAULT_TAPS: usize = 512;

/// Adaptive echo canceller.
pub struct EchoCanceller {
    taps: usize,
    /// Filter coefficients.
    w: Vec<f32>,
    /// Ring buffer of the most recent `taps` reference samples.
    ring: Vec<f32>,
    /// Index the next reference sample is written to.
    pos: usize,
    /// Running sum of squares of the ring, for NLMS normalisation.
    ref_power: f32,
    /// Adaptation rate, 0..2. Lower is slower but more stable.
    mu: f32,
    /// Smoothed powers used for the double-talk guard and for reporting.
    smooth_mic: f32,
    smooth_out: f32,
    smooth_ref: f32,
    enabled: bool,
}

impl EchoCanceller {
    pub fn new(taps: usize) -> Self {
        let taps = taps.max(16);
        Self {
            taps,
            w: vec![0.0; taps],
            ring: vec![0.0; taps],
            pos: 0,
            ref_power: 0.0,
            mu: 0.25,
            smooth_mic: 0.0,
            smooth_out: 0.0,
            smooth_ref: 0.0,
            enabled: true,
        }
    }

    pub fn set_enabled(&mut self, on: bool) {
        self.enabled = on;
        if !on {
            self.reset();
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn reset(&mut self) {
        self.w.iter_mut().for_each(|v| *v = 0.0);
        self.ring.iter_mut().for_each(|v| *v = 0.0);
        self.pos = 0;
        self.ref_power = 0.0;
        self.smooth_mic = 0.0;
        self.smooth_out = 0.0;
        self.smooth_ref = 0.0;
    }

    /// Echo return loss enhancement in dB: how much echo was removed.
    ///
    /// Meaningful only while the far end is talking; near-end speech is not
    /// echo and legitimately survives, which lowers the figure.
    pub fn erle_db(&self) -> f32 {
        if self.smooth_out <= 1e-12 || self.smooth_mic <= 1e-12 {
            return 0.0;
        }
        10.0 * (self.smooth_mic / self.smooth_out).log10()
    }

    #[inline]
    fn push_reference(&mut self, x: f32) {
        let old = self.ring[self.pos];
        // Maintained incrementally; recomputing the sum every sample would
        // dominate the cost of the whole filter.
        self.ref_power += x * x - old * old;
        if self.ref_power < 0.0 {
            self.ref_power = 0.0;
        }
        self.ring[self.pos] = x;
        self.pos = (self.pos + 1) % self.taps;
    }

    /// Cancels echo from `mic` in place, using `reference` as the signal that
    /// was played out. Both slices must be the same length and time-aligned.
    ///
    /// Returns the estimated ERLE in dB.
    pub fn process(&mut self, mic: &mut [f32], reference: &[f32]) -> f32 {
        if !self.enabled {
            return 0.0;
        }
        let n = mic.len().min(reference.len());

        for i in 0..n {
            self.push_reference(reference[i]);

            // Estimate the echo: w · history, most recent sample first.
            let mut estimate = 0.0f32;
            let mut idx = (self.pos + self.taps - 1) % self.taps;
            for k in 0..self.taps {
                estimate += self.w[k] * self.ring[idx];
                idx = if idx == 0 { self.taps - 1 } else { idx - 1 };
            }

            let d = mic[i];
            let e = d - estimate;
            mic[i] = e;

            // Track powers for the guard and for ERLE.
            const A: f32 = 0.999;
            self.smooth_mic = A * self.smooth_mic + (1.0 - A) * d * d;
            self.smooth_out = A * self.smooth_out + (1.0 - A) * e * e;
            self.smooth_ref = A * self.smooth_ref + (1.0 - A) * reference[i] * reference[i];

            if self.should_adapt() {
                // NLMS: step normalised by reference power, so adaptation speed
                // does not depend on how loud the far end happens to be.
                let norm = self.ref_power + 1e-6;
                let step = self.mu * e / norm;
                let mut idx = (self.pos + self.taps - 1) % self.taps;
                for k in 0..self.taps {
                    self.w[k] += step * self.ring[idx];
                    idx = if idx == 0 { self.taps - 1 } else { idx - 1 };
                }
            }
        }
        self.erle_db()
    }

    /// Whether it is safe to update the filter.
    ///
    /// Two conditions. There must be enough far-end signal to learn from at
    /// all, and the microphone must not be dominated by something the
    /// reference cannot explain — which is what near-end speech looks like.
    #[inline]
    fn should_adapt(&self) -> bool {
        // Nothing playing: the filter would only chase noise.
        if self.smooth_ref < 1e-8 {
            return false;
        }
        // Double talk: the residual is large relative to the far-end signal, so
        // the microphone is carrying something that is not echo. Adapting here
        // is what makes an echo canceller diverge and start howling.
        self.smooth_out <= self.smooth_ref * 2.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Deterministic pseudo-noise, standing in for far-end speech.
    fn noise(len: usize, seed: u32, amp: f32) -> Vec<f32> {
        let mut s = seed.wrapping_mul(2_654_435_761).wrapping_add(12345);
        (0..len)
            .map(|_| {
                s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                ((s >> 8) as f32 / 8_388_608.0 - 1.0) * amp
            })
            .collect()
    }

    /// A short synthetic echo path: a delay plus a few decaying reflections.
    fn echo_path() -> Vec<f32> {
        let mut h = vec![0.0f32; 64];
        h[12] = 0.60;
        h[19] = -0.28;
        h[31] = 0.15;
        h[47] = -0.07;
        h
    }

    fn convolve(x: &[f32], h: &[f32]) -> Vec<f32> {
        let mut y = vec![0.0f32; x.len()];
        for n in 0..x.len() {
            let mut acc = 0.0;
            for (k, &hk) in h.iter().enumerate() {
                if n >= k {
                    acc += hk * x[n - k];
                }
            }
            y[n] = acc;
        }
        y
    }

    fn rms(x: &[f32]) -> f32 {
        if x.is_empty() {
            return 0.0;
        }
        (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt()
    }

    #[test]
    fn cancels_a_synthetic_echo_path() {
        // Far end talking alone: the canceller should learn the path and remove
        // most of the echo.
        let far = noise(48_000, 1, 0.3);
        let echo = convolve(&far, &echo_path());

        let mut aec = EchoCanceller::new(DEFAULT_TAPS);
        let mut residual_tail = Vec::new();

        for (i, (m, r)) in echo.chunks(480).zip(far.chunks(480)).enumerate() {
            let mut block = m.to_vec();
            aec.process(&mut block, r);
            // Measure once it has had time to converge.
            if i > 60 {
                residual_tail.extend_from_slice(&block);
            }
        }

        let before = rms(&echo[echo.len() / 2..]);
        let after = rms(&residual_tail);
        let erle = 20.0 * (before / after.max(1e-9)).log10();

        assert!(
            erle > 12.0,
            "only {erle:.1} dB of echo removed (before {before:.5}, after {after:.5})"
        );
    }

    #[test]
    fn leaves_near_end_speech_alone_when_nothing_is_playing() {
        // With silence on the far end there is no echo to cancel, so the
        // microphone must pass through essentially untouched.
        let near = noise(9600, 7, 0.2);
        let silence = vec![0.0f32; 9600];

        let mut aec = EchoCanceller::new(DEFAULT_TAPS);
        let mut out = Vec::new();
        for (m, r) in near.chunks(480).zip(silence.chunks(480)) {
            let mut block = m.to_vec();
            aec.process(&mut block, r);
            out.extend_from_slice(&block);
        }

        let diff = rms(&out
            .iter()
            .zip(&near)
            .map(|(a, b)| a - b)
            .collect::<Vec<_>>());
        assert!(
            diff < 1e-4,
            "near-end audio was altered with no reference playing: {diff}"
        );
    }

    #[test]
    fn double_talk_does_not_destroy_the_filter() {
        // The classic failure: adapting while the near end talks makes the
        // filter diverge, and the echo comes back louder than it started.
        // 200 blocks of 480 samples: enough for converge / double-talk / recover.
        let far = noise(96_000, 3, 0.3);
        let echo = convolve(&far, &echo_path());
        let near = noise(96_000, 9, 0.35);

        let mut aec = EchoCanceller::new(DEFAULT_TAPS);

        // Converge on echo alone first.
        for (m, r) in echo.chunks(480).zip(far.chunks(480)).take(60) {
            let mut b = m.to_vec();
            aec.process(&mut b, r);
        }
        let converged_erle = aec.erle_db();

        // Now both talk at once for a while.
        for i in 60..160 {
            let mut b: Vec<f32> = echo[i * 480..(i + 1) * 480]
                .iter()
                .zip(&near[i * 480..(i + 1) * 480])
                .map(|(e, n)| e + n)
                .collect();
            aec.process(&mut b, &far[i * 480..(i + 1) * 480]);
            assert!(
                b.iter().all(|s| s.is_finite() && s.abs() < 10.0),
                "output blew up during double talk at block {i}"
            );
        }

        // Then far end alone again: the filter should still be useful.
        let mut residual = Vec::new();
        for i in 160..200 {
            let mut b = echo[i * 480..(i + 1) * 480].to_vec();
            aec.process(&mut b, &far[i * 480..(i + 1) * 480]);
            residual.extend_from_slice(&b);
        }

        let before = rms(&echo[160 * 480..200 * 480]);
        let after = rms(&residual);
        let erle = 20.0 * (before / after.max(1e-9)).log10();
        assert!(
            erle > 8.0,
            "filter degraded through double talk: {erle:.1} dB (was {converged_erle:.1})"
        );
    }

    #[test]
    fn disabled_canceller_is_a_pass_through() {
        let mut aec = EchoCanceller::new(128);
        aec.set_enabled(false);
        let reference = noise(480, 2, 0.3);
        let original = noise(480, 5, 0.2);
        let mut block = original.clone();
        aec.process(&mut block, &reference);
        assert_eq!(block, original);
    }

    #[test]
    fn reset_clears_the_learned_path() {
        let far = noise(9600, 11, 0.3);
        let echo = convolve(&far, &echo_path());
        let mut aec = EchoCanceller::new(256);
        for (m, r) in echo.chunks(480).zip(far.chunks(480)) {
            let mut b = m.to_vec();
            aec.process(&mut b, r);
        }
        assert!(aec.w.iter().any(|w| w.abs() > 1e-4), "nothing was learned");

        aec.reset();
        assert!(aec.w.iter().all(|w| *w == 0.0));
        assert_eq!(aec.erle_db(), 0.0);
    }

    #[test]
    fn output_stays_finite_on_pathological_input() {
        // Loud, sustained, correlated input is what makes naive LMS explode.
        let mut aec = EchoCanceller::new(256);
        for i in 0..200 {
            let reference = vec![0.9f32; 480];
            let mut mic = vec![if i % 2 == 0 { 0.9 } else { -0.9 }; 480];
            aec.process(&mut mic, &reference);
            assert!(
                mic.iter().all(|s| s.is_finite()),
                "non-finite output at block {i}"
            );
        }
    }
}
