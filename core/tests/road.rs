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
//!
//! # Provenance is part of the measurement
//!
//! The first four recordings this was run against were captured on the
//! **phone's own microphone**, not the headset's — a metre or more from the
//! rider's mouth instead of a centimetre. That was discovered after every
//! number below had been produced and reported.
//!
//! It is not a small discrepancy. The app records from a boom microphone
//! inside the helmet; a phone in a pocket or on a mount hears mostly wind with
//! a distant, largely unintelligible voice somewhere behind it. Those are
//! different signals, and the figures taken from that audio —
//! 57% recall, 54% precision, RNNoise's VAD firing on 38% of labelled speech,
//! 25 dB of over-suppression — describe the chain's behaviour on the wrong
//! one.
//!
//! What made it invisible was that nothing here asks. The harness takes a
//! directory of raw audio and reports on it, and audio carries no record of
//! what captured it. `docs/VOICE_MODEL.md` §5.3 says in writing that
//! recordings must come through the real channel because the microphone
//! response is part of the domain — and that requirement was applied to
//! recordings not yet made while the ones already in hand went unquestioned
//! for a whole investigation.
//!
//! **So: check how a clip was captured before trusting anything this prints.**
//! Headset-recorded audio needs its own baseline; none of the numbers from the
//! phone-mic set carry over.
//!
//! # What it found, 2026-08-14: the floor climbing onto the voice
//!
//! Two clips came in through the app's own diagnostic capture — one of long
//! spoken phrases, one of singing — reporting that the chain cut pieces out of
//! both. This harness is what turned that into a number, and the number is the
//! reason `NoiseFloorTracker` now rate-limits how fast its estimate may rise.
//!
//! The singing clip is 36.4% strongly voiced. Before the limit:
//!
//! ```text
//!     Off         58.5% transmitted   floor -40.8
//!     Standard    29.0% transmitted   floor -52.3
//!     Helmet      24.8% transmitted   floor -56.1
//! ```
//!
//! **Standard put less on the wire than the clip has voiced audio in it.**
//! After the limit, with nothing else changed:
//!
//! ```text
//!     Off         67.9% transmitted   floor -44.1
//!     Standard    54.5% transmitted   floor -63.6
//!     Helmet      54.5% transmitted   floor -68.8
//! ```
//!
//! Two things in that are worth keeping. **`Off` improved too**, which places
//! the fault in the gate's floor rather than in the suppressor — the profiles
//! only changed how much of the voice was left for the floor to climb onto.
//! And **`Auto` settled differently**: `Helmet` before, `Standard` after, because
//! an inflated floor reads as a noisier room and Auto reached for the more
//! aggressive profile. That is a second, quieter symptom of the same bug.
//!
//! The spoken clip barely moved (64.3% -> 64.5%), which matches what was
//! reported about it: chewing rather than dropouts, because the hold and fade
//! envelope around a voiced block covers a gate that closes only briefly.
//!
//! **Every figure above was measured without the enhancer**, which this harness
//! did not run until 2026-08-15. They compare two suppressors honestly against
//! each other and describe a chain that does not ship. With DeepFilterNet in
//! front, as `engine.rs` has it, the same clips read:
//!
//! ```text
//!     singing         Off 86.0%   Light 54.9%   Standard 54.8%   Helmet 54.8%
//!     speech          Off 96.1%   Light 65.6%   Standard 61.9%   Helmet 62.6%
//! ```
//!
//! `Off` — the enhancer alone, with no profile suppression at all — puts the
//! most on the wire by a wide margin on both. That is worth being careful
//! about rather than pleased about: a transmitted *share* is not quality, and
//! neither clip carries labels, so nothing here says whether the extra is
//! speech or leakage.
//!
//! # What it found, 2026-08-15: the enhancer makes level separation real
//!
//! `how_far_the_enhancer_separates_the_voice_from_the_gaps` exists to test the
//! one thing a level- or SNR-derived speech decision depends on: whether the
//! voice and the gaps are far enough apart in level for any threshold to sit
//! between them. `docs/MUSIC_GATE.md` recorded six hand-built features failing
//! because, over music, they were 1.5 dB apart — and DeepFilterNet taking that
//! to 16.0 dB. **On one clip**, which the file said plainly.
//!
//! ```text
//!     clip               labeller       n  raw voi  raw gap     sep |  enh voi  enh gap     sep
//!     singing            raw-pitch    503    -17.8    -42.4    24.7 |    -18.2    -45.9    27.8
//!     singing            enh-pitch    511    -17.8    -42.7    24.9 |    -18.2    -46.2    28.0
//!     speech-phrases     raw-pitch    302    -17.0    -37.9    20.9 |    -17.0    -41.5    24.5
//!     speech-phrases     enh-pitch    300    -17.2    -37.8    20.5 |    -17.0    -41.5    24.5
//!     voice-over-motor   raw-pitch      7    -10.7    -13.6     2.9 |    -12.5    -35.1    22.6
//!     voice-over-motor   enh-pitch    104    -11.1    -13.8     2.6 |    -15.1    -36.4    21.3
//! ```
//!
//! **Two regimes, and the difference is what is in the gaps.** The first two
//! clips are quiet-room recordings: their gaps sit at -38 to -42 dBFS and the
//! separation is already 21 to 25 dB before the enhancer touches anything, so
//! they say nothing about the hard case. `voice-over-motor` is the hard case —
//! a motor filling the gaps to within **2.6 dB** of the voice — and it
//! replicates the MUSIC_GATE result in a different noise: **2.6 dB to 21.3 dB**,
//! with the enhancer taking 22 dB out of the gaps and 4 dB off the voice.
//!
//! So the figure that a level-derived decision rests on is no longer one clip.
//! Two, in two noise types, both through the real channel.
//!
//! **The labeller is the part to be suspicious of, so there are two.** Blocks
//! are split by periodicity rather than by level, because using level to label
//! blocks whose level separation is the measurement would be circular. But a
//! motor masks harmonics: on the raw signal the pitch search finds only 7
//! voiced blocks in 15 seconds, which is too few to conclude anything from.
//! Labelling from periodicity measured on the *enhanced* signal finds 104 and
//! gives the same answer, which is what makes the result trustworthy rather
//! than the 22.6 alone. That labeller has its own bias — it is the enhancer's
//! own output, so a block it wrongly silenced would be counted as a gap — which
//! is why both are printed with the count each rests on. On the two easy clips
//! they agree within 0.4 dB, which is the check that they measure the same
//! thing when nothing is blinding either.
//!
//! Also settled, incidentally: with 21 dB of raw separation on the quiet clips,
//! the only way the gate could have been cutting that speech is the floor
//! climbing onto it — which is what was found.
//!
//! # How long the gaps are, 2026-08-15
//!
//! Whether a 1000 ms hold can stand in for a speech detector depends entirely
//! on this, and it had not been measured:
//!
//! ```text
//!     clip                   n     p50     p90     max  under 1s time in <1s
//!     singing              127      10      30     110      100%       100%
//!     speech-phrases       126      20     150     960      100%       100%
//!     voice-over-motor      32      20     150    2690       94%        25%
//! ```
//!
//! **Read the median as a consonant, not a pause.** The labeller marks a block
//! voiced at 0.75 harmonicity, and an unvoiced consonant inside a word drops
//! below that, so most of these "gaps" are 10 to 30 ms of `s` or `f`. What
//! matters is the tail.
//!
//! By that reading the hold is generously sized for pauses *within* speech:
//! the longest gap in the sung clip is 110 ms and in the spoken one 960 ms, so
//! every gap in both fits inside 1000 ms. The motor clip has two that do not,
//! up to 2.69 s, and although they are only 6% of the gaps they hold 75% of the
//! quiet time — those are silences *between* phrases rather than pauses inside
//! one.
//!
//! The consequence for anything latched at speech and held: a hold alone
//! covers every intra-phrase pause measured here, and does not cover
//! inter-phrase silence. Something that must survive silence has to be latched
//! rather than held.
//!
//! # A caution about `Off`
//!
//! With the enhancer in front, `Off` — no profile suppression at all — puts far
//! more on the wire than any profile:
//!
//! ```text
//!     singing            Off 86.0%   Light 54.9%   Standard 54.8%   Helmet 54.8%
//!     speech             Off 96.1%   Light 65.6%   Standard 61.9%   Helmet 62.6%
//!     voice-over-motor   Off 99.8%   Light 31.4%   Standard 23.9%   Helmet 30.4%
//! ```
//!
//! **99.8% on the motor clip is not a good result, it is the absence of a
//! gate.** A transmitted share cannot tell a chain that sends the right blocks
//! from one that sends everything, and none of these clips carries labels. The
//! figure is here because it is the one number this harness can produce without
//! them, and it is exactly the number that would flatter a broken chain.

use std::fs;
use std::path::PathBuf;

use mumbleway_core::audio::denoise::{CaptureProcessor, FRAME_SIZE};
use mumbleway_core::audio::dsp::{rms, to_dbfs, RumbleFilter, SpeechBand};
use mumbleway_core::audio::pitch::PitchTracker;
use mumbleway_core::audio::{Enhancer, NoiseProfile};

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

/// The enhancer, built the way the worker builds it.
///
/// **Everything this file measures was measured without it until 2026-08-15**,
/// which made every absolute number here describe a chain that does not ship.
/// `engine.rs` runs `Enhancer::process` immediately before
/// `CaptureProcessor::suppress`, and DeepFilterNet takes 7 to 11 dB out of the
/// speech before the suppressor sees it — so a floor, a level or a threshold
/// measured without it is measured against a different signal.
///
/// Always the full model here, never the cheap one: this is a measurement rig
/// on a desktop, not a phone deciding what it can afford, and the performance
/// ladder's choice would make one run incomparable with the next.
fn enhancer_for_measurement() -> Enhancer {
    Enhancer::new()
}

fn run(profile: NoiseProfile, signal: &[f32], spans: &[(f32, f32)]) -> Run {
    let mut chain = CaptureProcessor::new(profile);
    let mut enhancer = enhancer_for_measurement();
    let mut block = [0.0f32; FRAME_SIZE];
    let (mut sent, mut counted) = (0usize, 0usize);
    let (mut level, mut floor) = (0.0f64, 0.0f64);
    // The four cells of the only table that matters.
    let (mut hit, mut miss, mut false_alarm) = (0usize, 0usize, 0usize);

    for (i, chunk) in signal.chunks_exact(FRAME_SIZE).enumerate() {
        block.copy_from_slice(chunk);
        enhancer.process(&mut block);
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
    let mut enhancer = enhancer_for_measurement();
    let mut blk = [0.0f32; FRAME_SIZE];
    let (mut sum, mut n) = (0.0f64, 0usize);
    for chunk in signal.chunks_exact(FRAME_SIZE) {
        blk.copy_from_slice(chunk);
        enhancer.process(&mut blk);
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
    let mut enhancer = enhancer_for_measurement();
    let mut block = [0.0f32; FRAME_SIZE];
    let mut out = Vec::new();
    for (i, chunk) in signal.chunks_exact(FRAME_SIZE).enumerate() {
        block.copy_from_slice(chunk);
        enhancer.process(&mut block);
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
            let mut enhancer = enhancer_for_measurement();
            let mut block = [0.0f32; FRAME_SIZE];
            let mut out: Vec<u8> = Vec::with_capacity(signal.len() * 4);
            for chunk in signal.chunks_exact(FRAME_SIZE) {
                block.copy_from_slice(chunk);
                enhancer.process(&mut block);
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
                let mut enhancer = enhancer_for_measurement();
                let mut blk = [0.0f32; FRAME_SIZE];
                let (mut n, mut vad_ok, mut snr_ok, mut gate_ok, mut sent) = (0, 0, 0, 0, 0);
                for (i, chunk) in signal.chunks_exact(FRAME_SIZE).enumerate() {
                    blk.copy_from_slice(chunk);
                    enhancer.process(&mut blk);
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

/// How far the enhancer pulls the voice away from the gaps.
///
/// **This is the measurement the whole "can a level threshold work" question
/// turns on.** `docs/MUSIC_GATE.md` records six hand-built features failing on
/// music because, with music playing, the gaps between words sat within 1.5 dB
/// of the speech — no threshold on level can separate populations that
/// overlap. DeepFilterNet took that to 16.0 dB, which is what makes a
/// level-derived decision viable at all. That figure came from **one clip**,
/// and the file says so.
///
/// This extends it, and does so **without hand labels**, which the recordings
/// that arrive through the app do not carry. Blocks are split by
/// *periodicity* — the YIN pitch search, which knows nothing about level — and
/// then the level of each population is measured. Using level to label blocks
/// whose level separation is the thing being measured would be circular; using
/// periodicity is not.
///
/// Read the last column. It is the headroom any SNR-onset detector would have
/// to work with.
#[test]
#[ignore = "needs MUMBLEWAY_ROAD_AUDIO"]
fn how_far_the_enhancer_separates_the_voice_from_the_gaps() {
    let clips = clips();
    assert!(!clips.is_empty(), "no clips in MUMBLEWAY_ROAD_AUDIO");

    println!("\nLEVEL SEPARATION, voiced vs the rest (dB)");
    println!(
        "    {:<18} {:<10} {:>5} {:>8} {:>8} {:>7} | {:>8} {:>8} {:>7}",
        "clip", "labeller", "n", "raw voi", "raw gap", "sep", "enh voi", "enh gap", "sep"
    );
    for (name, signal) in &clips {
        // The labeller: periodicity on the untouched signal, so both columns
        // are scored on exactly the same blocks.
        let voiced = periodicity(signal);

        let raw_levels = block_levels(signal);
        let mut enhanced = signal.clone();
        let mut enhancer = enhancer_for_measurement();
        for chunk in enhanced.chunks_exact_mut(FRAME_SIZE) {
            enhancer.process(chunk);
        }
        let enhanced_levels = block_levels(&enhanced);

        // **Two labellers, because the first one can be blinded by the very
        // noise being measured.** On a clip with an engine in it the pitch
        // search finds almost nothing periodic in the raw audio -- the motor
        // masks the harmonics -- so the voiced population collapses to a
        // handful of blocks and a separation computed from it means little.
        //
        // The second labels from periodicity measured on the *enhanced*
        // signal, where the pitch is visible again, and applies that one
        // labelling to both columns so the comparison stays honest. It is the
        // better judge of which blocks are voice; it is also the enhancer's
        // own output, so a block it wrongly silenced would be counted as a gap
        // and would flatter the result. Neither labeller is above suspicion,
        // which is why both are printed with the count they rest on.
        let voiced_enh = periodicity(&enhanced);

        for (label, marks) in [("raw-pitch", &voiced), ("enh-pitch", &voiced_enh)] {
            let n = marks.iter().filter(|v| **v >= 0.75).count();
            let (raw_voiced, raw_rest) = voiced_means_db(marks, &raw_levels);
            let (enh_voiced, enh_rest) = voiced_means_db(marks, &enhanced_levels);
            println!(
                "    {:<18} {:<10} {:>5} {:>8.1} {:>8.1} {:>7.1} | {:>8.1} {:>8.1} {:>7.1}",
                name,
                label,
                n,
                raw_voiced,
                raw_rest,
                raw_voiced - raw_rest,
                enh_voiced,
                enh_rest,
                enh_voiced - enh_rest
            );
        }
    }
    println!("\n    For comparison, the one clip in MUSIC_GATE.md: 1.5 dB raw, 16.0 dB enhanced.");
    // How long the quiet stretches actually are.
    //
    // **This decides whether a hold can stand in for a speech detector.** If
    // every gap is shorter than the hold, a measurement latched at speech and
    // held across the gap never goes stale; if gaps run to seconds, the hold
    // expires inside them and whatever depended on it is guessing again.
    // Pauses inside a phrase and silence between phrases are different animals,
    // and a median hides that, so the distribution is what is printed.
    println!("\nGAPS BETWEEN VOICED RUNS (ms), labelled on the enhanced signal");
    println!(
        "    {:<18} {:>5} {:>7} {:>7} {:>7} {:>9} {:>10}",
        "clip", "n", "p50", "p90", "max", "under 1s", "time in <1s"
    );
    for (name, signal) in &clips {
        let mut enhanced = signal.clone();
        let mut enhancer = enhancer_for_measurement();
        for chunk in enhanced.chunks_exact_mut(FRAME_SIZE) {
            enhancer.process(chunk);
        }
        let voiced = periodicity(&enhanced);
        let mut gaps: Vec<f32> = Vec::new();
        let mut run = 0usize;
        let mut seen_voice = false;
        for v in &voiced {
            if *v >= 0.75 {
                if seen_voice && run > 0 {
                    gaps.push(run as f32 * 10.0);
                }
                seen_voice = true;
                run = 0;
            } else {
                run += 1;
            }
        }
        // Leading and trailing runs are dropped on purpose: they are the ends
        // of the recording, not gaps between two phrases.
        if gaps.is_empty() {
            println!("    {name:<18}     0   (no gap between two voiced runs)");
            continue;
        }
        gaps.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let short = gaps.iter().filter(|g| **g < 1000.0).count();
        let in_short: f32 = gaps.iter().filter(|g| **g < 1000.0).sum();
        let all: f32 = gaps.iter().sum();
        println!(
            "    {:<18} {:>5} {:>7.0} {:>7.0} {:>7.0} {:>8.0}% {:>9.0}%",
            name,
            gaps.len(),
            percentile(&gaps, 0.50),
            percentile(&gaps, 0.90),
            gaps[gaps.len() - 1],
            100.0 * short as f32 / gaps.len() as f32,
            100.0 * in_short / all.max(1.0)
        );
    }
}

/// Per-block level in dBFS, on whatever signal it is handed.
fn block_levels(signal: &[f32]) -> Vec<f32> {
    signal
        .chunks_exact(FRAME_SIZE)
        .map(|c| to_dbfs(rms(c)))
        .collect()
}

/// Mean level of the periodic blocks, and of everything else.
///
/// The bar is the same 0.75 the rest of this file reports against, so the two
/// numbers can be read together. Means rather than percentiles because the
/// question is how far apart the populations sit on average, which is what a
/// fixed threshold between them has to survive.
fn voiced_means_db(voiced: &[f32], levels: &[f32]) -> (f32, f32) {
    let (mut inside, mut outside) = (Vec::new(), Vec::new());
    for (i, level) in levels.iter().enumerate() {
        match voiced.get(i) {
            Some(v) if *v >= 0.75 => inside.push(*level),
            Some(_) => outside.push(*level),
            None => {}
        }
    }
    if inside.is_empty() || outside.is_empty() {
        return (0.0, 0.0);
    }
    let mean = |v: &[f32]| v.iter().sum::<f32>() / v.len() as f32;
    (mean(&inside), mean(&outside))
}
