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
//!
//! It is also only correct if the reference is *aligned* with the echo, and it
//! is not aligned by construction: the reference is taken where audio is handed
//! to the device, and the echo comes back after everything between there and
//! the speaker. [`Aligner`] measures that and shifts the reference; without it
//! the filter is being asked to model a delay rather than a room, and the taps
//! run out long before the echo arrives.

use std::collections::VecDeque;

/// Filter length in taps.
///
/// 1024 taps at 48 kHz models ~21 ms of echo *spread* — the direct path plus
/// early reflections, measured from wherever [`Aligner`] has decided the echo
/// begins. It is not, and never was, a budget for the bulk delay: aligning
/// first is what makes a short filter enough, and the two together are what
/// make a speakerphone work at all.
///
/// It was 512 (10.7 ms), sized for a helmet speaker centimetres from the
/// microphone and proven against a synthetic path whose last reflection was at
/// 0.98 ms. That was true and useless in the reported case — two phones on
/// loudspeaker, where the tap-to-speaker latency alone put the echo outside the
/// window and no amount of adaptation could reach it.
pub const DEFAULT_TAPS: usize = 1024;

/// How far back the aligner can look for the echo, in samples: 1 s at 48 kHz.
///
/// The bulk delay is everything between the reference being *taken* and the
/// sound leaving the speaker — the device-rate `pending` buffer, the OS output
/// buffer, and the headset or speaker itself. Bluetooth HFP alone can spend
/// over 100 ms there. A second is far more than any of it and costs 192 KB.
const HISTORY_SAMPLES: usize = 48_000;

/// One envelope point per 10 ms, matching the capture block.
const ENV_HOP: usize = 480;

/// Envelope points kept for the search: 4 s.
const ENV_POINTS: usize = 400;

/// Longest delay searched, in envelope points — 500 ms.
const MAX_LAG_POINTS: usize = 50;

/// Points needed before a search means anything.
const MIN_SEARCH_POINTS: usize = MAX_LAG_POINTS + 32;

/// How often to re-run the search, in envelope points — about a second.
///
/// Not once at startup. The alignment moves: the elastic jitter buffer plays a
/// backlog off at up to 2×, the reference queue is cleared outright when the
/// worker falls behind, and a route change to or from Bluetooth moves it by
/// more than the filter is long.
const SEARCH_EVERY: usize = 100;

/// Correlation a candidate must reach before the alignment is moved.
const ACCEPT_CORR: f32 = 0.5;

/// A second arrival counts as real at this fraction of the strongest one.
///
/// There is often more than one path. A phone that mixes what it is playing
/// back into its own capture — an internal route rather than the air — delivers
/// a copy that is early, loud and almost undistorted, while the sound coming
/// back through the room arrives tens of milliseconds later. Both are echo,
/// both are in the reference, and a filter pointed at either one alone leaves
/// the other untouched.
const PEAK_FRACTION: f32 = 0.6;

/// Longest filter the spread is allowed to buy: 4 096 taps, 85 ms.
///
/// Only reached when two arrivals are genuinely that far apart, and only paid
/// for while something is playing — see [`EchoCanceller::idle_run`].
const MAX_TAPS: usize = 4096;

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
    align: Aligner,
    /// Consecutive silent reference samples seen, saturating at `taps`.
    ///
    /// Doubling the filter length doubled the arithmetic on every sample, and
    /// most samples have nothing to cancel: on a headset there is no acoustic
    /// path at all, and even on a speakerphone nobody is talking most of the
    /// time. Once the whole ring is silence the estimate is provably zero, and
    /// both loops can be skipped rather than multiplying a thousand zeros.
    ///
    /// This is what pays for the longer filter — and it more than pays, because
    /// the old 512-tap version was doing the full multiply against a ring of
    /// zeros for every silent block of every call.
    idle_run: usize,
}

/// Finds how far behind the reference the echo actually is, and holds enough
/// history to look back that far.
///
/// The reference is taken where the samples are *handed to the device*, which
/// is not where they leave the speaker. Everything in between — the device-rate
/// buffer, the OS, the radio, the driver — is latency nobody measured, and the
/// adaptive filter can only look backwards. Handing it a reference that has not
/// been played yet is asking it to predict the future, and it answers by
/// learning nothing.
///
/// So: correlate the loudness of what was played against the loudness of what
/// came back, at 10 ms resolution, and shift the reference by the answer. Block
/// energies rather than samples because the echo path colours the sound and
/// delays it, and only the delay is wanted here — the filter takes care of the
/// colouring, which is the one thing it is good at.
///
/// **It keys on loudness changing over time, so it needs the far end to be
/// speech-shaped.** Syllables give it everything it needs; a steady tone or
/// unbroken music has a flat envelope and nothing to correlate, and the
/// alignment simply stays where it was rather than moving to a wrong answer —
/// `ACCEPT_CORR` is what enforces that. This is the right failure: a stale
/// alignment still cancels, a confidently wrong one cancels nothing.
struct Aligner {
    history: Vec<f32>,
    pos: usize,
    acc_ref: f32,
    acc_mic: f32,
    acc_n: usize,
    env_ref: VecDeque<f32>,
    env_mic: VecDeque<f32>,
    since_search: usize,
    /// Current alignment in samples, applied to the reference.
    lag: usize,
    /// How far the arrivals spread beyond `lag`, in samples: what the filter
    /// has to be long enough to cover.
    span: usize,
    corr: f32,
    /// Cleared only by the test that shows what this class costs when it is
    /// missing — which is the whole of the reported fault, and is otherwise
    /// invisible in a suite where every path is a millisecond long.
    searching: bool,
}

impl Aligner {
    fn new() -> Self {
        Self {
            history: vec![0.0; HISTORY_SAMPLES],
            pos: 0,
            acc_ref: 0.0,
            acc_mic: 0.0,
            acc_n: 0,
            env_ref: VecDeque::with_capacity(ENV_POINTS),
            env_mic: VecDeque::with_capacity(ENV_POINTS),
            since_search: 0,
            lag: 0,
            span: DEFAULT_TAPS,
            corr: 0.0,
            searching: true,
        }
    }

    fn reset(&mut self) {
        self.history.fill(0.0);
        self.pos = 0;
        self.acc_ref = 0.0;
        self.acc_mic = 0.0;
        self.acc_n = 0;
        self.env_ref.clear();
        self.env_mic.clear();
        self.since_search = 0;
        self.lag = 0;
        self.span = DEFAULT_TAPS;
        self.corr = 0.0;
    }

    /// Stores one reference sample and returns the one that should be fed to
    /// the filter for this instant — the same sample when the lag is zero.
    #[inline]
    fn push(&mut self, reference: f32) -> f32 {
        self.history[self.pos] = reference;
        let idx = (self.pos + HISTORY_SAMPLES - self.lag) % HISTORY_SAMPLES;
        let aligned = self.history[idx];
        self.pos = (self.pos + 1) % HISTORY_SAMPLES;
        aligned
    }

    /// Accumulates the envelopes. Returns true when the alignment moved, which
    /// invalidates everything the filter has learned.
    fn observe(&mut self, reference: f32, mic: f32) -> bool {
        self.acc_ref += reference * reference;
        self.acc_mic += mic * mic;
        self.acc_n += 1;
        if self.acc_n < ENV_HOP {
            return false;
        }
        let n = self.acc_n as f32;
        // Log energy: the search is about *when*, not about how loud, and a
        // linear envelope lets one shout dominate the correlation.
        let db = |p: f32| 10.0 * (p / n + 1e-12).log10();
        if self.env_ref.len() == ENV_POINTS {
            self.env_ref.pop_front();
            self.env_mic.pop_front();
        }
        self.env_ref.push_back(db(self.acc_ref));
        self.env_mic.push_back(db(self.acc_mic));
        self.acc_ref = 0.0;
        self.acc_mic = 0.0;
        self.acc_n = 0;

        self.since_search += 1;
        if !self.searching
            || self.since_search < SEARCH_EVERY
            || self.env_ref.len() < MIN_SEARCH_POINTS
        {
            return false;
        }
        self.since_search = 0;
        self.search()
    }

    /// Correlates the two envelopes over the plausible lags and adopts the best
    /// one if it is convincing. Returns whether the lag changed.
    fn search(&mut self) -> bool {
        let n = self.env_ref.len();
        let window = n - MAX_LAG_POINTS;
        let mic: Vec<f32> = self.env_mic.iter().skip(MAX_LAG_POINTS).copied().collect();
        let mic_mean = mic.iter().sum::<f32>() / window as f32;
        let mic_var: f32 = mic.iter().map(|v| (v - mic_mean).powi(2)).sum();
        if mic_var < 1.0 {
            return false; // nothing came back to correlate against
        }

        let mut best = (0usize, 0.0f32);
        let mut scores = [0.0f32; MAX_LAG_POINTS];
        for (lag, score) in scores.iter_mut().enumerate() {
            let start = MAX_LAG_POINTS - lag;
            let r: Vec<f32> = self
                .env_ref
                .iter()
                .skip(start)
                .take(window)
                .copied()
                .collect();
            let r_mean = r.iter().sum::<f32>() / window as f32;
            let r_var: f32 = r.iter().map(|v| (v - r_mean).powi(2)).sum();
            if r_var < 1.0 {
                continue; // the far end was silent through this stretch
            }
            let cov: f32 = r
                .iter()
                .zip(&mic)
                .map(|(a, b)| (a - r_mean) * (b - mic_mean))
                .sum();
            let corr = cov / (r_var * mic_var).sqrt();
            *score = corr;
            if corr > best.1 {
                best = (lag, corr);
            }
        }

        self.corr = best.1;
        if best.1 < ACCEPT_CORR {
            return false;
        }

        // Every arrival worth cancelling, not just the loudest. The filter is
        // then pointed at the earliest and made long enough to reach the last:
        // an internally mixed copy and its acoustic twin are one echo with two
        // arrival times, and cancelling half of it sounds like cancelling none.
        let floor = (best.1 * PEAK_FRACTION).max(ACCEPT_CORR);
        let mut earliest = best.0;
        let mut latest = best.0;
        for (lag, corr) in scores.iter().enumerate() {
            if *corr >= floor {
                earliest = earliest.min(lag);
                latest = latest.max(lag);
            }
        }

        // One point short, deliberately. The search resolves to 10 ms and the
        // filter can only look backwards from where it is pointed: aiming a
        // block early puts the true arrival inside its span instead of just
        // before the first tap, where it would be invisible however long the
        // filter was.
        let lag = earliest.saturating_sub(1) * ENV_HOP;
        // Two points of headroom: one for the aim-early above, one for the
        // resolution at the far end.
        let span = (latest - earliest + 2) * ENV_HOP;
        if lag == self.lag && span == self.span {
            return false;
        }
        self.lag = lag;
        self.span = span;
        true
    }
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
            align: Aligner::new(),
            idle_run: 0,
        }
    }

    /// How far behind the reference the echo was last measured to be, in
    /// milliseconds, and how convincing that measurement was.
    ///
    /// Worth showing: a canceller that is doing nothing looks identical to one
    /// with nothing to do, and this is the number that tells them apart.
    pub fn alignment(&self) -> (f32, f32) {
        (self.align.lag as f32 * 1000.0 / 48_000.0, self.align.corr)
    }

    /// How much echo path the filter currently spans, in milliseconds, starting
    /// from [`Self::alignment`].
    ///
    /// The pair is what matters, not either alone: the alignment deliberately
    /// points *before* the earliest arrival, so an alignment that reads early
    /// is the design working rather than a miss. What has to be true is that
    /// the echo falls somewhere in `lag ..= lag + span`.
    pub fn filter_span_ms(&self) -> f32 {
        self.taps as f32 * 1000.0 / 48_000.0
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
        self.forget_path();
        self.align.reset();
    }

    /// Throws away the learned impulse response but keeps the alignment.
    ///
    /// Separate from [`Self::reset`] because a moved alignment invalidates the
    /// coefficients — they describe a path measured from somewhere else — while
    /// the delay measurement that caused it is the one thing worth keeping.
    fn forget_path(&mut self) {
        self.w.iter_mut().for_each(|v| *v = 0.0);
        self.ring.iter_mut().for_each(|v| *v = 0.0);
        self.pos = 0;
        self.ref_power = 0.0;
        self.smooth_mic = 0.0;
        self.smooth_out = 0.0;
        self.smooth_ref = 0.0;
        self.idle_run = 0;
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
    /// was played out.
    ///
    /// The two must be the same length and must correspond to the same instant
    /// *as the caller sees it*; they do not have to be aligned, and in practice
    /// they never are. Whatever delay the playback path adds between the two is
    /// measured here and taken out — see [`Aligner`].
    ///
    /// Returns the estimated ERLE in dB.
    pub fn process(&mut self, mic: &mut [f32], reference: &[f32]) -> f32 {
        if !self.enabled {
            return 0.0;
        }
        let n = mic.len().min(reference.len());

        for i in 0..n {
            // Measured against what was played and what came back, not against
            // what the filter has been told to believe: `observe` sees the raw
            // pair, `push` hands over the shifted one.
            if self.align.observe(reference[i], mic[i]) {
                // The measurement decides the filter's length as well as where
                // it points: one arrival needs the default, two far apart need
                // enough to reach from the first to the last.
                let want = self.align.span.clamp(DEFAULT_TAPS, MAX_TAPS);
                if want != self.taps {
                    self.taps = want;
                    self.w = vec![0.0; want];
                    self.ring = vec![0.0; want];
                }
                self.forget_path();
            }
            let aligned = self.align.push(reference[i]);
            self.push_reference(aligned);

            // Nothing has come out of the speaker for a whole filter length, so
            // every tap is multiplying a zero. Skipping is not an approximation
            // here — the estimate is exactly zero and there is nothing to
            // adapt towards.
            self.idle_run = if aligned.abs() < 1e-7 {
                (self.idle_run + 1).min(self.taps)
            } else {
                0
            };
            if self.idle_run >= self.taps {
                const A: f32 = 0.999;
                let d = mic[i];
                self.smooth_mic = A * self.smooth_mic + (1.0 - A) * d * d;
                self.smooth_out = A * self.smooth_out + (1.0 - A) * d * d;
                self.smooth_ref *= A;
                continue;
            }

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
            // The aligned sample, not the raw one: this is the "is there
            // anything to cancel right now" signal that gates adaptation, and
            // the answer has to be about the echo in *this* block rather than
            // about audio that has not reached the speaker yet.
            self.smooth_ref = A * self.smooth_ref + (1.0 - A) * aligned * aligned;

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

    /// Noise with a speech-like loudness contour: bursts and gaps at roughly a
    /// syllabic rate, which is what the aligner actually keys on.
    fn syllabic(len: usize, seed: u32) -> Vec<f32> {
        let base = noise(len, seed, 0.4);
        base.iter()
            .enumerate()
            .map(|(i, s)| {
                let t = i as f32 / 48_000.0;
                // 3.5 Hz, never quite silent, so the far end is always present
                // enough for the double-talk guard to allow adaptation.
                let env = 0.08 + 0.92 * (0.5 - 0.5 * (t * 3.5 * std::f32::consts::TAU).cos());
                s * env
            })
            .collect()
    }

    /// A speakerphone path: the same room response, arriving 120 ms late.
    ///
    /// The delay is not the room. It is the reference being taken where audio
    /// is handed to the device and the echo coming back after the device-rate
    /// buffer, the OS and the speaker have each had their turn — 120 ms is
    /// unremarkable for a phone and modest for Bluetooth.
    ///
    /// Built as a delay in front of the path rather than a longer path, because
    /// that is what it is, and because a 5 760-tap impulse response would hide
    /// the point: no filter this side of absurd covers it, and alignment costs
    /// nothing.
    fn delayed_path(delay: usize) -> Vec<f32> {
        let short = echo_path();
        let mut h = vec![0.0f32; delay + short.len()];
        h[delay..].copy_from_slice(&short);
        h
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

    /// The reported fault, as a test: two phones on loudspeaker.
    ///
    /// Before the aligner this failed flat — not "cancelled less", but left the
    /// echo untouched, because every tap of the filter sat in the 120 ms of
    /// silence before the echo began. It is the whole reason the canceller
    /// appeared to be switched off while being switched on.
    #[test]
    fn cancels_an_echo_delayed_by_the_playback_path() {
        const DELAY: usize = 120 * 48; // 120 ms at 48 kHz
        let far = noise(48_000 * 8, 1, 0.3);
        let echo = convolve(&far, &delayed_path(DELAY));

        let mut aec = EchoCanceller::new(DEFAULT_TAPS);
        let mut before = Vec::new();
        let mut after = Vec::new();

        for (i, (m, r)) in echo.chunks(480).zip(far.chunks(480)).enumerate() {
            let mut block = m.to_vec();
            aec.process(&mut block, r);
            // The search needs its window, and the filter needs to converge on
            // the alignment once it has one.
            if i > 600 {
                before.extend_from_slice(m);
                after.extend_from_slice(&block);
            }
        }

        let (lag_ms, corr) = aec.alignment();
        assert!(
            (100.0..=125.0).contains(&lag_ms),
            "alignment should land within a block of 120 ms, got {lag_ms:.0} ms (corr {corr:.2})"
        );
        let erle = 20.0 * (rms(&before) / rms(&after)).log10();
        assert!(
            erle > 10.0,
            "delayed echo should be cancelled once aligned, got {erle:.1} dB"
        );

        // And the other half, which is the point: the same filter, the same
        // signal, the same everything except that it is never told where the
        // echo is. This is what shipped, and what "echo cancellation is on and
        // makes no difference" looked like from the outside.
        let mut blind = EchoCanceller::new(DEFAULT_TAPS);
        blind.align.searching = false;
        let mut blind_after = Vec::new();
        for (i, (m, r)) in echo.chunks(480).zip(far.chunks(480)).enumerate() {
            let mut block = m.to_vec();
            blind.process(&mut block, r);
            if i > 600 {
                blind_after.extend_from_slice(&block);
            }
        }
        let blind_erle = 20.0 * (rms(&before) / rms(&blind_after)).log10();
        assert!(
            blind_erle < 1.0,
            "unaligned, there is nothing within reach to cancel: expected ~0 dB, got {blind_erle:.1} dB"
        );
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

    /// Two arrivals at once: the phone mixing its own playback into the capture
    /// buffer, *and* the same sound coming back through the room.
    ///
    /// The internal copy is early, loud and barely coloured; the acoustic one
    /// is late and filtered. Pointing the filter at either alone leaves the
    /// other at full level, which is still an echo and still loops.
    #[test]
    fn cancels_an_internal_copy_and_its_acoustic_twin() {
        const INTERNAL: usize = 5 * 48; // 5 ms, essentially the buffer itself
        const ACOUSTIC: usize = 60 * 48; // 60 ms round the room

        let mut h = vec![0.0f32; ACOUSTIC + 64];
        h[INTERNAL] = 0.55; // a clean copy, no room in it
        for (i, v) in echo_path().iter().enumerate() {
            h[ACOUSTIC + i] += *v;
        }

        // Modulated at a syllabic rate, because the aligner correlates
        // loudness over time and flat noise has no loudness over time to
        // correlate. Speech always has this; steady tones and unbroken music
        // do not, and the estimator is correspondingly weaker on them.
        let far = syllabic(48_000 * 10, 3);
        let echo = convolve(&far, &h);

        let mut aec = EchoCanceller::new(DEFAULT_TAPS);
        let mut before = Vec::new();
        let mut after = Vec::new();
        for (i, (m, r)) in echo.chunks(480).zip(far.chunks(480)).enumerate() {
            let mut block = m.to_vec();
            aec.process(&mut block, r);
            if i > 800 {
                before.extend_from_slice(m);
                after.extend_from_slice(&block);
            }
        }

        assert!(
            aec.taps > DEFAULT_TAPS,
            "the filter should have grown to reach the second arrival, stayed at {}",
            aec.taps
        );
        let erle = 20.0 * (rms(&before) / rms(&after)).log10();
        assert!(
            erle > 10.0,
            "both arrivals should be cancelled together, got {erle:.1} dB"
        );
    }

    /// The idle skip has to be a shortcut, not a behaviour.
    ///
    /// Same signals, same filter, with the reference silent for long enough to
    /// take the fast path and then loud again: the output must match a run that
    /// never took it, sample for sample, and the filter must still converge
    /// afterwards.
    #[test]
    fn skipping_a_silent_reference_changes_nothing() {
        let mut far = vec![0.0f32; 48_000 * 2];
        far.extend(noise(48_000 * 4, 5, 0.3));
        let echo = convolve(&far, &echo_path());
        let near = noise(far.len(), 9, 0.05);

        let run = |skip: bool| {
            let mut aec = EchoCanceller::new(DEFAULT_TAPS);
            let mut out = Vec::new();
            for (i, (e, r)) in echo.chunks(480).zip(far.chunks(480)).enumerate() {
                let mut block: Vec<f32> =
                    e.iter().zip(&near[i * 480..]).map(|(a, b)| a + b).collect();
                if !skip {
                    // 480 samples cannot reach a 1024-tap run, so clearing the
                    // counter each call keeps every sample on the long path.
                    aec.idle_run = 0;
                }
                aec.process(&mut block, r);
                out.extend_from_slice(&block);
            }
            out
        };

        let fast = run(true);
        let slow = run(false);
        let worst = fast
            .iter()
            .zip(&slow)
            .map(|(a, b)| (a - b).abs())
            .fold(0.0f32, f32::max);
        assert!(
            worst < 1e-6,
            "the idle shortcut must be exact, worst sample differs by {worst:e}"
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
