//! Does the leading consonant survive the chain?
//!
//! A rider reported word starts on "p", "sh" and "ch" being swallowed, and
//! heard it through the listen sheet's chain playback — which runs the
//! *processing* and not the transmit envelope. That distinction turns out to
//! matter, because of where the look-ahead sits.
//!
//! In `run_worker` the order is:
//!
//! ```text
//! enhancer.process(&mut block);                       // <- the answer
//! processor.process_with_reference(&mut block, ..);   // <- the gate's gain
//! ...
//! onset_delay.shift(&mut block);                      // <- the look-ahead
//! let allowed = ..                                    // <- transmit envelope
//! ```
//!
//! # The gate was the obvious suspect and it is innocent
//!
//! The noise gate applies its gain *inside* the processor and the look-ahead
//! is applied after it, so the delay line receives audio the gate has already
//! judged. That reads like the whole explanation: `voice_active` is false
//! before the detector fires, the gate is fed −120 dB, and delaying a silenced
//! block cannot bring it back.
//!
//! **Measured, it is wrong.** In the 160 ms before an opening the chain's
//! output is 3 to 7 dB *louder* than its input — the gate's release is far too
//! slow to reach zero in that time and the AGC lifts what is left. The gate is
//! not eating word starts.
//!
//! # The enhancer is
//!
//! Comparing the 30 ms before an opening against the 100 ms after it — a
//! leading fricative against the vowel that follows it, same speaker, same
//! conditions, in the band fricatives live in — DeepFilterNet attenuates the
//! first far harder than the second:
//!
//! | Clip | Onset | Vowel | Penalty |
//! |---|---|---|---|
//! | iPhone, "shalom" heard as "alom" | −23.0 dB | −5.8 dB | **−17.2 dB** |
//! | road, quiet background | −23.9 dB | −10.0 dB | −13.9 dB |
//! | voice over loud music | −3.9 dB | −1.5 dB | −2.4 dB |
//!
//! Which is what the model is *for*: an unvoiced consonant is noise-like and
//! pitchless, and at the start of an utterance the model's own SNR estimate is
//! still at the floor because it has been looking at silence. Note the third
//! row — over loud music there is no penalty to fix, because the gap before a
//! word is not quiet either. **The fault needs a quiet background**, which is
//! also where it is safe to fix.
//!
//! **So the look-ahead cannot fix this either**, and for a different reason
//! than the one above: the enhancer runs in front of the delay line as well,
//! so what is delayed has already been stripped. Raising `ONSET_LOOKAHEAD_MS`
//! from 80 to 160 was measured against `transmitting` transitions — two stages
//! downstream of the damage — and does not address this symptom.
//!
//! # Two things this got wrong before it got them right
//!
//! **Broadband energy is too blunt a yardstick.** See [`FRICATIVE_HZ`].
//!
//! **The windows must not come from the run being measured.** Defining an
//! opening by the chain's live `speaking` output means every configuration
//! scores a different set of word starts, because every configuration changes
//! those decisions. It produced a table where more relief scored a *worse*
//! penalty than less. The openings now come from the log beside the clip, which
//! is not ground truth but is at least the same for every row.
//!
//! ```text
//! set MW_CLIP=C:\ml_data\rides\20260810-1849-000.raw
//! set MW_QUIET=-55,-50,-45,-40      :: optional, sweeps the guard
//! cargo test --release --test onset_survival -- --ignored --nocapture
//! ```
//!
//! Ignored because it is a measurement, not an assertion.

use mumbleway_core::audio::deepfilter::Enhancer;
use mumbleway_core::audio::denoise::{CaptureProcessor, NoiseProfile, FRAME_SIZE};
use mumbleway_core::audio::dsp::Biquad;

/// Where the sounds being lost live.
///
/// **The metric, not the fix.** Averaging broadband energy over the blocks
/// before an opening was the first attempt and it is too blunt to tune
/// against: most of that energy is low-frequency background and breath, which
/// the enhancer is *supposed* to remove, and it swamped the thing being
/// measured. "sh", "s", "p" and "ch" are high-frequency by construction, so the
/// question "did the consonant survive" is a question about this band.
const FRICATIVE_HZ: f32 = 3_000.0;

/// Mean power of a block above [`FRICATIVE_HZ`], in linear units.
fn hf(hp: &mut Biquad, block: &[f32]) -> f64 {
    let mut sum = 0.0f64;
    for &s in block {
        let h = hp.process(s) as f64;
        sum += h * h;
    }
    sum / block.len() as f64
}

fn db(sum: f64, n: u64) -> f32 {
    if n == 0 {
        return f32::NAN;
    }
    20.0 * ((sum / n as f64).sqrt() + 1e-12).log10() as f32
}

#[test]
#[ignore]
fn onset_survival() {
    let path = std::env::var("MW_CLIP").expect("set MW_CLIP to a 48 kHz mono f32 .raw");
    let bytes = std::fs::read(&path).expect("could not read MW_CLIP");
    let audio: Vec<f32> = bytes
        .chunks_exact(4)
        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .collect();
    eprintln!("clip: {path}\n");

    // The labels beside the clip, so the cost of protecting the onset can be
    // priced in the same run. A cap that saves the consonant and lets the
    // music back in has not helped anyone.
    let labels: Vec<bool> = {
        let csv = path.replace(".raw", ".csv");
        let mut at = None;
        let mut out = Vec::new();
        if let Ok(text) = std::fs::read_to_string(&csv) {
            for line in text.lines() {
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                let parts: Vec<&str> = line.split(',').collect();
                if parts[0].parse::<u64>().is_err() {
                    at = parts.iter().position(|c| *c == "speaking");
                    continue;
                }
                if let Some(i) = at {
                    out.push(parts.get(i).is_some_and(|v| *v == "1"));
                }
            }
        }
        out
    };

    println!(
        "{:<20} {:>9} {:>12} {:>12} {:>14} {:>12} {:>9}",
        "cap", "openings", "cut: onset", "cut: vowel", "onset penalty", "separation", "relaxed"
    );

    // The flat sweep is the evidence that a lower cap is not the answer — it
    // moves the onset and the vowel together. The guard rows are the same lever
    // pointed only at word starts, swept over where it listens, how big a jump
    // counts, how long it holds and how far it relaxes.
    //
    let mut cases: Vec<(String, f32, Option<Option<f32>>)> = [24.0f32, 12.0, 6.0]
        .iter()
        .map(|a| (format!("{a:.0} dB, flat"), *a, None))
        .collect();
    // `MW_QUIET` sweeps where the guard stops opening because the background is
    // too loud to let back in. Unset, it runs the shipping value only.
    for q in std::env::var("MW_QUIET")
        .ok()
        .map(|s| {
            s.split(',')
                .filter_map(|v| v.trim().parse::<f32>().ok())
                .map(Some)
                .collect::<Vec<_>>()
        })
        .unwrap_or_else(|| vec![None])
    {
        cases.push((
            match q {
                None => "24 dB + guard".to_string(),
                Some(q) => format!("guard, quiet {q:.0}"),
            },
            24.0,
            Some(q),
        ));
    }

    for (label, atten, tuning) in cases {
        let profile = NoiseProfile::Standard;
        let mut enhancer = Enhancer::with_atten_lim(atten);
        enhancer.set_onset_guard(tuning.is_some());
        if let Some(Some(none_db)) = tuning {
            enhancer.set_onset_quiet(none_db - 15.0, none_db);
        }
        let mut processor = CaptureProcessor::new(profile);

        // Per block: the high band at the microphone and after the enhancer —
        // the consonant question — plus the enhancer's broadband output, which
        // is what separation is measured on.
        let mut raw_hf: Vec<f64> = Vec::new();
        let mut enh_hf: Vec<f64> = Vec::new();
        let mut enh_wide: Vec<f64> = Vec::new();
        let mut speaking: Vec<bool> = Vec::new();
        let mut relax: Vec<f32> = Vec::new();

        // Two filters, one per signal, so each keeps its own state — sharing
        // one would run the microphone and the enhanced block through the same
        // delay line alternately and measure neither.
        let mut hp_raw = Biquad::high_pass(48_000.0, FRICATIVE_HZ, 0.707);
        let mut hp_enh = Biquad::high_pass(48_000.0, FRICATIVE_HZ, 0.707);

        for chunk in audio.chunks_exact(FRAME_SIZE) {
            let mut block = chunk.to_vec();
            // The microphone, before anything. An unvoiced consonant is
            // noise-like by construction, which is exactly what a speech
            // enhancer is trained to remove -- so this stage has to be visible
            // separately or its effect is invisible.
            let raw = hf(&mut hp_raw, &block);
            enhancer.process(&mut block);
            relax.push(enhancer.onset_relax());
            // What the gate is about to judge, before it has touched it. The
            // consonant question is asked of the high band; separation stays
            // broadband, because that is the number the model was adopted for
            // and changing its definition mid-investigation would make every
            // earlier figure in this file incomparable.
            let before = hf(&mut hp_enh, &block);
            let wide: f64 = block.iter().map(|s| (*s as f64) * (*s as f64)).sum();
            let a = processor.process(&mut block);
            raw_hf.push(raw);
            enh_hf.push(before);
            enh_wide.push(wide / FRAME_SIZE as f64);
            speaking.push(a.speaking);
        }

        // **The model's own delay, which is not always zero.**
        //
        // `DeepFilterNet3_ll` — what ships — has a look-ahead of 0, so its
        // output frame is its input frame and every index below lines up. The
        // plain `DeepFilterNet3` holds 2 frames, so `enh[i]` is the enhancement
        // of `raw[i - 2]`. Comparing them at the same index across a 3-block
        // onset window overlaps the window with its own subject by two thirds,
        // which is enough to reverse the answer — and it did, on the first run
        // of the model comparison, before this existed.
        let lag = enhancer.model_lookahead();
        // For each opening, the window immediately before it — the audio the
        // look-ahead exists to rescue.
        // **The comparison, not the window.** Averaging the whole 160 ms before
        // an opening mostly averages background — p50 of the measured lead is
        // 10 ms — and the enhancer is *supposed* to remove background by 20 dB.
        // That number says nothing about the consonant.
        //
        // So: the last 30 ms before the opening, which is where a leading
        // fricative lives, against the first 100 ms after it, which is the
        // vowel the detector fired on. Same clip, same speaker, same
        // conditions. If the enhancer attenuates the first far harder than the
        // second, it is eating word starts, and no look-ahead can help because
        // the enhancer runs in front of the delay line as well.
        const ONSET: usize = 3; // 30 ms before
        const VOWEL: usize = 10; // 100 ms after
        let (mut on_raw, mut on_enh, mut on_n) = (0.0f64, 0.0f64, 0u64);
        let (mut vo_raw, mut vo_enh, mut vo_n) = (0.0f64, 0.0f64, 0u64);
        let mut openings = 0u32;
        // **From the log, not from this run.** Defining the windows by the
        // chain's live `speaking` output was the first version and it made the
        // table incoherent: every configuration changes those decisions, so
        // every row measured a different set of word starts, and rows came out
        // non-monotonic — more relief scoring a worse penalty than less. The
        // openings have to be the same blocks for every row or the differences
        // are population changes wearing the costume of an effect.
        //
        // The log's `speaking` column is not ground truth either — it is what
        // the chain decided on the day — but it is *fixed*, which is the only
        // property this comparison needs.
        let marks: &[bool] = if labels.len() >= speaking.len() {
            &labels
        } else {
            &speaking
        };
        // Where the guard fired, split the same way. "Open 40% of the time"
        // reads identically whether it is covering every word start or none of
        // them, so the two windows are counted separately.
        let (mut on_hit, mut vo_hit) = (0u64, 0u64);
        for i in 1..speaking.len() {
            if !(marks[i] && !marks[i - 1]) {
                continue;
            }
            openings += 1;
            for j in i.saturating_sub(ONSET)..i {
                let Some(&e) = enh_hf.get(j + lag) else {
                    continue;
                };
                on_raw += raw_hf[j];
                on_enh += e;
                on_n += 1;
                if relax[j] > 0.0 {
                    on_hit += 1;
                }
            }
            for j in i..(i + VOWEL).min(speaking.len()) {
                let Some(&e) = enh_hf.get(j + lag) else {
                    continue;
                };
                vo_raw += raw_hf[j];
                vo_enh += e;
                vo_n += 1;
                if relax[j] > 0.0 {
                    vo_hit += 1;
                }
            }
        }

        // Speech against gaps on the enhancer's own output, from the log's
        // labels — the number the model was adopted for. 1.5 dB at the
        // microphone became 16 dB; whatever the cap costs shows here.
        let (mut sp, mut spn, mut gp, mut gpn) = (0.0f64, 0u64, 0.0f64, 0u64);
        for (i, &want) in labels.iter().enumerate() {
            // Labels are stamped at capture, so the enhanced block they
            // describe is `lag` later. Same correction as the windows above.
            let Some(&e) = enh_wide.get(i + lag) else {
                break;
            };
            if want {
                sp += e;
                spn += 1;
            } else {
                gp += e;
                gpn += 1;
            }
        }
        let sep = db(sp, spn) - db(gp, gpn);

        let on_cut = db(on_enh, on_n) - db(on_raw, on_n);
        let vo_cut = db(vo_enh, vo_n) - db(vo_raw, vo_n);
        let (relaxed, total) = enhancer.onset_relief();
        let pct = |hit: u64, n: u64| 100.0 * hit as f64 / n.max(1) as f64;
        println!(
            "{:<20} {:>9} {:>9.1} dB {:>9.1} dB {:>11.1} dB {:>9.1} dB {:>8.1}%   \
             fired: onset {:.0}%, vowel {:.0}%",
            label,
            openings,
            on_cut,
            vo_cut,
            on_cut - vo_cut,
            sep,
            pct(relaxed, total),
            pct(on_hit, on_n),
            pct(vo_hit, vo_n),
        );
    }

    println!(
        "\n\"cut\" is how much the enhancer removed, against the microphone, for\n\
         the 30 ms before each opening and the 100 ms after it. The penalty is\n\
         the difference: how much harder the consonant is hit than the vowel it\n\
         leads into. \"separation\" is speech against gaps on the enhancer's own\n\
         output, from the log's labels -- the number a lower cap spends.\n\
         \n\
         Note what the table does *not* show: a cap low enough to leave the\n\
         onset alone. The penalty falls with the cap but does not close, because\n\
         it is the model's SNR estimate at the start of an utterance and not\n\
         the ceiling. A fix has to know an onset is happening."
    );
}
