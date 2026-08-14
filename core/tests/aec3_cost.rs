//! Two cancellers, one room, on the device that has to run them.
//!
//! # Why this exists
//!
//! Build 123 recorded an iPhone alone in a room, hearing nothing but its own
//! loudspeaker, and the decision log said what no earlier recording could:
//!
//! ```text
//! erle_db          p10 -6.00   median -0.20   p90  6.30
//! aec_confidence   p10  0.91   median  0.96   p90  0.98
//! aec_spread_ms    p10 40      median 140     p90 440
//! ```
//!
//! A confident alignment, a correct reference, and nothing cancelled. The
//! filter covers 21.3 ms and **every one of the 1 776 loud blocks had the echo
//! spread wider than that**. [`mumbleway_core::audio::aec`]'s `_WHY_NO_GROWTH`
//! records why it cannot simply be lengthened: a time-domain NLMS normalised by
//! one total power converges worse the further it spans.
//!
//! So this measures the alternative on the phone that has to afford it. Two
//! questions, and the second only matters if the first is answered:
//!
//! 1. **Does it cancel a real room** — 140 to 440 ms of tail, not the 1.3 ms of
//!    `echo_path()`.
//! 2. **What does it cost per block**, against the ~900 µs the enhancer leaves.
//!
//! # The room is synthetic, and here is what that is worth
//!
//! `CLAUDE.md` is blunt that a generator written after a hypothesis by the same
//! hand can only show a fault does not reproduce offline. This one is different
//! in one specific way and no other: **its length was measured on the device
//! first**. The 140 ms and 440 ms below are `aec_spread_ms` from
//! `20260814-0239-000`, not numbers chosen to make a point. What remains
//! synthetic is the *shape* — an exponentially decaying diffuse tail, which is
//! what a room does, but not what that particular room did.
//!
//! It cannot tell us the new canceller will fix the reported fault. It can tell
//! us whether it survives a tail the current one demonstrably does not, which
//! is the question worth a dependency.
//!
//! ```text
//! cargo test --release --target aarch64-linux-android --test aec3_cost --no-run
//! adb push target/aarch64-linux-android/release/deps/aec3_cost-<hash> /data/local/tmp/aec3_cost
//! adb shell chmod 755 /data/local/tmp/aec3_cost
//! adb shell "/data/local/tmp/aec3_cost --ignored --nocapture"
//! ```

use std::time::Instant;

use mumbleway_core::audio::aec::{EchoCanceller, DEFAULT_TAPS};
use mumbleway_core::audio::testsig;

const RATE: usize = 48_000;
const BLOCK: usize = 480;

/// Deterministic pseudo-noise in -1..1. Seeded, because a harness that draws a
/// different room each run cannot be compared with itself.
struct Rng(u32);

impl Rng {
    fn next(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (self.0 >> 8) as f32 / 8_388_608.0 - 1.0
    }
}

/// A room impulse response: a direct arrival, then a diffuse tail that decays
/// 60 dB over `rt60_ms`.
///
/// `delay_ms` is not the room. It is everything between the reference being
/// taken and the sound leaving the speaker — the device buffer, the OS, the
/// driver — and build 123 measured it at 30 ms on that iPhone.
///
/// The tail is noise under an exponential envelope, which is the standard
/// model for the diffuse part of a small room and is what makes this test
/// different from `echo_path()`: that one is four discrete reflections inside
/// 1.3 ms, and a 1024-tap filter covers it completely. This one does not fit.
fn room(delay_ms: usize, rt60_ms: usize) -> Vec<f32> {
    let delay = delay_ms * RATE / 1000;
    let tail = rt60_ms * RATE / 1000;
    let mut h = vec![0.0f32; delay + tail];
    let mut rng = Rng(0x51ED_2701);

    // The direct path, and the loudest thing in the response.
    h[delay] = 0.60;

    // Early reflections: a handful of discrete bounces in the first 20 ms,
    // which is the part the current filter can actually reach.
    for (offset_ms, gain) in [(3.0, -0.31), (7.5, 0.22), (12.0, -0.17), (18.0, 0.12)] {
        let i = delay + (offset_ms * RATE as f32 / 1000.0) as usize;
        if i < h.len() {
            h[i] += gain;
        }
    }

    // And the diffuse tail. -60 dB over rt60 is exp(-6.908 * t / rt60).
    for n in 0..tail {
        let t = n as f32 / tail as f32;
        h[delay + n] += 0.25 * rng.next() * (-6.908 * t).exp();
    }
    h
}

fn convolve(x: &[f32], h: &[f32]) -> Vec<f32> {
    let mut y = vec![0.0f32; x.len()];
    for (n, out) in y.iter_mut().enumerate() {
        let mut acc = 0.0f32;
        let first = n.saturating_sub(h.len() - 1);
        for k in first..=n {
            acc += h[n - k] * x[k];
        }
        *out = acc;
    }
    y
}

fn power(x: &[f32]) -> f64 {
    x.iter().map(|v| (*v as f64) * (*v as f64)).sum::<f64>() / x.len().max(1) as f64
}

fn db(before: f64, after: f64) -> f32 {
    (10.0 * (before / after.max(1e-18)).log10()) as f32
}

/// What fraction of the response's energy falls outside the filter's reach.
fn beyond(h: &[f32], taps: usize) -> f32 {
    let total: f64 = h.iter().map(|v| (*v as f64) * (*v as f64)).sum();
    // Measured from the direct arrival, which is where the aligner points.
    let peak = h
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.abs().partial_cmp(&b.1.abs()).unwrap())
        .map(|(i, _)| i)
        .unwrap_or(0);
    let inside: f64 = h[peak..(peak + taps).min(h.len())]
        .iter()
        .map(|v| (*v as f64) * (*v as f64))
        .sum();
    (100.0 * (1.0 - inside / total.max(1e-18))) as f32
}

#[test]
#[ignore]
fn what_a_real_room_costs_each_canceller() {
    // Shorter on a phone, and only because of the scaffolding. Convolving 25 s
    // against a 21 000-tap room is 25 billion scalar multiply-adds — minutes on
    // an OPPO, and none of it is what is being measured. Both cancellers
    // converge inside three seconds, so ten is ample; the desktop run uses 25
    // for a tighter quality figure and the two agree to within a decibel.
    let secs: usize = std::env::var("MW_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(25);
    let len = RATE * secs;
    let window = RATE * 3.min(secs / 3).max(1);

    // **Voiced speech plus its unvoiced half, and the second part is not
    // decoration.**
    //
    // `testsig::speech` alone is a 120 Hz harmonic stack: exactly periodic,
    // repeating every 400 samples, with energy at 33 discrete frequencies and
    // none between them. An adaptive filter cannot identify a room from that —
    // there is nothing to identify it *at*, everywhere else — and every filter
    // that matches those 33 points scores perfectly. Run this harness on it and
    // the current canceller reports 50 dB on a helmet room while AEC3 reports
    // 1.7, and both numbers are artefacts: ours had overfitted a rank-deficient
    // input, and AEC3's delay estimator could not resolve a signal that is
    // ambiguous modulo its own pitch period.
    //
    // That is `CLAUDE.md`'s rule about synthetic signals, arriving from an
    // unexpected direction: the generator did not flatter the hypothesis, it
    // flattered the *incumbent*, because the incumbent was tuned alongside it.
    //
    // Real speech is quasi-periodic with fricatives and formant transitions all
    // over the band. Mixing the shaped-noise generator in restores the
    // broadband excitation that identifying a room requires.
    let voiced = testsig::speech(len, 120.0, 0.30);
    let unvoiced = testsig::whisper(len, 0.30, 0x5EED);
    let far: Vec<f32> = voiced
        .iter()
        .zip(&unvoiced)
        .map(|(v, u)| v + u * 0.8)
        .collect();

    println!("\n  a room the current filter cannot reach");
    println!("  far end: {secs} s of speech, 120 Hz");
    println!(
        "  filter:  {} taps = {:.1} ms\n",
        DEFAULT_TAPS,
        DEFAULT_TAPS as f32 * 1000.0 / RATE as f32
    );
    println!(
        "  {:<26} {:>8} {:>10} {:>10} {:>10}",
        "room", "outside", "ours dB", "aec3 dB", "aec3 µs"
    );
    println!("  {}", "-".repeat(70));

    // p10, median and p90 of what build 123 measured, plus the helmet the
    // current filter was designed for as a control.
    for (name, delay_ms, rt60_ms) in [
        ("helmet, 20 ms tail", 5, 20),
        ("measured p10, 40 ms", 30, 40),
        ("measured median, 140 ms", 30, 140),
        ("measured p90, 440 ms", 30, 440),
    ] {
        let h = room(delay_ms, rt60_ms);
        let echoed = convolve(&far, &h);
        let outside = beyond(&h, DEFAULT_TAPS);

        // A room floor under the echo. Real rooms are never digitally silent,
        // and a microphone that reads exact zero between phrases is a signal
        // no detector in either canceller was designed against — AEC3 in
        // particular runs stationarity and saturation estimators that a
        // noiseless input drives into corners.
        let floor = testsig::white(len, 0.0012, 0xF100 + rt60_ms as u64);
        let echo: Vec<f32> = echoed.iter().zip(&floor).map(|(e, n)| e + n).collect();

        // The tail of the run only: both need time to find the path.
        let measured_from = len - window;
        let mut ours = EchoCanceller::new(DEFAULT_TAPS);
        let mut ours_out = Vec::with_capacity(window);
        for i in 0..len / BLOCK {
            let r = &far[i * BLOCK..(i + 1) * BLOCK];
            let mut block = echo[i * BLOCK..(i + 1) * BLOCK].to_vec();
            ours.process(&mut block, r);
            if i * BLOCK >= measured_from {
                ours_out.extend_from_slice(&block);
            }
        }

        let (aec3_db, aec3_us) = run_aec3(&far, &echo, measured_from);
        println!(
            "  {name:<26} {outside:>7.1}% {:>10.1} {:>10.1} {:>10.0}",
            db(power(&echo[measured_from..]), power(&ours_out)),
            aec3_db,
            aec3_us,
        );
    }

    println!(
        "\n  outside = share of the room's energy beyond {} taps from the direct\n  \
         arrival, which is what a longer filter would have to reach.",
        DEFAULT_TAPS
    );
    println!("  ours µs is in aec_cost.rs: 970 at 1024 taps on the OPPO.");
}

/// Sonora's AEC3, fed the same signals in the same order.
///
/// **Render and capture in lockstep, one 10 ms frame each.** That is the
/// contract, and it is the thing the engine cannot honour today: the reference
/// queue is fed only when there is something to play, so on build 123 44% of
/// blocks pushed nothing at all. Here both streams are continuous by
/// construction, which is exactly the assumption this harness cannot test.
fn run_aec3(far: &[f32], echo: &[f32], measured_from: usize) -> (f32, f64) {
    use sonora::config::EchoCanceller;
    use sonora::{AudioProcessing, Config, StreamConfig};

    // The canceller alone. Noise suppression and AGC stay off because this
    // chain has its own and the question here is only about the echo — turning
    // them on would flatter the ERLE with work the rest of the chain already
    // does.
    let cfg = Config {
        echo_canceller: Some(EchoCanceller::default()),
        ..Default::default()
    };
    let stream = StreamConfig::new(RATE as u32, 1);
    let mut apm = AudioProcessing::builder()
        .config(cfg)
        .capture_config(stream)
        .render_config(stream)
        .build();

    let mut render_out = vec![0.0f32; BLOCK];
    let mut capture_out = vec![0.0f32; BLOCK];
    let mut kept = Vec::with_capacity(BLOCK * 512);
    let mut total_us = 0f64;
    let mut counted = 0usize;

    for i in 0..far.len() / BLOCK {
        let r = &far[i * BLOCK..(i + 1) * BLOCK];
        let m = &echo[i * BLOCK..(i + 1) * BLOCK];
        let t = Instant::now();
        apm.process_render_f32(&[r], &mut [&mut render_out]).unwrap();
        apm.process_capture_f32(&[m], &mut [&mut capture_out])
            .unwrap();
        let us = t.elapsed().as_secs_f64() * 1e6;
        // The first second is convergence and allocation, not steady state.
        if i > 100 {
            total_us += us;
            counted += 1;
        }
        if i * BLOCK >= measured_from {
            kept.extend_from_slice(&capture_out);
        }
    }

    // What it thinks it did, against what it did. If these disagree the
    // harness is wrong, not the canceller — which is the more likely of the
    // two and is worth being able to tell apart.
    let s = apm.statistics();
    println!(
        "      [aec3 says: erle {:?} dB, erl {:?}, delay {:?} ms, divergent {:?}]",
        s.echo_return_loss_enhancement.map(|v| (v * 10.0).round() / 10.0),
        s.echo_return_loss.map(|v| (v * 10.0).round() / 10.0),
        s.delay_ms,
        s.divergent_filter_fraction,
    );

    (
        db(power(&echo[measured_from..]), power(&kept)),
        total_us / counted.max(1) as f64,
    )
}
