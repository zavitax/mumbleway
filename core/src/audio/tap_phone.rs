//! Taps on the phone's own microphone, by amplitude alone.
//!
//! **The rider's rule, and deliberately the whole of it**: a sharp peak of
//! amplitude with a relatively fast decline is a tap. Three of them inside
//! three seconds is the gesture. Nothing else is consulted — no spectral
//! ratio, no tail measurement, no speech verdict from the chain.
//!
//! # Why this path is the simple one
//!
//! It is the easy problem, and it was being solved with the hard problem's
//! tools. The phone's microphone is 48 kHz and full-band, the thing striking
//! it is in contact with its case, and nothing else in the room is. The
//! difficult cases all belong to the other path: a helmet hears plosives from
//! two centimetres away and this app's own cues coming back a round trip
//! later, and the rules that deal with those were costing the phone recall it
//! did not need to lose.
//!
//! So the two were separated at a rider's request, and this half kept short.
//! What a tap has to be here:
//!
//! - **loud, against the room**: [`MARGIN_DB`] over the trailing average,
//!   which is what makes the rule work in a car park and in a slipstream
//!   without a number for either;
//! - **sharp**: [`ATTACK_DB`] in one millisecond, which is what rejects
//!   speech and music — both reach the margin, neither crosses it that fast,
//!   because only contact with the case does;
//! - **brief**: back under the margin inside [`MAX_DECAY_MS`], which is what
//!   rejects a shout, a horn, or anything else loud that stays.
//!
//! The band-pass and the self-noise gate are in [`super::tap_envelope`],
//! because they describe the microphone rather than the gesture.

use super::gesture::{Banked, Candidate, GesturePattern, TapGesture};
use super::tap_envelope::{tuned, Envelope, TapStats, FRAME};

/// How far above the trailing average a frame must reach, in dB.
///
/// The labeller's rule is eight times the trailing median, which is 18 dB, and
/// it finds every gesture in the corpus. Scored by
/// `core/tests/acoustic_tap_corpus.rs`; `MW_TAP_MARGIN_DB` sweeps it.
const MARGIN_DB: f32 = 18.0;

/// How sharply it must arrive: this frame against the one before, in dB.
///
/// **This is the test that does the work**, and it separates *contact* from
/// *airborne*. A fingertip on the case is coupled to the microphone through
/// the case, so the level steps up inside a single millisecond; anything
/// arriving through air has its onset shaped by the speaker, the room and the
/// distance.
///
/// Measured over 1175 s of corpus, as p5 / median / p95 of the attack of every
/// impulse sharp enough to be judged:
///
/// | | attack, dB |
/// |---|---|
/// | taps on the phone body | +9.2 / **+23.4** / +29.1 |
/// | taps held in a hand | +10.8 / **+25.1** / +34.5 |
/// | real music, real microphone | +7.0 / **+7.9** / +11.6 |
/// | every road and voice ride | ~+7.1 / +8…+13 / — |
///
/// Twelve decibels of daylight between music's p95 and the taps' median.
/// Swept against both halves: at 7 dB real music completes **52 gestures an
/// hour**; at 12 it completes none; at 14 it still completes none while
/// phone-tap recall is at its best measured value, ten of ten. Above 16,
/// recall falls without buying anything, since music is already at zero.
///
/// `MW_TAP_ATTACK_DB` sweeps it.
const ATTACK_DB: f32 = 14.0;

/// How long an impulse may stay over the margin before it is not a tap, in ms.
///
/// The decline half of the rider's rule. Measured at a tenth within forty
/// milliseconds on recordings whose taps were audible, with a p90 of 46 ms on
/// one and 107 on the other — which was a desk and rang. 140 covers both and
/// still rejects anything sustained.
const MAX_DECAY_MS: u32 = 140;

/// Dead time after a tap, in ms.
///
/// **Not a tuning knob.** A tap on a phone's body rings, and the ringing
/// crosses the threshold again on the way down, so one strike offers several
/// candidates a few tens of milliseconds apart. This is what makes a tap one
/// tap, and the gesture's own `GAP_MIN_MS` is the second line of the same
/// defence.
const LATENT_MS: u32 = 70;

/// Listens for taps on the phone's own microphone.
pub struct PhoneTapDetector {
    envelope: Envelope,
    pattern: GesturePattern,
    margin_db: f32,
    attack_db: f32,
    /// When the frame that started a candidate arrived, and how far over the
    /// floor it has reached since.
    rising_at_ms: Option<u64>,
    peak_over: f32,
    latent_until_ms: u64,
    stats: TapStats,
}

impl Default for PhoneTapDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl PhoneTapDetector {
    pub fn new() -> Self {
        Self {
            envelope: Envelope::new(),
            pattern: GesturePattern::new(),
            margin_db: tuned("MW_TAP_MARGIN_DB", MARGIN_DB),
            attack_db: tuned("MW_TAP_ATTACK_DB", ATTACK_DB),
            rising_at_ms: None,
            peak_over: f32::NEG_INFINITY,
            latent_until_ms: 0,
            stats: TapStats::default(),
        }
    }

    pub fn stats(&self) -> TapStats {
        self.stats
    }

    /// Tells the detector how loud this app's own output is.
    pub fn hear_own_output(&mut self, level_dbfs: f32) {
        self.envelope.hear_own_output(level_dbfs, false);
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
        let r = self.envelope.frame(frame);
        self.stats.frames += 1;
        self.stats.level_db = r.level_db;
        self.stats.floor_db = r.floor_db;
        self.stats.meter_db = self.envelope.meter_db();

        if r.deaf {
            // Whatever was in flight is abandoned rather than finished on
            // audio nobody was allowed to look at.
            self.rising_at_ms = None;
            self.peak_over = f32::NEG_INFINITY;
            return None;
        }

        match self.rising_at_ms {
            None => {
                // Loud, against the room, and sharp. Both, because speech and
                // music reach the margin too and neither arrives this fast.
                if r.over >= self.margin_db && r.attack >= self.attack_db {
                    self.stats.impulse_attack_db = r.attack;
                    self.stats.impulse_over_db = r.over;
                    self.rising_at_ms = Some(r.at_ms);
                    self.peak_over = r.over;
                }
            }
            Some(since) => {
                self.peak_over = self.peak_over.max(r.over);
                if r.over >= self.margin_db {
                    if r.at_ms.saturating_sub(since) > MAX_DECAY_MS as u64 {
                        // Still loud long after a tap would have died away:
                        // a sound, not a strike.
                        self.rising_at_ms = None;
                        self.stats.peak_db = self.peak_over;
                        self.stats.discarded_long += 1;
                        self.peak_over = f32::NEG_INFINITY;
                    }
                } else {
                    // Fell back in time. That is the whole of the rule.
                    let strength_db = self.peak_over;
                    self.rising_at_ms = None;
                    self.peak_over = f32::NEG_INFINITY;
                    self.stats.peak_db = strength_db;
                    if r.at_ms < self.latent_until_ms {
                        return None;
                    }
                    self.latent_until_ms = r.at_ms + LATENT_MS as u64;
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
            Banked::RestartedOnMagnitude => {
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

    /// A tap as it was measured: full level in the first frame, then a decay
    /// to a tenth in about forty milliseconds, in the band taps occupy.
    ///
    /// **The frequencies here used to be wrong, and wrong in the direction
    /// that mattered.** This generator ran at 2.399 and 7.13 radians a sample,
    /// which is 18.3 kHz and — past Nyquist — an aliased 6.5 kHz. So for as
    /// long as it existed, the fixture proving taps are *found* was
    /// spectrally a cymbal. 1.6 and 3.1 kHz are inside the measured band.
    pub(super) fn tap(into: &mut Vec<f32>, peak: f32) {
        for ms in 0..60 {
            let a = peak * (-(ms as f32) / 14.0).exp();
            for i in 0..FRAME {
                let t = (ms * FRAME + i) as f32;
                into.push(a * ((t * 0.209).sin() + (t * 0.406).sin()) * 0.5);
            }
        }
    }

    pub(super) fn quiet(into: &mut Vec<f32>, ms: usize) {
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

        let mut d = PhoneTapDetector::new();
        let mut banked = false;
        assert!(
            d.push(&audio, &mut banked).is_some(),
            "no gesture: {:?}",
            d.stats()
        );
        assert_eq!(d.stats().gestures, 1);
        assert_eq!(d.stats().banked, 3, "a tick for each tap");
    }

    #[test]
    fn a_quiet_microphone_hears_nothing() {
        let mut audio = Vec::new();
        quiet(&mut audio, 4_000);
        let mut d = PhoneTapDetector::new();
        let mut banked = false;
        assert!(d.push(&audio, &mut banked).is_none());
        assert_eq!(d.stats().candidates, 0, "silence produced candidates");
    }

    /// The decline half of the rule: a tap is over in forty milliseconds, a
    /// shout is not.
    #[test]
    fn a_sound_that_does_not_die_away_is_not_a_tap() {
        let mut audio = Vec::new();
        quiet(&mut audio, 400);
        for m in 0..800 {
            for i in 0..FRAME {
                let t = (m * FRAME + i) as f32;
                audio.push(0.3 * ((t * 0.209).sin() + (t * 0.406).sin()) * 0.5);
            }
        }
        quiet(&mut audio, 400);

        let mut d = PhoneTapDetector::new();
        let mut banked = false;
        assert!(d.push(&audio, &mut banked).is_none());
        assert_eq!(d.stats().gestures, 0, "a sustained sound was a gesture");
        assert!(
            d.stats().discarded_long > 0,
            "it was rejected for the wrong reason"
        );
    }

    /// The sharpness half: a voice reaches the same level, over tens of
    /// milliseconds instead of one.
    #[test]
    fn a_slow_swell_is_not_a_tap_however_loud_it_gets() {
        let mut audio = Vec::new();
        quiet(&mut audio, 400);
        for m in 0..300 {
            let a = 0.4 * (m as f32 / 300.0);
            for i in 0..FRAME {
                let t = (m * FRAME + i) as f32;
                audio.push(a * (t * 0.209).sin());
            }
        }
        quiet(&mut audio, 400);

        let mut d = PhoneTapDetector::new();
        let mut banked = false;
        assert!(d.push(&audio, &mut banked).is_none());
        assert_eq!(d.stats().gestures, 0);
    }

    /// Music on a beat, which is the false arm that matters.
    ///
    /// A drum is struck somewhere else and arrives through air: its onset is
    /// shaped by the speaker, the room and the distance, and comes out at a
    /// median of 7.9 dB a millisecond against 23 for a fingertip on the case.
    /// Three of them on an even beat, because **a bar of music is matched
    /// impulses at matched intervals** — exactly what the gesture pattern
    /// looks for, and better matched than a human hand manages. The attack is
    /// the only thing between a backbeat and a stop command.
    #[test]
    fn music_on_a_beat_is_not_a_gesture() {
        let mut audio = Vec::new();
        quiet(&mut audio, 400);
        for _ in 0..3 {
            for ms in 0..200 {
                let a =
                    0.5 * (ms as f32 / 8.0).min(1.0) * (-(ms as f32 - 8.0).max(0.0) / 40.0).exp();
                for i in 0..FRAME {
                    let t = (ms * FRAME + i) as f32;
                    audio.push(a * ((t * 0.209).sin() + (t * 0.406).sin()) * 0.5);
                }
            }
            quiet(&mut audio, 100);
        }
        quiet(&mut audio, 400);

        let mut d = PhoneTapDetector::new();
        let mut banked = false;
        assert!(d.push(&audio, &mut banked).is_none());
        assert_eq!(
            d.stats().gestures,
            0,
            "music on a beat was a gesture: {:?}",
            d.stats()
        );
    }

    /// **The feedback loop this nearly shipped with.** The tick played to
    /// acknowledge a tap is a 2 kHz click, which is exactly the band a tap on
    /// the phone's body occupies, and the detector runs on the raw microphone
    /// before any canceller. Without the gate, the acknowledgement is heard
    /// as the next tap and acknowledged in turn.
    #[test]
    fn the_app_does_not_hear_its_own_noise() {
        let mut d = PhoneTapDetector::new();
        let mut banked = false;
        let mut quiet_audio = Vec::new();
        quiet(&mut quiet_audio, 400);
        d.push(&quiet_audio, &mut banked);

        for _ in 0..3 {
            d.hear_own_output(-20.0);
            let mut audio = Vec::new();
            tap(&mut audio, 0.5);
            quiet(&mut audio, 60);
            d.push(&audio, &mut banked);
        }
        assert_eq!(
            d.stats().gestures,
            0,
            "the app heard its own cue as taps: {:?}",
            d.stats()
        );
    }
}
