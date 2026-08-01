//! Sample-rate conversion between the device and the 48 kHz pipeline.
//!
//! Devices that cannot do 48 kHz are rare but real (some Bluetooth headsets and
//! Android devices insist on 44.1 kHz, and a helmet intercom is usually a
//! Bluetooth headset). Catmull-Rom interpolation is a good fit: far better than
//! linear for the modest ratios involved, and cheap enough for the audio thread.

/// Streaming resampler that keeps phase across buffers.
pub struct Resampler {
    ratio: f64,
    pos: f64,
    /// The three most recent input samples, so interpolation can look backwards
    /// across a buffer boundary.
    hist: [f32; 3],
    primed: bool,
}

impl Resampler {
    pub fn new(from_hz: u32, to_hz: u32) -> Self {
        Self {
            ratio: from_hz as f64 / to_hz as f64,
            pos: 0.0,
            hist: [0.0; 3],
            primed: false,
        }
    }

    /// True when input and output rates match and processing can be skipped.
    pub fn is_identity(&self) -> bool {
        (self.ratio - 1.0).abs() < 1e-9
    }

    pub fn reset(&mut self) {
        self.pos = 0.0;
        self.hist = [0.0; 3];
        self.primed = false;
    }

    #[inline]
    fn catmull_rom(p0: f32, p1: f32, p2: f32, p3: f32, t: f32) -> f32 {
        let t2 = t * t;
        let t3 = t2 * t;
        0.5 * ((2.0 * p1)
            + (-p0 + p2) * t
            + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t2
            + (-p0 + 3.0 * p1 - 3.0 * p2 + p3) * t3)
    }

    /// Resamples `input`, appending to `out`.
    pub fn process(&mut self, input: &[f32], out: &mut Vec<f32>) {
        if self.is_identity() {
            out.extend_from_slice(input);
            return;
        }
        if input.is_empty() {
            return;
        }
        if !self.primed {
            // Seed history with the first sample so the stream starts cleanly
            // rather than interpolating from silence.
            self.hist = [input[0]; 3];
            self.primed = true;
        }

        for &sample in input {
            // Emit every output sample that falls before the newly arrived one.
            while self.pos < 1.0 {
                let t = self.pos as f32;
                out.push(Self::catmull_rom(
                    self.hist[0],
                    self.hist[1],
                    self.hist[2],
                    sample,
                    t,
                ));
                self.pos += self.ratio;
            }
            self.pos -= 1.0;
            self.hist[0] = self.hist[1];
            self.hist[1] = self.hist[2];
            self.hist[2] = sample;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(len: usize, freq: f32, rate: f32) -> Vec<f32> {
        (0..len)
            .map(|i| (2.0 * std::f32::consts::PI * freq * i as f32 / rate).sin())
            .collect()
    }

    #[test]
    fn identity_rate_passes_through_untouched() {
        let mut r = Resampler::new(48_000, 48_000);
        assert!(r.is_identity());
        let input = sine(100, 440.0, 48_000.0);
        let mut out = Vec::new();
        r.process(&input, &mut out);
        assert_eq!(out, input);
    }

    #[test]
    fn upsampling_produces_roughly_the_right_count() {
        let mut r = Resampler::new(44_100, 48_000);
        let input = sine(4410, 440.0, 44_100.0);
        let mut out = Vec::new();
        r.process(&input, &mut out);
        let expected = 4410.0 * 48_000.0 / 44_100.0;
        assert!(
            (out.len() as f32 - expected).abs() < 10.0,
            "got {} samples, expected about {expected}",
            out.len()
        );
    }

    #[test]
    fn downsampling_produces_roughly_the_right_count() {
        let mut r = Resampler::new(48_000, 16_000);
        let input = sine(4800, 440.0, 48_000.0);
        let mut out = Vec::new();
        r.process(&input, &mut out);
        assert!(
            (out.len() as i32 - 1600).abs() < 10,
            "got {} samples, expected about 1600",
            out.len()
        );
    }

    #[test]
    fn preserves_a_tone_across_conversion() {
        // A 440 Hz tone resampled 44.1k -> 48k should keep its amplitude and stay
        // smooth, not develop steps or gaps.
        let mut r = Resampler::new(44_100, 48_000);
        let input = sine(44_100, 440.0, 44_100.0);
        let mut out = Vec::new();
        r.process(&input, &mut out);

        let tail = &out[1000..out.len() - 1000];
        let peak = tail.iter().fold(0.0f32, |m, s| m.max(s.abs()));
        assert!((peak - 1.0).abs() < 0.05, "amplitude drifted to {peak}");

        // No sample-to-sample jump should exceed what a 440 Hz sine can do.
        let max_step = tail
            .windows(2)
            .map(|w| (w[1] - w[0]).abs())
            .fold(0.0f32, f32::max);
        assert!(max_step < 0.1, "discontinuity of {max_step} in output");
    }

    #[test]
    fn keeps_phase_across_buffer_boundaries() {
        // Feeding one long buffer and many small ones must agree, or every
        // callback boundary would click.
        let input = sine(4410, 440.0, 44_100.0);

        let mut whole = Vec::new();
        Resampler::new(44_100, 48_000).process(&input, &mut whole);

        let mut chunked = Vec::new();
        let mut r = Resampler::new(44_100, 48_000);
        for c in input.chunks(147) {
            r.process(c, &mut chunked);
        }

        assert_eq!(whole.len(), chunked.len(), "sample counts diverged");
        let worst = whole
            .iter()
            .zip(&chunked)
            .map(|(a, b)| (a - b).abs())
            .fold(0.0f32, f32::max);
        assert!(worst < 1e-5, "chunked output differed by {worst}");
    }

    #[test]
    fn handles_empty_input() {
        let mut r = Resampler::new(44_100, 48_000);
        let mut out = Vec::new();
        r.process(&[], &mut out);
        assert!(out.is_empty());
    }

    #[test]
    fn output_stays_finite_and_bounded() {
        let mut r = Resampler::new(8_000, 48_000);
        let input = sine(800, 300.0, 8_000.0);
        let mut out = Vec::new();
        r.process(&input, &mut out);
        assert!(out.iter().all(|s| s.is_finite() && s.abs() < 1.5));
    }
}
