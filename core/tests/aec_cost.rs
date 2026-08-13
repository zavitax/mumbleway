//! What the echo canceller costs per block, on the device it has to fit on.
//!
//! `chain_cost` cannot answer this. It hands the chain an **empty reference**
//! (`chain_cost.rs:116`), so the canceller takes its idle shortcut and skips
//! both loops — a true measurement of the case where it is free, and no
//! measurement at all of the case that matters. The cost is also folded into
//! `suppression` there, alongside six other stages.
//!
//! So: the canceller alone, per 10 ms block, in the three states it is
//! actually in during a call.
//!
//! ```text
//! cargo test --release --target aarch64-linux-android --test aec_cost --no-run
//! adb push target/aarch64-linux-android/release/deps/aec_cost-<hash> /data/local/tmp/aec_cost
//! adb shell /data/local/tmp/aec_cost --ignored --nocapture
//! ```
//!
//! The budget is 10 000 µs a block and the enhancer takes about 6 900 of them
//! on the OPPO, so what is left for everything else is around 900. Read these
//! numbers against that, not against the 10 ms.

use std::time::Instant;

use mumbleway_core::audio::aec::{EchoCanceller, DEFAULT_TAPS};
use mumbleway_core::audio::testsig;

const BLOCK: usize = 480;
const RATE: usize = 48_000;

/// Microseconds per block, mean and worst, over `blocks` of real work.
fn measure(name: &str, reference: &[f32], mic: &[f32]) -> (f64, f64, f32) {
    let mut aec = EchoCanceller::new(DEFAULT_TAPS);
    let mut total = 0f64;
    let mut worst = 0f64;
    let n = reference.len() / BLOCK;

    for i in 0..n {
        let r = &reference[i * BLOCK..(i + 1) * BLOCK];
        let mut block = mic[i * BLOCK..(i + 1) * BLOCK].to_vec();
        let t = Instant::now();
        aec.process(&mut block, r);
        let us = t.elapsed().as_secs_f64() * 1e6;
        // The first second is convergence and allocation, not steady state.
        if i > 100 {
            total += us;
            worst = worst.max(us);
        }
    }
    let mean = total / (n - 100) as f64;
    let span = aec.filter_span_ms();
    println!("  {name:<34} {mean:>7.1} {worst:>9.1}   {span:>5.1} ms");
    (mean, worst, span)
}

#[test]
#[ignore]
fn what_the_canceller_costs_per_block() {
    let secs = 20;
    let len = RATE * secs;

    // A far end that is talking, with the syllabic contour the aligner needs.
    let far = testsig::speech(len, 120.0, 0.35);

    // One arrival, 120 ms out: a phone loudspeaker. The filter stays at its
    // default length.
    let mut single = vec![0.0f32; len];
    let d = 120 * RATE / 1000;
    for n in d..len {
        single[n] = 0.6 * far[n - d];
    }

    // Two arrivals 55 ms apart — an internally mixed copy and its acoustic
    // twin — which is what makes the filter grow.
    let mut dual = vec![0.0f32; len];
    let (a, b) = (5 * RATE / 1000, 60 * RATE / 1000);
    for n in b..len {
        dual[n] = 0.55 * far[n - a] + 0.6 * far[n - b];
    }

    println!("\n  echo canceller, per 10 ms block, {secs} s each");
    println!("  state                              mean µs   worst µs   filter");
    println!("  ----------------------------------------------------------------");

    let silent = vec![0.0f32; len];
    measure("idle (nothing playing)", &silent, &far);
    measure("far end talking, one arrival", &far, &single);
    measure("far end talking, two arrivals", &far, &dual);

    // The candidate rungs, priced. Cost is O(taps) per sample in two loops, so
    // this should be a straight line — and if it is, every rung on the ladder
    // can be read off it rather than guessed at.
    println!("\n  by filter length, far end talking");
    println!("  taps          covers      mean µs   worst µs");
    println!("  ----------------------------------------------");
    for taps in [1024usize, 768, 512, 384, 256, 128] {
        let mut aec = EchoCanceller::new(taps);
        let mut total = 0f64;
        let mut worst = 0f64;
        let n = len / BLOCK;
        for i in 0..n {
            let r = &far[i * BLOCK..(i + 1) * BLOCK];
            let mut block = single[i * BLOCK..(i + 1) * BLOCK].to_vec();
            let t = Instant::now();
            aec.process(&mut block, r);
            let us = t.elapsed().as_secs_f64() * 1e6;
            if i > 100 {
                total += us;
                worst = worst.max(us);
            }
        }
        println!(
            "  {taps:<5} {:>9.1} ms {:>11.1} {:>10.1}",
            taps as f32 * 1000.0 / RATE as f32,
            total / (n - 100) as f64,
            worst
        );
    }
}
