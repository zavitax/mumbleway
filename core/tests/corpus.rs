//! Writes a noise corpus for training, from the generators already here.
//!
//! The training pipeline in `docs/VOICE_MODEL.md` needs hours of helmet noise
//! with nobody talking. The honest source is a dedicated ride, and until there
//! is one there are two stand-ins, of which only this one is usable.
//!
//! The other was to carve the quiet parts out of the recordings we have. It
//! yielded 6.2 seconds from 450: both neural detectors flag almost every
//! second of those recordings as possibly-speech, and anything they flag has
//! to be thrown away, because a second of the rider's own voice smuggled into
//! a noise pool teaches the network that this voice is what it should remove.
//!
//! So: the seeded generators in [`testsig`], which were written as an
//! adversarial *test* set and turn out to be the only unlimited noise source
//! available. They are synthetic and that is a real limitation — a model
//! trained on them has learnt this project's idea of wind, not wind. What they
//! buy is the whole pipeline, end to end, today: mixing, labels, training,
//! evaluation against the real recordings. When a noise-only ride exists,
//! only the contents of this directory change.
//!
//! ```text
//! MUMBLEWAY_CORPUS_OUT=C:/ml_data/noise_synth MUMBLEWAY_CORPUS_MINUTES=60 \
//!   cargo test --release --test corpus -- --ignored --nocapture
//! ```

use std::fs;
use std::path::PathBuf;

use mumbleway_core::audio::testsig;

const RATE: usize = 48_000;

/// One noise clip: a kind, a seed, and a loudness.
///
/// Randomised rather than a fixed menu. A network trained on wind at exactly
/// one level learns that level; the point of a seeded generator is that the
/// sweep costs nothing.
fn clip(kind: usize, seed: u64, seconds: usize) -> (String, Vec<f32>) {
    let len = RATE * seconds;
    // Spread the amplitude over the range a helmet actually presents, which
    // the road recordings put between roughly -25 and -8 dBFS.
    let amp = 0.08 + (seed % 17) as f32 * 0.045;

    match kind {
        0 => ("wind".into(), testsig::wind(len, amp, seed)),
        1 => (
            "engine".into(),
            // Firing frequency across idle to motorway revs.
            testsig::engine(len, 28.0 + (seed % 40) as f32, amp, seed),
        ),
        2 => ("traffic".into(), testsig::traffic(len, amp, seed)),
        3 => ("music".into(), testsig::music(len, amp * 0.8, seed)),
        4 => ("unknown".into(), testsig::unknown(len, amp, seed)),
        _ => {
            // The mixtures, which is what a helmet actually contains: never
            // one source, always weather over machinery.
            let mut mixed = testsig::wind(len, amp, seed);
            let engine = testsig::engine(len, 30.0 + (seed % 30) as f32, amp * 0.9, seed + 1);
            for (m, e) in mixed.iter_mut().zip(engine) {
                *m = (*m + e).clamp(-1.0, 1.0);
            }
            if seed % 3 == 0 {
                let extra = testsig::traffic(len, amp * 0.6, seed + 2);
                for (m, t) in mixed.iter_mut().zip(extra) {
                    *m = (*m + t).clamp(-1.0, 1.0);
                }
            }
            ("mixed".into(), mixed)
        }
    }
}

#[test]
#[ignore = "writes a training corpus; see the module comment"]
fn write_a_noise_corpus() {
    let out = std::env::var("MUMBLEWAY_CORPUS_OUT")
        .expect("set MUMBLEWAY_CORPUS_OUT to a directory to write into");
    let minutes: usize = std::env::var("MUMBLEWAY_CORPUS_MINUTES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(30);
    fs::create_dir_all(&out).expect("creating the corpus directory");

    // Ten-second clips: long enough to hold a gust and a gear change, short
    // enough that the mixer can draw a fresh one often.
    const CLIP_SECONDS: usize = 10;
    let wanted = minutes * 60 / CLIP_SECONDS;

    // Six kinds, weighted so that mixtures — the realistic case — dominate.
    let kinds = [0usize, 1, 2, 3, 4, 5, 5, 5];
    let mut written = 0usize;
    for i in 0..wanted {
        let kind = kinds[i % kinds.len()];
        let seed = 1_000 + i as u64 * 7;
        let (name, samples) = clip(kind, seed, CLIP_SECONDS);
        let mut bytes = Vec::with_capacity(samples.len() * 4);
        for s in &samples {
            bytes.extend_from_slice(&s.to_le_bytes());
        }
        let path = PathBuf::from(&out).join(format!("{name}_{seed:05}.raw"));
        fs::write(&path, &bytes).expect("writing a clip");
        written += 1;
    }

    println!(
        "wrote {written} clips ({} minutes) to {out}",
        written * CLIP_SECONDS / 60
    );
}
