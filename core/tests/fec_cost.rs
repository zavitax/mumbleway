//! What protection actually costs, in bytes and in quality.
//!
//! The encoder is given a fixed bitrate, and Opus carves the in-band FEC copy
//! out of that same budget rather than adding to it. So the interesting
//! question is not "how much bandwidth does 50% FEC add" — the answer to that
//! is expected to be "almost none" — but "how much of the primary frame does
//! it spend to get there".
//!
//! Measured on a synthetic voiced signal: a 140 Hz glottal-ish pulse train
//! under three formants, with pauses. Not speech, but it exercises the same
//! parts of the codec — periodic, band-limited, with silence between phrases.

use mumbleway_core::audio::codec::{Quality, VoiceEncoder, FRAME_SAMPLES};

const RATE: f32 = 48_000.0;
const FRAMES: usize = 1500; // 30 seconds

/// A real ride, if one is to hand. `MW_CLIP` points at a headerless f32
/// mono 48 kHz capture — the format `record.rs` writes and the corpus
/// keeps. Without it the test falls back to a synthetic signal and says so,
/// because a measurement on invented audio is worth saying out loud.
fn real_clip() -> Option<Vec<f32>> {
    let path = std::env::var("MW_CLIP").ok()?;
    let bytes = std::fs::read(&path).ok()?;
    let pcm: Vec<f32> = bytes
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect();
    let want = FRAMES * FRAME_SAMPLES;
    (pcm.len() >= want).then(|| pcm[..want].to_vec())
}

/// Something voice-shaped: a pulse train through three formants, with gaps.
fn speechlike() -> Vec<f32> {
    let n = FRAMES * FRAME_SAMPLES;
    let mut out = vec![0.0f32; n];
    for (i, s) in out.iter_mut().enumerate() {
        let t = i as f32 / RATE;
        // Pauses every 1.4 s, so the encoder meets silence as it would in use.
        if (t % 1.4) > 1.05 {
            continue;
        }
        let f0 = 140.0 + 8.0 * (2.0 * std::f32::consts::PI * 0.7 * t).sin();
        let mut v = 0.0;
        for (h, (fmt, amp)) in [(700.0, 1.0), (1220.0, 0.5), (2600.0, 0.25)]
            .into_iter()
            .enumerate()
        {
            let _ = h;
            let (fmt, amp): (f32, f32) = (fmt, amp);
            v += amp * (2.0 * std::f32::consts::PI * fmt * t).sin();
        }
        // Pitch pulses, so it is periodic rather than a chord.
        let pulse = ((2.0 * std::f32::consts::PI * f0 * t).sin())
            .max(0.0)
            .powi(3);
        *s = 0.35 * v * pulse;
    }
    out
}

#[test]
#[ignore = "measurement: prints the cost of each protection level"]
fn what_protection_costs() {
    let (pcm, source) = match real_clip() {
        Some(p) => (p, "a real ride via MW_CLIP"),
        None => (
            speechlike(),
            "SYNTHETIC — set MW_CLIP to a .raw for a real one",
        ),
    };

    println!();
    println!(
        "Quality::Balanced, target 24000 bit/s, 20 ms frames, {} s",
        FRAMES / 50
    );
    println!("signal: {}", source);
    println!();
    println!("  loss%   bytes   bit/s   vs 0%");

    let mut baseline_bytes = 0usize;

    for (i, pct) in [0u8, 10, 20, 30, 40, 50, 75, 100].into_iter().enumerate() {
        let mut enc = VoiceEncoder::new(Quality::Balanced).unwrap();
        enc.set_packet_loss_perc(pct).unwrap();

        let mut bytes = 0usize;

        for frame in pcm.chunks(FRAME_SAMPLES) {
            let packet = enc.encode(frame).unwrap();
            bytes += packet.len();
        }

        let bits = bytes as f32 * 8.0 / (FRAMES as f32 * 0.02);
        if i == 0 {
            baseline_bytes = bytes;
        }
        println!(
            "  {:>4}   {:>6}  {:>6.0}  {:>+6.1}%",
            pct,
            bytes,
            bits,
            (bytes as f32 - baseline_bytes as f32) / baseline_bytes as f32 * 100.0
        );
    }
    println!();
}
