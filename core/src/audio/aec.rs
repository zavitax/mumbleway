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
use std::sync::atomic::{AtomicBool, Ordering};

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

/// How close to the best score counts as tied, for preferring the earlier lag.
const NEAR_BEST: f32 = 0.95;

/// How much better a new alignment must be before it is worth the reset.
const MOVE_MARGIN: f32 = 1.15;

/// Output louder than input by this factor means the filter is diverging.
///
/// A working canceller subtracts; one that adds is broken, and it does not
/// recover on its own — NLMS drives the error down, so an error that is
/// growing means the step size has stopped being a step and the coefficients
/// are running away. Heard as a rising roar, which is worse than the echo it
/// was removing. 9 dB is well past anything double talk produces.
const DIVERGED: f32 = 8.0;

/// The filter does **not** grow to span two distant arrivals. It was tried.
///
/// Growing it to 4 096 taps to cover an internally mixed copy and its acoustic
/// twin together measured **2.9 dB** — almost exactly one of the two arrivals —
/// and 2.5× more convergence time changed nothing, so it was not settling
/// slowly, it was settling badly.
///
/// The reason is in the normalisation. This is a time-domain NLMS whose step is
/// divided by *one* number, the total power in the reference ring. Speech is
/// strongly coloured, so that single figure is dominated by the low end and the
/// step is wrong for every other frequency at once — an error that grows with
/// filter length, because a longer filter spans more of the spectrum's dynamic
/// range per update.
///
/// **This is why both production echo cancellers are frequency-domain.**
/// SpeexDSP's MDF normalises per bin (`power_1[i]`, from a per-bin `power[i]`),
/// and WebRTC's AEC3 uses a partitioned block frequency-domain filter — 128-point
/// FFT, 64-sample partitions, 65 bins each — with a per-partition constraint that
/// zeroes the non-causal half of every partition's impulse response specifically
/// to stop it diverging. Neither of them makes a long time-domain filter work,
/// because it does not.
///
/// So the filter stays at [`DEFAULT_TAPS`] and is aimed at the strongest
/// arrival. A second arrival further away than 21 ms survives, and covering it
/// properly is a frequency-domain job that is not attempted here.
const _WHY_NO_GROWTH: () = ();

/// Which canceller a new [`EchoCanceller`] should be.
///
/// Process-wide, and read when one is built rather than per block: swapping
/// mid-call would hand the new one a room it has never heard and the rider a
/// second of echo while it learned.
static USE_AEC3: AtomicBool = AtomicBool::new(true);

/// Chooses the canceller for every [`EchoCanceller`] built afterwards.
///
/// **Default is AEC3**, on the measurement in [`super::aec3`]: on the OPPO it
/// is 400 µs against 970 and 40 dB better on a real room. The old filter stays
/// reachable because `sonora-aec3` was a fortnight old when this landed, and a
/// bad build should be comparable rather than arguable.
pub fn set_use_aec3(on: bool) {
    USE_AEC3.store(on, Ordering::Relaxed);
}

pub fn use_aec3() -> bool {
    USE_AEC3.load(Ordering::Relaxed)
}

/// Echo cancellation: whichever of the two is in force.
///
/// The pair sit behind one type deliberately. Everything upstream — the capture
/// processor, the worker, the performance ladder, the diagnostics panel — asks
/// the same questions of both, and a chain that had to know which canceller it
/// was talking to would grow that knowledge in a dozen places.
pub struct EchoCanceller {
    inner: Inner,
}

enum Inner {
    /// The time-domain filter. Kept, not deleted: it is the only one of the two
    /// that has ever shipped, and it is the fallback if the port has to be
    /// abandoned.
    Nlms(Box<Nlms>),
    Aec3(Box<super::aec3::Aec3>),
}

impl EchoCanceller {
    /// Whichever [`set_use_aec3`] last said.
    pub fn new(taps: usize) -> Self {
        if use_aec3() {
            Self::aec3()
        } else {
            Self::nlms(taps)
        }
    }

    pub fn aec3() -> Self {
        Self {
            inner: Inner::Aec3(Box::new(super::aec3::Aec3::new())),
        }
    }

    pub fn nlms(taps: usize) -> Self {
        Self {
            inner: Inner::Nlms(Box::new(Nlms::new(taps))),
        }
    }

    /// True when AEC3 is the one running, for the panel.
    pub fn is_aec3(&self) -> bool {
        matches!(self.inner, Inner::Aec3(_))
    }

    pub fn process(&mut self, mic: &mut [f32], reference: &[f32]) -> f32 {
        match &mut self.inner {
            Inner::Nlms(a) => a.process(mic, reference),
            Inner::Aec3(a) => a.process(mic, reference),
        }
    }

    pub fn set_enabled(&mut self, on: bool) {
        match &mut self.inner {
            Inner::Nlms(a) => a.set_enabled(on),
            Inner::Aec3(a) => a.set_enabled(on),
        }
    }

    pub fn is_enabled(&self) -> bool {
        match &self.inner {
            Inner::Nlms(a) => a.is_enabled(),
            Inner::Aec3(a) => a.is_enabled(),
        }
    }

    pub fn reset(&mut self) {
        match &mut self.inner {
            Inner::Nlms(a) => a.reset(),
            Inner::Aec3(a) => a.reset(),
        }
    }

    pub fn erle_db(&self) -> f32 {
        match &self.inner {
            Inner::Nlms(a) => a.erle_db(),
            Inner::Aec3(a) => a.erle_db(),
        }
    }

    pub fn alignment(&self) -> (f32, f32) {
        match &self.inner {
            Inner::Nlms(a) => a.alignment(),
            Inner::Aec3(a) => a.alignment(),
        }
    }

    pub fn filter_span_ms(&self) -> f32 {
        match &self.inner {
            Inner::Nlms(a) => a.filter_span_ms(),
            Inner::Aec3(a) => a.filter_span_ms(),
        }
    }

    pub fn measured_spread_ms(&self) -> f32 {
        match &self.inner {
            Inner::Nlms(a) => a.measured_spread_ms(),
            Inner::Aec3(a) => a.measured_spread_ms(),
        }
    }

    pub fn taps(&self) -> usize {
        match &self.inner {
            Inner::Nlms(a) => a.taps(),
            // Not a number AEC3 has. Reported as the full length so the
            // ladder's "is it already shortened" check answers no, which is
            // true: there is nothing here to shorten.
            Inner::Aec3(_) => DEFAULT_TAPS,
        }
    }

    /// **Only the old filter has taps to set**, so this is where the
    /// performance ladder's `AecCut` stops applying.
    ///
    /// That is not an oversight and it is not free. `relief::AecCut` exists to
    /// claw back up to 840 µs from a device that has given up everything else,
    /// and AEC3 offers no equivalent dial. What makes it acceptable is that the
    /// thing it was clawing back *from* is gone: the filter it was shortening
    /// cost 970 µs on the OPPO and AEC3 costs 400, so the rung is trying to
    /// recover less than the swap already saved.
    pub fn set_taps(&mut self, taps: usize) {
        if let Inner::Nlms(a) = &mut self.inner {
            a.set_taps(taps);
        }
    }
}

/// Adaptive echo canceller, time domain.
struct Nlms {
    taps: usize,
    /// Filter coefficients.
    w: Vec<f32>,
    /// The most recent `taps` reference samples, stored **twice**.
    ///
    /// A ring would be the obvious structure and it was the slow one. Walking
    /// it backwards needs `idx = if idx == 0 { taps - 1 } else { idx - 1 }`,
    /// a data-dependent branch on every tap, and neither the estimate nor the
    /// update vectorises through it: measured 1.1 GFLOP/s for a pure
    /// multiply-accumulate, which is scalar speed on a machine that should
    /// manage several times that.
    ///
    /// Writing each sample at `pos` *and* at `pos + taps` costs one extra
    /// store per sample and makes the window always contiguous:
    /// `hist[pos .. pos + taps]` is the last `taps` samples, oldest first, at
    /// every point in the cycle. Both loops become a plain walk over two
    /// slices, which is the shape LLVM turns into SSE or NEON without being
    /// asked.
    hist: Vec<f32>,
    /// Where the next reference sample goes, in `0..taps`.
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
    /// Samples since `ref_power` was recomputed from scratch and the output
    /// was checked for divergence.
    since_audit: usize,
    /// The last coefficients that were measurably cancelling, and how well.
    ///
    /// Somewhere to fall back to that is not zero — see [`Self::audit`].
    good_w: Vec<f32>,
    good_erle: f32,
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
    /// Contiguous copies for the search, kept so it allocates nothing.
    scratch_ref: Vec<f32>,
    scratch_mic: Vec<f32>,
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
            scratch_ref: Vec::with_capacity(ENV_POINTS),
            scratch_mic: Vec::with_capacity(ENV_POINTS),
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

        // Copied into scratch once, not once per lag.
        //
        // This ran on the audio thread and allocated fifty-one vectors every
        // time — one for the microphone envelope and one for every lag —
        // copying 350 floats into each. The arithmetic was never the problem
        // at once a second; **the allocation was**, because a `malloc` that
        // takes a slow path inside a 10 ms deadline is a dropped block, and
        // this chain has a rule against allocating here that this was quietly
        // breaking.
        //
        // The deques are ring buffers, so a contiguous view needs one copy
        // regardless. One is enough.
        self.scratch_ref.clear();
        self.scratch_ref.extend(self.env_ref.iter().copied());
        self.scratch_mic.clear();
        self.scratch_mic
            .extend(self.env_mic.iter().skip(MAX_LAG_POINTS));
        let mic = &self.scratch_mic[..];

        let mic_mean = mic.iter().sum::<f32>() / window as f32;
        let mic_var = super::dsp::sq_diff_const(mic, mic_mean);
        if mic_var < 1.0 {
            return false; // nothing came back to correlate against
        }

        let mut best = (0usize, 0.0f32);
        let mut scores = [0.0f32; MAX_LAG_POINTS];
        for (lag, score) in scores.iter_mut().enumerate() {
            let start = MAX_LAG_POINTS - lag;
            let r = &self.scratch_ref[start..start + window];
            let r_mean = r.iter().sum::<f32>() / window as f32;
            let r_var = super::dsp::sq_diff_const(r, r_mean);
            if r_var < 1.0 {
                continue; // the far end was silent through this stretch
            }
            // Σ(r-r̄)(m-m̄) expanded to Σrm − n·r̄·m̄, so the mean-removal does
            // not need its own pass and the remaining sum is the shared
            // four-lane dot product.
            let cov = super::dsp::dot(r, mic) - window as f32 * r_mean * mic_mean;
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
        // The earliest lag that is *as good as* the best, not the best itself.
        //
        // Sound cannot arrive before it was made, so when two lags score within
        // a whisker of each other the earlier one is the physical answer and
        // the later one is speech correlating with itself. The 10 ms search
        // grid makes this matter: a 25 ms delay falls between bins and scores
        // slightly below a spurious match at 300 ms, and taking the maximum
        // pointed the filter a quarter of a second past the echo.
        // The filter reaches this far forward from wherever it is aimed.
        let reach = DEFAULT_TAPS / ENV_HOP;
        let floor = (best.1 * PEAK_FRACTION).max(ACCEPT_CORR);
        let first = scores.iter().position(|c| *c >= floor).unwrap_or(best.0);
        let last = scores
            .iter()
            .rposition(|c| *c >= floor)
            .unwrap_or(best.0)
            .max(first);

        // **When the arrivals spread wider than the filter can cover, the
        // maximum is not an arrival at all.**
        //
        // Correlating loudness against loudness cannot resolve two copies of
        // the same sound: handed a microphone carrying an early internal copy
        // and a late acoustic one, the score peaks *between* them, at their
        // centroid, with a correlation of 1.00 — a delay at which nothing was
        // ever emitted. Measured: copies at 5 ms and 60 ms produced a confident
        // alignment of 20 ms and cancelled nothing, because the filter's window
        // contained neither.
        //
        // This is a limit of the method rather than a tuning problem, and it is
        // why AEC3 does not rely on the estimate alone: its filter is
        // partitioned across the whole plausible range, so a centroid estimate
        // is harmless because both arrivals are inside it anyway.
        //
        // Here the filter is short, so the answer is to aim at the earliest
        // real arrival instead of the imaginary middle. The near copy goes, the
        // far one survives, and the far one is the quieter of the two on every
        // route where this happens.
        let earliest = if last - first > reach {
            first
        } else {
            let near_best = (best.1 * NEAR_BEST).max(ACCEPT_CORR);
            scores
                .iter()
                .enumerate()
                .find(|(lag, c)| **c >= near_best && best.0.saturating_sub(*lag) < reach)
                .map(|(lag, _)| lag)
                .unwrap_or(best.0)
        };

        // One point short of that, deliberately. The search resolves to 10 ms
        // and the filter can only look backwards from where it is pointed:
        // aiming a block early puts the arrival inside its span instead of
        // just before the first tap, where it would be invisible however long
        // the filter was.
        let lag = earliest.saturating_sub(1) * ENV_HOP;

        // Reported, not acted on: how far apart the arrivals are, which is the
        // number that says whether the far one is being left behind.
        let spread = last.saturating_sub(first) * ENV_HOP;

        if lag == self.lag {
            self.span = spread;
            return false;
        }

        // Hysteresis. Moving the alignment throws away everything the filter
        // has learned, so a new answer has to be clearly better than the one in
        // use — not merely different, and not merely this second's winner.
        //
        // **Without this the two-arrival case cancels nothing at all.** An
        // internal copy and its acoustic twin score within a hair of each
        // other, so the winner alternates between them from one search to the
        // next, the filter is reset every second, and it never converges on
        // either. The same flapping is what left a spurious 400 ms alignment
        // behind a filter that had spent the run pointed correctly.
        //
        // Speex reaches the same conclusion from the other direction: its
        // background filter is promoted to foreground only when `(Sff - See)²`
        // says it is genuinely better, so a filter that is merely different
        // never displaces one that is working.
        let current = (self.lag / ENV_HOP + 1).min(MAX_LAG_POINTS - 1);
        if self.corr > 0.0 && best.1 < scores[current] * MOVE_MARGIN {
            self.span = spread;
            return false;
        }

        self.lag = lag;
        self.span = spread;
        true
    }
}

impl Nlms {
    fn new(taps: usize) -> Self {
        let taps = taps.max(16);
        Self {
            taps,
            w: vec![0.0; taps],
            hist: vec![0.0; taps * 2],
            pos: 0,
            ref_power: 0.0,
            mu: 0.25,
            smooth_mic: 0.0,
            smooth_out: 0.0,
            smooth_ref: 0.0,
            enabled: true,
            align: Aligner::new(),
            idle_run: 0,
            since_audit: 0,
            good_w: vec![0.0; taps],
            good_erle: 0.0,
        }
    }

    /// Recomputes what a running sum drifts away from, and catches a filter
    /// that has stopped subtracting and started adding.
    ///
    /// **`ref_power` is maintained incrementally**, one add and one subtract
    /// per sample, because recomputing a thousand squares per sample would
    /// cost more than the filter. Over a call that is millions of `f32`
    /// operations against a running total, and the error accumulates in one
    /// direction as easily as the other. It normalises the NLMS step, so a
    /// total that has drifted *low* makes every step too large — which is
    /// divergence, arriving quietly, after minutes of working correctly.
    ///
    /// Once per block: 1 024 multiply-adds against the ~1 000 000 the filter
    /// itself does in that time.
    fn audit(&mut self) {
        self.ref_power = self.window().iter().map(|v| v * v).sum();
        if self.smooth_ref <= 1e-9 {
            return; // nothing playing; there is nothing to judge
        }

        let erle = self.erle_db();
        if self.smooth_out > self.smooth_mic * DIVERGED {
            // Diverged. Go back to the last set of coefficients that was
            // measurably working rather than to zero.
            //
            // Zeroing was the first attempt and it is the wrong move: it throws
            // away a converged path because of a transient, and then spends a
            // second re-learning it while the echo is audible again. SpeexDSP
            // does this properly — it runs a background filter that adapts and
            // a foreground filter that produces the output, promotes background
            // to foreground only when `(Sff - See)²` says the background is
            // genuinely better, and *backtracks* foreground into background
            // when it is genuinely worse. A filter that has gone bad never
            // reaches the output at all.
            //
            // This keeps one filter and one snapshot instead of two live
            // filters, because two doubles the arithmetic on every sample and
            // this chain has 10 ms for everything. It gets the recovery without
            // the running cost, and gives up the part where the output is
            // protected during the drift *before* the divergence is detected.
            if self.good_erle > 3.0 {
                self.w.copy_from_slice(&self.good_w);
            } else {
                self.forget_path();
            }
            self.smooth_out = self.smooth_mic;
            return;
        }

        // Working better than the snapshot: take a new one. Cheap, because it
        // only happens while the filter is improving, which it cannot do for
        // long.
        if erle > self.good_erle + 1.0 && erle > 3.0 {
            self.good_w.resize(self.taps, 0.0);
            self.good_w.copy_from_slice(&self.w);
            self.good_erle = erle;
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

    /// How long the filter is, in taps.
    pub fn taps(&self) -> usize {
        self.taps
    }

    /// Sets the filter length.
    ///
    /// The ladder's `ShortAec` rung asks for half, and the tail below the
    /// ladder asks for less again — see [`super::relief::AecCut`]. Cheap to act
    /// on, because the coefficients are thrown away either way: a filter
    /// learned at one length does not describe the same path at another. It is
    /// nothing like as cheap to *ignore* — the cost is linear in the length, at
    /// ≈0.95 µs per tap per block on the OPPO.
    ///
    /// **The alignment survives.** It is a measurement of the playback path,
    /// which a shorter filter does not change; throwing it away would mean
    /// re-finding the echo from scratch on the device least able to afford the
    /// search. This is [`Self::forget_path`]'s distinction, and the reason it
    /// exists separately from [`Self::reset`].
    pub fn set_taps(&mut self, taps: usize) {
        let want = taps.max(16);
        if want == self.taps {
            return;
        }
        self.taps = want;
        self.w = vec![0.0; want];
        self.hist = vec![0.0; want * 2];
        self.good_w = vec![0.0; want];
        self.pos = 0;
        self.ref_power = 0.0;
        self.good_erle = 0.0;
        self.idle_run = 0;
    }

    /// How far apart the arrivals were measured to be, in milliseconds.
    ///
    /// Zero is the ordinary case: one path, and the filter is on it. A figure
    /// larger than [`Self::filter_span_ms`] is the panel's way of saying there
    /// is a second echo it is not reaching — which is a real condition with a
    /// real cause, usually a phone mixing its own playback into the capture
    /// alongside the sound coming back through the room.
    pub fn measured_spread_ms(&self) -> f32 {
        self.align.span as f32 * 1000.0 / 48_000.0
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
        self.hist.iter_mut().for_each(|v| *v = 0.0);
        self.pos = 0;
        self.ref_power = 0.0;
        self.smooth_mic = 0.0;
        self.smooth_out = 0.0;
        self.smooth_ref = 0.0;
        self.idle_run = 0;
        self.since_audit = 0;
        self.good_w.iter_mut().for_each(|v| *v = 0.0);
        self.good_erle = 0.0;
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
        // The sample about to be overwritten is the one leaving the window.
        let old = self.hist[self.pos];
        // Maintained incrementally; recomputing the sum every sample would
        // dominate the cost of the whole filter. `audit` corrects the drift
        // once a block.
        self.ref_power += x * x - old * old;
        if self.ref_power < 0.0 {
            self.ref_power = 0.0;
        }
        self.hist[self.pos] = x;
        self.hist[self.pos + self.taps] = x;
        self.pos += 1;
        if self.pos == self.taps {
            self.pos = 0;
        }
    }

    /// The last `taps` reference samples, oldest first. Always contiguous.
    #[inline]
    fn window(&self) -> &[f32] {
        &self.hist[self.pos..self.pos + self.taps]
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
                // A moved alignment invalidates the coefficients outright:
                // they describe a path measured from somewhere else. The
                // snapshot goes with them.
                self.forget_path();
                self.good_erle = 0.0;
            }
            let aligned = self.align.push(reference[i]);
            self.push_reference(aligned);

            self.since_audit += 1;
            if self.since_audit >= ENV_HOP {
                self.since_audit = 0;
                self.audit();
            }

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

            // Estimate the echo: w · history, over two contiguous slices.
            //
            // **Four accumulators rather than one, and that is the whole
            // trick.** `iter().map(..).sum()` is a single running total, and
            // floating-point addition is not associative, so LLVM is not
            // allowed to reorder it — the adds stay a serial dependency chain
            // one element at a time however wide the machine is, and the
            // multiplies cannot get ahead of them. Summing into four
            // independent lanes and combining at the end is a different
            // summation order, which is exactly why it is faster and why it
            // has to be written out rather than hoped for.
            //
            // The order change is immaterial here: this is a dot product of a
            // learned filter against a signal, and the filter adapts to
            // whatever the sum reports.
            let win = &self.hist[self.pos..self.pos + self.taps];
            let estimate = super::dsp::dot(&self.w, win);

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
                let win = &self.hist[self.pos..self.pos + self.taps];
                for (wk, x) in self.w.iter_mut().zip(win) {
                    *wk += step * x;
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

        let mut aec = Nlms::new(DEFAULT_TAPS);
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
        let mut blind = Nlms::new(DEFAULT_TAPS);
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

        let mut aec = Nlms::new(DEFAULT_TAPS);
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
    /// buffer, *and* the same sound coming back through the room 55 ms later.
    ///
    /// **The filter takes the nearer one and the far one survives**, and this
    /// test exists to pin that rather than to claim otherwise. Growing the
    /// filter to span both measured 2.9 dB — one arrival's worth — for four
    /// times the arithmetic, because a time-domain NLMS normalised by a single
    /// total power converges badly over a long span on coloured input. See
    /// [`_WHY_NO_GROWTH`].
    ///
    /// So the assertion is the honest one: the dominant arrival goes, which is
    /// most of the echo and all of what a short filter can be asked for.
    #[test]
    fn cancels_the_nearer_of_two_arrivals() {
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
        let far = syllabic(48_000 * 12, 3);
        let echo = convolve(&far, &h);

        let mut aec = Nlms::new(DEFAULT_TAPS);
        let mut before = Vec::new();
        let mut after = Vec::new();
        for (i, (m, r)) in echo.chunks(480).zip(far.chunks(480)).enumerate() {
            let mut block = m.to_vec();
            aec.process(&mut block, r);
            if i > 900 {
                before.extend_from_slice(m);
                after.extend_from_slice(&block);
            }
        }

        assert_eq!(
            aec.taps, DEFAULT_TAPS,
            "the filter must not grow; growing it was measured and was worse"
        );
        // The ceiling is 2.15 dB and no tuning reaches past it.
        //
        // The two arrivals carry 0.55^2 and 0.47 of the echo's power. Removing
        // the near one entirely leaves the far one, which is
        // 10*log10(0.774 / 0.472) = 2.15 dB of improvement — the whole of what
        // is available to a filter that can only be in one place. Measured
        // 1.6 dB, so it takes most of what there is.
        //
        // A regression shows up as ~0, which is what the two earlier designs
        // both produced: -0.6 dB aiming at the centroid, and 2.9 dB from a
        // 4x longer filter that cost four times the arithmetic for 1.3 dB.
        let erle = 20.0 * (rms(&before) / rms(&after)).log10();
        assert!(
            erle > 1.2,
            "the nearer arrival should still go, got {erle:.1} dB against a 2.15 dB ceiling"
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
            let mut aec = Nlms::new(DEFAULT_TAPS);
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

        let mut aec = Nlms::new(DEFAULT_TAPS);
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

        let mut aec = Nlms::new(DEFAULT_TAPS);

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
        let mut aec = Nlms::new(128);
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
        let mut aec = Nlms::new(256);
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
        let mut aec = Nlms::new(256);
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
