//! What one capture block costs, stage by stage, on this machine.
//!
//! The panel measures this on the device, which is where the answer that
//! matters lives. This is the offline companion: it runs the same stages in the
//! same order over a real ride, with nothing else competing for the core, so
//! the numbers are the work itself rather than the work plus whatever else the
//! phone was doing.
//!
//! **Both are needed, and they answer different questions.** On a device, wall
//! clock includes being descheduled, so a stage can look expensive because the
//! system was busy — that is honest about the deadline and useless for deciding
//! which stage to make cheaper. Here nothing interrupts, so the ratios are the
//! code. Compare the two and the difference is contention.
//!
//! ```text
//! set MW_CLIP=C:\ml_data\rides\20260810-1006-000.raw
//! cargo test --release --test chain_cost -- --ignored --nocapture
//! ```
//!
//! Ignored because it is a measurement, not an assertion. A timing test that
//! fails on a busy machine teaches people to ignore failures.

use mumbleway_core::audio::codec::{Quality, VoiceEncoder, FRAME_SAMPLES};
use mumbleway_core::audio::deepfilter::Enhancer;
use mumbleway_core::audio::dehiss::Expander;
use mumbleway_core::audio::denoise::{CaptureProcessor, NoiseProfile, FRAME_SIZE};
use mumbleway_core::audio::feedback::{FeedbackGuard, FeedbackMode};
use mumbleway_core::audio::timing::{Lap, Stage, StageTimings, STAGES, STAGE_NAMES};

#[test]
#[ignore]
fn chain_cost() {
    let audio: Vec<f32> = match std::env::var("MW_CLIP") {
        Ok(path) => {
            let bytes = std::fs::read(&path).expect("could not read MW_CLIP");
            eprintln!("clip: {path}");
            bytes
                .chunks_exact(4)
                .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                .collect()
        }
        Err(_) => {
            eprintln!("clip: synthetic (set MW_CLIP to a 48 kHz mono f32 .raw)");
            (0..FRAME_SIZE * 2000)
                .map(|i| 0.2 * (i as f32 * 0.03).sin() + 0.02 * ((i * 7) as f32).sin())
                .collect()
        }
    };

    // The profile a helmet at speed actually runs, which is the case the cost
    // question is being asked about.
    let profile = NoiseProfile::Helmet;
    let mut enhancer = Enhancer::new();
    let mut processor = CaptureProcessor::new(profile);
    let mut guard = FeedbackGuard::new();
    guard.set_mode(FeedbackMode::Guard);
    let mut expander = Expander::new();
    let mut encoder = VoiceEncoder::new(Quality::Balanced).expect("encoder");

    let mut timings = StageTimings::default();
    let mut frame: Vec<f32> = Vec::with_capacity(FRAME_SAMPLES);
    // No far end in an offline run, so nothing to cancel. The canceller becomes
    // a pass-through of its own accord when handed an empty reference, which is
    // worth knowing when reading the suppression row: on a real call it does
    // more than this says.
    let echo_ref: Vec<f32> = Vec::new();

    for chunk in audio.chunks_exact(FRAME_SIZE) {
        let mut block = chunk.to_vec();
        let started = std::time::Instant::now();
        let mut lap = Lap::new();

        // Stands in for the input measurements and taps the worker does here.
        let mut peak = 0.0f32;
        for &s in block.iter() {
            peak = peak.max(s.abs());
        }
        std::hint::black_box(peak);
        timings.record(Stage::Input, lap.split());

        enhancer.process(&mut block);
        timings.record(Stage::Enhancer, lap.split());

        let analysis = processor.process_with_reference(&mut block, &echo_ref);
        timings.record(Stage::Suppression, lap.split());

        guard.process(&mut block, &echo_ref);
        timings.record(Stage::Feedback, lap.split());

        expander.process(&mut block, analysis.level_db, analysis.noise_floor_db);
        timings.record(Stage::Dehiss, lap.split());

        // The transmit decision itself is a handful of comparisons; what sits
        // in this span on the real worker is the envelope and the decision log.
        std::hint::black_box(analysis.speaking);
        timings.record(Stage::Transmit, lap.split());

        frame.extend_from_slice(&block);
        if frame.len() >= FRAME_SAMPLES {
            let _ = encoder.encode(&frame[..FRAME_SAMPLES]);
            frame.clear();
        }
        timings.record(Stage::Encode, lap.split());

        timings.block(
            started.elapsed().as_micros().min(u32::MAX as u128) as u32,
            0.0,
        );
    }

    let blocks = timings.blocks();
    assert!(blocks > 0, "the clip is shorter than one block");
    eprintln!("{blocks} blocks, {profile:?} profile\n");
    eprintln!(
        "{:<16} {:>9} {:>9}  {:>6}",
        "stage", "mean ms", "worst ms", "share"
    );

    let total: f32 = (0..STAGES)
        .map(|i| timings.mean_us(stage_at(i)))
        .sum::<f32>()
        .max(1e-6);
    for i in 0..STAGES {
        let s = stage_at(i);
        eprintln!(
            "{:<16} {:>9.3} {:>9.3}  {:>5.1}%",
            STAGE_NAMES[i],
            timings.mean_us(s) / 1000.0,
            timings.worst_us(s) as f32 / 1000.0,
            100.0 * timings.mean_us(s) / total,
        );
    }
    eprintln!(
        "\n{:<16} {:>9.3} {:>9.3}",
        "whole block",
        timings.block_mean_us() / 1000.0,
        timings.block_worst_us() as f32 / 1000.0,
    );
    eprintln!(
        "{:<16} {:>9.3}   <- scheduling and anything untimed",
        "unattributed",
        timings.unattributed_us() / 1000.0,
    );
    eprintln!(
        "\nbudget is 10 ms a block; this machine is using {:.1}% of it",
        timings.block_mean_us() / 100.0,
    );
}

fn stage_at(i: usize) -> Stage {
    match i {
        0 => Stage::Input,
        1 => Stage::Enhancer,
        2 => Stage::Suppression,
        3 => Stage::Feedback,
        4 => Stage::Dehiss,
        5 => Stage::Transmit,
        _ => Stage::Encode,
    }
}
