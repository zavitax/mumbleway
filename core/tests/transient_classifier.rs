//! A proposed transient classifier, scored against the corpus.
//!
//! **Run with `MW_TAP_CORPUS=<dir>`**, like the detector's own rig. Ignored by
//! default; the corpus is a rider's microphone and is not in the repository.
//!
//! The classifier below is **verbatim as proposed**, so what is measured is it
//! and not a paraphrase of it. Only `main` is dropped and the scoring added.
//! Two deviations from the proposal are worth naming because they would
//! otherwise look like results: `packet_size` is stored and never read, and
//! `sample_rate` of the struct is used while the constructor's argument of the
//! same name is what the normalisation divides by — both harmless here, both
//! left alone.
//!
//! What this answers is narrow and worth stating: whether a classifier reading
//! ZCR and a decay ratio over a 35 ms tail separates the taps a rider
//! performed from the music and road noise in the same corpus. It says nothing
//! about whether the audio reaching it is intact — see
//! `acoustic_tap_corpus.rs` and the 66.9% digital silence on the helmet
//! recording, which no classifier can do anything about.

#![allow(dead_code)]

use std::collections::VecDeque;
use std::path::PathBuf;

// ---------------------------------------------------------------- as proposed

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum TransientClass {
    Tap,
    Click,
    Clink,
    Thud,
    None,
}

pub struct StreamingAnalyzer {
    sample_rate: usize,
    packet_size: usize,

    // Time-decoupled history buffer
    history_buffer: VecDeque<f32>,
    history_capacity: usize,

    // Dynamic sample count allocations (calculated at runtime)
    samples_required_lookahead: usize,
    samples_15ms: usize,

    // State machine trackers
    onset_detected: bool,
    samples_since_onset: usize,
    onset_index_in_history: usize,

    // Adaptive thresholds
    energy_jump_threshold: f32,
    prev_packet_energy: f32,

    /// **Instrumentation, not part of the proposal.** The two features the
    /// decision tree reads, kept from the last evaluation so the scoring can
    /// report *why* a transient was classified as it was. The tree below is
    /// untouched.
    last_zcr: f32,
    last_decay: f32,
}

impl StreamingAnalyzer {
    pub fn new(sample_rate: usize, packet_size: usize) -> Self {
        // Support up to 50ms of context regardless of 8kHz or 96kHz sample rates
        let history_duration_ms = 50.0;
        let history_capacity = ((history_duration_ms / 1000.0) * sample_rate as f32) as usize;

        // Dynamically compute temporal look-aheads in absolute sample counts
        let samples_required_lookahead = (0.035 * sample_rate as f32) as usize; // 35ms evaluation tail
        let samples_15ms = (0.015 * sample_rate as f32) as usize; // 15ms decay milestone

        Self {
            sample_rate,
            packet_size,
            history_buffer: VecDeque::with_capacity(history_capacity),
            history_capacity,
            samples_required_lookahead,
            samples_15ms,
            onset_detected: false,
            samples_since_onset: 0,
            onset_index_in_history: 0,
            energy_jump_threshold: 4.5, // ~13dB jump threshold
            prev_packet_energy: 0.0001,
            last_zcr: 0.0,
            last_decay: 0.0,
        }
    }

    /// Pushes streaming frames of arbitrary sizes (handles changing sample/packet sizes seamlessly)
    pub fn process_packet(&mut self, packet: &[f32]) -> TransientClass {
        let mut current_energy = 0.0;

        for &sample in packet {
            current_energy += sample * sample;

            if self.history_buffer.len() >= self.history_capacity {
                self.history_buffer.pop_front();
                if self.onset_detected && self.onset_index_in_history > 0 {
                    self.onset_index_in_history -= 1;
                }
            }
            self.history_buffer.push_back(sample);
        }
        current_energy /= packet.len() as f32;

        // Onset Detection Phase
        if !self.onset_detected {
            let energy_ratio = current_energy / self.prev_packet_energy.max(0.00001);
            // Dynamic absolute amplitude gate prevents background hiss from triggering high rates
            if energy_ratio > self.energy_jump_threshold && current_energy > 0.0003 {
                self.onset_detected = true;
                self.samples_since_onset = 0;
                self.onset_index_in_history =
                    self.history_buffer.len().saturating_sub(packet.len());
            }
        }

        self.prev_packet_energy = current_energy;

        // Feature Aggregation Ticker
        if self.onset_detected {
            self.samples_since_onset += packet.len();

            // Wait until enough physical time (35ms) has elapsed past the onset timestamp
            if self.samples_since_onset >= self.samples_required_lookahead {
                let (result, zcr, decay) = self.evaluate_transient_from_history();
                self.last_zcr = zcr;
                self.last_decay = decay;
                self.onset_detected = false;
                self.samples_since_onset = 0;
                return result;
            }
        }

        TransientClass::None
    }

    fn evaluate_transient_from_history(&self) -> (TransientClass, f32, f32) {
        let history_slice: Vec<f32> = self.history_buffer.iter().copied().collect();
        let start_idx = self.onset_index_in_history;
        let analysis_window = &history_slice[start_idx..];

        if analysis_window.is_empty() {
            return (TransientClass::None, 0.0, 0.0);
        }

        // Isolate exact peak location
        let mut peak_idx = 0;
        let mut max_val = 0.0f32;
        for (i, &sample) in analysis_window.iter().enumerate() {
            if sample.abs() > max_val {
                max_val = sample.abs();
                peak_idx = i;
            }
        }

        if max_val < 0.008 {
            return (TransientClass::None, 0.0, 0.0);
        }

        // 1. Calculate Zero-Crossing Rate (ZCR)
        let mut zero_crossings = 0;
        for pair in analysis_window.windows(2) {
            if (pair[0] >= 0.0 && pair[1] < 0.0) || (pair[0] < 0.0 && pair[1] >= 0.0) {
                zero_crossings += 1;
            }
        }
        let raw_zcr = zero_crossings as f32 / analysis_window.len() as f32;

        // 2. Frequency-Scale Normalization
        // ZCR naturally dilutes as sampling rate climbs. We normalize it to a 44.1kHz reference frame.
        let normalized_zcr = raw_zcr * (44100.0 / self.sample_rate as f32);

        // 3. Compute Decay Ratio using calculated multi-rate step allocations
        let mut peak_energy = 0.0;
        let p_start = peak_idx.saturating_sub((0.0005 * self.sample_rate as f32) as usize); // 0.5ms pre-roll
        let p_end =
            (peak_idx + ((0.0015 * self.sample_rate as f32) as usize)).min(analysis_window.len());
        for &s in &analysis_window[p_start..p_end] {
            peak_energy += s * s;
        }
        peak_energy /= (p_end - p_start) as f32;

        let mut tail_energy = 0.0;
        let t_start = (peak_idx + self.samples_15ms).min(analysis_window.len());
        let t_end = (t_start + self.samples_15ms).min(analysis_window.len());

        let decay_ratio = if t_end > t_start {
            for &s in &analysis_window[t_start..t_end] {
                tail_energy += s * s;
            }
            tail_energy /= (t_end - t_start) as f32;
            tail_energy / peak_energy.max(0.00001)
        } else {
            0.0
        };

        // --- Multi-Rate Decision Tree Profile ---
        // Using Normalized ZCR makes these constants highly robust from 8kHz to 96kHz.
        let class = if normalized_zcr > 0.26 {
            if decay_ratio > 0.28 {
                TransientClass::Clink
            } else {
                TransientClass::Click
            }
        } else if normalized_zcr < 0.05 {
            TransientClass::Thud
        } else if decay_ratio < 0.12 {
            TransientClass::Tap
        } else {
            TransientClass::None
        };
        (class, normalized_zcr, decay_ratio)
    }
}

// ------------------------------------------------------------------- scoring

fn read_s16(path: &PathBuf) -> Vec<f32> {
    let raw = std::fs::read(path).expect("read");
    raw.chunks_exact(2)
        .map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0)
        .collect()
}

/// 16-bit mono PCM out of a `.wav`, by finding its `data` chunk.
fn read_wav(path: &PathBuf) -> Vec<f32> {
    let raw = std::fs::read(path).expect("read");
    let mut at = 12;
    let mut channels = 1usize;
    while at + 8 <= raw.len() {
        let id = &raw[at..at + 4];
        let size =
            u32::from_le_bytes([raw[at + 4], raw[at + 5], raw[at + 6], raw[at + 7]]) as usize;
        let body = at + 8;
        if id == b"fmt " {
            channels = u16::from_le_bytes([raw[body + 2], raw[body + 3]]) as usize;
        } else if id == b"data" {
            let end = (body + size).min(raw.len());
            let all: Vec<f32> = raw[body..end]
                .chunks_exact(2)
                .map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0)
                .collect();
            return if channels > 1 {
                all.chunks(channels)
                    .map(|c| c.iter().sum::<f32>() / c.len() as f32)
                    .collect()
            } else {
                all
            };
        }
        at = body + size + (size & 1);
    }
    panic!("{}: no data chunk", path.display());
}

/// The clicks the audio itself says are there, as 10 ms block indices.
///
/// The same rule `tools/tap/label_from_audio.py` and the detector's own rig
/// use, so both are judged against one ground truth.
fn audible_clicks(pcm: &[f32]) -> Vec<usize> {
    const B: usize = 480;
    let peaks: Vec<f32> = pcm
        .chunks(B)
        .map(|c| c.iter().fold(0.0f32, |m, s| m.max(s.abs())))
        .collect();
    let mut hits: Vec<usize> = Vec::new();
    for i in 100..peaks.len() {
        let mut w: Vec<f32> = peaks[i - 100..i - 2].to_vec();
        w.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let reference = w[w.len() / 2] + 1e-6;
        if peaks[i] > reference * 8.0 && peaks[i] > 0.02 {
            if hits.last().is_some_and(|&l| i - l <= 5) {
                continue;
            }
            hits.push(i);
        }
    }
    hits
}

struct Score {
    counts: [usize; 4],
    /// Audible clicks that produced a `Tap` within 60 ms of the click.
    matched: usize,
    clicks: usize,
    seconds: f64,
}

fn run(pcm: &[f32]) -> Score {
    let clicks = audible_clicks(pcm);
    let mut a = StreamingAnalyzer::new(48_000, 480);
    let mut counts = [0usize; 4];
    // Block index of every `Tap`, for matching against the clicks.
    let mut taps: Vec<usize> = Vec::new();
    for (i, packet) in pcm.chunks(480).enumerate() {
        match a.process_packet(packet) {
            TransientClass::Tap => {
                counts[0] += 1;
                taps.push(i);
            }
            TransientClass::Click => counts[1] += 1,
            TransientClass::Clink => counts[2] += 1,
            TransientClass::Thud => counts[3] += 1,
            TransientClass::None => {}
        }
    }
    // The classifier answers 35 ms after the onset, so a `Tap` lands up to
    // four blocks after the click that caused it. Six blocks of slack.
    let matched = clicks
        .iter()
        .filter(|&&c| taps.iter().any(|&t| t >= c && t - c <= 6))
        .count();
    Score {
        counts,
        matched,
        clicks: clicks.len(),
        seconds: pcm.len() as f64 / 48_000.0,
    }
}

/// The features at every transient, so a verdict can be read rather than
/// guessed at.
fn features(pcm: &[f32], label: &str) {
    let mut a = StreamingAnalyzer::new(48_000, 480);
    let mut rows: Vec<(f32, f32, TransientClass)> = Vec::new();
    for packet in pcm.chunks(480) {
        let c = a.process_packet(packet);
        if c != TransientClass::None {
            rows.push((a.last_zcr, a.last_decay, c));
        }
    }
    if rows.is_empty() {
        println!("  {label:<40} no transients");
        return;
    }
    let med = |mut v: Vec<f32>| {
        v.sort_by(|x, y| x.partial_cmp(y).unwrap());
        v[v.len() / 2]
    };
    let zcr = med(rows.iter().map(|r| r.0).collect());
    let dec = med(rows.iter().map(|r| r.1).collect());
    println!(
        "  {label:<40} n {:>4}   median ZCR {:.3}  decay {:.3}            (Tap wants ZCR 0.05-0.26 and decay < 0.12)",
        rows.len(),
        zcr,
        dec
    );
}

/// The same recording, with the digital silence taken out.
///
/// **This is the confound that has to be removed before the classifier can be
/// blamed.** The helmet recording is 66.9% bit-exact zero in runs of about
/// 120 ms — a session fault, not a microphone one — and a 35 ms analysis
/// window landing across a gap has neither a decay nor a zero-crossing rate
/// worth the name. Scored per contiguous run of real audio, with the analyser
/// reset for each, the question becomes the one actually being asked: given
/// intact audio, does this classifier find a tap on a helmet?
fn runs_of_real_audio(pcm: &[f32]) -> Vec<Vec<f32>> {
    const B: usize = 480;
    let mut out: Vec<Vec<f32>> = Vec::new();
    let mut current: Vec<f32> = Vec::new();
    for block in pcm.chunks(B) {
        if block.iter().all(|s| *s == 0.0) {
            if current.len() >= B * 10 {
                out.push(std::mem::take(&mut current));
            } else {
                current.clear();
            }
        } else {
            current.extend_from_slice(block);
        }
    }
    if current.len() >= B * 10 {
        out.push(current);
    }
    out
}

#[test]
#[ignore = "needs MW_TAP_CORPUS pointing at recordings with audible taps"]
fn the_features_behind_each_verdict() {
    let dir = PathBuf::from(std::env::var("MW_TAP_CORPUS").expect("MW_TAP_CORPUS"));
    println!(
        "median features at every transient the classifier judged
"
    );
    for (stem, label) in [
        ("20261011-1109-000", "phone body (works: 120/161)"),
        ("20261011-1638-000", "HELMET (fails: 0/24)"),
        ("20261009-1609-000", "hand-held"),
        ("20261009-1611-000", "desk"),
    ] {
        let p = dir.join(format!("{stem}.s16"));
        if p.exists() {
            features(&read_s16(&p), label);
        }
    }
    if let Some(p) = dir
        .parent()
        .map(|p| p.join("music_shipping").join("music_only__input.wav"))
        .filter(|p| p.exists())
    {
        features(&read_wav(&p), "REAL MUSIC (no taps)");
    }

    // And the helmet again, with the session fault's silence removed.
    let helmet = dir.join("20261011-1638-000.s16");
    if helmet.exists() {
        let pcm = read_s16(&helmet);
        let runs = runs_of_real_audio(&pcm);
        let total: usize = runs.iter().map(|r| r.len()).sum();
        println!(
            "
helmet, silence excised: {} runs, {:.1} s of {:.1} s kept",
            runs.len(),
            total as f64 / 48_000.0,
            pcm.len() as f64 / 48_000.0
        );
        let (mut clicks, mut taps) = (0, 0);
        for r in &runs {
            let s = run(r);
            clicks += s.clicks;
            taps += s.counts[0];
        }
        println!("  {clicks} audible clicks in those runs, {taps} classified Tap");
        let joined: Vec<f32> = runs.concat();
        features(&joined, "HELMET, intact audio only");
    }
}

#[test]
#[ignore = "needs MW_TAP_CORPUS pointing at recordings with audible taps"]
fn the_proposed_classifier_against_the_corpus() {
    let dir = PathBuf::from(std::env::var("MW_TAP_CORPUS").expect("MW_TAP_CORPUS"));

    println!(
        "{:<42} {:>6} {:>8} {:>7} {:>7} {:>7} {:>7}",
        "", "sec", "clicks", "Tap", "Click", "Clink", "Thud"
    );
    println!("{}", "-".repeat(90));

    let mut rides: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("corpus dir")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "s16"))
        .collect();
    rides.sort();

    for path in rides {
        let pcm = read_s16(&path);
        if pcm.len() < 48_000 {
            continue;
        }
        let s = run(&pcm);
        if s.clicks == 0 && s.counts.iter().all(|&c| c == 0) {
            continue;
        }
        let name = path.file_stem().unwrap().to_string_lossy().to_string();
        let note = std::fs::read_to_string(path.with_extension("note"))
            .ok()
            .and_then(|t| t.lines().last().map(|l| l.trim().to_string()))
            .unwrap_or_default();
        println!(
            "{:<42} {:>6.1} {:>8} {:>7} {:>7} {:>7} {:>7}",
            format!("{name} {note}"),
            s.seconds,
            s.clicks,
            s.counts[0],
            s.counts[1],
            s.counts[2],
            s.counts[3],
        );
        println!(
            "{:<42} {:>6} of the audible clicks produced a Tap",
            "",
            format!("{}/{}", s.matched, s.clicks)
        );
    }

    // The negative side: real music through a real microphone, with no taps
    // in it, so every `Tap` here is a false positive and needs no labelling.
    if let Some(p) = dir
        .parent()
        .map(|p| p.join("music_shipping").join("music_only__input.wav"))
        .filter(|p| p.exists())
    {
        let pcm = read_wav(&p);
        let s = run(&pcm);
        println!(
            "\n{:<42} {:>6.1} {:>8} {:>7} {:>7} {:>7} {:>7}",
            "music_only__input  (REAL MUSIC, NO TAPS)",
            s.seconds,
            s.clicks,
            s.counts[0],
            s.counts[1],
            s.counts[2],
            s.counts[3],
        );
        println!(
            "  every Tap here is a false positive: {:.0} per hour",
            s.counts[0] as f64 * 3600.0 / s.seconds
        );
    }
}

/// Where the `Tap` band's lower edge should sit, measured.
///
/// **The proposal's 0.05 is above every helmet tap in the corpus**, which is
/// why it answered `Thud` to all twenty-four of them while answering `Tap` to
/// 120 of 161 taps on the phone's body. That is not a mistake in the feature,
/// it is the physics the band measurement already recorded: a phone-body tap
/// is 93% between 1 and 4 kHz and a tap felt through a helmet is a thud under
/// 500 Hz. The crossing rate says the same thing in one number — 0.077 against
/// 0.021 — and the edge was drawn between them.
///
/// Real music is lower still, at 0.005, so there is room below the helmet and
/// above the music. This finds how much.
#[test]
#[ignore = "needs MW_TAP_CORPUS pointing at recordings with audible taps"]
fn where_the_tap_bands_lower_edge_belongs() {
    let dir = PathBuf::from(std::env::var("MW_TAP_CORPUS").expect("MW_TAP_CORPUS"));

    /// Every transient, as (zcr, decay), which is all the tree reads.
    fn judged(pcm: &[f32]) -> Vec<(f32, f32)> {
        let mut a = StreamingAnalyzer::new(48_000, 480);
        let mut out = Vec::new();
        for packet in pcm.chunks(480) {
            if a.process_packet(packet) != TransientClass::None {
                out.push((a.last_zcr, a.last_decay));
            }
        }
        out
    }

    // The tree with only the lower edge moved; everything else as proposed.
    fn is_tap(zcr: f32, decay: f32, floor: f32) -> bool {
        zcr <= 0.26 && zcr >= floor && decay < 0.12
    }

    let load = |stem: &str| {
        let p = dir.join(format!("{stem}.s16"));
        p.exists().then(|| judged(&read_s16(&p)))
    };
    let helmet = load("20261011-1638-000").unwrap_or_default();
    let phone = load("20261011-1109-000").unwrap_or_default();
    let desk = load("20261009-1611-000").unwrap_or_default();
    let music = dir
        .parent()
        .map(|p| p.join("music_shipping").join("music_only__input.wav"))
        .filter(|p| p.exists())
        .map(|p| judged(&read_wav(&p)))
        .unwrap_or_default();
    let music_seconds = 138.8;

    println!(
        "{:>7}  {:>14}  {:>14}  {:>12}  {:>18}",
        "floor", "helmet taps", "phone taps", "desk taps", "MUSIC false/hour"
    );
    for floor in [0.050f32, 0.040, 0.030, 0.025, 0.020, 0.015, 0.010] {
        let c = |v: &[(f32, f32)]| v.iter().filter(|(z, d)| is_tap(*z, *d, floor)).count();
        let m = c(&music);
        println!(
            "{floor:>7.3}  {:>5} of {:<5}  {:>5} of {:<5}  {:>4} of {:<4}  {:>8.0} per hour",
            c(&helmet),
            helmet.len(),
            c(&phone),
            phone.len(),
            c(&desk),
            desk.len(),
            m as f64 * 3600.0 / music_seconds,
        );
    }
    println!(
        "\n(the proposal is the 0.050 row; 'of N' is the transients the \
         classifier judged at all)"
    );
}

/// The decay ratio on its own, which is the proposal's strongest part.
///
/// Worth separating from the crossing rate because the two behave quite
/// differently on this corpus: the rate cannot be retuned to admit a helmet
/// without admitting the music with it, while the decay is 0.002 on a helmet
/// tap against 0.156 on music — the right way round, and by a factor of
/// seventy-five. This asks whether that survives the distributions, which is
/// the question the medians cannot answer and the crossing rate failed.
#[test]
#[ignore = "needs MW_TAP_CORPUS pointing at recordings with audible taps"]
fn the_decay_ratio_on_its_own() {
    let dir = PathBuf::from(std::env::var("MW_TAP_CORPUS").expect("MW_TAP_CORPUS"));
    fn decays(pcm: &[f32]) -> Vec<f32> {
        let mut a = StreamingAnalyzer::new(48_000, 480);
        let mut out = Vec::new();
        for packet in pcm.chunks(480) {
            if a.process_packet(packet) != TransientClass::None {
                out.push(a.last_decay);
            }
        }
        out
    }
    let load = |stem: &str| {
        let p = dir.join(format!("{stem}.s16"));
        if p.exists() {
            decays(&read_s16(&p))
        } else {
            Vec::new()
        }
    };
    let helmet = load("20261011-1638-000");
    let phone = load("20261011-1109-000");
    let music = dir
        .parent()
        .map(|p| p.join("music_shipping").join("music_only__input.wav"))
        .filter(|p| p.exists())
        .map(|p| decays(&read_wav(&p)))
        .unwrap_or_default();

    println!(
        "{:>8}  {:>14}  {:>14}  {:>18}",
        "decay <", "helmet", "phone", "MUSIC false/hour"
    );
    for t in [0.12f32, 0.08, 0.05, 0.03, 0.02, 0.01] {
        let c = |v: &[f32]| v.iter().filter(|d| **d < t).count();
        println!(
            "{t:>8.2}  {:>5} of {:<5}  {:>5} of {:<5}  {:>8.0} per hour",
            c(&helmet),
            helmet.len(),
            c(&phone),
            phone.len(),
            c(&music) as f64 * 3600.0 / 138.8,
        );
    }
}

/// Does the decay ratio separate speech from taps?
///
/// **The question report 4 turns on.** A plosive at a boom microphone is an
/// impact — the attack test cannot see a difference and should not be expected
/// to, because there is none. What differs is what follows: a tap is followed
/// by nothing, a consonant by its vowel.
///
/// The first attempt at that was "the level must not come back over the margin
/// for 90 ms", and the corpus threw it out: real taps ring, the ringing crosses
/// the margin, and recall on two of the three tap recordings went to zero. So
/// the question becomes whether the *ratio* of tail energy to peak energy —
/// the one feature of the proposed classifier that scored well, 21 of 21 on
/// helmet taps — separates speech too, rather than only music.
#[test]
#[ignore = "needs MW_TAP_CORPUS pointing at recordings with audible taps"]
fn whether_decay_separates_speech_from_taps() {
    let dir = PathBuf::from(std::env::var("MW_TAP_CORPUS").expect("MW_TAP_CORPUS"));
    fn decays(pcm: &[f32]) -> Vec<f32> {
        let mut a = StreamingAnalyzer::new(48_000, 480);
        let mut out = Vec::new();
        for packet in pcm.chunks(480) {
            if a.process_packet(packet) != TransientClass::None {
                out.push(a.last_decay);
            }
        }
        out
    }
    let spread = |v: &[f32]| {
        if v.is_empty() {
            return "none".to_string();
        }
        let mut v = v.to_vec();
        v.sort_by(|x, y| x.partial_cmp(y).unwrap());
        let at = |q: f64| v[((v.len() - 1) as f64 * q) as usize];
        format!(
            "n {:>4}  p5 {:.3}  med {:.3}  p95 {:.3}",
            v.len(),
            at(0.05),
            at(0.50),
            at(0.95)
        )
    };

    println!("decay ratio: tail energy over peak energy, per transient\n");
    for (stem, what) in [
        ("20261011-1109-000", "TAPS on the phone body"),
        ("20261011-1638-000", "TAPS on the helmet"),
        ("20261009-1609-000", "TAPS in the hand"),
        ("20260815-0622-000", "voice with motor noise"),
        ("20260814-2149-000", "singing"),
        ("20260815-2155-000", "voice, low room noise"),
        ("20260815-2156-000", "voice over motorcycle noise"),
        ("20260809-1201-000", "voice"),
    ] {
        let p = dir.join(format!("{stem}.s16"));
        if p.exists() {
            println!("  {:<30} {}", what, spread(&decays(&read_s16(&p))));
        }
    }
}
