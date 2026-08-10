//! What diagnostic recording costs per block, and where.
//!
//! A rider on a Snapdragon 450 reported the phone struggling badly *while
//! recording*, even at the bottom of the performance ladder — and asked
//! whether the CSV writing is the problem and whether buffering would help.
//!
//! Worth measuring rather than reasoning about, because the guesses point in
//! different directions: both writers are already `BufWriter`s, so "add
//! buffering" is already done; but the log line formats eight floats a block,
//! and Rust's float formatting is not cheap. A hundred blocks a second makes
//! small numbers matter.
//!
//! ```text
//! cargo test --release --test record_cost -- --ignored --nocapture
//! ```
//!
//! Split so each answer stands alone:
//!
//! * **convert** — 480 `f32` to 960 bytes of `i16`, allocating a fresh `Vec`
//!   per block exactly as `write_loop` does today.
//! * **convert, reusing** — the same, into a buffer kept between blocks. The
//!   difference is the allocation.
//! * **format** — the decision line, through `writeln!` into an in-memory
//!   sink, which is what the CSV costs with the file itself taken out.
//! * **format, by hand** — the same line without the float formatter, to
//!   price what replacing it could buy before anyone tries.
//!
//! Ignored because it is a measurement, not an assertion.

use std::io::Write;
use std::time::Instant;

const FRAME: usize = 480;
const BLOCKS: usize = 20_000; // 200 seconds of audio

fn main_line(out: &mut Vec<u8>, i: u64) {
    let _ = writeln!(
        out,
        "{},{},{},{},{:.3},{:.1},{:.1},{:.1},{:.3},{:.3},{},{},{:.1}",
        i,
        1u8,
        1u8,
        1u8,
        0.873_f32,
        14.2_f32,
        -41.1_f32,
        -66.0_f32,
        0.512_f32,
        0.409_f32,
        0u8,
        0u8,
        6.0_f32
    );
}

/// The same row without the float formatter.
///
/// Fixed point, written through the integer path: `14.2` becomes `142` and a
/// decimal point is placed by hand. Not proposed — priced, so that "replace
/// the float formatting" can be compared against what it would save.
fn hand_line(out: &mut Vec<u8>, i: u64) {
    fn tenths(out: &mut Vec<u8>, v: f32) {
        let scaled = (v * 10.0).round() as i64;
        let (sign, n) = if scaled < 0 {
            ("-", -scaled)
        } else {
            ("", scaled)
        };
        let _ = write!(out, "{sign}{}.{}", n / 10, n % 10);
    }
    fn thousandths(out: &mut Vec<u8>, v: f32) {
        let scaled = (v * 1000.0).round() as i64;
        let (sign, n) = if scaled < 0 {
            ("-", -scaled)
        } else {
            ("", scaled)
        };
        let _ = write!(out, "{sign}{}.{:03}", n / 1000, n % 1000);
    }
    let _ = write!(out, "{i},1,1,1,");
    thousandths(out, 0.873);
    out.push(b',');
    tenths(out, 14.2);
    out.push(b',');
    tenths(out, -41.1);
    out.push(b',');
    tenths(out, -66.0);
    out.push(b',');
    thousandths(out, 0.512);
    out.push(b',');
    thousandths(out, 0.409);
    let _ = write!(out, ",0,0,");
    tenths(out, 6.0);
    out.push(b'\n');
}

#[test]
#[ignore]
fn record_cost() {
    let block: Vec<f32> = (0..FRAME).map(|i| 0.3 * (i as f32 * 0.07).sin()).collect();

    let report = |name: &str, us: f64| {
        let per_block_us = us / BLOCKS as f64;
        println!(
            "{name:<24} {per_block_us:7.2} us/block   {:5.2}% of a 10 ms block   {:6.2}% of one core",
            100.0 * per_block_us / 10_000.0,
            100.0 * per_block_us / 10_000.0,
        );
    };

    // 1. Convert, allocating per block, exactly as write_loop does.
    let t = Instant::now();
    let mut sunk = 0usize;
    for _ in 0..BLOCKS {
        let mut bytes = Vec::with_capacity(block.len() * 2);
        for s in &block {
            let v = (s.clamp(-1.0, 1.0) * 32767.0) as i16;
            bytes.extend_from_slice(&v.to_le_bytes());
        }
        sunk += bytes.len();
    }
    std::hint::black_box(sunk);
    report("convert (alloc/block)", t.elapsed().as_micros() as f64);

    // 2. The same, into a buffer kept between blocks.
    let t = Instant::now();
    let mut bytes = Vec::with_capacity(block.len() * 2);
    for _ in 0..BLOCKS {
        bytes.clear();
        for s in &block {
            let v = (s.clamp(-1.0, 1.0) * 32767.0) as i16;
            bytes.extend_from_slice(&v.to_le_bytes());
        }
        std::hint::black_box(bytes.len());
    }
    report("convert (reused buf)", t.elapsed().as_micros() as f64);

    // 3. The decision line as it is written today, into memory.
    let mut out: Vec<u8> = Vec::with_capacity(BLOCKS * 80);
    let t = Instant::now();
    for i in 0..BLOCKS as u64 {
        main_line(&mut out, i);
    }
    let formatted = t.elapsed().as_micros() as f64;
    let bytes_written = out.len();
    report("csv line (writeln!)", formatted);

    // 4. The same line without the float formatter.
    let mut out2: Vec<u8> = Vec::with_capacity(BLOCKS * 80);
    let t = Instant::now();
    for i in 0..BLOCKS as u64 {
        hand_line(&mut out2, i);
    }
    report("csv line (fixed point)", t.elapsed().as_micros() as f64);

    println!(
        "\ncsv row is {} bytes; at 100 blocks a second that is {:.1} kB/s, so the\n\
         8 kB default BufWriter flushes about every {:.1} s. The audio is\n\
         {} bytes a block -- {:.0} kB/s -- and flushes every {:.2} s.",
        bytes_written / BLOCKS,
        bytes_written as f64 / BLOCKS as f64 * 100.0 / 1024.0,
        8192.0 / (bytes_written as f64 / BLOCKS as f64 * 100.0),
        FRAME * 2,
        (FRAME * 2) as f64 * 100.0 / 1024.0,
        8192.0 / ((FRAME * 2) as f64 * 100.0),
    );

    // ---- and now the part the numbers above deliberately do not include ----
    //
    // **Everything so far wrote to memory.** That prices the conversion and
    // the formatting and says nothing at all about the file, which is the
    // thing a rider on a slow phone actually suspects. A `write` that reaches
    // flash can block for milliseconds, and the earlier "0.1% of a core" did
    // not cover it.
    //
    // So: the same traffic, to a real file, at the buffer size that ships and
    // at larger ones. `BLOCKS` of audio is 200 seconds of recording.
    println!("\nreal files, {} blocks (200 s of recording):", BLOCKS);
    println!(
        "{:<14} {:>10} {:>12} {:>12}",
        "pcm buffer", "total ms", "us/block", "worst ms"
    );

    let dir = std::env::temp_dir().join(format!("mw-io-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let pcm: Vec<u8> = {
        let mut v = Vec::with_capacity(FRAME * 2);
        for s in &block {
            v.extend_from_slice(&(((*s).clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes());
        }
        v
    };

    for cap in [8 * 1024usize, 64 * 1024, 256 * 1024] {
        let path = dir.join(format!("cap{cap}.s16"));
        let file = std::fs::File::create(&path).expect("create");
        let mut w = std::io::BufWriter::with_capacity(cap, file);
        let mut worst = 0u128;
        let t = Instant::now();
        for _ in 0..BLOCKS {
            let one = Instant::now();
            let _ = w.write_all(&pcm);
            worst = worst.max(one.elapsed().as_micros());
        }
        let _ = w.flush();
        let total = t.elapsed();
        println!(
            "{:<14} {:>10.1} {:>12.2} {:>12.2}",
            format!("{} kB", cap / 1024),
            total.as_micros() as f64 / 1000.0,
            total.as_micros() as f64 / BLOCKS as f64,
            worst as f64 / 1000.0,
        );
        let _ = std::fs::remove_file(&path);
    }
    let _ = std::fs::remove_dir_all(&dir);

    println!(
        "\nworst ms is the single slowest write_all -- the one that would stall\n\
         the writer thread and, if it happens often enough, fill the 200-block\n\
         channel and start dropping capture."
    );
}
