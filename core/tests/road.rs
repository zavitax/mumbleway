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
use mumbleway_core::audio::dsp::{rms, to_dbfs, RumbleFilter, SpeechBand};
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
    /// Share of labelled speech that was transmitted. Low is "it cut me off".
    recall: f32,
    /// Share of transmitted blocks that were labelled speech. Low is "they
    /// hear my wind".
    precision: f32,
    /// Whether there were labels to score against at all.
    labelled: bool,
}

fn run(profile: NoiseProfile, signal: &[f32], spans: &[(f32, f32)]) -> Run {
    let mut chain = CaptureProcessor::new(profile);
    let mut block = [0.0f32; FRAME_SIZE];
    let (mut sent, mut counted) = (0usize, 0usize);
    let (mut level, mut floor) = (0.0f64, 0.0f64);
    // The four cells of the only table that matters.
    let (mut hit, mut miss, mut false_alarm) = (0usize, 0usize, 0usize);

    for (i, chunk) in signal.chunks_exact(FRAME_SIZE).enumerate() {
        block.copy_from_slice(chunk);
        let a = chain.process(&mut block);
        if a.warming_up {
            continue;
        }
        counted += 1;
        let at = i as f32 * FRAME_SIZE as f32 / 48_000.0;
        let is_speech = spans.iter().any(|(s, e)| at >= *s && at <= *e);
        match (a.speaking, is_speech) {
            (true, true) => hit += 1,
            (false, true) => miss += 1,
            (true, false) => false_alarm += 1,
            (false, false) => {}
        }
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
        // How much of what was said went out, and how much of what went out
        // was said. A transmitted *share* cannot distinguish a chain that
        // sends the right quarter of a clip from one that sends the wrong
        // quarter, and until now nothing here could tell them apart.
        recall: hit as f32 / (hit + miss).max(1) as f32,
        precision: hit as f32 / (hit + false_alarm).max(1) as f32,
        labelled: hit + miss > 0,
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

/// Spans, in seconds, where somebody was actually talking.
///
/// Read from a `NAME.speech` sidecar next to the clip: one `start end` pair per
/// line. Hand-written by the person who made the recording, which is the only
/// authority there is — no measure here can be validated against a label that
/// another measure produced, and using one would make the whole exercise
/// circular.
fn speech_spans(dir: &str, name: &str) -> Vec<(f32, f32)> {
    let path = PathBuf::from(dir).join(format!("{name}.speech"));
    let Ok(text) = fs::read_to_string(path) else {
        return Vec::new();
    };
    text.lines()
        .filter_map(|line| {
            let mut it = line.split_whitespace();
            let a: f32 = it.next()?.parse().ok()?;
            let b: f32 = it.next()?.parse().ok()?;
            Some((a, b))
        })
        .collect()
}

/// The distribution of a per-block measure inside the labelled speech and
/// outside it.
///
/// This is the whole point of having labels. A threshold is a claim that two
/// populations can be told apart, and until the populations are separated
/// there is nothing to check the claim against — the earlier runs could say
/// only "the peaks are 0.8" and not whether the peaks were the voice.
fn split_by_label(scores: &[f32], spans: &[(f32, f32)]) -> (Vec<f32>, Vec<f32>) {
    let (mut inside, mut outside) = (Vec::new(), Vec::new());
    for (i, s) in scores.iter().enumerate() {
        let at = i as f32 * FRAME_SIZE as f32 / 48_000.0;
        if spans.iter().any(|(a, b)| at >= *a && at <= *b) {
            inside.push(*s);
        } else {
            outside.push(*s);
        }
    }
    inside.sort_by(|a, b| a.partial_cmp(b).unwrap());
    outside.sort_by(|a, b| a.partial_cmp(b).unwrap());
    (inside, outside)
}

/// Where the signal goes, stage by stage, in dB.
///
/// On real helmet audio the Helmet profile reports a level of -84.9 dBFS
/// against -18.2 with suppression off — sixty-seven decibels, which is not
/// suppression working but suppression removing the thing it was protecting.
/// The reported level is measured after the filters and the network and before
/// the gate, so the loss is attributable, and this attributes it: each stage is
/// run alone on the same audio so the cost of each can be named rather than
/// argued about.
fn stage_levels(profile: NoiseProfile, signal: &[f32]) -> (f32, f32, f32, f32) {
    let raw = to_dbfs(rms(signal));

    // The high-pass alone.
    let mut hp = signal.to_vec();
    let mut rumble = RumbleFilter::new(
        48_000.0,
        match profile {
            NoiseProfile::Off => 60.0,
            NoiseProfile::Light => 90.0,
            NoiseProfile::Helmet => 180.0,
            _ => 120.0,
        },
    );
    rumble.process(&mut hp);
    let after_hp = to_dbfs(rms(&hp));

    // And the band filter on top of it.
    let corner = match profile {
        NoiseProfile::Off => None,
        NoiseProfile::Light => Some(12_000.0),
        NoiseProfile::Helmet => Some(6_500.0),
        _ => Some(10_000.0),
    };
    if let Some(hz) = corner {
        let mut band = SpeechBand::new(48_000.0, hz);
        band.process(&mut hp);
    }
    let after_band = to_dbfs(rms(&hp));

    // Everything, which is what the chain reports.
    let mut chain = CaptureProcessor::new(profile);
    let mut blk = [0.0f32; FRAME_SIZE];
    let (mut sum, mut n) = (0.0f64, 0usize);
    for chunk in signal.chunks_exact(FRAME_SIZE) {
        blk.copy_from_slice(chunk);
        let a = chain.process(&mut blk);
        if a.warming_up {
            continue;
        }
        sum += a.level_db as f64;
        n += 1;
    }
    (raw, after_hp, after_band, (sum / n.max(1) as f64) as f32)
}

/// How much a feature knows about whether the rider is talking, 0.5 to 1.0.
///
/// The probability that a randomly chosen speech block scores higher than a
/// randomly chosen non-speech one — the area under the ROC curve. Chosen
/// because it needs no threshold: a feature judged at one cut point can be
/// flattered or ruined by where the cut fell, and comparing candidates that
/// way compares the cuts rather than the features. 0.5 is a coin.
fn separation(inside: &[f32], outside: &[f32]) -> f32 {
    if inside.is_empty() || outside.is_empty() {
        return 0.5;
    }
    let mut better = 0.0f64;
    for a in inside {
        for b in outside {
            // Ties count a half, which is what keeps a feature that is
            // constant at 0.5 rather than at 0 or 1.
            better += match a.partial_cmp(b) {
                Some(std::cmp::Ordering::Greater) => 1.0,
                Some(std::cmp::Ordering::Equal) => 0.5,
                _ => 0.0,
            };
        }
    }
    (better / (inside.len() as f64 * outside.len() as f64)) as f32
}

/// Every per-block number the chain computes, gathered so they can be
/// compared on the same footing.
fn features(profile: NoiseProfile, signal: &[f32]) -> Vec<(f32, [f32; 5])> {
    let mut chain = CaptureProcessor::new(profile);
    let mut block = [0.0f32; FRAME_SIZE];
    let mut out = Vec::new();
    for (i, chunk) in signal.chunks_exact(FRAME_SIZE).enumerate() {
        block.copy_from_slice(chunk);
        let a = chain.process(&mut block);
        if a.warming_up {
            continue;
        }
        let at = i as f32 * FRAME_SIZE as f32 / 48_000.0;
        out.push((
            at,
            [a.vad, a.snr_db, a.harmonicity, a.modulation, a.level_db],
        ));
    }
    out
}

const FEATURE_NAMES: [&str; 5] = ["vad", "snr_db", "harmonicity", "modulation", "level_db"];

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

/// Writes what the chain hands the encoder, so another tool can look at it.
///
/// A neural VAD run on the raw microphone is answering a different question
/// from one run where this app would put it, which is after the suppression.
/// Whether denoising helps such a model or destroys what it needs is exactly
/// the sort of thing that gets assumed; this makes it answerable.
#[test]
#[ignore = "needs MUMBLEWAY_ROAD_AUDIO and MUMBLEWAY_ROAD_DUMP"]
fn dump_the_suppressed_audio() {
    let Ok(out_dir) = std::env::var("MUMBLEWAY_ROAD_DUMP") else {
        panic!("set MUMBLEWAY_ROAD_DUMP to a directory to write into");
    };
    fs::create_dir_all(&out_dir).expect("creating the dump directory");

    for (name, signal) in clips() {
        for profile in [NoiseProfile::Light, NoiseProfile::Helmet] {
            let mut chain = CaptureProcessor::new(profile);
            let mut block = [0.0f32; FRAME_SIZE];
            let mut out: Vec<u8> = Vec::with_capacity(signal.len() * 4);
            for chunk in signal.chunks_exact(FRAME_SIZE) {
                block.copy_from_slice(chunk);
                chain.process(&mut block);
                for s in &block {
                    out.extend_from_slice(&s.to_le_bytes());
                }
            }
            let path = PathBuf::from(&out_dir).join(format!("{name}__{profile:?}.raw"));
            fs::write(&path, &out).expect("writing the dump");
            println!("wrote {}", path.display());
        }
    }
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

        let spans = speech_spans(
            &std::env::var("MUMBLEWAY_ROAD_AUDIO").unwrap_or_default(),
            &name,
        );
        if !spans.is_empty() {
            // Which of the numbers the chain already computes actually knows
            // anything. Measured under Helmet, the profile a rider at speed
            // would be using.
            // Which stage is actually throwing the speech away.
            //
            // Sweeping thresholds to find out was a mistake: two different
            // knobs produced the same numbers and neither moved past a limit,
            // which says only that something else binds — not what. Each
            // condition is recorded separately, so the stage that fails on a
            // speech block can be named instead of inferred.
            {
                let mut chain = CaptureProcessor::new(NoiseProfile::Helmet);
                let mut blk = [0.0f32; FRAME_SIZE];
                let (mut n, mut vad_ok, mut snr_ok, mut gate_ok, mut sent) = (0, 0, 0, 0, 0);
                for (i, chunk) in signal.chunks_exact(FRAME_SIZE).enumerate() {
                    blk.copy_from_slice(chunk);
                    let a = chain.process(&mut blk);
                    if a.warming_up {
                        continue;
                    }
                    let at = i as f32 * FRAME_SIZE as f32 / 48_000.0;
                    if !spans.iter().any(|(s, e)| at >= *s && at <= *e) {
                        continue;
                    }
                    n += 1;
                    if a.vad_says_speech {
                        vad_ok += 1;
                    }
                    if a.snr_says_speech {
                        snr_ok += 1;
                    }
                    if a.gate_open {
                        gate_ok += 1;
                    }
                    if a.speaking {
                        sent += 1;
                    }
                }
                let pct = |x: usize| x as f32 / n.max(1) as f32 * 100.0;
                println!(
                    "    OF LABELLED SPEECH BLOCKS (Helmet): vad {:.0}%  snr {:.0}%  gate {:.0}%  sent {:.0}%",
                    pct(vad_ok),
                    pct(snr_ok),
                    pct(gate_ok),
                    pct(sent)
                );
            }

            println!("    WHERE THE SIGNAL GOES (dBFS):");
            for profile in [
                NoiseProfile::Off,
                NoiseProfile::Standard,
                NoiseProfile::Helmet,
            ] {
                let (raw, hp, band, chain) = stage_levels(profile, &signal);
                println!(
                    "        {:<9} raw {raw:>6.1} -> high-pass {hp:>6.1} -> band {band:>6.1} -> chain {chain:>6.1}",
                    format!("{profile:?}")
                );
            }

            println!("    HOW MUCH EACH FEATURE KNOWS (0.50 = a coin):");
            let rows = features(NoiseProfile::Helmet, &signal);
            for (f, name) in FEATURE_NAMES.iter().enumerate() {
                let (mut inside, mut outside) = (Vec::new(), Vec::new());
                for (at, values) in &rows {
                    if spans.iter().any(|(a, b)| *at >= *a && *at <= *b) {
                        inside.push(values[f]);
                    } else {
                        outside.push(values[f]);
                    }
                }
                println!("        {name:<14} {:.3}", separation(&inside, &outside));
            }

            let (inside, outside) = split_by_label(&scores, &spans);
            println!(
                "    LABELLED  speech({} blocks)  p10 {:.2} p50 {:.2} p90 {:.2}",
                inside.len(),
                percentile(&inside, 0.10),
                percentile(&inside, 0.50),
                percentile(&inside, 0.90),
            );
            println!(
                "              rest ({} blocks)  p50 {:.2} p90 {:.2} p99 {:.2} max {:.2}",
                outside.len(),
                percentile(&outside, 0.50),
                percentile(&outside, 0.90),
                percentile(&outside, 0.99),
                percentile(&outside, 1.0),
            );
            // What a threshold placed here would actually do, which is the only
            // form in which a threshold is worth discussing.
            for bar in [0.75f32, 0.65, 0.55, 0.45, 0.35, 0.30, 0.20] {
                let caught = inside.iter().filter(|s| **s >= bar).count() as f32
                    / inside.len().max(1) as f32;
                let false_alarm = outside.iter().filter(|s| **s >= bar).count() as f32
                    / outside.len().max(1) as f32;
                println!(
                    "              bar {bar:.2}: catches {:>5.1}% of speech, {:>5.1}% of the rest",
                    caught * 100.0,
                    false_alarm * 100.0
                );
            }
        }

        for profile in [
            NoiseProfile::Off,
            NoiseProfile::Light,
            NoiseProfile::Standard,
            NoiseProfile::Helmet,
            NoiseProfile::Auto,
        ] {
            let r = run(profile, &signal, &spans);
            let settled = if profile == NoiseProfile::Auto {
                format!(" -> {:?}", r.settled_on)
            } else {
                String::new()
            };
            let scored = if r.labelled {
                format!(
                    "  kept {:>5.1}% of speech, {:>5.1}% of what it sent was speech",
                    r.recall * 100.0,
                    r.precision * 100.0
                )
            } else {
                String::new()
            };
            println!(
                "    {:<9}{:>7.1}% transmitted   level {:>6.1}  floor {:>6.1}{settled}{scored}",
                format!("{profile:?}"),
                r.transmitted * 100.0,
                r.level_db,
                r.floor_db,
            );
        }
    }
}
