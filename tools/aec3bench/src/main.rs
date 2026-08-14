//! What AEC3 costs per block, on the phone that would have to run it.
//!
//! # Why
//!
//! Build 123 measured the current canceller on a real room and it removed a
//! median of **0.2 dB**. The room's echo was spread over 140 ms (440 at p90)
//! and the filter covers 21.3. `core/src/audio/aec.rs`'s `_WHY_NO_GROWTH`
//! records why that filter cannot simply be lengthened, so the question became
//! whether a frequency-domain canceller fits in the budget instead.
//!
//! On a desktop, `core/tests/aec3_cost.rs` says AEC3 holds 37–39 dB on every
//! room including the p90 one, where half the echo energy is beyond anything
//! 1024 taps could reach, and the current filter manages 0.0. **This answers
//! the other half: what it costs on an OPPO A3s.**
//!
//! Read against two numbers already measured on this same phone:
//!
//! - the current filter at 1024 taps: **970 µs a block** (`core/tests/aec_cost.rs`)
//! - what the enhancer leaves of a 10 ms block: **about 900 µs** (`chain_cost`)
//!
//! ```text
//! cargo build --release --target aarch64-linux-android
//! adb push target/aarch64-linux-android/release/aec3bench /data/local/tmp/
//! adb shell chmod 755 /data/local/tmp/aec3bench
//! adb shell /data/local/tmp/aec3bench
//! ```

use std::time::Instant;

use rustfft::{num_complex::Complex32, FftPlanner};
use sonora::config::EchoCanceller;
use sonora::{AudioProcessing, Config, StreamConfig};

const RATE: usize = 48_000;
const BLOCK: usize = 480;

struct Rng(u32);

impl Rng {
    fn next(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (self.0 >> 8) as f32 / 8_388_608.0 - 1.0
    }
}

/// Far-end speech: a voiced harmonic stack plus its unvoiced half.
///
/// **The second part is what makes this a valid test.** A bare harmonic stack
/// is exactly periodic and excites a few dozen discrete frequencies, which no
/// adaptive filter can identify a room from — and which flatters any filter
/// that happens to match at those points. `core/tests/aec3_cost.rs` has the
/// long version of this; it cost a wrong answer to find.
fn far_end(len: usize) -> Vec<f32> {
    let mut rng = Rng(0x5EED_1234);
    let mut low = 0.0f32;
    (0..len)
        .map(|i| {
            let t = i as f32 / RATE as f32;
            let mut v = 0.0f32;
            let mut h = 1;
            while (120.0 * h as f32) < 4_000.0 && h <= 40 {
                v += (2.0 * std::f32::consts::PI * 120.0 * h as f32 * t).sin()
                    / (h as f32).powf(1.2);
                h += 1;
            }
            // Shaped noise for the unvoiced half: a one-pole tilt on white,
            // which is roughly a fricative's spectrum.
            low = 0.86 * low + 0.14 * rng.next();
            let unvoiced = rng.next() - low;
            let syllable = 0.05 + 0.95 * (0.5 + 0.5 * (2.0 * std::f32::consts::PI * 3.5 * t).sin());
            (v * 0.30 * 0.4 + unvoiced * 0.24) * syllable
        })
        .collect()
}

/// A direct arrival, early reflections, and a diffuse tail decaying 60 dB over
/// `rt60_ms`. `delay_ms` is the playback path, not the room.
fn room(delay_ms: usize, rt60_ms: usize) -> Vec<f32> {
    let delay = delay_ms * RATE / 1000;
    let tail = rt60_ms * RATE / 1000;
    let mut h = vec![0.0f32; delay + tail];
    let mut rng = Rng(0x51ED_2701);
    h[delay] = 0.60;
    for (offset_ms, gain) in [(3.0f32, -0.31f32), (7.5, 0.22), (12.0, -0.17), (18.0, 0.12)] {
        let i = delay + (offset_ms * RATE as f32 / 1000.0) as usize;
        if i < h.len() {
            h[i] += gain;
        }
    }
    for n in 0..tail {
        let t = n as f32 / tail as f32;
        h[delay + n] += 0.25 * rng.next() * (-6.908 * t).exp();
    }
    h
}

/// Convolution through an FFT, because the direct form is 8 billion
/// multiply-adds here and none of it is what is being timed.
fn convolve(x: &[f32], h: &[f32]) -> Vec<f32> {
    let n = (x.len() + h.len()).next_power_of_two();
    let mut planner = FftPlanner::new();
    let fwd = planner.plan_fft_forward(n);
    let inv = planner.plan_fft_inverse(n);

    let mut a: Vec<Complex32> = (0..n)
        .map(|i| Complex32::new(*x.get(i).unwrap_or(&0.0), 0.0))
        .collect();
    let mut b: Vec<Complex32> = (0..n)
        .map(|i| Complex32::new(*h.get(i).unwrap_or(&0.0), 0.0))
        .collect();
    fwd.process(&mut a);
    fwd.process(&mut b);
    for (x, y) in a.iter_mut().zip(&b) {
        *x *= *y;
    }
    inv.process(&mut a);
    let scale = 1.0 / n as f32;
    a[..x.len()].iter().map(|c| c.re * scale).collect()
}

fn power(x: &[f32]) -> f64 {
    x.iter().map(|v| (*v as f64) * (*v as f64)).sum::<f64>() / x.len().max(1) as f64
}

fn main() {
    let secs: usize = std::env::args()
        .nth(1)
        .and_then(|v| v.parse().ok())
        .unwrap_or(20);
    let len = RATE * secs;
    let window = RATE * 4;
    let far = far_end(len);

    println!();
    // Not `CARGO_PKG_VERSION` — that is this harness's version, and printing it
    // beside the word "sonora" said 0.1.0 for a dependency that is 0.2.0.
    println!("  AEC3 via sonora, {secs} s per room");
    println!("  block = 10 ms; the current filter is 970 us at 1024 taps on this phone");
    println!();
    println!(
        "  {:<26} {:>9} {:>9} {:>9} {:>9}",
        "room", "erle dB", "mean us", "p95 us", "worst us"
    );
    println!("  {}", "-".repeat(68));

    for (name, delay_ms, rt60_ms) in [
        ("helmet, 20 ms tail", 5, 20),
        ("measured p10, 40 ms", 30, 40),
        ("measured median, 140 ms", 30, 140),
        ("measured p90, 440 ms", 30, 440),
    ] {
        let h = room(delay_ms, rt60_ms);
        let echoed = convolve(&far, &h);
        // A room floor: real microphones are never digitally silent, and AEC3's
        // stationarity estimators were not designed against one that is.
        let mut rng = Rng(0xF100 + rt60_ms as u32);
        let echo: Vec<f32> = echoed.iter().map(|e| e + rng.next() * 0.0012).collect();

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
        let mut kept: Vec<f32> = Vec::with_capacity(window);
        let mut times: Vec<f64> = Vec::with_capacity(len / BLOCK);
        let measured_from = len - window;

        for i in 0..len / BLOCK {
            let r = &far[i * BLOCK..(i + 1) * BLOCK];
            let m = &echo[i * BLOCK..(i + 1) * BLOCK];
            let t = Instant::now();
            apm.process_render_f32(&[r], &mut [&mut render_out]).unwrap();
            apm.process_capture_f32(&[m], &mut [&mut capture_out]).unwrap();
            let us = t.elapsed().as_secs_f64() * 1e6;
            // The first second is convergence and allocation, not steady state.
            if i > 100 {
                times.push(us);
            }
            if i * BLOCK >= measured_from {
                kept.extend_from_slice(&capture_out);
            }
        }

        let erle = 10.0 * (power(&echo[measured_from..]) / power(&kept).max(1e-18)).log10();
        let mean = times.iter().sum::<f64>() / times.len().max(1) as f64;
        times.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let p95 = times[times.len() * 95 / 100];
        let worst = *times.last().unwrap_or(&0.0);
        println!("  {name:<26} {erle:>9.1} {mean:>9.0} {p95:>9.0} {worst:>9.0}");
    }

    println!();
    println!("  Ignore the worst column: it is a phone doing other things.");
    println!("  render + capture together, which is the whole per-block cost.");
}
