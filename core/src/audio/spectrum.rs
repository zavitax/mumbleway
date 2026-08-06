//! What the capture chain is doing to a voice, as three spectra.
//!
//! The chain has a lot of stages and, until now, no way to watch any of them.
//! When a rider says "it cut me off", nothing showed *which* stage cut them —
//! the gate, the VAD, the feedback guard, or a profile that was never right for
//! the noise they were in. The counters in the diagnostics panel say what
//! happened afterwards; they cannot say what the signal looked like.
//!
//! Three taps, deliberately chosen so the gaps between them are the interesting
//! part:
//!
//!   * [`TAP_RAW`] — the microphone, after input gain and before any DSP.
//!   * [`TAP_PRE_GATE`] — what the noise gate is about to judge, so everything
//!     the suppressor did shows as the distance from the raw trace.
//!   * [`TAP_SENT`] — what actually goes to the encoder, so the distance from
//!     pre-gate is what the gate and the transmit envelope took away.
//!
//! All three are transformed from the *same* block. That is the whole value: a
//! rider watching the sent trace collapse while the other two keep moving is
//! watching the gate close on one particular breath, not on three different
//! ones a few tens of milliseconds apart.
//!
//! Nothing here runs unless somebody is looking. See [`SpectrumAnalyser::due`]
//! and the arming counter on `AudioShared`: the diagnostics panel asks for a
//! frame, that ask expires, and the transforms stop by themselves.

use super::dsp::fft;

/// The microphone, after input gain, before any processing.
pub const TAP_RAW: usize = 0;
/// What the noise gate is about to see.
pub const TAP_PRE_GATE: usize = 1;
/// What reaches the encoder.
pub const TAP_SENT: usize = 2;
/// How many taps there are.
pub const TAPS: usize = 3;

/// Transform size.
///
/// 1024 samples is 21 ms at 48 kHz and gives 46.9 Hz bins. 512 would let a
/// block be transformed with no history at all, but 93.75 Hz bins put the whole
/// bottom two octaves into one, and the bottom is where wind lives. 2048 buys
/// resolution nobody reads off a phone screen and doubles the cost.
const FFT_SIZE: usize = 1024;

/// Bands drawn on screen.
///
/// Third-octave, which is the spacing every hardware analyser has used for
/// fifty years and the reason those displays read as "a spectrum" rather than
/// "some bars".
pub const BANDS: usize = 24;

/// The range worth drawing. Below 50 Hz the rumble filter has already cut, and
/// above 16 kHz there is nothing a helmet speaker or Opus at these bitrates
/// will carry.
const BAND_LOW_HZ: f32 = 50.0;
const BAND_HIGH_HZ: f32 = 16_000.0;

/// Quietest level drawn. Anything below this is silence as far as the eye is
/// concerned, and letting the scale run to -inf would waste most of the height.
pub const FLOOR_DB: f32 = -100.0;

/// How often a frame is produced, in capture blocks.
///
/// Every third 10 ms block is 33 Hz — faster than an eye resolves, so the
/// display is limited by its own smoothing rather than by the sample rate, and
/// two thirds of the cost is simply not paid.
const BLOCKS_PER_FRAME: u64 = 3;

/// Rise and fall of the on-screen bands, per produced frame.
///
/// Asymmetric on purpose, and this is what separates a readable analyser from a
/// flickering hedge: peaks must arrive immediately or the display lies about
/// transients, and they must fall slowly or the eye cannot follow anything.
/// Applied in the dB domain so the fall reads as a constant speed rather than
/// as an exponential crawl toward the floor.
const ATTACK: f32 = 0.5;
const RELEASE: f32 = 0.12;

/// One frame of analysis: three spectra taken from the same block.
#[derive(Debug, Clone, Copy)]
pub struct SpectrumFrame {
    /// Band energies in dBFS, floored at [`FLOOR_DB`], indexed by tap.
    pub bands: [[f32; BANDS]; TAPS],
    /// How tonal the pre-gate signal is, 0 flat to 1 pure.
    ///
    /// Spectral flatness, which is nearly free once the bins exist. It is a
    /// *display* number and deliberately not the one the transmit gate uses:
    /// flatness cannot tell a voice from an idling V-twin, because both are
    /// strongly tonal. See the pitch-constrained measure in `denoise` for the
    /// one that decides whether to open the microphone.
    pub harmonicity: f32,
    /// Increments once per frame. A frame whose `seq` has stopped moving is a
    /// stopped worker, which on screen looks exactly like silence.
    pub seq: u64,
}

impl Default for SpectrumFrame {
    fn default() -> Self {
        Self {
            bands: [[FLOOR_DB; BANDS]; TAPS],
            harmonicity: 0.0,
            seq: 0,
        }
    }
}

/// Rolling analysis of the capture chain.
///
/// Allocated once by the audio worker and never again: every buffer it needs is
/// owned and reused, because it runs on a thread that must not wait for an
/// allocator.
pub struct SpectrumAnalyser {
    /// The most recent [`FFT_SIZE`] samples at each tap.
    rings: [[f32; FFT_SIZE]; TAPS],
    /// Hann window, precomputed.
    window: [f32; FFT_SIZE],
    /// First and last bin of each band, inclusive.
    edges: [(usize, usize); BANDS],
    /// Smoothed band levels, carried between frames.
    smoothed: [[f32; BANDS]; TAPS],
    /// Scratch for the transform. Two buffers, reused every time.
    re: Vec<f32>,
    im: Vec<f32>,
    seq: u64,
}

impl SpectrumAnalyser {
    pub fn new() -> Self {
        let mut window = [0.0f32; FFT_SIZE];
        for (i, w) in window.iter_mut().enumerate() {
            // Periodic rather than symmetric — the same form the spectral
            // de-hisser uses, and the right one when successive frames overlap.
            *w = 0.5
                - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / FFT_SIZE as f32).cos();
        }

        Self {
            rings: [[0.0; FFT_SIZE]; TAPS],
            window,
            edges: band_edges(),
            smoothed: [[FLOOR_DB; BANDS]; TAPS],
            re: vec![0.0; FFT_SIZE],
            im: vec![0.0; FFT_SIZE],
            seq: 0,
        }
    }

    /// Whether this block is one the analyser wants.
    ///
    /// Counted in blocks rather than measured against a clock, for the same
    /// reason the capture watchdog counts blocks: a clock can jump and a block
    /// count cannot.
    pub fn due(&self, block_index: u64) -> bool {
        block_index % BLOCKS_PER_FRAME == 0
    }

    /// Adds a block to one tap's history.
    ///
    /// Cheap and unconditional. Callers push on every block whether or not a
    /// frame is due, because a transform needs [`FFT_SIZE`] samples of history
    /// and skipping pushes would leave holes in it.
    pub fn push(&mut self, tap: usize, block: &[f32]) {
        debug_assert!(tap < TAPS);
        let n = block.len().min(FFT_SIZE);
        let ring = &mut self.rings[tap];
        ring.copy_within(n.., 0);
        ring[FFT_SIZE - n..].copy_from_slice(&block[block.len() - n..]);
    }

    /// Transforms all three taps and writes the result into `out`.
    ///
    /// Never allocates.
    pub fn analyse(&mut self, out: &mut SpectrumFrame) {
        self.seq = self.seq.wrapping_add(1);
        out.seq = self.seq;

        for tap in 0..TAPS {
            self.transform(tap);
            self.reduce(tap, out);
            if tap == TAP_PRE_GATE {
                out.harmonicity = self.flatness_of_last_transform();
            }
        }
    }

    /// Windows one tap's history into the scratch buffers and transforms it.
    fn transform(&mut self, tap: usize) {
        for i in 0..FFT_SIZE {
            self.re[i] = self.rings[tap][i] * self.window[i];
            self.im[i] = 0.0;
        }
        fft(&mut self.re, &mut self.im, false);
    }

    /// Collapses the transform into bands and smooths them.
    fn reduce(&mut self, tap: usize, out: &mut SpectrumFrame) {
        for band in 0..BANDS {
            let (first, last) = self.edges[band];

            // Mean of *power*, then to dB. Averaging dB would under-weight the
            // loudest bin and make a pure tone read quieter than it is.
            let mut power = 0.0f32;
            for bin in first..=last {
                power += self.re[bin] * self.re[bin] + self.im[bin] * self.im[bin];
            }
            power /= (last - first + 1) as f32;

            // The transform is unnormalised and windowed; scale so a full-scale
            // sine reads near 0 dBFS rather than at some arbitrary offset.
            let db = if power > 0.0 {
                10.0 * (power / (FFT_SIZE as f32 * 0.25).powi(2)).log10()
            } else {
                FLOOR_DB
            }
            .max(FLOOR_DB);

            let prev = self.smoothed[tap][band];
            let rate = if db > prev { ATTACK } else { RELEASE };
            let next = prev + (db - prev) * rate;
            self.smoothed[tap][band] = next;
            out.bands[tap][band] = next;
        }
    }

    /// Spectral flatness of whatever is currently in the scratch buffers.
    ///
    /// The ratio of geometric to arithmetic mean power: 1 for white noise, near
    /// 0 for a pure tone. Reported as `1 - flatness` so it rises with tonality,
    /// which is the direction a reader expects of something labelled
    /// "harmonicity".
    fn flatness_of_last_transform(&self) -> f32 {
        // Only up to Nyquist, and skipping DC, which carries no pitch and is
        // dominated by any residual offset.
        let bins = &self.re[1..FFT_SIZE / 2];
        let imag = &self.im[1..FFT_SIZE / 2];

        let mut log_sum = 0.0f64;
        let mut sum = 0.0f64;
        let mut counted = 0usize;
        for (r, i) in bins.iter().zip(imag.iter()) {
            let p = (r * r + i * i) as f64;
            // A floor, because ln(0) is -inf and one empty bin would otherwise
            // drag the geometric mean to zero on its own.
            let p = p.max(1e-20);
            log_sum += p.ln();
            sum += p;
            counted += 1;
        }
        if counted == 0 || sum <= 0.0 {
            return 0.0;
        }

        let geometric = (log_sum / counted as f64).exp();
        let arithmetic = sum / counted as f64;
        let flatness = (geometric / arithmetic).clamp(0.0, 1.0) as f32;
        1.0 - flatness
    }

    /// Centre frequency of each band, for labelling an axis.
    pub fn band_centres() -> [f32; BANDS] {
        let mut out = [0.0f32; BANDS];
        for (i, c) in out.iter_mut().enumerate() {
            *c = band_centre(i);
        }
        out
    }
}

impl Default for SpectrumAnalyser {
    fn default() -> Self {
        Self::new()
    }
}

/// Geometric centre of a band, log-spaced across the drawn range.
fn band_centre(band: usize) -> f32 {
    let t = (band as f32 + 0.5) / BANDS as f32;
    BAND_LOW_HZ * (BAND_HIGH_HZ / BAND_LOW_HZ).powf(t)
}

/// First and last bin of every band.
///
/// Computed once. The lowest bands are narrower than one bin — at 46.9 Hz
/// resolution the first third-octave is well under a bin wide — so they are
/// clamped to a single bin and several of them legitimately share it. That is
/// honest: down there the display is an energy indicator, not a spectrum, and
/// the rumble filter has already taken most of it out.
fn band_edges() -> [(usize, usize); BANDS] {
    const BIN_HZ: f32 = super::denoise::SAMPLE_RATE as f32 / FFT_SIZE as f32;
    let max_bin = FFT_SIZE / 2 - 1;

    let mut edges = [(0usize, 0usize); BANDS];
    for (band, edge) in edges.iter_mut().enumerate() {
        let lower = BAND_LOW_HZ
            * (BAND_HIGH_HZ / BAND_LOW_HZ).powf(band as f32 / BANDS as f32);
        let upper = BAND_LOW_HZ
            * (BAND_HIGH_HZ / BAND_LOW_HZ).powf((band + 1) as f32 / BANDS as f32);

        let first = ((lower / BIN_HZ).floor() as usize).clamp(1, max_bin);
        let last = ((upper / BIN_HZ).ceil() as usize).clamp(first, max_bin);
        *edge = (first, last);
    }
    edges
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = super::super::denoise::SAMPLE_RATE as f32;

    fn tone(hz: f32, len: usize, amp: f32) -> Vec<f32> {
        (0..len)
            .map(|i| amp * (2.0 * std::f32::consts::PI * hz * i as f32 / SR).sin())
            .collect()
    }

    /// Zero-mean white noise.
    ///
    /// The halving matters: dividing by 2^31 rather than 2^30 gives -1..0, a
    /// signal with a large negative offset rather than noise, and its DC smears
    /// into the low bands through the window.
    fn noise(len: usize, amp: f32, seed: u64) -> Vec<f32> {
        let mut state = seed;
        (0..len)
            .map(|_| {
                state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
                ((state >> 33) as f32 / (1u64 << 30) as f32 - 1.0) * amp
            })
            .collect()
    }

    /// Feeds a signal in block-sized pieces until the smoothing has settled.
    fn settle(analyser: &mut SpectrumAnalyser, signal: &[f32], tap: usize) -> SpectrumFrame {
        let mut frame = SpectrumFrame::default();
        for chunk in signal.chunks(480) {
            analyser.push(tap, chunk);
            analyser.analyse(&mut frame);
        }
        frame
    }

    fn loudest_band(bands: &[f32; BANDS]) -> usize {
        let mut best = 0;
        for i in 1..BANDS {
            if bands[i] > bands[best] {
                best = i;
            }
        }
        best
    }

    #[test]
    fn a_tone_lands_in_the_band_it_belongs_to() {
        // The whole display is worthless if the frequency axis is wrong, and a
        // wrong axis looks perfectly plausible on screen.
        for hz in [200.0f32, 1000.0, 4000.0] {
            let mut a = SpectrumAnalyser::new();
            let frame = settle(&mut a, &tone(hz, 48_000, 0.5), TAP_RAW);
            let peak = loudest_band(&frame.bands[TAP_RAW]);
            let centre = band_centre(peak);
            let ratio = (centre / hz).max(hz / centre);
            assert!(
                ratio < 1.3,
                "{hz} Hz peaked in the band centred at {centre} Hz"
            );
        }
    }

    #[test]
    fn white_noise_is_flat_across_the_display() {
        let mut a = SpectrumAnalyser::new();
        let frame = settle(&mut a, &noise(48_000, 0.3, 7), TAP_RAW);

        // Skip the lowest bands: several of them share one bin, so they are not
        // independent measurements and comparing them means nothing.
        let bands = &frame.bands[TAP_RAW][6..];
        let hi = bands.iter().cloned().fold(f32::MIN, f32::max);
        let lo = bands.iter().cloned().fold(f32::MAX, f32::min);
        assert!(hi - lo < 12.0, "noise spanned {:.1} dB across bands", hi - lo);
    }

    #[test]
    fn silence_reads_as_the_floor_and_not_as_noise() {
        let mut a = SpectrumAnalyser::new();
        let frame = settle(&mut a, &vec![0.0f32; 48_000], TAP_RAW);
        for (i, db) in frame.bands[TAP_RAW].iter().enumerate() {
            assert!(*db <= FLOOR_DB + 0.01, "band {i} sat at {db} dB in silence");
        }
    }

    #[test]
    fn a_tone_is_tonal_and_noise_is_not() {
        // This is the display number only. It cannot tell a voice from an
        // engine — both are tonal — which is exactly why the transmit gate uses
        // a different measure.
        let mut tonal = SpectrumAnalyser::new();
        let t = settle(&mut tonal, &tone(440.0, 48_000, 0.5), TAP_PRE_GATE);

        let mut flat = SpectrumAnalyser::new();
        let n = settle(&mut flat, &noise(48_000, 0.3, 11), TAP_PRE_GATE);

        // White noise does not score 0, and cannot. Bin powers in a single
        // periodogram are exponentially distributed, and the geometric mean of
        // that distribution is exp(-gamma) — about 0.56 — of its arithmetic
        // mean. So perfectly flat noise measures a flatness near 0.56, and this
        // number near 0.44, however long it is averaged. Any threshold drawn on
        // this value on screen has to be set above that, not near zero.
        assert!(t.harmonicity > 0.8, "a pure tone scored {}", t.harmonicity);
        assert!(n.harmonicity < 0.6, "white noise scored {}", n.harmonicity);
        assert!(
            t.harmonicity > n.harmonicity + 0.3,
            "tone {} barely beat noise {}",
            t.harmonicity,
            n.harmonicity
        );
    }

    #[test]
    fn the_taps_are_independent() {
        // Three traces that quietly shared state would show the gate closing on
        // all of them at once, which is the one thing this display exists to
        // disprove.
        let mut a = SpectrumAnalyser::new();
        let loud = tone(1000.0, 480, 0.5);
        let quiet = vec![0.0f32; 480];
        let mut frame = SpectrumFrame::default();
        for _ in 0..200 {
            a.push(TAP_RAW, &loud);
            a.push(TAP_PRE_GATE, &loud);
            a.push(TAP_SENT, &quiet);
            a.analyse(&mut frame);
        }

        let raw = frame.bands[TAP_RAW][loudest_band(&frame.bands[TAP_RAW])];
        let sent = frame.bands[TAP_SENT][loudest_band(&frame.bands[TAP_SENT])];
        assert!(
            raw > sent + 40.0,
            "raw {raw:.1} dB against sent {sent:.1} dB — the taps are shared"
        );
    }

    #[test]
    fn peaks_arrive_faster_than_they_leave() {
        // Fast attack and slow release is what makes an analyser readable. If
        // this inverts, transients vanish before they can be seen.
        let mut a = SpectrumAnalyser::new();
        let loud = tone(1000.0, 480, 0.5);
        let mut frame = SpectrumFrame::default();

        for _ in 0..200 {
            a.push(TAP_RAW, &loud);
            a.analyse(&mut frame);
        }
        let peak = loudest_band(&frame.bands[TAP_RAW]);
        let steady = frame.bands[TAP_RAW][peak];

        // One frame of silence should barely dent it.
        a.push(TAP_RAW, &vec![0.0f32; 480]);
        a.analyse(&mut frame);
        let after_one = frame.bands[TAP_RAW][peak];
        assert!(
            steady - after_one < 25.0,
            "one silent block dropped the band {:.1} dB",
            steady - after_one
        );
    }

    #[test]
    fn frames_are_numbered_so_a_stopped_worker_is_visible() {
        let mut a = SpectrumAnalyser::new();
        let mut frame = SpectrumFrame::default();
        a.analyse(&mut frame);
        let first = frame.seq;
        a.analyse(&mut frame);
        assert_eq!(frame.seq, first + 1);
    }

    #[test]
    fn frames_are_produced_on_every_third_block() {
        let a = SpectrumAnalyser::new();
        let due: Vec<u64> = (0..9).filter(|b| a.due(*b)).collect();
        assert_eq!(due, vec![0, 3, 6]);
    }

    #[test]
    fn band_edges_rise_and_stay_inside_the_transform() {
        let edges = band_edges();
        for (i, (first, last)) in edges.iter().enumerate() {
            assert!(first <= last, "band {i} runs backwards");
            assert!(*last < FFT_SIZE / 2, "band {i} reaches past Nyquist");
            if i > 0 {
                assert!(
                    *first >= edges[i - 1].0,
                    "band {i} starts below the one before it"
                );
            }
        }
        let centres = SpectrumAnalyser::band_centres();
        assert!(centres[0] > BAND_LOW_HZ && *centres.last().unwrap() < BAND_HIGH_HZ);
    }
}
