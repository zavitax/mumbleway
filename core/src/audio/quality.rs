//! How much of a voice survived, as a number.
//!
//! Every change to the suppression chain so far has been judged by ear, on one
//! device, by one person. That is not good enough for a decision like "should
//! the gate be harder in wind", because the two failure modes sound completely
//! different and both feel like progress: a chain that suppresses too little
//! lets wind through, and a chain that suppresses too much quietly removes the
//! speech along with it. The second one *sounds* cleaner.
//!
//! So: a score, computed from a clean reference and the degraded signal that
//! came out of the pipeline, which rises when a listener would understand more
//! words and falls when they would understand fewer.
//!
//! # What this is, precisely
//!
//! **Band-envelope correlation. It is not STOI**, and the distinction is worth
//! stating plainly because the shape is similar enough to be mistaken for it:
//! decimate, split into third-octave bands, take short-time envelopes,
//! normalise each segment, correlate band by band, average.
//!
//! STOI additionally clips the degraded envelope from above, at 1.178 times the
//! reference, so that a badly attenuated band scores merely poorly instead of
//! arbitrarily badly. That clip was tried here and removed, because with the
//! energies already normalised it manufactures the correlation it is supposed
//! to measure: flat noise sits near the reference's RMS, so roughly half the
//! samples clip to an exact scaled copy of the reference and the other half sit
//! flat, and that hybrid correlates about 0.8 with the reference no matter what
//! the degraded signal actually was. Noise in place of speech scored 0.83.
//!
//! Rather than keep guessing at the published formula, this measures the thing
//! it can defend. Dropping the clip means a silenced band scores zero rather
//! than "merely bad" — which for the question being asked, *did the chain
//! remove the voice along with the noise*, is the desirable behaviour and not a
//! loss.
//!
//! So: **absolute values are not comparable to published STOI figures**, and
//! should not be quoted as if they were. What the number is good for is
//! comparison against itself — this chain against that one, this SNR against
//! that one, before a change against after it — which is all the tests ask.

use super::dsp::fft;

/// Rate the analysis runs at.
///
/// 48 kHz divides by exactly 3, so decimation is a clean integer step with no
/// resampler and no fractional-delay argument. Speech intelligibility lives
/// well below 8 kHz, so nothing that matters is lost.
const RATE: usize = 16_000;
const DECIMATE: usize = 3;

/// Analysis frame and hop. 512 samples at 16 kHz is 32 ms, hopping 16 ms —
/// the same order as the 25.6 ms / 12.8 ms the published method uses.
const FRAME: usize = 512;
const HOP: usize = FRAME / 2;

/// Third-octave bands, from 150 Hz up. Below 150 Hz there is pitch but very
/// little intelligibility, and the top band lands near 5 kHz where consonants
/// have given up most of their information.
const BANDS: usize = 15;
const FIRST_CENTRE_HZ: f32 = 150.0;

/// Frames per comparison segment: 30 frames of 16 ms is about half a second,
/// which is roughly the span over which a listener assembles a word.
const SEGMENT: usize = 30;

/// Intelligibility of `degraded` measured against `clean`, roughly 0..1.
///
/// Both must be the same length and time-aligned, which offline they always
/// are: the harness makes the degraded signal by putting the clean one through
/// the pipeline.
///
/// Returns 0 when there is nothing to measure — too short, or silent.
pub fn intelligibility(clean: &[f32], degraded: &[f32]) -> f32 {
    let n = clean.len().min(degraded.len());
    if n < FRAME * DECIMATE * 2 {
        return 0.0;
    }

    let a = decimate(&clean[..n]);
    let b = decimate(&degraded[..n]);

    let env_a = band_envelopes(&a);
    let env_b = band_envelopes(&b);
    let frames = env_a[0].len().min(env_b[0].len());
    if frames < SEGMENT {
        return 0.0;
    }

    // The loudest band in the whole reference, so "this band is empty" is
    // judged against the signal rather than against an absolute level that
    // would move with the recording's gain.
    let peak_band_energy = env_a
        .iter()
        .map(|band| band[..frames].iter().map(|v| v * v).sum::<f32>() / frames as f32)
        .fold(0.0f32, f32::max)
        * SEGMENT as f32;

    let mut total = 0.0f64;
    let mut counted = 0usize;

    for start in 0..=(frames - SEGMENT) {
        for band in 0..BANDS {
            let x = &env_a[band][start..start + SEGMENT];
            let y = &env_b[band][start..start + SEGMENT];

            // Normalise the degraded segment to the reference's energy.
            // This is what makes the measure care about *shape* rather than
            // level: a quieter copy of the same speech is exactly as
            // intelligible, and a chain that scored better simply for being
            // louder would send us chasing gain.
            let energy_x: f32 = x.iter().map(|v| v * v).sum();
            let energy_y: f32 = y.iter().map(|v| v * v).sum();
            if energy_x <= 1e-12 || energy_y <= 1e-12 {
                continue;
            }
            // A band the reference has nothing in cannot say whether the
            // degraded signal kept anything, and correlating two lots of
            // numerical dust produces an arbitrary number that is averaged in
            // as though it meant something.
            if energy_x < peak_band_energy * 1e-4 {
                continue;
            }
            let scale = (energy_x / energy_y).sqrt();
            let mut ys = [0.0f32; SEGMENT];
            for i in 0..SEGMENT {
                ys[i] = y[i] * scale;
            }

            total += correlation(&x[..SEGMENT], &ys) as f64;
            counted += 1;
        }
    }

    if counted == 0 {
        return 0.0;
    }
    (total / counted as f64).clamp(0.0, 1.0) as f32
}

/// Drops the rate by an integer factor, low-passing first so what is folded
/// down is not the noise we are trying to measure.
fn decimate(x: &[f32]) -> Vec<f32> {
    // A short moving average is a crude anti-alias filter, and crude is
    // adequate here: it is applied identically to both signals, so whatever it
    // does to one it does to the other, and the measure is a comparison.
    let mut out = Vec::with_capacity(x.len() / DECIMATE + 1);
    let mut i = 0;
    while i + DECIMATE <= x.len() {
        let mut sum = 0.0;
        for j in 0..DECIMATE {
            sum += x[i + j];
        }
        out.push(sum / DECIMATE as f32);
        i += DECIMATE;
    }
    out
}

/// Energy in each third-octave band, frame by frame.
fn band_envelopes(x: &[f32]) -> Vec<Vec<f32>> {
    let edges = band_edges();
    let mut out = vec![Vec::new(); BANDS];

    let window: Vec<f32> = (0..FRAME)
        .map(|i| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / FRAME as f32).cos())
        .collect();

    let mut re = vec![0.0f32; FRAME];
    let mut im = vec![0.0f32; FRAME];

    let mut start = 0;
    while start + FRAME <= x.len() {
        for i in 0..FRAME {
            re[i] = x[start + i] * window[i];
            im[i] = 0.0;
        }
        fft(&mut re, &mut im, false);

        for (band, &(lo, hi)) in edges.iter().enumerate() {
            let mut power = 0.0f32;
            for bin in lo..=hi {
                power += re[bin] * re[bin] + im[bin] * im[bin];
            }
            // Root energy, which is the envelope the method correlates.
            out[band].push(power.sqrt());
        }
        start += HOP;
    }
    out
}

/// First and last bin of each third-octave band.
fn band_edges() -> [(usize, usize); BANDS] {
    let bin_hz = RATE as f32 / FRAME as f32;
    let max_bin = FRAME / 2 - 1;
    let step = 2f32.powf(1.0 / 3.0);

    let mut edges = [(0usize, 0usize); BANDS];
    for (i, edge) in edges.iter_mut().enumerate() {
        let centre = FIRST_CENTRE_HZ * step.powi(i as i32);
        let lo = centre / step.sqrt();
        let hi = centre * step.sqrt();
        let first = ((lo / bin_hz).round() as usize).clamp(1, max_bin);
        let last = ((hi / bin_hz).round() as usize).clamp(first, max_bin);
        *edge = (first, last);
    }
    edges
}

/// Pearson correlation of two equal-length segments.
fn correlation(x: &[f32], y: &[f32]) -> f32 {
    let n = x.len() as f32;
    let mean_x = x.iter().sum::<f32>() / n;
    let mean_y = y.iter().sum::<f32>() / n;

    let mut num = 0.0f32;
    let mut den_x = 0.0f32;
    let mut den_y = 0.0f32;
    for i in 0..x.len() {
        let dx = x[i] - mean_x;
        let dy = y[i] - mean_y;
        num += dx * dy;
        den_x += dx * dx;
        den_y += dy * dy;
    }
    if den_x <= 1e-12 || den_y <= 1e-12 {
        return 0.0;
    }
    num / (den_x.sqrt() * den_y.sqrt())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::testsig;

    #[test]
    fn a_signal_against_itself_scores_at_the_top() {
        let s = testsig::speech(48_000 * 2, 140.0, 0.4);
        let score = intelligibility(&s, &s);
        assert!(score > 0.95, "identical signals scored {score}");
    }

    #[test]
    fn quieter_is_not_less_intelligible() {
        // The measure has to care about shape, not level. A rider turning the
        // volume down has not become harder to understand, and a chain that
        // scored better simply by being louder would send us chasing gain.
        let s = testsig::speech(48_000 * 2, 140.0, 0.4);
        let quiet: Vec<f32> = s.iter().map(|v| v * 0.25).collect();
        let score = intelligibility(&s, &quiet);
        assert!(score > 0.9, "a quieter copy scored {score}");
    }

    #[test]
    fn noise_in_place_of_speech_scores_at_the_bottom() {
        let s = testsig::speech(48_000 * 2, 140.0, 0.4);
        let n = testsig::white(48_000 * 2, 0.4, 3);
        let score = intelligibility(&s, &n);
        assert!(score < 0.3, "noise scored {score} against speech");
    }

    #[test]
    fn silence_scores_at_the_bottom() {
        // The case that matters most, because it is what over-suppression looks
        // like: a chain that removed the voice entirely must not score well.
        let s = testsig::speech(48_000 * 2, 140.0, 0.4);
        let silence = vec![0.0f32; s.len()];
        let score = intelligibility(&s, &silence);
        assert!(score < 0.2, "silence scored {score}");
    }

    #[test]
    fn the_score_falls_as_noise_rises() {
        // Monotonicity is the property every test in this suite leans on. If
        // the measure is not ordered, no assertion built on it means anything.
        let s = testsig::speech(48_000 * 3, 140.0, 0.4);
        let mut previous = 1.0f32;
        for snr_db in [20.0f32, 10.0, 0.0, -10.0] {
            let mixed = testsig::mix(&s, &testsig::white(s.len(), 1.0, 9), snr_db);
            let score = intelligibility(&s, &mixed);
            assert!(
                score <= previous + 0.02,
                "{snr_db} dB scored {score}, above the {previous} before it"
            );
            previous = score;
        }
        assert!(previous < 0.75, "even -10 dB SNR still scored {previous}");
    }
}
