//! Hearing a tap, instead of feeling it.
//!
//! **This replaces the accelerometer as the primary sensor, and the original
//! specification predicted it would have to.** It said the documented remedy
//! for pocket false positives is "audio first with the accelerometer
//! confirming", chose the accelerometer only because iOS exposes one input
//! route at a time, and named the risk exactly: at 100 Hz a tap's
//! high-frequency content is above Nyquist, so the operator's ω² weighting
//! "buys far less than it does at Android's 400". In the field it bought
//! nothing — a rider tapped and the gesture did not fire.
//!
//! # What a tap sounds like
//!
//! Measured on two recordings whose taps the rider confirmed were audible,
//! 2026-10-09:
//!
//! - it rises **threefold between consecutive milliseconds**;
//! - it stands about **ten times above the trailing second**;
//! - it **decays to a tenth within forty milliseconds** (p90: 46 ms on one
//!   recording, 107 ms on the other, which was a desk and rang).
//!
//! Forty milliseconds at 48 kHz is forty one-millisecond frames of shape. The
//! accelerometer gave four samples for the same event. That ratio is the whole
//! argument for this module.
//!
//! # Which microphone
//!
//! Whichever one there is, which is the other half of the idea:
//!
//! - **While capturing**, the helmet's. The audio is already flowing through
//!   the chain, so this costs nothing — and a rider tapping their helmet to
//!   stop talking is a gesture that needs no hand off the bars.
//! - **While listening**, the phone's own. That is the half with a cost: an
//!   iOS session wanting input cannot be the output-only `.playback` the
//!   listening state uses today, and `CLAUDE.md` records that offering A2DP to
//!   a session that wants input lets iOS take the input away — reported once
//!   as "recording only works when music is not playing".
//!
//! There is a large consolation in that second case, and it is not a
//! coincidence: an app holding a live input is one iOS does not suspend. The
//! tap gesture dying a minute after the screen locked was the app being
//! suspended and Core Motion stopping with it.
//!
//! # What it does not do
//!
//! It never keeps the audio. Only the envelope is examined, frame by frame,
//! and nothing is stored, transmitted or written. That matters most in the
//! listening state, where the microphone would be open for a whole ride, and
//! it belongs in `docs/privacy.md` rather than being left implied.

use super::gesture::{Banked, Candidate, GesturePattern, TapGesture};

/// The envelope's frame, in samples at 48 kHz. One millisecond.
///
/// Fine enough to see the attack — a tap triples inside one — and coarse
/// enough that a ride is a few hundred thousand frames rather than tens of
/// millions.
pub const FRAME: usize = 48;

/// The lowest level the floor may believe in, in dBFS.
///
/// One least-significant bit of sixteen-bit audio. See the note where it is
/// used: this is the same fault that made the accelerometer detector arm
/// itself ninety times an hour, and setting the clamp to a number picked for
/// being low rather than for meaning something reproduced it exactly.
const QUANTISATION_DBFS: f32 = -90.0;

/// How fast the reference level follows the audio, per 1 ms frame.
///
/// **A slow average, not a minimum statistic, and that distinction cost a
/// measurement to learn.** `NoiseFloorTracker` tracks the quietest recent
/// moment, which is right for ψ on an accelerometer — steady at rest — and
/// quite wrong for a 1 ms audio envelope, which is spiky: the quietest frame
/// in the last second sits near silence between words while a typical frame
/// is forty decibels above it. Measured against that, every frame of ordinary
/// speech cleared a twenty-six decibel margin and nothing was ever a tap.
///
/// What works is the rule `tools/tap/label_from_audio.py` already uses to find
/// these taps: loud **relative to the trailing second**. This is that, as an
/// exponential average, which costs one multiply instead of a sorted window.
///
/// 1/1000 gives roughly a one-second memory. A forty-millisecond tap twenty
/// decibels up moves it by under a decibel, so it does not chase its own
/// trigger and needs no gating.
const REFERENCE_ALPHA: f32 = 0.001;

/// How far above the trailing average a frame must reach, in dB.
///
/// The labeller's rule is eight times the trailing median, which is 18 dB, and
/// it finds every gesture in the corpus. Scored by
/// `core/tests/acoustic_tap_corpus.rs`; override with `MW_TAP_MARGIN_DB` to
/// sweep it.
const MARGIN_DB: f32 = 18.0;

/// How sharply it must arrive: this frame against the one before.
///
/// Measured at threefold, which is ~9.5 dB. The test is what separates a tap
/// from speech — a voice rises over tens of milliseconds, a strike inside one.
const ATTACK_DB: f32 = 7.0;

/// The longest a tap may take to fall back below the margin.
///
/// Measured: a tenth of peak within 40 ms typically, 107 ms on a desk that
/// rang. Past this it is a sound, not a strike.
const MAX_DECAY_MS: u32 = 140;

/// Dead time after a candidate, to skip the ring rather than count it twice.
const LATENT_MS: u32 = 70;

/// What the detector is seeing, for the panel and the rig.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AcousticStats {
    pub frames: u64,
    pub candidates: u64,
    /// Impulses that arrived sharply but never fell back in time.
    pub discarded_long: u64,
    pub banked: u64,
    pub gestures: u64,
    pub level_db: f32,
    pub floor_db: f32,
    pub peak_db: f32,
    pub run: u8,
}

/// Listens for N taps in whatever microphone it is given.
pub struct AcousticTapDetector {
    /// The trailing average the margin is measured against.
    reference_db: f32,
    pattern: GesturePattern,
    previous_db: f32,
    /// When the frame that started a candidate arrived, and how loud it was.
    rising_at_ms: Option<u64>,
    peak_db: f32,
    latent_until_ms: u64,
    frame_ms: u64,
    stats: AcousticStats,
}

impl AcousticTapDetector {
    pub fn new(taps_wanted: u8) -> Self {
        Self {
            reference_db: f32::NEG_INFINITY,
            pattern: GesturePattern::new(taps_wanted),
            previous_db: f32::NEG_INFINITY,
            rising_at_ms: None,
            peak_db: f32::NEG_INFINITY,
            latent_until_ms: 0,
            frame_ms: 0,
            stats: AcousticStats::default(),
        }
    }

    pub fn stats(&self) -> AcousticStats {
        self.stats
    }

    pub fn set_taps_wanted(&mut self, taps: u8) {
        self.pattern.set_taps_wanted(taps);
    }

    /// Feeds a block of 48 kHz mono audio, however long it happens to be.
    ///
    /// Answers `Some` on the frame that completes a gesture. Returns through
    /// `banked` whether a single tap landed, which is what the acknowledging
    /// tick is played from: three taps heard and no gesture is a completely
    /// different diagnosis from three taps not heard, and only a per-tap
    /// signal can tell a rider which they have.
    pub fn push(&mut self, block: &[f32], banked: &mut bool) -> Option<TapGesture> {
        let mut gesture = None;
        for frame in block.chunks(FRAME) {
            if let Some(g) = self.frame(frame, banked) {
                gesture = Some(g);
            }
        }
        gesture
    }

    fn frame(&mut self, frame: &[f32], banked: &mut bool) -> Option<TapGesture> {
        self.frame_ms += 1;
        self.stats.frames += 1;

        let peak = frame.iter().fold(0.0f32, |m, s| m.max(s.abs()));
        // Clamped at one quantisation step, and this is load-bearing.
        //
        // **The accelerometer detector shipped the identical fault and this
        // one was written with it in hand and still got it wrong**, which is
        // worth recording: the clamp was set to −140 dBFS, a number chosen for
        // being safely low rather than for meaning anything. A digitally
        // silent block then dragged the minimum statistic down there, and on
        // real recordings the floor sat at −137 while the signal sat at −40 —
        // ninety-seven decibels over a margin of twenty-six, so every frame
        // was a candidate, nothing ever fell back, and the detector found
        // none of the taps a rider could plainly hear.
        //
        // −90 dBFS is one least-significant bit of sixteen-bit audio. Below
        // that there is nothing a microphone can have told us, so the floor
        // has no business going there.
        let level_db = (20.0 * (peak + 1e-7).log10()).max(QUANTISATION_DBFS);
        let at_ms = self.frame_ms;

        // The reference follows the audio slowly enough that a tap cannot
        // drag it up behind itself — see `REFERENCE_ALPHA` — so unlike the
        // minimum-statistics floor it needs no gate, and there is no state in
        // which it can latch.
        if !self.reference_db.is_finite() {
            self.reference_db = level_db;
        }
        self.reference_db += (level_db - self.reference_db) * REFERENCE_ALPHA;
        let floor_db = self.reference_db;
        let over = level_db - floor_db;
        let attack = level_db - self.previous_db;
        self.previous_db = level_db;
        self.stats.level_db = level_db;
        self.stats.floor_db = floor_db;

        match self.rising_at_ms {
            None => {
                // **Both tests, and the attack is the one that rejects
                // speech.** A voice crosses the margin too, over tens of
                // milliseconds; a strike crosses it inside one.
                if over >= MARGIN_DB && attack >= ATTACK_DB {
                    self.rising_at_ms = Some(at_ms);
                    self.peak_db = over;
                }
            }
            Some(since) => {
                self.peak_db = self.peak_db.max(over);
                if over >= MARGIN_DB {
                    if at_ms.saturating_sub(since) > MAX_DECAY_MS as u64 {
                        // Still loud long after a tap would have died away.
                        self.rising_at_ms = None;
                        self.stats.peak_db = self.peak_db;
                        self.stats.discarded_long += 1;
                        self.peak_db = f32::NEG_INFINITY;
                    }
                } else {
                    // Fell back in time: a strike.
                    let strength_db = self.peak_db;
                    self.rising_at_ms = None;
                    self.peak_db = f32::NEG_INFINITY;
                    self.stats.peak_db = strength_db;
                    if at_ms < self.latent_until_ms {
                        return None;
                    }
                    self.latent_until_ms = at_ms + LATENT_MS as u64;
                    self.stats.candidates += 1;
                    return self.offer(
                        Candidate {
                            at_ms: since,
                            strength_db,
                        },
                        banked,
                    );
                }
            }
        }
        None
    }

    fn offer(&mut self, c: Candidate, banked: &mut bool) -> Option<TapGesture> {
        match self.pattern.offer(c) {
            Banked::Ignored => None,
            Banked::Added { run } => {
                self.stats.banked += 1;
                self.stats.run = run;
                *banked = true;
                None
            }
            Banked::RestartedOnMagnitude | Banked::RestartedOnInterval => {
                self.stats.banked += 1;
                self.stats.run = 1;
                *banked = true;
                None
            }
            Banked::Completed(g) => {
                self.stats.banked += 1;
                self.stats.gestures += 1;
                self.stats.run = 0;
                *banked = true;
                Some(g)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tap as it was measured: a sharp frame, then a decay to a tenth in
    /// about forty milliseconds.
    fn tap(into: &mut Vec<f32>, peak: f32) {
        for ms in 0..60 {
            let a = peak * (-(ms as f32) / 14.0).exp();
            for i in 0..FRAME {
                // Broadband, which is what a strike is.
                let t = (ms * FRAME + i) as f32;
                into.push(a * ((t * 2.399).sin() + (t * 7.13).sin()) * 0.5);
            }
        }
    }

    fn quiet(into: &mut Vec<f32>, ms: usize) {
        for m in 0..ms {
            for i in 0..FRAME {
                let t = (m * FRAME + i) as f32;
                into.push(0.0008 * (t * 0.37).sin());
            }
        }
    }

    #[test]
    fn three_taps_on_a_beat_are_a_gesture() {
        let mut audio = Vec::new();
        quiet(&mut audio, 400);
        for _ in 0..3 {
            tap(&mut audio, 0.4);
            quiet(&mut audio, 160);
        }
        quiet(&mut audio, 200);

        let mut d = AcousticTapDetector::new(3);
        let mut banked = false;
        let g = d.push(&audio, &mut banked);
        assert!(g.is_some(), "no gesture: {:?}", d.stats());
        assert_eq!(d.stats().gestures, 1);
        assert_eq!(d.stats().banked, 3, "a tick for each tap");
    }

    #[test]
    fn a_quiet_microphone_hears_nothing() {
        let mut audio = Vec::new();
        quiet(&mut audio, 4_000);
        let mut d = AcousticTapDetector::new(3);
        let mut banked = false;
        assert!(d.push(&audio, &mut banked).is_none());
        assert_eq!(d.stats().gestures, 0);
        assert_eq!(d.stats().candidates, 0, "silence produced candidates");
    }

    #[test]
    fn a_sound_that_does_not_die_away_is_not_a_tap() {
        // The test that separates a strike from everything else loud: a tap
        // is over in forty milliseconds, a shout is not.
        let mut audio = Vec::new();
        quiet(&mut audio, 400);
        for m in 0..800 {
            for i in 0..FRAME {
                let t = (m * FRAME + i) as f32;
                audio.push(0.3 * ((t * 2.399).sin() + (t * 7.13).sin()) * 0.5);
            }
        }
        quiet(&mut audio, 400);

        let mut d = AcousticTapDetector::new(3);
        let mut banked = false;
        assert!(d.push(&audio, &mut banked).is_none());
        assert_eq!(
            d.stats().gestures,
            0,
            "a sustained sound completed a gesture"
        );
    }

    #[test]
    fn a_slow_swell_is_not_a_tap_however_loud_it_gets() {
        // The attack test, which is what rejects a voice: it reaches the same
        // level, over tens of milliseconds instead of one.
        let mut audio = Vec::new();
        quiet(&mut audio, 400);
        for m in 0..300 {
            let a = 0.4 * (m as f32 / 300.0);
            for i in 0..FRAME {
                let t = (m * FRAME + i) as f32;
                audio.push(a * (t * 2.399).sin());
            }
        }
        quiet(&mut audio, 400);

        let mut d = AcousticTapDetector::new(3);
        let mut banked = false;
        assert!(d.push(&audio, &mut banked).is_none());
        assert_eq!(d.stats().gestures, 0);
    }
}
