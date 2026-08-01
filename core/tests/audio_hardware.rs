//! Hardware-dependent checks for the audio engine.
//!
//! These open real capture and playback devices, so they are `#[ignore]`d by
//! default and only run on request:
//!
//! ```text
//! cargo test --test audio_hardware -- --ignored --nocapture
//! ```
//!
//! Everything that can be verified without hardware lives in the unit tests.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use mumbleway_core::audio::engine::{list_devices, AudioConfig, AudioEngine, TransmitMode};
use mumbleway_core::audio::{NoiseProfile, Quality};

#[test]
#[ignore = "requires a microphone and speakers"]
fn lists_at_least_one_device_of_each_kind() {
    let (inputs, outputs) = list_devices();
    println!("inputs:  {inputs:?}");
    println!("outputs: {outputs:?}");
    assert!(!inputs.is_empty(), "no input devices found");
    assert!(!outputs.is_empty(), "no output devices found");
}

#[test]
#[ignore = "requires a microphone and speakers"]
fn engine_starts_and_produces_encoded_frames() {
    let frames = Arc::new(AtomicUsize::new(0));
    let bytes = Arc::new(AtomicUsize::new(0));

    let f = frames.clone();
    let b = bytes.clone();

    let engine = AudioEngine::start(
        AudioConfig {
            noise_profile: NoiseProfile::Helmet,
            quality: Quality::Balanced,
            // Continuous, so the test does not depend on anyone speaking into
            // the microphone or on the gate opening.
            transmit_mode: TransmitMode::Continuous,
            input_device: None,
            output_device: None,
        },
        move |_seq, packet, _terminator| {
            f.fetch_add(1, Ordering::Relaxed);
            b.fetch_add(packet.len(), Ordering::Relaxed);
        },
    )
    .expect("audio engine should start on a machine with a microphone");

    // Two seconds of capture is 100 frames of 20 ms.
    std::thread::sleep(Duration::from_secs(2));

    let n = frames.load(Ordering::Relaxed);
    let total = bytes.load(Ordering::Relaxed);
    let level = engine.shared().input_level_db();
    println!("frames={n} bytes={total} input_level={level:.1} dBFS");

    // Allow generous slack for device start-up, but the pipeline must clearly be
    // running in real time rather than trickling or stalled.
    assert!(
        n >= 50,
        "expected roughly 100 frames in 2 s, got {n} — the capture path is not \
         keeping up with real time"
    );
    assert!(total > 0, "frames were produced but carried no Opus data");
    assert!(level.is_finite(), "input level meter produced {level}");

    drop(engine);
}

#[test]
#[ignore = "requires a microphone and speakers"]
fn engine_shuts_down_cleanly_and_can_restart() {
    // Reconnecting or changing devices restarts the engine, so this must not
    // leave the device claimed.
    for round in 0..2 {
        let engine = AudioEngine::start(AudioConfig::default(), |_, _, _| {})
            .unwrap_or_else(|e| panic!("round {round} failed to start: {e}"));
        std::thread::sleep(Duration::from_millis(400));
        drop(engine);
        std::thread::sleep(Duration::from_millis(300));
    }
}
