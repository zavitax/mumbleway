//! Removing the steady hiss a microphone adds to speech.
//!
//! Distinct from everything else in this pipeline. The rumble filter takes out
//! wind and engine, which are low and loud; the feedback guard takes out howl,
//! which is a tone that grows; noise suppression takes out the road, which is
//! broadband but *modulated* by speed and gear. Hiss is none of those: it is
//! quiet, high, and almost perfectly stationary — preamp noise, or a cheap
//! codec's floor — and stationarity is exactly the property both of these
//! methods exploit.
//!
//! Two approaches, because they fail in opposite directions and a rider should
//! be able to pick the failure they mind less:
//!
//! * [`Expander`] works in the time domain and only ever changes the level of
//!   the whole block. It cannot make speech sound synthetic, because it does
//!   not reshape it; what it can do is breathe, the floor rising and falling
//!   between words.
//! * [`SpectralSubtractor`] works per frequency bin and can remove hiss from
//!   underneath speech rather than only between words. The price is the
//!   classic one: subtract too much and the residue turns into "musical noise",
//!   little tones flickering in the gaps.
//!
//! Neither is on by default. Both discard something, and on a link that is
//! already carrying a voice through a helmet at speed, the safest setting is
//! the one that changes nothing.

use std::f32::consts::PI;

/// Which method to use, if any.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DehissMode {
    Off,
    /// Level-based, below the tracked noise floor.
    Expander,
    /// Per-bin subtraction of a learned noise spectrum.
    Spectral,
}

impl DehissMode {
    pub fn from_index(i: u8) -> Self {
        match i {
            1 => DehissMode::Expander,
            2 => DehissMode::Spectral,
            _ => DehissMode::Off,
        }
    }
}

/// Turns quiet things down further, in proportion to how quiet they are.
///
/// A gate answers one question — is this above the threshold — and applies its
/// answer to the whole block, so a word that starts softly is cut off at the
/// front and a breath between words slams to silence. An expander asks how far
/// below the floor the block sits and attenuates by a multiple of that, which
/// leaves loud speech untouched, soft speech nearly untouched, and hiss (which
/// is always far below) heavily reduced.
///
/// The gain is smoothed asymmetrically for the usual reason: attack must be
/// slow enough not to chew the start of a word, release fast enough that the
/// hiss does not ride out behind it.
pub struct Expander {
    /// How many dB of attenuation per dB below the threshold, minus one.
    /// A ratio of 3 attenuates 2 dB for every 1 dB below.
    ratio: f32,
    /// Never attenuate by more than this, so the floor drops without the line
    /// going conspicuously dead.
    range_db: f32,
    /// How far above the tracked floor the expander stops acting.
    knee_db: f32,
    gain_db: f32,
    attack: f32,
    release: f32,
}

impl Expander {
    /// Rising towards no attenuation, per block. Deliberately quicker than the
    /// fall: being late to get out of the way of a word is audible, being late
    /// to come back is not.
    const ATTACK: f32 = 0.5;
    const RELEASE: f32 = 0.12;

    pub fn new(ratio: f32, range_db: f32, knee_db: f32) -> Self {
        Self {
            ratio: ratio.max(1.0),
            range_db: range_db.max(0.0),
            knee_db: knee_db.max(0.0),
            gain_db: 0.0,
            attack: Self::ATTACK,
            release: Self::RELEASE,
        }
    }

    /// The everyday setting: gentle, and incapable of removing a whole word.
    pub fn standard() -> Self {
        Self::new(2.5, 18.0, 8.0)
    }

    pub fn reset(&mut self) {
        self.gain_db = 0.0;
    }

    /// Attenuates `buf` given this block's level and the tracked noise floor.
    ///
    /// Returns the gain applied, in dB, for the diagnostics panel.
    pub fn process(&mut self, buf: &mut [f32], level_db: f32, floor_db: f32) -> f32 {
        // Measured against the floor plus a knee rather than an absolute
        // threshold: the floor climbs with road speed, and a fixed threshold
        // would go from doing nothing at a standstill to swallowing speech on a
        // motorway.
        let threshold = floor_db + self.knee_db;
        let target = if level_db >= threshold {
            0.0
        } else {
            (-(threshold - level_db) * (self.ratio - 1.0)).max(-self.range_db)
        };

        let coeff = if target > self.gain_db {
            self.attack
        } else {
            self.release
        };
        self.gain_db += (target - self.gain_db) * coeff;

        let gain = 10f32.powf(self.gain_db / 20.0);
        for s in buf.iter_mut() {
            *s *= gain;
        }
        self.gain_db
    }
}

/// Subtracts a learned noise spectrum from each block.
///
/// The noise estimate is only updated while the caller says nobody is speaking,
/// which is what makes this safe: a spectrum learned during speech would have
/// the speech subtracted out of it.
///
/// Overlap-add with a Hann window at 50% overlap, which sums to unity and so
/// reconstructs exactly when the gain is 1. The window matters more than usual
/// here — rectangular blocks would put a discontinuity at every boundary, and
/// those are heard as a buzz at the block rate rather than as hiss removed.
pub struct SpectralSubtractor {
    size: usize,
    /// Rolling estimate of noise magnitude per bin.
    noise: Vec<f32>,
    /// Input waiting to be transformed.
    input: Vec<f32>,
    /// Finished samples, one hop's worth, waiting to be handed back.
    ready: Vec<f32>,
    /// The previous frame's second half, awaiting its overlapping partner.
    tail: Vec<f32>,
    window: Vec<f32>,
    filled: usize,
    ready_pos: usize,
    /// How much of the estimate to remove. Above 1 removes more than was
    /// measured, which suppresses harder at the cost of musical noise.
    over: f32,
    /// Never attenuate a bin below this fraction of its original magnitude.
    /// The floor is what keeps the residue sounding like quiet hiss rather than
    /// like a shower of little tones.
    floor: f32,
    learned: bool,
}

impl SpectralSubtractor {
    /// 512 samples at 48 kHz is ~10.7 ms: long enough to resolve the low end of
    /// speech, short enough that the gain follows a syllable rather than
    /// smearing across it.
    pub const SIZE: usize = 512;

    /// How quickly the noise estimate follows a change, per non-speech block.
    const ADAPT: f32 = 0.05;

    pub fn new() -> Self {
        let size = Self::SIZE;
        // Periodic rather than symmetric, so overlapped copies sum to one.
        let window = (0..size)
            .map(|i| 0.5 - 0.5 * (2.0 * PI * i as f32 / size as f32).cos())
            .collect();
        Self {
            size,
            noise: vec![0.0; size / 2 + 1],
            input: vec![0.0; size],
            ready: vec![0.0; size / 2],
            tail: vec![0.0; size / 2],
            window,
            // Half a frame of zeros ahead of the first real sample, which is
            // the latency overlap-add costs: one hop, about 5 ms here.
            filled: size / 2,
            ready_pos: 0,
            over: 1.5,
            floor: 0.12,
            learned: false,
        }
    }

    pub fn reset(&mut self) {
        self.noise.iter_mut().for_each(|n| *n = 0.0);
        self.input.iter_mut().for_each(|s| *s = 0.0);
        self.ready.iter_mut().for_each(|s| *s = 0.0);
        self.tail.iter_mut().for_each(|s| *s = 0.0);
        self.filled = self.size / 2;
        self.ready_pos = 0;
        self.learned = false;
    }

    /// Processes a block in place. `speaking` gates learning, not filtering.
    ///
    /// Sample by sample rather than by chunks, because the caller's block size
    /// has nothing to do with the frame size and need not divide it: an earlier
    /// version tried to line the two up and walked off the end of the buffer
    /// the moment a block arrived that was not a multiple of the hop.
    pub fn process(&mut self, buf: &mut [f32], speaking: bool) {
        let hop = self.size / 2;
        for s in buf.iter_mut() {
            let out = self.ready[self.ready_pos];
            self.input[self.filled] = *s;
            self.filled += 1;
            self.ready_pos += 1;

            // Both counters reach their limit on the same sample, by
            // construction: `filled` starts one hop ahead and each frame
            // produces exactly one hop of output.
            if self.filled == self.size {
                self.transform(speaking);
                self.input.copy_within(hop.., 0);
                self.filled = hop;
                self.ready_pos = 0;
            }
            *s = out;
        }
    }

    fn transform(&mut self, speaking: bool) {
        let size = self.size;
        let mut re: Vec<f32> = (0..size).map(|i| self.input[i] * self.window[i]).collect();
        let mut im = vec![0.0f32; size];
        fft(&mut re, &mut im, false);

        let bins = size / 2 + 1;
        for k in 0..bins {
            let mag = (re[k] * re[k] + im[k] * im[k]).sqrt();
            if !speaking {
                let a = if self.learned { Self::ADAPT } else { 1.0 };
                self.noise[k] += (mag - self.noise[k]) * a;
            }
            if !self.learned {
                continue;
            }
            // Subtract in magnitude, keep the phase. Phase is left alone
            // because the ear is largely deaf to it here, and estimating a
            // "clean" phase is where this family of algorithms goes wrong.
            let wanted = (mag - self.over * self.noise[k]).max(self.floor * mag);
            let gain = if mag > 1e-9 { wanted / mag } else { 1.0 };
            re[k] *= gain;
            im[k] *= gain;
            if k > 0 && k < size - k {
                re[size - k] = re[k];
                im[size - k] = -im[k];
            }
        }
        if !self.learned && !speaking {
            self.learned = true;
        }

        fft(&mut re, &mut im, true);

        // Overlap-add: the tail of the previous frame plus the head of this one
        // is finished and can go out; this frame's tail waits for the next.
        let hop = size / 2;
        for i in 0..hop {
            self.ready[i] = self.tail[i] + re[i] * self.window[i];
            self.tail[i] = re[hop + i] * self.window[hop + i];
        }
    }
}

impl Default for SpectralSubtractor {
    fn default() -> Self {
        Self::new()
    }
}

/// In-place radix-2 FFT.
///
/// Written out rather than pulled in: the only transform this file needs is a
/// power-of-two of one fixed size, and a dependency for that would be more
/// code to audit than the twenty lines it replaces.
fn fft(re: &mut [f32], im: &mut [f32], inverse: bool) {
    let n = re.len();
    debug_assert!(n.is_power_of_two());

    // Bit-reversal permutation.
    let mut j = 0usize;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }

    let sign = if inverse { 1.0 } else { -1.0 };
    let mut len = 2;
    while len <= n {
        let angle = sign * 2.0 * PI / len as f32;
        let (wr, wi) = (angle.cos(), angle.sin());
        for start in (0..n).step_by(len) {
            let (mut cr, mut ci) = (1.0f32, 0.0f32);
            for k in 0..len / 2 {
                let (ar, ai) = (re[start + k], im[start + k]);
                let (br, bi) = (re[start + k + len / 2], im[start + k + len / 2]);
                let (tr, ti) = (br * cr - bi * ci, br * ci + bi * cr);
                re[start + k] = ar + tr;
                im[start + k] = ai + ti;
                re[start + k + len / 2] = ar - tr;
                im[start + k + len / 2] = ai - ti;
                let next = (cr * wr - ci * wi, cr * wi + ci * wr);
                cr = next.0;
                ci = next.1;
            }
        }
        len <<= 1;
    }

    if inverse {
        let scale = 1.0 / n as f32;
        for i in 0..n {
            re[i] *= scale;
            im[i] *= scale;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(n: usize, hz: f32, amp: f32) -> Vec<f32> {
        (0..n)
            .map(|i| amp * (2.0 * PI * hz * i as f32 / 48_000.0).sin())
            .collect()
    }

    #[test]
    fn fft_round_trips() {
        let mut re = tone(512, 1000.0, 0.5);
        let original = re.clone();
        let mut im = vec![0.0; 512];
        fft(&mut re, &mut im, false);
        fft(&mut re, &mut im, true);
        for (a, b) in original.iter().zip(re.iter()) {
            assert!((a - b).abs() < 1e-3, "{a} vs {b}");
        }
    }

    #[test]
    fn expander_leaves_speech_alone_and_pushes_hiss_down() {
        let mut ex = Expander::standard();

        // Well above the floor: untouched, near enough.
        let mut loud = vec![0.5f32; 480];
        for _ in 0..20 {
            ex.process(&mut loud.clone(), -20.0, -60.0);
        }
        let gain = ex.process(&mut loud, -20.0, -60.0);
        assert!(gain > -0.5, "speech was attenuated by {gain} dB");

        // Down at the floor: pushed well down.
        let mut quiet = vec![0.01f32; 480];
        for _ in 0..40 {
            ex.process(&mut quiet.clone(), -60.0, -60.0);
        }
        let gain = ex.process(&mut quiet, -60.0, -60.0);
        assert!(gain < -10.0, "hiss only attenuated by {gain} dB");
    }

    #[test]
    fn expander_never_exceeds_its_range() {
        let mut ex = Expander::new(6.0, 12.0, 6.0);
        let mut buf = vec![0.001f32; 480];
        for _ in 0..200 {
            ex.process(&mut buf.clone(), -90.0, -50.0);
        }
        let gain = ex.process(&mut buf, -90.0, -50.0);
        assert!(gain >= -12.5, "range exceeded: {gain}");
    }

    #[test]
    fn spectral_subtraction_reduces_stationary_noise() {
        let mut ss = SpectralSubtractor::new();

        // Learn from noise alone. Deterministic, so the test cannot flake.
        let mut seed = 12345u32;
        let mut noise = || {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            ((seed >> 16) as f32 / 32768.0 - 1.0) * 0.02
        };
        for _ in 0..40 {
            let mut block: Vec<f32> = (0..480).map(|_| noise()).collect();
            ss.process(&mut block, false);
        }

        let mut block: Vec<f32> = (0..480).map(|_| noise()).collect();
        let before = crate::audio::dsp::rms(&block);
        ss.process(&mut block, false);
        let after = crate::audio::dsp::rms(&block);
        assert!(
            after < before * 0.8,
            "noise not reduced: {before} -> {after}"
        );
    }

    #[test]
    fn spectral_subtraction_keeps_a_tone_it_never_learned() {
        let mut ss = SpectralSubtractor::new();
        // Learn a silent floor, then pass a loud tone through while "speaking".
        for _ in 0..20 {
            ss.process(&mut vec![0.0f32; 480], false);
        }
        let mut block = tone(480, 1000.0, 0.4);
        let before = crate::audio::dsp::rms(&block);
        for _ in 0..4 {
            ss.process(&mut block, true);
        }
        let after = crate::audio::dsp::rms(&block);
        assert!(
            after > before * 0.5,
            "speech was gutted: {before} -> {after}"
        );
    }

    #[test]
    fn mode_maps_from_its_wire_index() {
        assert_eq!(DehissMode::from_index(0), DehissMode::Off);
        assert_eq!(DehissMode::from_index(1), DehissMode::Expander);
        assert_eq!(DehissMode::from_index(2), DehissMode::Spectral);
        // Anything a newer build might send falls back to changing nothing.
        assert_eq!(DehissMode::from_index(7), DehissMode::Off);
    }
}
