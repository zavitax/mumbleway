//! What the echo canceller costs per block, and what a shorter one would save.
//!
//! `chain_cost` cannot answer this: it hands the processor an **empty
//! reference**, which the canceller treats as a pass-through, so the AEC shows
//! up there as free. It is free in that case — that is the idle skip working —
//! but the question behind a performance rung is what it costs while an echo is
//! actually being cancelled, which is the one state that harness never enters.
//!
//! So this drives the canceller directly, with a real reference, at each filter
//! length worth considering.
//!
//! ```text
//! set MW_CLIP=C:\ml_data\rides\20260809-0142-000.raw
//! cargo test --release --test echo_cost -- --ignored --nocapture
//! ```
//!
//! **Read this as ratios, not as milliseconds for a phone.** Nothing here is
//! descheduled and the machine is a desktop; what transfers is the shape — how
//! the cost scales with taps, and how much of it the idle skip removes. The
//! absolute figure needs `adb` and the device this is for.
//!
//! Ignored because it is a measurement, not an assertion. A timing test that
//! fails on a busy machine teaches people to ignore failures.

use std::time::Instant;

use mumbleway_core::audio::aec::EchoCanceller;
use mumbleway_core::audio::testsig;

const BLOCK: usize = 480;
const RATE: usize = 48_000;

/// The block deadline: every stage of the chain shares this.
const BUDGET_US: f32 = 10_000.0;

fn clip() -> Vec<f32> {
    match std::env::var("MW_CLIP") {
        Ok(path) => {
            let bytes = std::fs::read(&path).expect("MW_CLIP unreadable");
            println!("  clip: {path}");
            bytes
                .chunks_exact(4)
                .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                .collect()
        }
        Err(_) => {
            println!("  clip: synthesised (set MW_CLIP to a .raw from a ride)");
            testsig::speech(RATE * 30, 120.0, 0.35)
        }
    }
}

/// Microseconds per block, mean and 99th percentile.
fn measure(taps: usize, reference: &[f32], mic: &[f32]) -> (f32, f32) {
    let mut aec = EchoCanceller::new(taps);
    let mut times: Vec<f32> = Vec::new();
    let blocks = reference.len() / BLOCK;

    for i in 0..blocks {
        let r = &reference[i * BLOCK..(i + 1) * BLOCK];
        let mut block = mic[i * BLOCK..(i + 1) * BLOCK].to_vec();
        let t = Instant::now();
        aec.process(&mut block, r);
        times.push(t.elapsed().as_nanos() as f32 / 1000.0);
    }

    // The first second is convergence and allocation; it is not the steady
    // state a rider spends the call in.
    times.drain(..100.min(times.len()));
    let mean = times.iter().sum::<f32>() / times.len() as f32;
    times.sort_by(|a, b| a.partial_cmp(b).unwrap());
    (mean, times[times.len() * 99 / 100])
}

#[test]
#[ignore]
fn what_the_canceller_costs() {
    let far = clip();
    let blocks = far.len() / BLOCK;
    println!("\n  {blocks} blocks, {:.1} s\n", blocks as f32 / 100.0);

    // The microphone hears a delayed, filtered copy of it — an echo worth
    // cancelling, so the filter adapts rather than sitting on zeros.
    let delay = 120 * RATE / 1000;
    let mut mic = vec![0.0f32; far.len()];
    for (offset, gain) in [(12usize, 0.60f32), (19, -0.28), (31, 0.15), (47, -0.07)] {
        let shift = delay + offset;
        for n in shift..far.len() {
            mic[n] += gain * far[n - shift];
        }
    }
    let silence = vec![0.0f32; far.len()];

    println!("  taps        active            idle        share of 10 ms");
    println!("  ---------------------------------------------------------");
    let mut rows = Vec::new();
    for taps in [512usize, 1024, 2048] {
        let (mean, p99) = measure(taps, &far, &mic);
        let (idle_mean, _) = measure(taps, &silence, &silence);
        println!(
            "  {taps:>4}   {mean:>6.0} µs p99 {p99:>5.0}   {idle_mean:>6.1} µs   {:>5.1}%",
            100.0 * mean / BUDGET_US
        );
        rows.push((taps, mean, p99, idle_mean));
    }

    let (_, at512, _, _) = rows[0];
    let (_, at1024, _, _) = rows[1];
    let (_, active, _, idle) = rows[1];
    println!();
    println!(
        "  halving 1024 -> 512 saves {:.0} µs a block, {:.2}% of the budget",
        at1024 - at512,
        100.0 * (at1024 - at512) / BUDGET_US
    );
    println!(
        "  the idle skip saves {:.0} µs a block whenever nobody is talking\n",
        active - idle
    );
}
