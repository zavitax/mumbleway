//! Renders what a listener actually hears, at each protection setting.
//!
//! Encodes a clip with the real `VoiceEncoder`, throws packets away in bursts,
//! plays what survives through the real `SpeakerBuffer`, and writes the result
//! so it can be listened to. The loss pattern is seeded and identical at every
//! setting, so the files differ only by what protection bought.
//!
//! Bursty rather than uniform, because uniform loss is unrealistically kind:
//! it hands the decoder an intact packet either side of every gap, which is
//! the one case in-band FEC handles perfectly. Interference does not do that.
//!
//! ```text
//! MW_CLIP=source_48k.raw MW_OUT=. cargo test --test fec_audition -- --ignored --nocapture
//! ```

use mumbleway_core::audio::codec::{Quality, VoiceEncoder, FRAME_SAMPLES};
use mumbleway_core::audio::jitter::SpeakerBuffer;

/// Gilbert–Elliott: a good state that rarely drops and a bad state that often
/// does, so losses arrive in runs the way they do on a moving vehicle.
struct Burst {
    state: u64,
    bad: bool,
    /// Chance per packet of falling into the bad state; set from the target.
    enter: f32,
}

const BAD_LEAVE: f32 = 0.35;
const BAD_DROP: f32 = 0.60;
const GOOD_DROP: f32 = 0.01;

impl Burst {
    /// `target` is the overall loss fraction wanted. The bad state's own
    /// figures are held fixed — it is a bad radio moment, and that does not
    /// change character with how often it happens — and how *often* the link
    /// falls into it is solved for. So doubling the loss doubles the number of
    /// bursts rather than making each one worse, which is what a rougher road
    /// does.
    fn new(seed: u64, target: f32) -> Self {
        let bad_share = ((target - GOOD_DROP) / (BAD_DROP - GOOD_DROP)).clamp(0.0, 0.9);
        let enter = BAD_LEAVE * bad_share / (1.0 - bad_share);
        Self {
            state: seed,
            bad: false,
            enter,
        }
    }

    fn next_f32(&mut self) -> f32 {
        // xorshift64*, so the pattern is identical on every machine and run.
        self.state ^= self.state >> 12;
        self.state ^= self.state << 25;
        self.state ^= self.state >> 27;
        (self.state.wrapping_mul(0x2545F491_4F6CDD1D) >> 40) as f32 / 16_777_216.0
    }

    /// True when this packet is lost.
    fn lost(&mut self) -> bool {
        let (switch, drop_p) = if self.bad {
            (BAD_LEAVE, BAD_DROP)
        } else {
            (self.enter, GOOD_DROP)
        };
        if self.next_f32() < switch {
            self.bad = !self.bad;
        }
        self.next_f32() < drop_p
    }
}

fn write_wav(path: &str, pcm: &[f32]) {
    let bytes: Vec<u8> = pcm
        .iter()
        .flat_map(|s| ((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes())
        .collect();
    let data = bytes.len() as u32;
    let mut out = Vec::with_capacity(44 + bytes.len());
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&48_000u32.to_le_bytes());
    out.extend_from_slice(&96_000u32.to_le_bytes()); // byte rate
    out.extend_from_slice(&2u16.to_le_bytes()); // block align
    out.extend_from_slice(&16u16.to_le_bytes()); // bits
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data.to_le_bytes());
    out.extend_from_slice(&bytes);
    std::fs::write(path, out).unwrap();
}

/// How far the damaged render sits from the undamaged one, in dB, over the
/// parts that carry signal. Both went through the same codec at the same
/// setting, so the difference is loss and nothing else.
fn seg_snr(reference: &[f32], damaged: &[f32]) -> f32 {
    let frame = FRAME_SAMPLES;
    let (mut total, mut n) = (0.0f32, 0usize);
    let len = reference.len().min(damaged.len());
    for i in (0..len.saturating_sub(frame)).step_by(frame) {
        let a = &reference[i..i + frame];
        let b = &damaged[i..i + frame];
        let energy: f32 = a.iter().map(|s| s * s).sum();
        if energy < 1e-3 {
            continue;
        }
        let noise: f32 = a
            .iter()
            .zip(b)
            .map(|(x, y)| (x - y) * (x - y))
            .sum::<f32>()
            .max(1e-12);
        total += 10.0 * (energy / noise).log10();
        n += 1;
    }
    total / n.max(1) as f32
}

/// One run: encode at `protection`, drop the same packets, play what is left.
fn render(pcm: &[f32], protection: u8, lose: bool, target: f32) -> (Vec<f32>, usize, usize) {
    let mut enc = VoiceEncoder::new(Quality::Balanced).unwrap();
    enc.set_packet_loss_perc(protection).unwrap();
    let mut buf = SpeakerBuffer::new().unwrap();
    let mut burst = Burst::new(0x5EED_1234_ABCD_0001, target);

    let mut out = Vec::with_capacity(pcm.len());
    let mut frame = vec![0.0f32; FRAME_SAMPLES];
    let (mut sent, mut dropped) = (0usize, 0usize);

    for (i, chunk) in pcm.chunks(FRAME_SAMPLES).enumerate() {
        if chunk.len() < FRAME_SAMPLES {
            break;
        }
        let packet = enc.encode(chunk).unwrap();
        sent += 1;
        // Sequence counts 10 ms units, so a 20 ms frame steps by two.
        let seq = i as u64 * 2;
        if lose && burst.lost() {
            dropped += 1;
        } else {
            buf.push(seq, packet, false);
        }
        // One frame in, one frame out: the buffer fills to its own target
        // first and returns None until it has, which is the real behaviour.
        match buf.pop(&mut frame) {
            Some(n) => out.extend_from_slice(&frame[..n]),
            None => out.extend(std::iter::repeat(0.0).take(FRAME_SAMPLES)),
        }
    }
    // Drain whatever the buffer is still holding.
    while let Some(n) = buf.pop(&mut frame) {
        out.extend_from_slice(&frame[..n]);
    }
    (out, sent, dropped)
}

#[test]
#[ignore = "renders wav files to listen to; needs MW_CLIP and MW_OUT"]
fn render_each_protection_level() {
    let clip = std::env::var("MW_CLIP").expect("set MW_CLIP to a headerless f32 48k mono .raw");
    let dir = std::env::var("MW_OUT").unwrap_or_else(|_| ".".into());
    let bytes = std::fs::read(&clip).unwrap();
    let pcm: Vec<f32> = bytes
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect();
    println!("clip: {:.1} s", pcm.len() as f32 / 48_000.0);

    // One no-loss render, to hear what the codec alone does.
    let target: f32 = std::env::var("MW_LOSS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0.12);
    println!("target loss: {:.1}%", target * 100.0);
    let (clean10, _, _) = render(&pcm, 10, false, target);
    write_wav(&format!("{dir}/00-no-loss.wav"), &clean10);
    println!("wrote 00-no-loss.wav  (codec only, no packets dropped)");
    println!();
    println!("  file            lost    damage vs its own no-loss render");

    for pct in [0u8, 10, 20, 30, 50, 100] {
        let (audio, sent, dropped) = render(&pcm, pct, true, target);
        // **Its own** reference, at the same protection setting. Comparing
        // every level against one shared reference measures the difference
        // between encoder settings, which swamps the difference loss makes:
        // the level that happens to match the reference scores 26 dB better
        // than the rest for no reason a listener would ever hear.
        let (reference, _, _) = render(&pcm, pct, false, target);
        let snr = seg_snr(&reference, &audio);
        write_wav(&format!("{dir}/fec-{pct:03}.wav"), &audio);
        println!(
            "  fec-{:03}.wav   {:>4} ({:.1}%)   {:>6.2} dB",
            pct,
            dropped,
            dropped as f32 / sent as f32 * 100.0,
            snr
        );
        let _ = sent;
    }
}
