//! Which frame does concealment actually play when two packets go missing?
//!
//! Opus in-band FEC puts a low-bitrate copy of the frame *immediately before*
//! a packet inside that packet. So the redundant copy of frame N rides in
//! packet N+1, and only in packet N+1.
//!
//! `SpeakerBuffer::pop` conceals a gap of up to two packets and hands
//! `decode_lost` whatever the next available packet is. When exactly one
//! packet is missing that packet is N+1 and the FEC is the right frame. When
//! two are missing it is N+2, whose FEC copy is of N+1 — a different frame
//! from the one the slot is for.
//!
//! This test does not assert which behaviour is correct. It identifies, by
//! tone, which frame comes out, so the answer is measured rather than reasoned
//! about.

use mumbleway_core::audio::codec::FRAME_SAMPLES;
use mumbleway_core::audio::codec::{Quality, VoiceEncoder};
use mumbleway_core::audio::jitter::SpeakerBuffer;

const RATE: f32 = 48_000.0;

/// A distinct tone per frame, so the output can be identified.
fn tone_hz(index: usize) -> f32 {
    [300.0, 700.0, 1500.0, 2600.0, 3700.0][index]
}

fn frames(n: usize) -> Vec<Vec<u8>> {
    let mut enc = VoiceEncoder::new(Quality::Balanced).unwrap();
    (0..n)
        .map(|i| {
            let hz = tone_hz(i);
            let pcm: Vec<f32> = (0..FRAME_SAMPLES)
                .map(|s| {
                    let t = (i * FRAME_SAMPLES + s) as f32 / RATE;
                    (2.0 * std::f32::consts::PI * hz * t).sin() * 0.4
                })
                .collect();
            enc.encode(&pcm).unwrap()
        })
        .collect()
}

/// Energy at one frequency, by correlation. Enough to tell five tones apart
/// without pulling in an FFT.
fn energy_at(samples: &[f32], hz: f32) -> f32 {
    let (mut re, mut im) = (0.0f32, 0.0f32);
    for (n, s) in samples.iter().enumerate() {
        let a = 2.0 * std::f32::consts::PI * hz * n as f32 / RATE;
        re += s * a.cos();
        im += s * a.sin();
    }
    (re * re + im * im).sqrt() / samples.len() as f32
}

/// Which of the five tones this block is loudest at.
fn identify(samples: &[f32]) -> (usize, f32) {
    let mut best = (0usize, 0.0f32);
    for i in 0..5 {
        let e = energy_at(samples, tone_hz(i));
        if e > best.1 {
            best = (i, e);
        }
    }
    best
}

#[test]
#[ignore = "diagnostic: prints what concealment plays, asserts nothing about it"]
fn what_concealment_plays_when_two_packets_are_lost() {
    let f = frames(5);
    let mut out = vec![0.0f32; FRAME_SAMPLES];

    // One lost packet: the next one available is N+1, whose FEC is of N.
    let mut single = SpeakerBuffer::new().unwrap();
    single.push(0, f[0].clone(), false);
    // frame 1 lost
    single.push(4, f[2].clone(), false);
    single.push(6, f[3].clone(), false);
    single.pop(&mut out);
    println!("one lost   : played slot 0 as tone {:?}", identify(&out));
    single.pop(&mut out);
    println!(
        "one lost   : slot 1 concealed as tone {:?}  (frame 1 is right)",
        identify(&out)
    );

    // Two lost packets: the next one available is N+2, whose FEC is of N+1.
    let mut double = SpeakerBuffer::new().unwrap();
    double.push(0, f[0].clone(), false);
    // frames 1 and 2 lost
    double.push(6, f[3].clone(), false);
    double.push(8, f[4].clone(), false);
    double.pop(&mut out);
    println!("two lost   : played slot 0 as tone {:?}", identify(&out));
    double.pop(&mut out);
    println!(
        "two lost   : slot 1 concealed as tone {:?}  (frame 1 is right)",
        identify(&out)
    );
    double.pop(&mut out);
    println!(
        "two lost   : slot 2 concealed as tone {:?}  (frame 2 is right)",
        identify(&out)
    );
    double.pop(&mut out);
    println!(
        "two lost   : slot 3 played     as tone {:?}  (frame 3 is right)",
        identify(&out)
    );
}
