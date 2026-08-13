//! What Opus complexity costs, and what it buys.
//!
//! The encoder has never set one, so it runs at libopus's default — 9 on a
//! scale to 10 — and encoding is about a fifth of the block on the machine
//! that measured it. Complexity is the one dial on this stage, and turning it
//! down is only worth doing if the intelligibility is still there afterwards.
//!
//! So both are measured together: microseconds per frame, and ESTOI of the
//! decoded audio against what went in.
//!
//! ```text
//! set MW_CLIP=C:\ml_data\rides\20260809-0142-000.raw
//! cargo test --release --test encode_cost -- --ignored --nocapture
//! ```
//!
//! Ignored because it is a measurement, not an assertion.

use std::time::Instant;

use mumbleway_core::audio::codec::{Quality, VoiceDecoder, VoiceEncoder, FRAME_SAMPLES};
use mumbleway_core::audio::quality::intelligibility;
use mumbleway_core::audio::testsig;

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
            println!("  clip: synthesised");
            testsig::speech(48_000 * 30, 120.0, 0.35)
        }
    }
}

#[test]
#[ignore]
fn what_opus_complexity_costs_and_buys() {
    let audio = clip();
    let frames = (audio.len() / FRAME_SAMPLES).min(3000);
    println!("  {frames} frames, {:.1} s\n", frames as f32 / 100.0);

    println!("  complexity   median      p99     ESTOI");
    println!("  ----------------------------------------");
    let mut rows = Vec::new();

    // 10 down to 0. The default is 9, and nothing in this project ever chose
    // it — it is simply what libopus does when nobody says.
    for complexity in [10i32, 9, 7, 5, 3, 1, 0] {
        let mut enc = VoiceEncoder::new(Quality::Balanced).expect("encoder");
        enc.set_complexity(complexity).expect("complexity");
        let mut dec = VoiceDecoder::new().expect("decoder");

        let mut times: Vec<f32> = Vec::new();
        let mut decoded: Vec<f32> = Vec::with_capacity(frames * FRAME_SAMPLES);

        for i in 0..frames {
            let pcm = &audio[i * FRAME_SAMPLES..(i + 1) * FRAME_SAMPLES];
            let t = Instant::now();
            let packet = enc.encode(pcm).expect("encode");
            times.push(t.elapsed().as_nanos() as f32 / 1000.0);
            let mut out = [0.0f32; FRAME_SAMPLES];
            let n = dec.decode(&packet, &mut out).expect("decode");
            decoded.extend_from_slice(&out[..n]);
        }

        let clean = &audio[..decoded.len().min(audio.len())];
        let estoi = intelligibility(clean, &decoded[..clean.len()]);
        // **The median, not the mean.** A mean over per-frame timings on a
        // machine doing anything else measures the interference as much as the
        // code: the first run of this had complexity 5 costing more than
        // complexity 9, which is not a thing an encoder can do. The median is
        // unmoved by a handful of descheduled frames, and the p99 beside it
        // says how much of that there was.
        times.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let median = times[times.len() / 2];
        let p99 = times[times.len() * 99 / 100];
        println!("  {complexity:>6}     {median:>7.1} µs {p99:>7.1}    {estoi:.3}");
        rows.push((complexity, median, estoi));
    }

    let (_, at_default, estoi_default) = rows[1];
    println!();
    for (c, mean, estoi) in &rows {
        if *c < 9 {
            println!(
                "  {c:>2}: {:>5.1}% of the default's cost, ESTOI {:+.3}",
                100.0 * mean / at_default,
                estoi - estoi_default
            );
        }
    }
    println!();
}
