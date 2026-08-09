//! DeepFilterNet 3, at the head of the capture chain.
//!
//! # Why it is first
//!
//! Everything else in this chain judges the signal by level, and
//! `docs/MUSIC_GATE.md` is the record of what that costs. With music playing,
//! the gaps between words sit **1.5 dB** below the speech — so no threshold can
//! separate them, and six hand-built features failed against exactly that.
//! Measured on the same clip against hand labels, DeepFilterNet takes 19.0 dB
//! out of the gaps and 4.5 dB out of the speech, turning 1.5 dB into 16.0 dB.
//!
//! Every stage after this one — the floor tracker, the gate, the AGC, the
//! profile chooser — reads a level. Putting the enhancer anywhere else would
//! leave them reading the old one.
//!
//! # The geometry is exactly ours
//!
//! 48 kHz, hop 480 samples, FFT 960. The capture worker hands out 480 samples
//! at 48 kHz. **A capture block is one model frame**, so nothing is buffered,
//! resampled or re-blocked on the way in — which removes the class of bug that
//! usually eats this kind of work.
//!
//! # What it costs
//!
//! One frame of look-ahead, so **10 ms of latency**, on top of the 80 ms onset
//! delay voice activation already carries. Measured per-frame cost in Python
//! with PyTorch on a desktop CPU was 4.8 ms against a 10 ms budget; this path
//! is Rust with `tract` and no Python, so that figure is the pessimistic end.
//! It has not been measured on a phone, and until it has, [`Enhancer::process`]
//! keeps a running worst case that the diagnostics panel shows.
//!
//! # It must never make things worse than not being here
//!
//! The model is built once, off the audio thread, and if that fails the
//! enhancer becomes a pass-through rather than an error: a rider whose phone
//! cannot load it should lose the improvement, not the call.

use anyhow::Result;
// The package is `deep_filter`; its library is named `df`. Importing by the
// package name is the obvious mistake and the compiler's message for it names
// neither.
use df::tract::{DfParams, DfTract, RuntimeParams};
use ndarray::{Array2, ArrayView2, ArrayViewMut2};

/// Samples per frame. The model's, and the same as the chain's `FRAME_SIZE`.
pub const HOP: usize = 480;

/// How much it may attenuate, in dB.
///
/// Not unlimited, deliberately. DeepFilterNet will take a background down by
/// 60 dB and more if it is asked to, and a helmet at speed is a background
/// that never stops — the result is a voice arriving out of complete silence,
/// which listeners report as "robotic" or "cutting out" even when every word is
/// intact. `docs/RECORDING.md` has the same finding about the gate.
///
/// 24 dB is enough to turn the 1.5 dB speech-to-gap separation measured on the
/// music clip into something a threshold can act on, without removing the room
/// altogether. It is a starting point and it is not yet tuned on a bike.
const ATTEN_LIM_DB: f32 = 24.0;

/// The deadline one block has to be returned in, in microseconds.
const BUDGET_US: u32 = 10_000;

/// Consecutive missed deadlines before the enhancer switches itself off.
///
/// One second's worth. Long enough that a scheduler hiccup or a cold cache
/// does not disable a feature that works, short enough that a phone which
/// simply cannot manage it is not allowed to ruin a whole ride.
const GIVE_UP_AFTER: u32 = 100;

/// Speech enhancement in front of everything else.
pub struct Enhancer {
    model: Option<Box<DfTract>>,
    /// Scratch, allocated once. The audio thread does not allocate.
    noisy: Array2<f32>,
    enhanced: Array2<f32>,
    /// Last signal-to-noise estimate the model reported, in dB. Published for
    /// the panel: it is the model's own opinion of what it is looking at, and
    /// the only number it offers about its own confidence.
    lsnr: f32,
    /// Worst frame time seen, in microseconds, and a count of frames that
    /// overran the block budget.
    ///
    /// The mean is not the interesting number. One frame over budget is a
    /// click in somebody's helmet, and a mean hides it.
    worst_us: u32,
    overruns: u32,
    frames: u64,
    /// Consecutive frames that missed the budget. Reset by any frame that
    /// makes it.
    run_of_overruns: u32,
    /// Switched off because it could not keep up. See [`Self::process`].
    gave_up: bool,
}

impl Enhancer {
    /// Builds the model. Slow — tens of milliseconds — so never on the audio
    /// thread.
    ///
    /// Returns a pass-through enhancer rather than an error if the model will
    /// not load, because losing the enhancement is a much smaller failure than
    /// losing the microphone.
    pub fn new() -> Self {
        let model = match Self::build() {
            Ok(m) => Some(Box::new(m)),
            Err(e) => {
                // With the reason. "It did not load" is not something anybody
                // can act on, and this is the one failure that silently costs
                // the whole improvement.
                tracing::warn!("DeepFilterNet did not load ({e:#}); the chain runs without it");
                None
            }
        };
        Self {
            model,
            noisy: Array2::zeros((1, HOP)),
            enhanced: Array2::zeros((1, HOP)),
            lsnr: 0.0,
            worst_us: 0,
            overruns: 0,
            frames: 0,
            run_of_overruns: 0,
            gave_up: false,
        }
    }

    fn build() -> Result<DfTract> {
        // Mono. Every route this app records from is one channel by the time
        // it reaches the chain.
        let params = RuntimeParams::default_with_ch(1).with_atten_lim(ATTEN_LIM_DB);
        let model = DfTract::new(DfParams::default(), &params)?;
        anyhow::ensure!(
            model.hop_size == HOP,
            "DeepFilterNet hop is {} samples, the chain's block is {HOP}",
            model.hop_size
        );
        Ok(model)
    }

    /// Whether it is enhancing right now.
    pub fn active(&self) -> bool {
        self.model.is_some() && !self.gave_up
    }

    /// Whether it loaded but then had to stop. Distinct from never having
    /// loaded: one is a phone that cannot keep up, the other is a build that
    /// went wrong, and they call for different answers.
    pub fn gave_up(&self) -> bool {
        self.gave_up
    }

    /// The model's own signal-to-noise estimate for the last frame, in dB.
    pub fn lsnr(&self) -> f32 {
        self.lsnr
    }

    /// Worst frame time in microseconds, and how many frames overran 10 ms.
    pub fn timing(&self) -> (u32, u32, u64) {
        (self.worst_us, self.overruns, self.frames)
    }

    /// Enhances one block in place.
    ///
    /// A no-op when the model did not load, so the caller has one path.
    pub fn process(&mut self, block: &mut [f32]) {
        if self.gave_up {
            return;
        }
        let Some(model) = self.model.as_mut() else {
            return;
        };
        if block.len() != HOP {
            // The chain is fixed at `FRAME_SIZE`, so this cannot happen from
            // the worker — but a wrong-sized block would panic inside the
            // model, and going quiet is not worth a shape mismatch.
            return;
        }

        self.noisy
            .as_slice_mut()
            .expect("contiguous")
            .copy_from_slice(block);

        let started = std::time::Instant::now();
        let noisy: ArrayView2<f32> = self.noisy.view();
        let enhanced: ArrayViewMut2<f32> = self.enhanced.view_mut();
        match model.process(noisy, enhanced) {
            Ok(lsnr) => {
                self.lsnr = lsnr;
                block.copy_from_slice(self.enhanced.as_slice().expect("contiguous"));
            }
            Err(e) => {
                // One bad frame is not worth silence. Leave the block as it
                // arrived and carry on; the counter below says it happened.
                tracing::debug!("DeepFilterNet frame failed: {e}");
            }
        }

        let us = started.elapsed().as_micros().min(u32::MAX as u128) as u32;
        self.worst_us = self.worst_us.max(us);
        // 10 ms, in microseconds. The budget one block has to be returned in.
        if us > BUDGET_US {
            self.overruns = self.overruns.saturating_add(1);
            self.run_of_overruns += 1;
            // **It stops rather than stuttering.** Measured at 3.55 ms a frame
            // on a desktop in release, but a phone core is slower and this has
            // not been measured on one. A model that cannot keep up does not
            // degrade gracefully -- it misses the deadline every frame, and a
            // missed deadline is a click in somebody's helmet for the rest of
            // the ride. Better to lose the enhancement and say so.
            //
            // A run, not a total: one slow frame is a scheduler hiccup, and a
            // hundred in a row is a phone that will never manage it.
            if self.run_of_overruns >= GIVE_UP_AFTER {
                self.gave_up = true;
                tracing::warn!(
                    "DeepFilterNet could not keep up ({} consecutive frames over {} ms);                      switching it off for this session",
                    self.run_of_overruns,
                    BUDGET_US / 1000
                );
            }
        } else {
            self.run_of_overruns = 0;
        }
        self.frames += 1;
    }

    /// Forgets the timing history, for the panel's Reset.
    pub fn reset_timing(&mut self) {
        self.worst_us = 0;
        self.overruns = 0;
        self.frames = 0;
        self.run_of_overruns = 0;
        // Deliberately not clearing `gave_up`. It is a fact about this device,
        // and a Reset button on a diagnostics panel should not quietly re-arm
        // something that was switched off for missing its deadline a hundred
        // times.
    }
}

impl Default for Enhancer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_model_loads_and_its_hop_is_our_block() {
        // The whole integration rests on this: if the model's hop were not
        // 480 samples at 48 kHz, every block would need buffering and the
        // latency argument would change with it.
        // Built directly rather than through `Enhancer::new`, which swallows
        // the reason by design -- here the reason is the whole point.
        let built = Enhancer::build();
        assert!(
            built.is_ok(),
            "the embedded DFN3 model did not load: {:#}",
            built.err().unwrap()
        );
        let m = built.unwrap();
        assert_eq!(m.hop_size, HOP);
        assert_eq!(m.sr, 48_000);
        eprintln!(
            "DFN3-ll: sr {} hop {} fft {} lookahead {} (conv {}, df {})",
            m.sr, m.hop_size, m.fft_size, m.lookahead, m.conv_lookahead, m.df_lookahead
        );
    }

    /// What one frame costs, in release. `cargo test --release -- --ignored
    /// --nocapture frame_cost`.
    ///
    /// Ignored because it is a measurement rather than an assertion, and a
    /// timing test that fails on a busy machine teaches people to ignore
    /// failures.
    #[test]
    #[ignore]
    fn frame_cost() {
        let mut e = Enhancer::new();
        assert!(e.active());
        let speech: Vec<f32> = (0..HOP * 200)
            .map(|i| 0.2 * (i as f32 * 0.03).sin() + 0.02 * ((i * 7) as f32).sin())
            .collect();
        for chunk in speech.chunks_exact(HOP) {
            let mut block = chunk.to_vec();
            e.process(&mut block);
        }
        let (worst, over, frames) = e.timing();
        eprintln!(
            "{frames} frames, worst {:.2} ms, {over} over the 10 ms budget",
            worst as f32 / 1000.0
        );
    }

    #[test]
    fn a_block_comes_back_the_same_length_and_finite() {
        let mut e = Enhancer::new();
        let mut block: Vec<f32> = (0..HOP).map(|i| 0.2 * (i as f32 * 0.05).sin()).collect();
        e.process(&mut block);
        assert_eq!(block.len(), HOP);
        assert!(
            block.iter().all(|s| s.is_finite() && s.abs() <= 1.5),
            "the enhancer produced something unplayable"
        );
    }

    #[test]
    fn silence_stays_silent() {
        // An enhancer that invents signal out of silence would be heard as a
        // gate opening on nothing, and would lift the noise floor the profile
        // chooser reads.
        let mut e = Enhancer::new();
        for _ in 0..20 {
            let mut block = vec![0.0f32; HOP];
            e.process(&mut block);
            assert!(
                block.iter().all(|s| s.abs() < 1e-3),
                "silence gained signal"
            );
        }
    }

    #[test]
    fn the_enhancer_actually_changes_the_audio() {
        // The guard on the regression above. If this ever stops being true --
        // the model failing to load, say -- then "the recorder captured the
        // microphone" would be trivially satisfied by an enhancer that does
        // nothing, and the ordering test would pass while measuring nothing.
        //
        // Speech-shaped tone under broadband noise: the enhancer should take
        // out enough of the noise that the block is measurably different.
        let mut e = Enhancer::new();
        assert!(e.active());
        let mut changed = false;
        let mut seed = 12345u32;
        for _ in 0..40 {
            let mut block: Vec<f32> = (0..HOP)
                .map(|i| {
                    seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                    let noise = (seed >> 16) as f32 / 32768.0 - 1.0;
                    0.25 * (i as f32 * 0.06).sin() + 0.15 * noise
                })
                .collect();
            let before = block.clone();
            e.process(&mut block);
            if block.iter().zip(&before).any(|(a, b)| (a - b).abs() > 1e-4) {
                changed = true;
            }
        }
        assert!(changed, "the enhancer left every block untouched");
    }

    #[test]
    fn a_wrong_sized_block_is_left_alone_rather_than_panicking() {
        let mut e = Enhancer::new();
        let mut block = vec![0.5f32; HOP - 1];
        e.process(&mut block);
        assert!(block.iter().all(|s| (*s - 0.5).abs() < 1e-6));
    }
}
