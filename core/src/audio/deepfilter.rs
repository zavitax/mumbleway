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

/// Where the model may skip work, in dB of its own SNR estimate.
///
/// `apply_stages` picks one of four paths per frame: below `MIN_DB` a zero
/// mask and no decoder at all; above `MAX_ERB_DB` no processing; above
/// `MAX_DF_DB` the cheap ERB decoder only; and between them both decoders,
/// which is the expensive one.
///
/// **The DF decoder is 19.2 MB of the model's 20 MB** — three GRUs of 512 — so
/// a frame that skips it is a different order of work. On a Snapdragon 450 the
/// two paths measure 7.9 ms and 4.7 ms against a 10 ms block.
///
/// `MAX_DF_DB` is therefore the whole cost lever, and it is the one this file
/// uses when a phone cannot keep up. See [`Effort`].
const MIN_DB: f32 = -10.0;
const MAX_ERB_DB: f32 = 30.0;
const MAX_DF_DB: f32 = 20.0;

/// How much work the enhancer is allowed to do.
///
/// **Because the alternative was all or nothing, and low-end phones got
/// nothing.** Until now a device that missed the deadline a hundred times in a
/// row switched the enhancer off for the session — and on the phone this was
/// reported from, that is exactly what happened. Measured on that same phone
/// (OPPO A3s, Snapdragon 450, Cortex-A53), with the model doing all the work,
/// the enhancer's own frames come in at a mean of 6.2 ms and a worst of 9.3 ms
/// — **inside the 10 ms budget, with nothing to spare for the rest of the
/// chain**. It is not that the model cannot run there. It is that the model
/// plus RNNoise plus the filters plus the encoder cannot.
///
/// So there are rungs between full and off, and each one was measured rather
/// than guessed. Separation is speech-to-gap in dB across the ride corpus, and
/// the cost is that phone's mean frame:
///
/// | Rung | `max_df` | Mean there | Worst frame | Separation, worst clip | Best clip |
/// |---|---|---|---|---|---|
/// | [`Effort::Full`] | 20 | 6.29 ms | 8.98 ms | 27.0 dB | 14.1 dB |
/// | [`Effort::Reduced`] | 0 | 4.62 ms | 9.08 ms | 24.2 dB | **15.7 dB** |
/// | [`Effort::ErbOnly`] | −15 | 3.94 ms | **5.79 ms** | 22.1 dB | 15.9 dB |
/// | [`Effort::Bypassed`] | — | 0 | 0 | none | none |
///
/// **The worst frame is the column that matters to the guard**, and it is the
/// one that does not improve until the bottom rung: `Reduced` cuts the mean by
/// a quarter and leaves the tail where it was, because the frames that run
/// both decoders still run both decoders — there are simply fewer of them.
/// `ErbOnly` is where the DF decoder stops running at all, and the tail halves.
///
/// **Stepping down is not purely a loss.** On voice over music — the clip this
/// model was adopted for — `Reduced` separates *better* than `Full`: 15.7 dB
/// against 14.1. The DF decoder takes 11.5 dB out of the speech at full effort
/// and 9.8 dB at reduced, and it is the speech being eaten, not the music
/// surviving. That is a measured account of "speech gets choppier in Helmet",
/// and it is why the middle rungs are worth having even on a fast phone.
///
/// It is still a loss on quieter material — 27.0 dB to 24.2 on one ride — so
/// this is a degradation path and not a new default.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effort {
    /// Everything the model can do. What a phone that keeps up runs.
    Full,
    /// The DF decoder only on the frames that most need it.
    Reduced,
    /// The ERB decoder alone; the 19.2 MB of GRUs never run.
    ErbOnly,
    /// Pass-through. The chain runs without the enhancer at all.
    Bypassed,
}

impl Effort {
    /// The DF threshold this rung runs at, in dB.
    ///
    /// −15 dB is the model's own `lsnr_min`, so every frame that is not
    /// already zero-masked takes the ERB-only path and the DF decoder is never
    /// reached. Written as the floor rather than as minus infinity because it
    /// is the value the corpus was measured at.
    fn max_df_db(self) -> f32 {
        match self {
            Effort::Full => MAX_DF_DB,
            Effort::Reduced => 0.0,
            Effort::ErbOnly | Effort::Bypassed => -15.0,
        }
    }

    /// The next rung down, or `None` at the bottom.
    fn weaker(self) -> Option<Effort> {
        match self {
            Effort::Full => Some(Effort::Reduced),
            Effort::Reduced => Some(Effort::ErbOnly),
            Effort::ErbOnly => Some(Effort::Bypassed),
            Effort::Bypassed => None,
        }
    }

    /// For the panel and the decision log.
    pub fn index(self) -> u8 {
        match self {
            Effort::Full => 0,
            Effort::Reduced => 1,
            Effort::ErbOnly => 2,
            Effort::Bypassed => 3,
        }
    }
}

/// Consecutive missed deadlines before the enhancer gives up a rung.
///
/// One second's worth. Long enough that a scheduler hiccup or a cold cache
/// does not cost quality on a phone that is coping, short enough that a phone
/// which simply cannot manage is not allowed to ruin a whole ride.
///
/// **It steps down, and never back up.** Climbing again would need the same
/// hysteresis argument the profile chooser needed, and it would be settled by
/// the same measurement that pushed it down in the first place — so a device
/// on the edge would oscillate, and every change of rung is audible. A rung is
/// treated as a fact about this device for the rest of the session, which is
/// what the old all-or-nothing guard already assumed and is the one part of it
/// worth keeping.
const STEP_DOWN_AFTER: u32 = 100;

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
    /// makes it, and by every step down.
    run_of_overruns: u32,
    /// How hard it is allowed to work. Only ever falls. See [`Effort`].
    effort: Effort,
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
            effort: Effort::Full,
        }
    }

    fn build() -> Result<DfTract> {
        // Mono. Every route this app records from is one channel by the time
        // it reaches the chain.
        let params = RuntimeParams::default_with_ch(1)
            .with_atten_lim(ATTEN_LIM_DB)
            .with_thresholds(MIN_DB, MAX_ERB_DB, MAX_DF_DB);
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
        self.model.is_some() && self.effort != Effort::Bypassed
    }

    /// Whether it loaded but then had to stop. Distinct from never having
    /// loaded: one is a phone that cannot keep up, the other is a build that
    /// went wrong, and they call for different answers.
    pub fn gave_up(&self) -> bool {
        self.effort == Effort::Bypassed
    }

    /// Which rung it is on. [`Effort::Full`] unless a phone made it step down.
    pub fn effort(&self) -> Effort {
        self.effort
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
        if self.effort == Effort::Bypassed {
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
            // **It gives up a rung rather than the whole feature.** A missed
            // deadline is a click in somebody's helmet, so it cannot simply
            // stutter on -- but the old guard went straight from everything to
            // nothing, and on the phone this was reported from the model was
            // measured at 6.2 ms a frame against a 10 ms budget. It was never
            // the model on its own that did not fit.
            //
            // A run, not a total: one slow frame is a scheduler hiccup, and a
            // hundred in a row is a device that will not manage this rung.
            if self.run_of_overruns >= STEP_DOWN_AFTER {
                self.step_down();
            }
        } else {
            self.run_of_overruns = 0;
        }
        self.frames += 1;
    }

    /// Drops to the next rung, or to pass-through at the bottom.
    ///
    /// **Public, and only downwards.** The overrun counter drives this in a
    /// real session and nothing else should — but the rungs have to be
    /// reachable deliberately to be measured, and `core/tests/chain_cost.rs`
    /// benchmarks the whole block at each one on the phone. Exposing the step
    /// rather than a `set_effort` keeps "it only ever falls" true of the type
    /// rather than true by convention.
    pub fn step_down(&mut self) {
        let Some(next) = self.effort.weaker() else {
            return;
        };
        self.effort = next;
        self.run_of_overruns = 0;
        // The thresholds are plain public fields on the model, so a rung costs
        // one assignment: no rebuild, no allocation, nothing that could block
        // the audio thread for the tens of milliseconds a reload would take.
        if let Some(model) = self.model.as_mut() {
            model.max_db_df_thresh = next.max_df_db();
        }
        match next {
            Effort::Bypassed => tracing::warn!(
                "DeepFilterNet could not keep up even at its lowest setting; \
                 the chain runs without it for this session"
            ),
            other => tracing::warn!(
                "DeepFilterNet could not keep up ({} consecutive frames over {} ms); \
                 stepping down to {other:?}",
                STEP_DOWN_AFTER,
                BUDGET_US / 1000
            ),
        }
    }

    /// Forgets the timing history, for the panel's Reset.
    pub fn reset_timing(&mut self) {
        self.worst_us = 0;
        self.overruns = 0;
        self.frames = 0;
        self.run_of_overruns = 0;
        // Deliberately not restoring the rung. It is a fact about this device,
        // and a Reset button on a diagnostics panel should not quietly re-arm
        // something that stepped down for missing its deadline a hundred times
        // — least of all while a rider is on the road listening to the result.
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

    /// What one frame costs, in release, on real audio.
    ///
    /// ```text
    /// set MW_CLIP=<a 48 kHz f32 mono .raw from the ride corpus>
    /// cargo test --release -- --ignored --nocapture frame_cost
    /// ```
    ///
    /// Falls back to a synthetic tone if no clip is given, which is enough to
    /// show the shape and nothing like a helmet at speed. The distribution of
    /// the model's own SNR estimate is the interesting output: it says which
    /// of the four `apply_stages` paths a real ride actually takes, and
    /// therefore how much the thresholds have to give.
    ///
    /// Ignored because it is a measurement rather than an assertion, and a
    /// timing test that fails on a busy machine teaches people to ignore
    /// failures.
    #[test]
    #[ignore]
    fn frame_cost() {
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
                eprintln!("clip: synthetic (set MW_CLIP to a 48 kHz f32 .raw)");
                (0..HOP * 500)
                    .map(|i| 0.2 * (i as f32 * 0.03).sin() + 0.02 * ((i * 7) as f32).sin())
                    .collect()
            }
        };

        let mut e = Enhancer::new();
        assert!(e.active());
        let mut per_frame = Vec::new();
        let mut lsnrs = Vec::new();
        for chunk in audio.chunks_exact(HOP) {
            let mut block = chunk.to_vec();
            let t0 = std::time::Instant::now();
            e.process(&mut block);
            per_frame.push(t0.elapsed().as_micros() as f32 / 1000.0);
            lsnrs.push(e.lsnr());
        }

        // The first few frames carry one-off setup.
        per_frame.drain(..5.min(per_frame.len()));
        let mut sorted = per_frame.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let pct = |p: f32| sorted[((sorted.len() - 1) as f32 * p) as usize];
        let mean = per_frame.iter().sum::<f32>() / per_frame.len() as f32;
        eprintln!(
            "{} frames  mean {:.2} ms  p50 {:.2}  p95 {:.2}  p99 {:.2}  worst {:.2}",
            per_frame.len(),
            mean,
            pct(0.50),
            pct(0.95),
            pct(0.99),
            sorted[sorted.len() - 1]
        );

        let mut ls = lsnrs.clone();
        ls.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let lp = |p: f32| ls[((ls.len() - 1) as f32 * p) as usize];
        eprintln!(
            "lsnr dB: p05 {:.1}  p25 {:.1}  p50 {:.1}  p75 {:.1}  p95 {:.1}",
            lp(0.05),
            lp(0.25),
            lp(0.50),
            lp(0.75),
            lp(0.95)
        );

        // Which of the four paths the ride actually takes, at the thresholds
        // in force. This is the number that says how much is on the table.
        let (mut zero, mut clean, mut erb_only, mut both) = (0, 0, 0, 0);
        for &l in &lsnrs {
            if l < MIN_DB {
                zero += 1;
            } else if l > MAX_ERB_DB {
                clean += 1;
            } else if l > MAX_DF_DB {
                erb_only += 1;
            } else {
                both += 1;
            }
        }
        let n = lsnrs.len() as f32;
        eprintln!(
            "stages: zero-mask {:.1}%  untouched {:.1}%  erb-only {:.1}%               both decoders {:.1}%  <- the expensive one",
            100.0 * zero as f32 / n,
            100.0 * clean as f32 / n,
            100.0 * erb_only as f32 / n,
            100.0 * both as f32 / n
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
    fn it_gives_up_a_rung_at_a_time_and_never_climbs_back() {
        // The behaviour the phone needed. The old guard went from everything
        // to nothing in one step, which is how a device measured at 6.2 ms a
        // frame against a 10 ms budget ended up with no enhancement at all.
        let mut e = Enhancer::new();
        assert_eq!(e.effort(), Effort::Full);
        assert!(e.active());

        for expected in [Effort::Reduced, Effort::ErbOnly, Effort::Bypassed] {
            e.step_down();
            assert_eq!(e.effort(), expected);
        }

        // The bottom is the bottom, and it stays there.
        assert!(e.gave_up());
        assert!(!e.active());
        e.step_down();
        assert_eq!(e.effort(), Effort::Bypassed);

        // And a Reset on the diagnostics panel clears the counters without
        // quietly re-arming something that could not keep up.
        e.reset_timing();
        assert_eq!(e.effort(), Effort::Bypassed);
        assert_eq!(e.timing(), (0, 0, 0));
    }

    #[test]
    fn each_rung_asks_the_model_for_less_work() {
        // The rung is a threshold on the model's own SNR estimate, and the
        // whole saving is that fewer frames reach the DF decoder — 19.2 MB of
        // the model's 20. If these ever stopped descending, stepping down
        // would cost quality and buy nothing.
        let steps = [
            Effort::Full,
            Effort::Reduced,
            Effort::ErbOnly,
            Effort::Bypassed,
        ];
        for pair in steps.windows(2) {
            assert!(
                pair[1].max_df_db() <= pair[0].max_df_db(),
                "{:?} does not ask for less than {:?}",
                pair[1],
                pair[0]
            );
        }
        assert!(steps
            .windows(2)
            .any(|p| p[1].max_df_db() < p[0].max_df_db()));

        // And the model is actually told, rather than the rung being a label.
        let mut e = Enhancer::new();
        e.step_down();
        assert_eq!(
            e.model.as_ref().map(|m| m.max_db_df_thresh),
            Some(Effort::Reduced.max_df_db()),
            "the rung changed but the model was not told"
        );
    }

    #[test]
    fn a_reduced_enhancer_still_enhances() {
        // Every rung above bypass has to do something, or stepping down is
        // just a slower way of switching off. Speech-shaped tone under noise,
        // the same shape the full-effort test uses.
        for rung in [Effort::Reduced, Effort::ErbOnly] {
            let mut e = Enhancer::new();
            while e.effort() != rung {
                e.step_down();
            }
            assert!(e.active(), "{rung:?} should still be enhancing");

            let mut changed = false;
            let mut seed = 4242u32;
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
                assert!(
                    block.iter().all(|s| s.is_finite()),
                    "{rung:?} produced something unplayable"
                );
            }
            assert!(changed, "{rung:?} left every block untouched");
        }
    }

    #[test]
    fn a_wrong_sized_block_is_left_alone_rather_than_panicking() {
        let mut e = Enhancer::new();
        let mut block = vec![0.5f32; HOP - 1];
        e.process(&mut block);
        assert!(block.iter().all(|s| (*s - 0.5).abs() < 1e-6));
    }
}
