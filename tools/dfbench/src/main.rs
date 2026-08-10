//! What one DeepFilterNet frame costs, on the device that has to run it.
//!
//! # Why this exists as its own binary
//!
//! The enhancer switches itself off on a low-end Android phone and the only
//! evidence for *why* was a red dot. Everything before this was measured on a
//! desktop x86 and scaled by guesswork — and the guesses disagreed with each
//! other by a factor of four, which is the difference between "ship a
//! threshold change" and "port to a different inference engine".
//!
//! A Play release is the only way to get a change onto the physical device
//! here (Play Protect refuses a locally built APK), which makes measuring
//! through the app a half-hour round trip per number. **This is pure Rust and
//! needs no APK at all**: build for `aarch64-linux-android`, `adb push`, run.
//! The phone answers in a minute.
//!
//! ```text
//! adb push dfbench /data/local/tmp/ && adb shell chmod 755 /data/local/tmp/dfbench
//! adb push clip.raw /data/local/tmp/
//! adb shell /data/local/tmp/dfbench /data/local/tmp/clip.raw
//! ```
//!
//! The clip is 48 kHz mono `f32` little-endian — the `.raw` the intake bot
//! writes beside every ride.

use std::time::Instant;

use anyhow::{Context, Result};
use df::tract::{DfParams, DfTract, RuntimeParams};
use ndarray::{Array2, ArrayView2, ArrayViewMut2};

/// The chain's block, and the model's frame. They are the same 480 samples,
/// which is the whole reason this model was chosen.
const HOP: usize = 480;

/// Matched to `core/src/audio/deepfilter.rs`. A benchmark of different
/// settings than the ones that ship measures the wrong thing.
const ATTEN_LIM_DB: f32 = 24.0;
const MIN_DB: f32 = -10.0;
const MAX_ERB_DB: f32 = 30.0;
const MAX_DF_DB: f32 = 20.0;

/// Speech and gap energy, so a cheaper setting can be judged rather than
/// assumed.
///
/// **The whole case for this model is a number**: with music playing the gaps
/// between words sit 1.5 dB below the speech, and DeepFilterNet turns that into
/// 16.0 dB. Any change that makes it cheaper has to be measured against the
/// same number, or it is just a change that makes it worse faster.
#[derive(Default)]
struct Separation {
    speech: f64,
    speech_n: u64,
    gap: f64,
    gap_n: u64,
}

impl Separation {
    fn push(&mut self, block: &[f32], speaking: bool) {
        let energy: f64 = block.iter().map(|s| (*s as f64) * (*s as f64)).sum();
        if speaking {
            self.speech += energy;
            self.speech_n += block.len() as u64;
        } else {
            self.gap += energy;
            self.gap_n += block.len() as u64;
        }
    }

    fn db(sum: f64, n: u64) -> f32 {
        if n == 0 {
            return f32::NAN;
        }
        20.0 * ((sum / n as f64).sqrt() + 1e-12).log10() as f32
    }

    fn report(&self) -> (f32, f32, f32) {
        let s = Self::db(self.speech, self.speech_n);
        let g = Self::db(self.gap, self.gap_n);
        (s, g, s - g)
    }

    fn known(&self) -> bool {
        self.speech_n > 0 && self.gap_n > 0
    }
}

/// The `speaking` column of the decision log beside a ride.
///
/// The detector's own opinion rather than hand labels — which is what
/// `docs/MUSIC_GATE.md` used as well, and is the only labelling that exists
/// for every recording in the corpus. Columns are found by name: two were
/// added on the end after these files were written.
fn speaking_labels(path: &str) -> Result<Vec<bool>> {
    let text = std::fs::read_to_string(path).with_context(|| format!("could not read {path}"))?;
    let mut at = None;
    let mut out = Vec::new();
    for line in text.lines() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let parts: Vec<&str> = line.split(',').collect();
        if parts[0].parse::<u64>().is_err() {
            at = parts.iter().position(|c| *c == "speaking");
            continue;
        }
        let Some(i) = at else { continue };
        if let Some(v) = parts.get(i) {
            out.push(*v == "1");
        }
    }
    anyhow::ensure!(!out.is_empty(), "no rows in {path}");
    Ok(out)
}

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let path = args.next();

    // Thresholds can be overridden on the command line, because the whole
    // point of running this on the phone is to try them there: `MAX_DF_DB`
    // decides how often the 19.2 MB decoder runs, and that is the only lever
    // on cost that does not change inference engine.
    let mut min_db = MIN_DB;
    let mut max_erb_db = MAX_ERB_DB;
    let mut max_df_db = MAX_DF_DB;
    let mut labels: Option<String> = None;
    let rest: Vec<String> = args.collect();
    let mut i = 0;
    while i < rest.len() {
        let value = || -> Result<f32> {
            rest.get(i + 1)
                .context("a threshold flag needs a number after it")?
                .parse()
                .context("thresholds are in dB")
        };
        match rest[i].as_str() {
            "--min-db" => min_db = value()?,
            "--max-erb-db" => max_erb_db = value()?,
            "--max-df-db" => max_df_db = value()?,
            "--log" => {
                labels = Some(
                    rest.get(i + 1)
                        .context("--log needs the .csv beside the clip")?
                        .clone(),
                )
            }
            other => anyhow::bail!("unknown flag {other}"),
        }
        i += 2;
    }

    let audio: Vec<f32> = match path {
        Some(p) => {
            let bytes = std::fs::read(&p).with_context(|| format!("could not read {p}"))?;
            println!("clip: {p}");
            bytes
                .chunks_exact(4)
                .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                .collect()
        }
        None => {
            // Enough to show the shape, and nothing like a helmet at speed.
            println!("clip: synthetic (pass a 48 kHz mono f32 .raw for a real one)");
            (0..HOP * 1000)
                .map(|i| 0.2 * (i as f32 * 0.03).sin() + 0.02 * ((i * 7) as f32).sin())
                .collect()
        }
    };

    println!(
        "thresholds: min {min_db} dB  max-erb {max_erb_db} dB  max-df {max_df_db} dB"
    );

    let built = Instant::now();
    let params = RuntimeParams::default_with_ch(1)
        .with_atten_lim(ATTEN_LIM_DB)
        .with_thresholds(min_db, max_erb_db, max_df_db);
    let mut model = DfTract::new(DfParams::default(), &params)?;
    anyhow::ensure!(
        model.hop_size == HOP,
        "hop is {} samples, the chain's block is {HOP}",
        model.hop_size
    );
    println!(
        "model: loaded in {} ms, sr {} hop {} fft {} lookahead {}",
        built.elapsed().as_millis(),
        model.sr,
        model.hop_size,
        model.fft_size,
        model.lookahead
    );

    let speaking = match &labels {
        Some(p) => Some(speaking_labels(p)?),
        None => None,
    };

    let mut noisy = Array2::<f32>::zeros((1, HOP));
    let mut enhanced = Array2::<f32>::zeros((1, HOP));
    let mut per_frame = Vec::with_capacity(audio.len() / HOP);
    let mut lsnrs = Vec::with_capacity(audio.len() / HOP);
    let mut before = Separation::default();
    let mut after = Separation::default();

    for (i, chunk) in audio.chunks_exact(HOP).enumerate() {
        noisy.as_slice_mut().expect("contiguous").copy_from_slice(chunk);
        let t0 = Instant::now();
        let view: ArrayView2<f32> = noisy.view();
        let out: ArrayViewMut2<f32> = enhanced.view_mut();
        let lsnr = model.process(view, out)?;
        per_frame.push(t0.elapsed().as_micros() as f32 / 1000.0);
        lsnrs.push(lsnr);

        // The model has no look-ahead, so frame `i` out is frame `i` in and
        // the label lines up without an offset. That is not true of the
        // standard DFN3 and is one of the reasons this variant was chosen.
        if let Some(labels) = &speaking {
            let Some(&talking) = labels.get(i) else { continue };
            before.push(chunk, talking);
            after.push(enhanced.as_slice().expect("contiguous"), talking);
        }
    }

    // The first few frames carry one-off setup.
    per_frame.drain(..5.min(per_frame.len()));
    anyhow::ensure!(!per_frame.is_empty(), "the clip is shorter than one frame");

    let mut sorted = per_frame.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let pct = |p: f32| sorted[((sorted.len() - 1) as f32 * p) as usize];
    let mean = per_frame.iter().sum::<f32>() / per_frame.len() as f32;

    println!(
        "{} frames  mean {:.2} ms  p50 {:.2}  p95 {:.2}  p99 {:.2}  worst {:.2}",
        per_frame.len(),
        mean,
        pct(0.50),
        pct(0.95),
        pct(0.99),
        sorted[sorted.len() - 1]
    );

    // Which of the four `apply_stages` paths the clip takes, and what each one
    // costs *here*. The split is the whole argument: the DF decoder carries
    // 19.2 MB of the model's 20 MB, so a frame that skips it is a different
    // order of work, and on a phone the difference decides whether the
    // enhancer can run at all.
    let mut buckets: [(u32, f32); 4] = [(0, 0.0); 4];
    for (&l, &ms) in lsnrs.iter().skip(5).zip(&per_frame) {
        let which = if l < min_db {
            0 // zero mask, no decoder
        } else if l > max_erb_db {
            1 // untouched
        } else if l > max_df_db {
            2 // ERB decoder only
        } else {
            3 // both, the expensive one
        };
        buckets[which].0 += 1;
        buckets[which].1 += ms;
    }
    let names = ["zero-mask", "untouched", "erb-only", "both decoders"];
    let n = per_frame.len() as f32;
    println!("stage            share    mean ms");
    for (i, (count, total)) in buckets.iter().enumerate() {
        if *count == 0 {
            continue;
        }
        println!(
            "  {:<14} {:5.1}%   {:6.2}",
            names[i],
            100.0 * *count as f32 / n,
            total / *count as f32
        );
    }

    let over = per_frame.iter().filter(|&&ms| ms > 10.0).count();
    println!(
        "over the 10 ms block budget: {} frames ({:.1}%)",
        over,
        100.0 * over as f32 / n
    );

    // What the cheaper setting cost. Printed beside the timing on purpose:
    // the two numbers are only meaningful together, and reporting a speed-up
    // without the separation beside it is how a change that makes everything
    // quieter gets mistaken for a change that makes speech clearer.
    if before.known() && after.known() {
        let (bs, bg, bsep) = before.report();
        let (as_, ag, asep) = after.report();
        println!("separation       speech      gaps    speech-to-gap");
        println!("  microphone   {bs:7.1}   {bg:7.1}      {bsep:6.1} dB");
        println!("  enhanced     {as_:7.1}   {ag:7.1}      {asep:6.1} dB");
        println!(
            "  the enhancer takes {:.1} dB out of the gaps and {:.1} dB out of the speech",
            bg - ag,
            bs - as_
        );
    } else if labels.is_some() {
        println!("separation: the log has no blocks of one kind or the other");
    }

    Ok(())
}
