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
//! # Which of the two models, and what the other one would buy
//!
//! `deep_filter` ships both DFN3 variants and the choice is a Cargo feature:
//! `default-model-ll` is what this uses, `default-model` is the plain one.
//!
//! Prompted by [AndroidDeepFilterNet](https://github.com/KaleyraVideo/AndroidDeepFilterNet),
//! which advertises a "~8 MB mobile-optimised model". **It is the plain DFN3**
//! — the same 7.6 MB archive already sitting in our own dependency — and the
//! library around it runs the same `tract` inference this does, from prebuilt
//! `.so` blobs, behind a JNI hop. Its own optimisation notes say the
//! quantisation that would have been a real win was abandoned because tract
//! cannot execute quantised ONNX, and the fusion it did ship is what
//! `declutter()` and `into_optimized()` already do at load — libDF calls both
//! on all three sub-graphs.
//!
//! So there is nothing to adopt, but there is something to *measure*, because
//! the model behind it is one word away. Same clip, same machine, per frame:
//!
//! | Model | Mean | p95 | p99 | Worst | Look-ahead |
//! |---|---|---|---|---|---|
//! | **DFN3-ll**, 34.7 MB — ships | 2.63 ms | 6.99 | 9.77 | 43.44 | **0** |
//! | DFN3 plain, 7.6 MB | **0.88 ms** | 2.31 | 3.83 | 11.60 | 2 frames, **20 ms** |
//!
//! **Three times cheaper on the mean and 2.6× on the p99**, which is far more
//! than the whole rest of the relief ladder can buy — every cheap rung together
//! is 0.54 ms. The price is 20 ms of algorithmic latency, and [`super::paydown`]
//! has just bought 36 ms back, so a device could take this and still be ahead
//! of where it was this morning.
//!
//! **Not switched, because the quality side is unmeasured.** The two models
//! differ in more than latency: the plain one may use future context and could
//! separate *better* rather than worse. Nobody has run the separation numbers,
//! and "3× cheaper" is not a reason to change what a rider hears on an
//! assumption. The measurement to run is `core/tests/onset_survival.rs`'s
//! separation column and `dfbench` on the OPPO, against both features.
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

/// The cap while a word is starting, in dB.
///
/// **This is the fix for swallowed word starts, and the reason it is a separate
/// number rather than a lower `ATTEN_LIM_DB`.** A rider reported "shalom"
/// arriving as "alom" and "pishtan" as "ishtan";
/// `core/tests/onset_survival.rs` found the cause, and it is this stage. An
/// unvoiced consonant is noise-like and pitchless — exactly what a speech
/// enhancer is trained to remove — and at the start of an utterance the model's
/// own SNR estimate is still at the floor because it has been looking at
/// silence. Measured against the vowel that follows it, on the same speaker in
/// the same conditions, the model takes 14 to 17 dB more out of the onset.
///
/// Lowering `ATTEN_LIM_DB` for the whole session does not fix it, and the sweep
/// in that test is what says so: the penalty is a *difference* between the
/// onset and the vowel, and a cap that relaxes both equally leaves the
/// difference where it was while paying for it in separation everywhere.
///
/// Relaxing the cap *only* while a word is starting is the same lever pointed
/// at the actual asymmetry. On the ride the complaint came from, over a fixed
/// set of word starts:
///
/// | | Onset | Vowel | Penalty | Separation | Relaxed |
/// |---|---|---|---|---|---|
/// | 24 dB flat | −23.0 | −5.8 | **−17.2** | 4.6 | — |
/// | 12 dB flat | −12.7 | −4.8 | −7.9 | 3.7 | — |
/// | 6 dB flat | −6.9 | −3.5 | −3.4 | **2.5** | — |
/// | 24 dB + guard | −14.3 | −5.7 | **−8.5** | **4.7** | 10% |
///
/// The guard halves the penalty while *keeping* the separation of a 24 dB cap,
/// where a flat cap that reaches the same penalty costs a quarter of it. 3 dB
/// is very nearly "pass the microphone through", and that is the intent: for
/// 50 ms at a word start, stop enhancing.
const ONSET_ATTEN_LIM_DB: f32 = 3.0;

/// Where the onset detector listens, in Hz.
///
/// High, on purpose. The sounds being lost are "sh", "s", "p" and "ch", which
/// are high-frequency by construction; the things that must *not* open this
/// window — an engine, a gust, a bassline — are low. Watching the whole band
/// would fire on all of them and hand the gate 50 ms of under-suppressed noise,
/// which is the music leakage this project has spent five features failing to
/// close.
///
/// 1500 rather than 3000, measured: 3 kHz clears an engine by more but misses
/// the low half of a "sh" and the burst of a "p", and came out 2 dB worse on
/// the penalty at every relaxation tried. Both are far above a firing
/// fundamental, so the margin 3 kHz buys was not being spent on anything.
const ONSET_HP_HZ: f32 = 1_500.0;

/// How far the high band must jump above its recent level to count as a start.
///
/// 6 dB. At 4 the guard opens on 24% of blocks for no better penalty; at 9 it
/// misses word starts that follow a breath rather than silence.
const ONSET_RISE_DB: f32 = 6.0;

/// How loud the high band may already be for the guard to open fully, and where
/// it stops opening at all, in dB.
///
/// **Without this the guard is a net loss on exactly the ride it must not
/// harm.** Relaxing the cap lets back in whatever the model was removing, so
/// what it costs depends entirely on how much that was. Measured on two rides
/// with the same tuning:
///
/// | Ride | Enhancer separation | Recall, Helmet | with guard |
/// |---|---|---|---|
/// | iPhone, quiet, "shalom" | 4.6 dB | 52.0% | **58.6%** |
/// | voice over loud music | 14.1 dB | 97.9% | **77.6%** |
///
/// **The floor trackers were the obvious suspect and they are innocent.** Both
/// sit downstream of the enhancer, and the level blocks were being decided at
/// rose 12 dB, so the reading was that they had learned a floor from the
/// relaxed blocks and lifted the gate to match — the fault the warm-up already
/// guards against. Freezing both through the window changed nothing: recall
/// stayed at 77.6%, the decided level stayed at −39.4 dB.
///
/// Splitting the transmit decision in two found it instead. Blocks where
/// RNNoise agreed with the SNR margin fell from 42.1% to 19.4%, with "VAD says
/// speech, SNR does not" collapsing from 2.7% to 0.2%. **It is the network.**
/// Handing a stateful denoiser audio that steps between heavily enhanced and
/// nearly raw every 50 ms leaves its noise estimate wrong all the time, not
/// only inside the window.
///
/// So the relief is scaled by how loud the band is that it would stop
/// suppressing. In a quiet room there is nothing to let back in and the guard
/// opens fully; over loud music it barely opens, and the ride that was already
/// good is left alone. At the shipping −45, the music ride keeps 94.7% recall
/// (from 97.9) and gains 11 points of precision, while the ride the complaint
/// came from gets the penalty above.
///
/// **This is a trade and not a free lunch**, and the honest statement of it is
/// that a loud enough background is a background a rider's word starts will
/// still be eaten by. Lower `ONSET_QUIET_NONE_DB` to protect the music ride
/// further at the cost of the fix; raise it for the reverse.
const ONSET_QUIET_FULL_DB: f32 = -60.0;
const ONSET_QUIET_NONE_DB: f32 = -45.0;

/// Below this the high band is silence and a "rise" is arithmetic on noise.
const ONSET_FLOOR_DB: f32 = -65.0;

/// One-pole coefficients for the recent-level envelope, per block.
///
/// Deliberately asymmetric, and the *slow* one is upwards. The envelope stands
/// for "how loud the high band has been lately": it must lag a rise, or there
/// is no rise left to detect, and it must catch up within a syllable or two so
/// a whole sentence does not sit in the relaxed state.
const ONSET_ENV_ATTACK: f32 = 0.03;
const ONSET_ENV_RELEASE: f32 = 0.25;

/// How fast the relaxation lets go, per block.
///
/// Instant on, linear off over five blocks — 50 ms, about the length of a
/// leading fricative. Stepping the cap back would put a seam in the middle of a
/// word, which is the artefact this whole change exists to remove; running much
/// longer than a consonant puts the relief on the vowel, which does not need it
/// and pays for it in separation.
const ONSET_RELEASE_PER_BLOCK: f32 = 0.2;

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
///
/// # `MIN_DB` was the suspect, and it is not guilty
///
/// The zero-mask branch does not attenuate — it *zeroes* — and 85.1% of a
/// clean ride takes it, so this was carried for a long time as the leading
/// untested explanation for speech sounding choppy: 85% of the file muted
/// outright with a hard edge at every boundary.
///
/// Measured with `dfbench --min-db` on that ride, against −16 dB, which is
/// below the model's own `lsnr_min` and so removes the branch entirely:
///
/// | `MIN_DB` | Mean frame | Zero-masked | Separation |
/// |---|---|---|---|
/// | **−10** (shipped) | **1.41 ms** | 85.1% | **14.1 dB** |
/// | −16 (branch gone) | 4.81 ms | 0% | 13.4 dB |
///
/// Removing it costs **3.4× the CPU and 0.7 dB of separation** — worse on both
/// axes, and worst on exactly the devices already stepping down the ladder.
/// The hard mute is doing real work: it is what makes the gaps silent enough
/// for the 1.8 dB the microphone gives to become 14.
///
/// What this cannot see is a click at a boundary, which no separation figure
/// measures. That needs an ear, and the chain playback in the listen sheet is
/// now the way to use one — but the cost above makes it a bad trade unless the
/// artefact turns out to be severe.
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

/// **It steps down, and never back up.** Climbing again would need the same
/// hysteresis argument the profile chooser needed, and it would be settled by
/// the same measurement that pushed it down in the first place — so a device
/// on the edge would oscillate, and every change of rung is audible. A rung is
/// treated as a fact about this device for the rest of the session.
///
/// *When* to step is decided in [`super::relief`], from the whole block's
/// wall clock rather than this stage's — see [`Enhancer::process`].
const _: () = ();

/// Watches the high band for the start of a word.
///
/// Level only — it never touches the audio. The filtered signal exists to be
/// measured and is thrown away, so a wrong answer here costs suppression for
/// 100 ms and can never damage the block.
struct OnsetGuard {
    /// Measurement only. Its output is squared and discarded.
    hp: super::dsp::Biquad,
    /// See [`ONSET_RISE_DB`], [`ONSET_RELEASE_PER_BLOCK`] and
    /// [`ONSET_QUIET_FULL_DB`]. Fields rather than constants so the harness can
    /// sweep them.
    rise_db: f32,
    release: f32,
    quiet_full_db: f32,
    quiet_none_db: f32,
    /// Recent high-band level in dB, or `None` until the first block has
    /// primed it — an envelope starting at zero or at −∞ makes the first block
    /// of every session either a false onset or an unreachable one.
    env_db: Option<f32>,
    /// How far the cap is currently relaxed, 1.0 at an onset falling to 0.
    relax: f32,
    /// Whether a fresh onset may fire.
    ///
    /// **The edge is the whole point, and leaving it out was measured.** With
    /// the guard triggering on any block whose high band stood above the
    /// envelope, the envelope's slow attack kept the condition true for the
    /// entire first syllable: it fired on 69% of onset blocks and **90% of
    /// vowel blocks**, relaxing the vowel harder than the consonant and moving
    /// the penalty 9.0 → 7.9 dB for a full dB of separation. That is the flat
    /// cap again, wearing a detector. It has to fire once per word start and
    /// then get out of the way.
    armed: bool,
    /// Blocks that ran with the cap relaxed at all. The share of a ride this
    /// covers is what says whether the window is doing something targeted or
    /// has quietly become the new cap.
    relaxed_blocks: u64,
}

impl OnsetGuard {
    fn new() -> Self {
        Self {
            hp: super::dsp::Biquad::high_pass(48_000.0, ONSET_HP_HZ, 0.707),
            rise_db: ONSET_RISE_DB,
            release: ONSET_RELEASE_PER_BLOCK,
            quiet_full_db: ONSET_QUIET_FULL_DB,
            quiet_none_db: ONSET_QUIET_NONE_DB,
            env_db: None,
            relax: 0.0,
            armed: true,
            relaxed_blocks: 0,
        }
    }

    /// Looks at one block and returns how far to relax the cap, 0 to 1.
    ///
    /// Called before the model runs, on the block the model is about to see.
    /// The model's lookahead is zero — it is the low-latency variant — so the
    /// frame this decides for is the frame that comes out.
    ///
    /// **A causal detector cannot see the first block of a rise before it has
    /// arrived**, so the very first 10 ms of a consonant is still processed at
    /// the full cap. That is the residual, and it is small against losing the
    /// whole 50–150 ms of it.
    fn look(&mut self, block: &[f32]) -> f32 {
        let mut sum = 0.0f32;
        for &s in block {
            let h = self.hp.process(s);
            sum += h * h;
        }
        let level_db = 10.0 * (sum / block.len() as f32 + 1e-12).log10();

        match self.env_db {
            None => self.env_db = Some(level_db),
            Some(env) => {
                let rising = level_db > ONSET_FLOOR_DB && level_db - env > self.rise_db;
                if rising && self.armed {
                    // Scaled by how quiet the band already was — see
                    // [`ONSET_QUIET_FULL_DB`]. `env` and not `level_db`: the
                    // question is how loud the *background* is, and `level_db`
                    // on this block is the word start itself.
                    self.relax = ((self.quiet_none_db - env)
                        / (self.quiet_none_db - self.quiet_full_db))
                        .clamp(0.0, 1.0);
                    self.armed = false;
                } else {
                    self.relax = (self.relax - self.release).max(0.0);
                }
                // Re-armed only once the rise has cleared *and* the window has
                // closed. Without the second half a long fricative would fire
                // again on its own tail.
                if !rising && self.relax == 0.0 {
                    self.armed = true;
                }
                // Updated *after* the test, so a block cannot raise the
                // envelope past itself and then be measured against it.
                let a = if level_db > env {
                    ONSET_ENV_ATTACK
                } else {
                    ONSET_ENV_RELEASE
                };
                self.env_db = Some(env + a * (level_db - env));
            }
        }
        if self.relax > 0.0 {
            self.relaxed_blocks += 1;
        }
        self.relax
    }
}

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
    /// The cap in force when no word is starting.
    base_lim_db: f32,
    /// What was last handed to the model, so an unchanged cap costs no call.
    applied_lim_db: f32,
    /// The cap while a word is starting. See [`ONSET_ATTEN_LIM_DB`].
    onset_lim_db: f32,
    /// Watches for word starts.
    onset: OnsetGuard,
    /// Whether the guard is allowed to act. Only the A/B in
    /// `core/tests/onset_survival.rs` turns it off.
    onset_guard: bool,
}

impl Enhancer {
    /// Builds the model. Slow — tens of milliseconds — so never on the audio
    /// thread.
    ///
    /// Returns a pass-through enhancer rather than an error if the model will
    /// not load, because losing the enhancement is a much smaller failure than
    /// losing the microphone.
    pub fn new() -> Self {
        Self::with_atten_lim(ATTEN_LIM_DB)
    }

    /// The same, with the attenuation cap overridden.
    ///
    /// **For measuring the cap, which is the knob that decides how much of a
    /// word start survives.** `ATTEN_LIM_DB` is the ceiling on how far the
    /// model may pull a frame down, and `core/tests/onset_survival.rs` shows
    /// onsets sitting right against it — a leading "sh" or "p" is noise-like
    /// and pitchless, which is what the model is trained to remove, and at the
    /// start of an utterance its own SNR estimate is still at the floor.
    ///
    /// Public so the sweep can run against the shipping code rather than a
    /// copy of it. Nothing in the app calls this.
    pub fn with_atten_lim(atten_lim_db: f32) -> Self {
        let model = match Self::build_with(atten_lim_db) {
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
            base_lim_db: atten_lim_db,
            applied_lim_db: atten_lim_db,
            onset_lim_db: ONSET_ATTEN_LIM_DB,
            onset: OnsetGuard::new(),
            onset_guard: true,
        }
    }

    /// Turns the word-start guard off, for measuring what it does.
    ///
    /// Public for `core/tests/onset_survival.rs`, which needs both sides of the
    /// comparison from the shipping code rather than from a copy of it. Nothing
    /// in the app calls this.
    pub fn set_onset_guard(&mut self, on: bool) {
        self.onset_guard = on;
    }

    /// Retunes the word-start guard: where it listens, how big a jump counts,
    /// how fast it lets go, and how far it relaxes the cap.
    ///
    /// Also for the harness. Four constants that interact — a higher corner
    /// wants a smaller jump, a longer window wants a weaker cap — and tuning
    /// them one at a time by editing the file and rebuilding is how a local
    /// minimum gets shipped.
    pub fn set_onset_tuning(&mut self, hp_hz: f32, rise_db: f32, release: f32, lim_db: f32) {
        self.onset.hp = super::dsp::Biquad::high_pass(48_000.0, hp_hz, 0.707);
        self.onset.rise_db = rise_db;
        self.onset.release = release;
        self.onset_lim_db = lim_db;
    }

    /// Where the guard stops opening because the background is too loud to let
    /// back in. See [`ONSET_QUIET_FULL_DB`]. Also for the harness.
    pub fn set_onset_quiet(&mut self, full_db: f32, none_db: f32) {
        self.onset.quiet_full_db = full_db;
        self.onset.quiet_none_db = none_db;
    }

    /// How many blocks ran with the cap relaxed, and how many ran at all.
    ///
    /// The share is the number to look at: a guard that is open most of the
    /// time is not a guard, it is a lower cap with extra steps.
    pub fn onset_relief(&self) -> (u64, u64) {
        (self.onset.relaxed_blocks, self.frames)
    }

    /// How far the cap was relaxed for the last block, 0 to 1.
    ///
    /// For the harness, which needs to know *where* the guard fired and not
    /// only how often: "open 40% of the time" reads the same whether it is
    /// covering every word start or none of them.
    pub fn onset_relax(&self) -> f32 {
        self.onset.relax
    }

    fn build_with(atten_lim_db: f32) -> Result<DfTract> {
        // Mono. Every route this app records from is one channel by the time
        // it reaches the chain.
        let params = RuntimeParams::default_with_ch(1)
            .with_atten_lim(atten_lim_db)
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
        if self.model.is_none() {
            return;
        }
        if block.len() != HOP {
            // The chain is fixed at `FRAME_SIZE`, so this cannot happen from
            // the worker — but a wrong-sized block would panic inside the
            // model, and going quiet is not worth a shape mismatch.
            return;
        }

        // Before the model, on the block the model is about to see. Deliberately
        // outside the stopwatch below: this is one biquad and a sum over 480
        // samples, and folding it into the enhancer's frame time would put it
        // in the number the performance ladder steps down on.
        let change = if self.onset_guard {
            let relax = self.onset.look(block);
            // Linear in dB, which is where the ear is and where the cap is
            // expressed. At relax = 1 the model may only pull the frame down by
            // `ONSET_ATTEN_LIM_DB`.
            let want = self.base_lim_db + relax * (self.onset_lim_db - self.base_lim_db);
            if (want - self.applied_lim_db).abs() > 0.01 {
                self.applied_lim_db = want;
                Some(want)
            } else {
                None
            }
        } else {
            None
        };

        let model = self.model.as_mut().expect("checked above");
        if let Some(want) = change {
            model.set_atten_lim(want);
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
            // **Counted, and no longer acted on.** This used to step the rung
            // down by itself, and that is exactly how a model measured at
            // 6.2 ms against a 10 ms budget came to switch itself off on the
            // phone it was reported from: the enhancer carried the only
            // stopwatch in the chain, so it was the only stage that could be
            // blamed for a late block. The deadline belongs to the block, so
            // the decision now belongs to the worker — see `audio::relief`.
            // The counters stay because the panel reports them.
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
                "DeepFilterNet stepped down to its lowest setting and the chain \
                 still could not keep up; it runs without the enhancer now"
            ),
            other => tracing::warn!("DeepFilterNet stepping down to {other:?}"),
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

    /// The guard's shape, without needing the model or a ride.
    ///
    /// Three properties, and each one was a bug before it was a test: it fires
    /// once per rise rather than continuously, it holds for about a fricative
    /// rather than a syllable, and it does not open on a loud background.
    #[test]
    fn the_word_start_guard_fires_once_and_only_when_there_is_room() {
        let mut g = OnsetGuard::new();
        let quiet = vec![0.0f32; HOP];
        // A block of broadband noise, which is what a leading "sh" looks like
        // to a level detector. Deterministic rather than random: a test that
        // fires on a different sample every run cannot bisect.
        let loud: Vec<f32> = (0..HOP)
            .map(|i| 0.2 * ((i as f32 * 12.9898).sin() * 43758.547).fract())
            .collect();

        for _ in 0..50 {
            g.look(&quiet);
        }
        assert_eq!(g.look(&quiet), 0.0, "silence must not open it");

        // The rise fires it, and it then lets go over about five blocks.
        let first = g.look(&loud);
        assert!(
            first > 0.5,
            "a jump into the high band must open it: {first}"
        );
        let mut open = 1;
        while g.look(&loud) > 0.0 {
            open += 1;
            assert!(open < 20, "it never closed while the loud signal continued");
        }
        assert!(
            (4..=7).contains(&open),
            "the window should be about a fricative long, was {open} blocks"
        );

        // Over a loud background there is nothing to hand back, so it stays
        // shut however sharp the rise. See `ONSET_QUIET_NONE_DB`.
        let mut g = OnsetGuard::new();
        let background: Vec<f32> = loud.iter().map(|s| s * 0.5).collect();
        for _ in 0..200 {
            g.look(&background);
        }
        let louder: Vec<f32> = loud.iter().map(|s| s * 4.0).collect();
        assert_eq!(
            g.look(&louder),
            0.0,
            "it must not relax the cap when the background is loud enough to come back with it"
        );
    }

    #[test]
    fn the_model_loads_and_its_hop_is_our_block() {
        // The whole integration rests on this: if the model's hop were not
        // 480 samples at 48 kHz, every block would need buffering and the
        // latency argument would change with it.
        // Built directly rather than through `Enhancer::new`, which swallows
        // the reason by design -- here the reason is the whole point.
        let built = Enhancer::build_with(ATTEN_LIM_DB);
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
