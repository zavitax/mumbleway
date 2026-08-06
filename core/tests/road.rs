//! The chain, measured against audio recorded from an actual helmet.
//!
//! Every other suite here runs on signals this project generated, and that has
//! been the standing caveat on all of it: synthesised wind is not recorded
//! wind, and the generator was written by the same hand that formed the
//! hypothesis it was meant to test. Twice now the synthetic suites have
//! disagreed with the reasoning the plan was built on, and there has been no
//! way to tell which was wrong.
//!
//! This closes that. It is `#[ignore]`d and takes its input from an
//! environment variable, so it is committed and reproducible without a
//! gigabyte of audio in the repository:
//!
//! ```text
//! ffmpeg -i clip.m4a -ac 1 -ar 48000 -f f32le clip.raw
//! MUMBLEWAY_ROAD_AUDIO=/path/to/dir cargo test --test road -- --ignored --nocapture
//! ```
//!
//! Raw 32-bit float, mono, 48 kHz — the format the chain works in, so there is
//! no decoder here to be wrong about.

use std::fs;
use std::path::PathBuf;

use mumbleway_core::audio::denoise::{CaptureProcessor, FRAME_SIZE};
use mumbleway_core::audio::dsp::{rms, to_dbfs};
use mumbleway_core::audio::pitch::PitchTracker;
use mumbleway_core::audio::NoiseProfile;

/// What one run of a clip through one profile looked like.
struct Run {
    /// Share of blocks the chain would have put on the wire, 0..1.
    transmitted: f32,
    /// Where the level sat, after the chain.
    level_db: f32,
    /// Where the chain thought the background was.
    floor_db: f32,
    /// The profile actually in force at the end, which differs from the one
    /// asked for only under `Auto`.
    settled_on: NoiseProfile,
}

fn run(profile: NoiseProfile, signal: &[f32]) -> Run {
    let mut chain = CaptureProcessor::new(profile);
    let mut block = [0.0f32; FRAME_SIZE];
    let (mut sent, mut counted) = (0usize, 0usize);
    let (mut level, mut floor) = (0.0f64, 0.0f64);

    for chunk in signal.chunks_exact(FRAME_SIZE) {
        block.copy_from_slice(chunk);
        let a = chain.process(&mut block);
        if a.warming_up {
            continue;
        }
        counted += 1;
        if a.speaking {
            sent += 1;
        }
        level += a.level_db as f64;
        floor += a.noise_floor_db as f64;
    }

    let n = counted.max(1) as f64;
    Run {
        transmitted: sent as f32 / n as f32,
        level_db: (level / n) as f32,
        floor_db: (floor / n) as f32,
        settled_on: chain.effective_profile(),
    }
}

/// How periodic the clip is, block by block.
///
/// Reported as a distribution rather than an average, because the average is
/// the one thing it cannot be read from. Speech is a minority of any recording
/// that has silence and wind in it, so a clip that is 10% clear speech and 90%
/// weather has a perfectly ordinary median and a very high top decile. Only
/// the tail says whether there is a voice in there at all.
fn periodicity(signal: &[f32]) -> Vec<f32> {
    let mut tracker = PitchTracker::new();
    let mut scores = Vec::new();
    for chunk in signal.chunks_exact(FRAME_SIZE) {
        scores.push(tracker.analyse(chunk).harmonicity);
    }
    scores
}

fn percentile(sorted: &[f32], p: f32) -> f32 {
    if sorted.is_empty() {
        return 0.0;
    }
    let i = ((sorted.len() - 1) as f32 * p).round() as usize;
    sorted[i]
}

/// Where in the clip the most voiced-looking blocks are, in seconds.
///
/// If there is speech, it is somewhere, and a listener can check the answer by
/// skipping to it. A measure nobody can check is a measure nobody should act
/// on, and every earlier surprise in this project came from acting on one.
fn most_voiced_moments(scores: &[f32], take: usize) -> Vec<(f32, f32)> {
    let mut indexed: Vec<(usize, f32)> = scores.iter().copied().enumerate().collect();
    indexed.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

    let mut picked: Vec<(f32, f32)> = Vec::new();
    for (i, score) in indexed {
        let at = i as f32 * FRAME_SIZE as f32 / 48_000.0;
        // Spread them out, or all ten land inside the same syllable.
        if picked.iter().any(|(t, _)| (t - at).abs() < 2.0) {
            continue;
        }
        picked.push((at, score));
        if picked.len() >= take {
            break;
        }
    }
    picked.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    picked
}

fn clips() -> Vec<(String, Vec<f32>)> {
    let Ok(dir) = std::env::var("MUMBLEWAY_ROAD_AUDIO") else {
        panic!("set MUMBLEWAY_ROAD_AUDIO to a directory of mono 48 kHz f32 .raw files");
    };
    let mut found: Vec<PathBuf> = fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("cannot read {dir}: {e}"))
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "raw"))
        .collect();
    found.sort();

    found
        .into_iter()
        .map(|path| {
            let bytes = fs::read(&path).expect("reading clip");
            let samples: Vec<f32> = bytes
                .chunks_exact(4)
                .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                .collect();
            let name = path.file_stem().unwrap().to_string_lossy().into_owned();
            (name, samples)
        })
        .collect()
}

#[test]
#[ignore = "needs MUMBLEWAY_ROAD_AUDIO; see the module comment"]
fn what_the_chain_does_with_real_helmet_audio() {
    for (name, signal) in clips() {
        let seconds = signal.len() as f32 / 48_000.0;
        let scores = periodicity(&signal);
        let loudest = most_voiced_moments(&scores, 8);
        let mut sorted = scores.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());

        println!(
            "\n=== {name}  {seconds:.1}s  raw {:.1} dBFS ===",
            to_dbfs(rms(&signal))
        );
        println!(
            "    periodicity  p50 {:.2}  p90 {:.2}  p99 {:.2}  max {:.2}",
            percentile(&sorted, 0.50),
            percentile(&sorted, 0.90),
            percentile(&sorted, 0.99),
            percentile(&sorted, 1.0),
        );
        let voiced = scores.iter().filter(|s| **s >= 0.75).count() as f32 / scores.len() as f32;
        println!(
            "    blocks over the 0.75 voiced bar: {:.2}%",
            voiced * 100.0
        );
        let listen: Vec<String> = loudest
            .iter()
            .map(|(t, s)| format!("{t:.1}s({s:.2})"))
            .collect();
        println!("    most voiced moments: {}", listen.join(" "));

        for profile in [
            NoiseProfile::Off,
            NoiseProfile::Light,
            NoiseProfile::Standard,
            NoiseProfile::Helmet,
            NoiseProfile::Auto,
        ] {
            let r = run(profile, &signal);
            let settled = if profile == NoiseProfile::Auto {
                format!(" -> {:?}", r.settled_on)
            } else {
                String::new()
            };
            println!(
                "    {:<9}{:>7.1}% transmitted   level {:>6.1}  floor {:>6.1}{settled}",
                format!("{profile:?}"),
                r.transmitted * 100.0,
                r.level_db,
                r.floor_db,
            );
        }
    }
}
