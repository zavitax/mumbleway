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
//! conditions — DeepFilterNet attenuates the first far harder than the second:
//!
//! | Clip | Onset | Vowel | Penalty |
//! |---|---|---|---|
//! | voice over music | −22.4 dB | −15.5 dB | −6.9 dB |
//! | iPhone, "shalom" heard as "alom" | −14.6 dB | −5.5 dB | **−9.0 dB** |
//! | iPhone, quiet room | −19.7 dB | −1.1 dB | **−18.5 dB** |
//!
//! Which is what the model is *for*: an unvoiced consonant is noise-like and
//! pitchless, and at the start of an utterance the model's own SNR estimate is
//! still at the floor because it has been looking at silence.
//!
//! **So the look-ahead cannot fix this either**, and for a different reason
//! than the one above: the enhancer runs in front of the delay line as well,
//! so what is delayed has already been stripped. Raising `ONSET_LOOKAHEAD_MS`
//! from 80 to 160 was measured against `transmitting` transitions — two stages
//! downstream of the damage — and does not address this symptom.
//!
//! The sweep below prices `ATTEN_LIM_DB`, which is the cap the onsets are
//! sitting against, and the separation it costs.
//!
//! ```text
//! set MW_CLIP=C:\ml_data\rides\20260810-1849-000.raw
//! cargo test --release --test onset_survival -- --ignored --nocapture
//! ```
//!
//! Ignored because it is a measurement, not an assertion.

use mumbleway_core::audio::deepfilter::Enhancer;
use mumbleway_core::audio::denoise::{CaptureProcessor, NoiseProfile, FRAME_SIZE};

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
        "{:<8} {:>10} {:>12} {:>12} {:>14} {:>12}",
        "atten", "openings", "cut: onset", "cut: vowel", "onset penalty", "separation"
    );

    for atten in [24.0f32, 18.0, 15.0, 12.0, 9.0, 6.0] {
        let profile = NoiseProfile::Standard;
        let mut enhancer = Enhancer::with_atten_lim(atten);
        let mut processor = CaptureProcessor::new(profile);

        // Per block: the energy handed to the processor, the energy it
        // returned, and whether the chain called it speech.
        let mut raw_e: Vec<f64> = Vec::new();
        let mut into: Vec<f64> = Vec::new();
        let mut out_of: Vec<f64> = Vec::new();
        let mut speaking: Vec<bool> = Vec::new();

        for chunk in audio.chunks_exact(FRAME_SIZE) {
            let mut block = chunk.to_vec();
            // The microphone, before anything. An unvoiced consonant is
            // noise-like by construction, which is exactly what a speech
            // enhancer is trained to remove -- so this stage has to be visible
            // separately or its effect is invisible.
            let raw: f64 = block.iter().map(|s| (*s as f64) * (*s as f64)).sum();
            enhancer.process(&mut block);
            // What the gate is about to judge, before it has touched it.
            let before: f64 = block.iter().map(|s| (*s as f64) * (*s as f64)).sum();
            let a = processor.process(&mut block);
            let after: f64 = block.iter().map(|s| (*s as f64) * (*s as f64)).sum();
            raw_e.push(raw / FRAME_SIZE as f64);
            into.push(before / FRAME_SIZE as f64);
            out_of.push(after / FRAME_SIZE as f64);
            speaking.push(a.speaking);
        }

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
        for i in 1..speaking.len() {
            if !(speaking[i] && !speaking[i - 1]) {
                continue;
            }
            openings += 1;
            for j in i.saturating_sub(ONSET)..i {
                on_raw += raw_e[j];
                on_enh += into[j];
                on_n += 1;
            }
            for j in i..(i + VOWEL).min(speaking.len()) {
                vo_raw += raw_e[j];
                vo_enh += into[j];
                vo_n += 1;
            }
        }

        // Speech against gaps on the enhancer's own output, from the log's
        // labels — the number the model was adopted for. 1.5 dB at the
        // microphone became 16 dB; whatever the cap costs shows here.
        let (mut sp, mut spn, mut gp, mut gpn) = (0.0f64, 0u64, 0.0f64, 0u64);
        for (i, &want) in labels.iter().enumerate() {
            let Some(&e) = into.get(i) else { break };
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
        println!(
            "{:<8} {:>10} {:>9.1} dB {:>9.1} dB {:>11.1} dB {:>9.1} dB",
            format!("{atten:.0} dB"),
            openings,
            on_cut,
            vo_cut,
            on_cut - vo_cut,
            sep,
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
