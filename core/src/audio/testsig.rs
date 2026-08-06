//! Signals to test the audio chain against.
//!
//! Every audio test in this crate so far has written its own generator: an LCG
//! here, a sine there, a different one-off in each file. That was fine while
//! each test needed one waveform, and it stops being fine now, because the
//! suppression work needs *the same* wind heard by two different chains, and a
//! noise that differs between two runs cannot compare them.
//!
//! So these are shared, seeded and deterministic. Same seed, same samples, on
//! any machine.
//!
//! # Why they are synthesised rather than recorded
//!
//! A committed corpus of three wav files is a corpus a chain can be tuned to
//! pass. Parameterised generators can be swept — a hundred different winds, a
//! hundred different engines — so "it passes" means it passes a family of
//! cases rather than the three that happened to be recorded. It also keeps the
//! repository small and the tests reproducible on a machine with no assets.
//!
//! The trade is honesty about what they are: these are *models* of wind and
//! engines, not recordings of them. They are built to have the properties that
//! matter to the decisions under test — a wind that is broadband, gusting and
//! aperiodic; an engine that is loud, low and strongly harmonic — and a chain
//! that passes here still has to be ridden with.

use std::f32::consts::PI;

/// Sample rate everything here is generated at.
pub const RATE: f32 = 48_000.0;

/// A small deterministic generator.
///
/// Hand-rolled rather than pulled from `rand` so that a seed means the same
/// samples for ever, independent of which version of a crate is resolved.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        // Any odd constant will do; this one is a well-known multiplier.
        Self(
            seed.wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407),
        )
    }

    /// Uniform in 0..1.
    ///
    /// Named `unit` rather than `next` so it cannot be mistaken for an
    /// iterator: a type with a `next` that is not `Iterator::next` reads as one
    /// at every call site.
    pub fn unit(&mut self) -> f32 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((self.0 >> 40) as f32) / ((1u64 << 24) as f32)
    }

    /// Uniform in -1..1.
    pub fn bipolar(&mut self) -> f32 {
        self.unit() * 2.0 - 1.0
    }

    /// Uniform in `lo..hi`.
    pub fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + self.unit() * (hi - lo)
    }
}

/// White noise.
pub fn white(len: usize, amp: f32, seed: u64) -> Vec<f32> {
    let mut rng = Rng::new(seed);
    (0..len).map(|_| rng.bipolar() * amp).collect()
}

/// Voiced speech: a pitch, its harmonics, and a syllable rhythm.
///
/// Not speech, and not pretending to be — there are no formant transitions and
/// no consonants. What it does have is the property the transmit decision turns
/// on: it is *periodic at a human pitch*, and it starts and stops the way
/// talking does.
pub fn speech(len: usize, f0_hz: f32, amp: f32) -> Vec<f32> {
    (0..len)
        .map(|i| {
            let t = i as f32 / RATE;
            // Harmonics all the way up through the speech band, not just the
            // first few. An earlier version stopped at five, which put every
            // scrap of energy below 700 Hz and left most of an intelligibility
            // measure's bands empty — and empty bands score near-perfectly
            // whatever is compared against them, so the measure looked broken
            // when the signal was.
            //
            // Rolling off as 1/h^1.2 is roughly a voiced vowel's distribution;
            // the cut at 4 kHz is where speech has spent its information.
            let mut v = 0.0;
            let mut h = 1;
            while (f0_hz * h as f32) < 4_000.0 && h <= 40 {
                v += (2.0 * PI * f0_hz * h as f32 * t).sin() / (h as f32).powf(1.2);
                h += 1;
            }
            // Syllables at about three and a half a second, and *deep* — real
            // speech swings twenty-odd dB between a vowel and the gap after
            // it. An earlier version varied only about 5 dB, which sounds
            // plausible and is not: with so little dynamic range an
            // intelligibility measure cannot tell speech from steady noise,
            // because there is barely a shape to compare.
            let syllable = 0.05 + 0.95 * (0.5 + 0.5 * (2.0 * PI * 3.5 * t).sin());
            v * syllable * amp * 0.4
        })
        .collect()
}

/// Whispered or unvoiced speech: shaped noise with the same rhythm.
///
/// The case a harmonicity gate is most likely to throw away, and therefore the
/// one worth keeping in the suite.
pub fn whisper(len: usize, amp: f32, seed: u64) -> Vec<f32> {
    let mut rng = Rng::new(seed);
    let mut low = 0.0f32;
    (0..len)
        .map(|i| {
            let t = i as f32 / RATE;
            // Gently low-passed noise sits where unvoiced speech does.
            low = low * 0.6 + rng.bipolar() * 0.4;
            let syllable = 0.5 + 0.5 * (2.0 * PI * 3.5 * t).sin();
            low * syllable * amp
        })
        .collect()
}

/// Wind in a helmet: broadband, gusting, aperiodic.
///
/// Brown-ish noise — an integrator over white, which gives the falling spectrum
/// moving air has — modulated by gusts of random depth and length. The gusts
/// are the point: steady noise is what a noise-floor tracker is *for*, and the
/// reason wind defeats a level-based gate is that it does not hold still.
pub fn wind(len: usize, amp: f32, seed: u64) -> Vec<f32> {
    let mut rng = Rng::new(seed);
    let mut brown = 0.0f32;

    // A handful of gusts at random times, depths and durations.
    let gusts: Vec<(f32, f32, f32)> = (0..6)
        .map(|_| {
            (
                rng.range(0.0, len as f32 / RATE),
                rng.range(0.15, 0.9),
                rng.range(0.2, 1.4),
            )
        })
        .collect();

    (0..len)
        .map(|i| {
            let t = i as f32 / RATE;
            brown = (brown + rng.bipolar() * 0.06).clamp(-1.0, 1.0);
            // Leak, or it wanders off as a DC drift rather than sounding like air.
            brown *= 0.999;

            let mut envelope = 0.35;
            for &(at, depth, width) in &gusts {
                let d = (t - at) / width;
                if d.abs() < 3.0 {
                    envelope += depth * (-d * d).exp();
                }
            }
            brown * envelope * amp
        })
        .collect()
}

/// An idling motorcycle: a low fundamental and a tall stack of harmonics.
///
/// **The adversarial case for anything that decides by tonality.** An engine at
/// a stoplight is strongly harmonic — more so than a voice — so a gate that
/// opens on "this is tonal" opens on this. What separates them is *where* the
/// fundamental is: firing frequencies live around 30–60 Hz, well below the
/// 75–350 Hz a human pitch occupies.
pub fn engine(len: usize, fundamental_hz: f32, amp: f32, seed: u64) -> Vec<f32> {
    let mut rng = Rng::new(seed);
    // Cylinders are never quite even, and the wobble is what stops this being a
    // synthetic buzz that any notch filter would remove.
    let wobble: Vec<f32> = (0..12).map(|_| rng.range(0.98, 1.02)).collect();
    let phase: Vec<f32> = (0..12).map(|_| rng.range(0.0, 2.0 * PI)).collect();

    (0..len)
        .map(|i| {
            let t = i as f32 / RATE;
            let mut v = 0.0;
            for h in 1..=12 {
                let f = fundamental_hz * h as f32 * wobble[h - 1];
                v += (2.0 * PI * f * t + phase[h - 1]).sin() / (h as f32).powf(0.8);
            }
            // Idle is not steady; it breathes.
            let breathing = 0.85 + 0.15 * (2.0 * PI * 1.7 * t).sin();
            v * breathing * amp * 0.25
        })
        .collect()
}

/// Several loud motorcycles waiting at a light.
///
/// Detuned copies at different fundamentals and phases, which is both what it
/// sounds like and what makes it worse than one: the beating between two nearly
/// equal fundamentals produces slow modulation that looks like speech rhythm.
pub fn traffic(len: usize, amp: f32, seed: u64) -> Vec<f32> {
    let mut rng = Rng::new(seed);
    let mut out = vec![0.0f32; len];
    for _ in 0..3 {
        let f = rng.range(28.0, 62.0);
        let level = rng.range(0.5, 1.0);
        let bike = engine(len, f, amp * level, rng.0);
        for (o, b) in out.iter_mut().zip(bike) {
            *o += b;
        }
    }
    out
}

/// Music: a chord, a bassline and a beat.
///
/// Harmonic *and* rhythmically structured, so it defeats naive tests in both
/// directions — a tonality check calls it voice, and an "is it modulated like
/// speech" check calls it voice too.
pub fn music(len: usize, amp: f32, seed: u64) -> Vec<f32> {
    let mut rng = Rng::new(seed);
    let root = rng.range(90.0, 130.0);
    // A minor triad, near enough, plus the octave.
    let chord = [1.0f32, 1.2, 1.5, 2.0];
    let beat_hz = rng.range(1.6, 2.6);

    (0..len)
        .map(|i| {
            let t = i as f32 / RATE;
            let mut v = 0.0;
            for (n, ratio) in chord.iter().enumerate() {
                for h in 1..=4 {
                    let f = root * ratio * h as f32;
                    v += (2.0 * PI * f * t + n as f32).sin() / (h as f32 * 2.0);
                }
            }
            // Percussive transients on the beat, which is where a limiter and a
            // gate both get tested.
            let beat = ((t * beat_hz).fract() * -12.0).exp();
            (v * 0.3 + beat * 0.8) * amp * 0.3
        })
        .collect()
}

/// Noise nobody designed for.
///
/// Randomised filter shape and modulation, so passing the suite cannot mean
/// having been tuned to the three named cases above. If a chain only survives
/// wind, engines and music, this is what finds out.
pub fn unknown(len: usize, amp: f32, seed: u64) -> Vec<f32> {
    let mut rng = Rng::new(seed);
    let pole = rng.range(0.1, 0.95); // anything from nearly white to very dark
    let mod_hz = rng.range(0.2, 9.0);
    let depth = rng.range(0.0, 0.9);
    let mut state = 0.0f32;

    (0..len)
        .map(|i| {
            let t = i as f32 / RATE;
            state = state * pole + rng.bipolar() * (1.0 - pole);
            let envelope = 1.0 - depth + depth * (0.5 + 0.5 * (2.0 * PI * mod_hz * t).sin());
            state * envelope * amp * 2.0
        })
        .collect()
}

/// Root mean square of a signal.
pub fn rms(x: &[f32]) -> f32 {
    if x.is_empty() {
        return 0.0;
    }
    (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt()
}

/// Mixes `noise` under `signal` at a given signal-to-noise ratio in dB.
///
/// The noise is scaled, never the signal, so the thing being measured keeps the
/// level it was generated at and two mixes at different SNRs remain comparable.
pub fn mix(signal: &[f32], noise: &[f32], snr_db: f32) -> Vec<f32> {
    let s = rms(signal);
    let n = rms(noise);
    if s <= 0.0 || n <= 0.0 {
        return signal.to_vec();
    }
    let wanted = s / 10f32.powf(snr_db / 20.0);
    let scale = wanted / n;

    signal
        .iter()
        .zip(noise.iter())
        .map(|(a, b)| a + b * scale)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_seed_gives_the_same_signal() {
        // Without this nothing else in the suite can compare two runs.
        assert_eq!(wind(4_800, 0.5, 7), wind(4_800, 0.5, 7));
        assert_ne!(wind(4_800, 0.5, 7), wind(4_800, 0.5, 8));
        assert_eq!(traffic(4_800, 0.5, 3), traffic(4_800, 0.5, 3));
    }

    #[test]
    fn every_generator_stays_finite_and_bounded() {
        // A generator that quietly produces a NaN or a value far outside the
        // range takes a whole suite down in a way that looks like a chain bug.
        let cases: Vec<(&str, Vec<f32>)> = vec![
            ("white", white(48_000, 0.5, 1)),
            ("speech", speech(48_000, 120.0, 0.5)),
            ("whisper", whisper(48_000, 0.5, 2)),
            ("wind", wind(48_000, 0.5, 3)),
            ("engine", engine(48_000, 45.0, 0.5, 4)),
            ("traffic", traffic(48_000, 0.5, 5)),
            ("music", music(48_000, 0.5, 6)),
            ("unknown", unknown(48_000, 0.5, 7)),
        ];
        for (name, signal) in cases {
            assert!(
                signal.iter().all(|v| v.is_finite()),
                "{name} produced a NaN"
            );
            assert!(
                signal.iter().all(|v| v.abs() < 8.0),
                "{name} ran away to {}",
                signal.iter().cloned().fold(0.0f32, |a, b| a.max(b.abs()))
            );
            assert!(rms(&signal) > 1e-4, "{name} produced near-silence");
        }
    }

    #[test]
    fn mixing_lands_on_the_requested_ratio() {
        let s = speech(48_000, 130.0, 0.4);
        let n = white(48_000, 1.0, 11);
        for snr_db in [20.0f32, 6.0, 0.0, -6.0] {
            let mixed = mix(&s, &n, snr_db);
            // Recover the noise by subtraction, since we know both parts.
            let noise: Vec<f32> = mixed.iter().zip(&s).map(|(m, a)| m - a).collect();
            let got = 20.0 * (rms(&s) / rms(&noise)).log10();
            assert!(
                (got - snr_db).abs() < 0.5,
                "asked for {snr_db} dB, mixed at {got} dB"
            );
        }
    }

    #[test]
    fn an_engine_beats_below_where_a_voice_can_go() {
        // The property the whole harmonicity plan rests on, asserted rather
        // than assumed: a motorcycle's firing fundamental sits *below* the
        // range a human pitch occupies, so a gate that searches only 75-350 Hz
        // rejects the stoplight case by construction.
        //
        // Note what is deliberately not asserted here. An earlier version of
        // this test claimed an engine is "more tonal than wind" by a plain
        // autocorrelation peak, and it failed — brown-ish wind is itself
        // strongly autocorrelated at long lags. Tonality alone does not
        // separate them, which is exactly why the transmit gate constrains the
        // lag range instead of thresholding tonality.
        for f0 in [30.0f32, 45.0, 60.0] {
            let e = engine(48_000, f0, 0.5, 21);
            let lag = strongest_lag(&e, 400, 2_000);
            let hz = RATE / lag as f32;
            assert!(
                hz < 75.0,
                "a {f0} Hz engine peaked at {hz:.0} Hz, inside the voice range"
            );
        }
    }

    #[test]
    fn speech_beats_inside_where_a_voice_lives() {
        // The other half of the same claim.
        for f0 in [90.0f32, 140.0, 220.0] {
            let s = speech(48_000, f0, 0.5);
            let lag = strongest_lag(&s, 100, 700);
            let hz = RATE / lag as f32;
            assert!(
                (hz - f0).abs() / f0 < 0.1,
                "{f0} Hz speech peaked at {hz:.0} Hz"
            );
        }
    }

    /// The lag, within a range, at which a signal most repeats itself.
    fn strongest_lag(x: &[f32], lo: usize, hi: usize) -> usize {
        let n = 9_600.min(x.len() / 2);
        let mut best = (lo, f32::MIN);
        for lag in lo..hi.min(x.len() - n) {
            let mut sum = 0.0f32;
            for i in 0..n {
                sum += x[i] * x[i + lag];
            }
            if sum > best.1 {
                best = (lag, sum);
            }
        }
        best.0
    }
}
