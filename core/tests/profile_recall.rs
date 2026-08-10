//! Does a profile carry a whole word, or chop it into pieces?
//!
//! **Written for a complaint the panel could describe but not quantify.** A
//! rider listening through the chain reported Helmet cutting into words, with
//! the *Voice detected* dot flapping between red and amber — which is the two
//! halves of the transmit decision disagreeing, block after block, and never
//! both agreeing at once.
//!
//! `chain_cost` answers what a block costs. This answers what a profile keeps,
//! over the same real ride, with the enhancer in front of it exactly as it
//! ships — which matters, because every one of these thresholds was tuned
//! against a raw microphone and DeepFilterNet now takes 7 to 11 dB out of the
//! speech before any of them sees it.
//!
//! ```text
//! set MW_CLIP=C:\ml_data\rides\20260810-1006-000.raw
//! cargo test --release --test profile_recall -- --ignored --nocapture
//! ```
//!
//! # What "cuts into words" looks like in numbers
//!
//! Not a lower share of speech — a profile that transmits 30% in one run and a
//! profile that transmits 30% in forty runs sound completely different, and
//! only the second one chops. So the shape of the runs is the measurement:
//! **more runs, shorter, is chopping**, and the drop-outs inside a phrase are
//! what a listener hears as a word being cut.
//!
//! Ignored because it is a measurement rather than an assertion.

use mumbleway_core::audio::deepfilter::Enhancer;
use mumbleway_core::audio::denoise::{CaptureProcessor, NoiseProfile, FRAME_SIZE};

#[test]
#[ignore]
fn profile_recall() {
    let audio: Vec<f32> = match std::env::var("MW_CLIP") {
        Ok(path) => {
            let bytes = std::fs::read(&path).expect("could not read MW_CLIP");
            eprintln!("clip: {path}\n");
            bytes
                .chunks_exact(4)
                .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                .collect()
        }
        Err(_) => panic!("set MW_CLIP to a 48 kHz mono f32 .raw from the corpus"),
    };

    // The `speaking` column of the log beside the clip, when there is one.
    //
    // **Not ground truth** — it is what the chain decided on the day, with
    // whatever profile was in force — but it is the only labelling that exists
    // for every ride in the corpus, and it is what the earlier tuning was
    // judged against. Used here to answer the question a recall number cannot:
    // whether extra transmitted blocks are the words that were being cut, or
    // the music that was being kept out.
    let labels: Option<Vec<bool>> = std::env::var("MW_CLIP").ok().and_then(|p| {
        let csv = p.replace(".raw", ".csv");
        let text = std::fs::read_to_string(&csv).ok()?;
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
            let i = at?;
            out.push(parts.get(i).is_some_and(|v| *v == "1"));
        }
        (!out.is_empty()).then_some(out)
    });
    if labels.is_some() {
        eprintln!("labels: the log's own `speaking` column\n");
    }

    println!(
        "{:<10} {:>7} {:>7} {:>7} {:>6} {:>6} {:>8} {:>7} {:>7}",
        "profile", "speak%", "both%", "veto%", "runs", "med", "gate-lvl", "recall", "prec"
    );

    for profile in [
        NoiseProfile::Light,
        NoiseProfile::Standard,
        NoiseProfile::Helmet,
    ] {
        // A fresh chain per profile, and a fresh enhancer with it. Both adapt,
        // and carrying either across would measure the previous profile's
        // noise floor as much as this one's thresholds.
        let mut enhancer = Enhancer::new();
        let mut processor = CaptureProcessor::new(profile);

        let mut speaking: Vec<bool> = Vec::with_capacity(audio.len() / FRAME_SIZE);
        let (mut vad_only, mut snr_only, mut both) = (0u32, 0u32, 0u32);
        let mut speech_energy = 0.0f64;
        let mut speech_blocks = 0u64;
        // Blocks the transmit decision accepted and the noise gate then threw
        // away on its own absolute threshold, and the level it judged them by.
        let mut vetoed = 0u32;
        let mut decided_level = 0.0f64;
        let mut decided = 0u64;

        for chunk in audio.chunks_exact(FRAME_SIZE) {
            let mut block = chunk.to_vec();
            enhancer.process(&mut block);
            let a = processor.process(&mut block);

            if a.vad_says_speech && a.snr_says_speech {
                both += 1;
                decided_level += a.level_db as f64;
                decided += 1;
                if !a.gate_open {
                    vetoed += 1;
                }
            } else if a.vad_says_speech {
                vad_only += 1;
            } else if a.snr_says_speech {
                snr_only += 1;
            }
            if a.speaking {
                let e: f64 = block.iter().map(|s| (*s as f64) * (*s as f64)).sum();
                speech_energy += e / block.len() as f64;
                speech_blocks += 1;
            }
            speaking.push(a.speaking);
        }

        // Runs of transmission, which is where chopping shows.
        let mut runs: Vec<usize> = Vec::new();
        let mut run = 0usize;
        for &s in &speaking {
            if s {
                run += 1;
            } else if run > 0 {
                runs.push(run);
                run = 0;
            }
        }
        if run > 0 {
            runs.push(run);
        }
        runs.sort_unstable();
        let median = runs.get(runs.len() / 2).copied().unwrap_or(0);

        // Drop-outs *inside* a phrase: a single closed block with speech
        // either side of it. This is the shape of a word being cut, as
        // distinct from the gate closing at the end of a sentence.
        let gaps = speaking
            .windows(3)
            .filter(|w| w[0] && !w[1] && w[2])
            .count();

        let n = speaking.len() as f32;
        let level = if speech_blocks > 0 {
            20.0 * ((speech_energy / speech_blocks as f64).sqrt() + 1e-12).log10()
        } else {
            f64::NAN
        };

        let _ = (gaps, vad_only, snr_only, level);

        // Recall: of the blocks labelled speech, how many go out. Precision:
        // of the blocks that go out, how many were labelled speech. Recall
        // alone is the number that made this look fixed; precision is what
        // says whether the music came with it.
        let (mut hit, mut labelled, mut sent) = (0u32, 0u32, 0u32);
        if let Some(labels) = &labels {
            for (i, &s) in speaking.iter().enumerate() {
                let want = labels.get(i).copied().unwrap_or(false);
                if want {
                    labelled += 1;
                }
                if s {
                    sent += 1;
                }
                if s && want {
                    hit += 1;
                }
            }
        }
        let pct = |a: u32, b: u32| {
            if b > 0 {
                100.0 * a as f32 / b as f32
            } else {
                f32::NAN
            }
        };

        println!(
            "{:<10} {:>6.1}% {:>6.1}% {:>6.1}% {:>6} {:>6} {:>8.1} {:>6.1}% {:>6.1}%",
            format!("{profile:?}"),
            100.0 * speaking.iter().filter(|s| **s).count() as f32 / n,
            100.0 * both as f32 / n,
            if both > 0 {
                100.0 * vetoed as f32 / both as f32
            } else {
                0.0
            },
            runs.len(),
            median,
            if decided > 0 {
                decided_level / decided as f64
            } else {
                f64::NAN
            },
            pct(hit, labelled),
            pct(hit, sent),
        );
    }

    println!(
        "\nspeak% is what would go out; vad%/snr% are the two halves of the\n\
         decision and both% is where they agree, which is the only place\n\
         speech is transmitted. runs/med are the shape of it in 10 ms blocks,\n\
         and gaps counts single closed blocks with speech either side -- the\n\
         shape of a word being cut rather than a sentence ending."
    );
}
