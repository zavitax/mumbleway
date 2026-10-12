//! Taps on a helmet's microphone, which is a harder problem than the phone's.
//!
//! **Separated from [`super::tap_phone`] at a rider's request**, and the
//! separation is justified by what this path has to survive that the phone's
//! does not:
//!
//! - **Plosives from two centimetres away.** /p/, /t/ and /k/ at a boom
//!   microphone are small impacts. The attack test cannot tell one from a
//!   fingertip and should not be expected to, because acoustically they are
//!   the same event — so this path needs a second test the phone's does not,
//!   and that is [`DECAY_DROP_DB`].
//! - **This app's own sound, a round trip late.** A cue crosses the link,
//!   comes out of the helmet's speaker, is picked up by its own microphone and
//!   crosses back — two or three hundred milliseconds, against the 120 that
//!   covers a phone's own speaker.
//! - **A narrowband link.** Hands-free is 8 or 16 kHz where the phone's own
//!   microphone is 48, so the band a tap arrives in is clipped at the top.
//!
//! Those are the reasons the one shared rule was simultaneously too strict for
//! the phone and too loose here. **This half is not yet right**, and is the
//! next thing to be worked on with a clean recording: the only helmet
//! recording in the corpus was 66.9% digital zero from a session fault, so
//! every measurement taken from it — including the attack figures quoted below
//! — is contaminated by gap-ends that look like infinite attacks.

use super::gesture::{Banked, Candidate, GesturePattern, TapGesture};
use super::tap_envelope::{tuned, Envelope, TapStats, FRAME};

/// How far above the trailing average a frame must reach, in dB.
const MARGIN_DB: f32 = 18.0;

/// How sharply it must arrive: this frame against the one before, in dB.
///
/// The same number as the phone's for now, and **held separately on purpose**
/// so it can move without touching the phone. The figures it was set from are
/// phone-mic figures; the helmet's own are not trustworthy yet, for the reason
/// in the module header.
const ATTACK_DB: f32 = 14.0;

/// How far an impulse must have fallen 15 to 30 ms after its peak, in dB.
///
/// **What separates a plosive from a tap**, and the test the phone's path does
/// not need. A tap is followed by nothing; a consonant is followed by its
/// vowel.
///
/// Measured 2026-10-12 as the tail-to-peak energy ratio of every transient in
/// the corpus — the one feature of a classifier proposed by the rider that
/// scored well on helmet taps:
///
/// | | p5 / median / p95 |
/// |---|---|
/// | taps on the helmet | 0.001 / **0.002** / **0.006** |
/// | taps on the phone body | 0.000 / 0.001 / 0.149 |
/// | voice with motor noise | **0.055** / 0.098 / 0.195 |
/// | voice over motorcycle noise | 0.000 / 0.105 / 0.454 |
///
/// **Those are energy ratios and this constant is a peak-level drop, which is
/// not the same unit.** Ten decibels of level is about a tenth in amplitude
/// and a hundredth in energy, so −10 dB here lands near 0.01 as a ratio —
/// between the helmet taps at 0.002 and the voice median at 0.098.
///
/// Swept in the detector rather than inferred from the table, because the
/// band-pass and windows differ from the harness the table came from: −10 dB
/// keeps every phone-body gesture and took the corpus from 152 false arms an
/// hour to 60.
///
/// **The first attempt at this failed and the difference is instructive.** It
/// required the level never to come back over the margin for 90 ms, and real
/// taps ring: the ringing crosses the margin and recall on two of three tap
/// recordings went to zero. A maximum over a window ignores the bumps a
/// never-exceeds test trips on.
const DECAY_DROP_DB: f32 = -10.0;

/// The window the drop is measured over, after the peak, in ms.
const DECAY_FROM_MS: u64 = 15;
const DECAY_TO_MS: u64 = 30;

/// How long after speech the detector stays deaf, in ms.
///
/// Short, because speech is reported per block and the flag is already one
/// block behind the callback: this bridges the gaps *within* an utterance
/// rather than outlasting it. A rider who stops talking to tap is heard a
/// tenth of a second later.
const SPEECH_TAIL_MS: u32 = 100;

/// How long an impulse may stay over the margin before it is not a tap.
const MAX_DECAY_MS: u32 = 140;

/// Dead time after a tap, in ms. See the phone path: a strike rings, and this
/// is what makes one strike one tap.
const LATENT_MS: u32 = 70;

/// An impulse that has peaked and is having its tail measured.
struct Held {
    candidate: Candidate,
    peak_db: f32,
    peak_at_ms: u64,
    tail_db: f32,
}

/// Listens for taps on a helmet's microphone.
pub struct HelmetTapDetector {
    envelope: Envelope,
    pattern: GesturePattern,
    margin_db: f32,
    attack_db: f32,
    decay_drop_db: f32,
    rising_at_ms: Option<u64>,
    peak_over: f32,
    /// The absolute level at the peak, and when it was, for the tail measure.
    peak_level_db: f32,
    peak_at_ms: u64,
    latent_until_ms: u64,
    held: Option<Held>,
    stats: TapStats,
}

impl Default for HelmetTapDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl HelmetTapDetector {
    pub fn new() -> Self {
        Self {
            envelope: Envelope::new(),
            pattern: GesturePattern::new(),
            margin_db: tuned("MW_TAP_MARGIN_DB", MARGIN_DB),
            attack_db: tuned("MW_TAP_ATTACK_DB", ATTACK_DB),
            decay_drop_db: tuned("MW_TAP_DECAY_DROP_DB", DECAY_DROP_DB),
            rising_at_ms: None,
            peak_over: f32::NEG_INFINITY,
            peak_level_db: f32::NEG_INFINITY,
            peak_at_ms: 0,
            latent_until_ms: 0,
            held: None,
            stats: TapStats {
                on_helmet: true,
                ..TapStats::default()
            },
        }
    }

    pub fn stats(&self) -> TapStats {
        self.stats
    }

    /// Tells the detector how loud this app's own output is.
    ///
    /// Always the Bluetooth tail here: this path exists because the route is a
    /// helmet, and the round trip is what the tail has to cover.
    pub fn hear_own_output(&mut self, level_dbfs: f32) {
        self.envelope.hear_own_output(level_dbfs, true);
    }

    /// Tells the detector the chain is hearing speech, so it should not look.
    ///
    /// Suppressed rather than filtered, because there is nothing in the sound
    /// to filter on: a plosive and a tap really are the same kind of event,
    /// and what tells them apart is that one happens while somebody is
    /// talking. Tapping mid-word is not a thing a rider does — they stop, then
    /// tap — so the gesture loses nothing it was going to get.
    pub fn hear_speech(&mut self) {
        self.stats.speech_blocks += 1;
        self.envelope.stop_looking_for(SPEECH_TAIL_MS);
    }

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
            self.held = None;
            self.rising_at_ms = None;
            self.peak_over = f32::NEG_INFINITY;
            return None;
        }

        // A held impulse gathers the loudest frame in its tail window and is
        // judged at the end of it. A maximum rather than a never-exceeds test:
        // taps ring, and the first version of this threw away every tap on two
        // of three recordings because of the ringing.
        if let Some(h) = self.held.as_mut() {
            let since_peak = r.at_ms.saturating_sub(h.peak_at_ms);
            if since_peak >= DECAY_FROM_MS {
                h.tail_db = h.tail_db.max(r.level_db);
            }
            if since_peak >= DECAY_TO_MS {
                let held = self.held.take().expect("just matched");
                let drop = held.tail_db - held.peak_db;
                self.stats.impulse_decay_db = drop;
                if drop <= self.decay_drop_db {
                    if let Some(g) = self.offer(held.candidate, banked) {
                        return Some(g);
                    }
                } else {
                    self.stats.discarded_ringing += 1;
                }
            }
        }

        match self.rising_at_ms {
            None => {
                if r.over >= self.margin_db && r.attack >= self.attack_db {
                    self.stats.impulse_attack_db = r.attack;
                    self.stats.impulse_over_db = r.over;
                    self.rising_at_ms = Some(r.at_ms);
                    self.peak_over = r.over;
                    self.peak_level_db = r.level_db;
                    self.peak_at_ms = r.at_ms;
                }
            }
            Some(since) => {
                if r.over > self.peak_over {
                    self.peak_over = r.over;
                    self.peak_level_db = r.level_db;
                    self.peak_at_ms = r.at_ms;
                }
                if r.over >= self.margin_db {
                    if r.at_ms.saturating_sub(since) > MAX_DECAY_MS as u64 {
                        self.rising_at_ms = None;
                        self.stats.peak_db = self.peak_over;
                        self.stats.discarded_long += 1;
                        self.peak_over = f32::NEG_INFINITY;
                    }
                } else {
                    let strength_db = self.peak_over;
                    self.rising_at_ms = None;
                    self.peak_over = f32::NEG_INFINITY;
                    self.stats.peak_db = strength_db;
                    if r.at_ms < self.latent_until_ms {
                        return None;
                    }
                    self.latent_until_ms = r.at_ms + LATENT_MS as u64;
                    self.stats.candidates += 1;
                    let candidate = Candidate {
                        at_ms: since,
                        strength_db,
                    };
                    if self.decay_drop_db >= 0.0 {
                        return self.offer(candidate, banked);
                    }
                    self.held = Some(Held {
                        candidate,
                        peak_db: self.peak_level_db,
                        peak_at_ms: self.peak_at_ms,
                        tail_db: f32::NEG_INFINITY,
                    });
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

    fn tap(into: &mut Vec<f32>, peak: f32) {
        for ms in 0..60 {
            let a = peak * (-(ms as f32) / 14.0).exp();
            for i in 0..FRAME {
                let t = (ms * FRAME + i) as f32;
                into.push(a * ((t * 0.209).sin() + (t * 0.406).sin()) * 0.5);
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

    /// A room at a given level, for the tests that need a floor the margin is
    /// measurable against.
    fn room(into: &mut Vec<f32>, ms: usize, level: f32) {
        for m in 0..ms {
            for i in 0..FRAME {
                let t = (m * FRAME + i) as f32;
                into.push(level * ((t * 0.37).sin() + (t * 1.1).sin()) * 0.5);
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

        let mut d = HelmetTapDetector::new();
        let mut banked = false;
        assert!(
            d.push(&audio, &mut banked).is_some(),
            "no gesture: {:?}",
            d.stats()
        );
        assert!(
            d.stats().on_helmet,
            "the stats must say which path they came from"
        );
    }

    /// **A consonant with its vowel behind it is not a tap.**
    ///
    /// The onset of /p/ is an impact and the attack test cannot tell it from a
    /// fingertip, because there is nothing to tell. What differs is the next
    /// thirty milliseconds — a tap has nothing in them, a consonant has a
    /// vowel.
    ///
    /// **It needs a real noise floor to be a fair test**, which took one
    /// failure to notice. Over near-silence the two tests cannot both be
    /// reached: the margin is eighteen decibels over the floor, so a tail
    /// quiet enough to fall under it is necessarily more than ten decibels
    /// below any loud peak, and the decay gate never gets a say. A room at
    /// −40 dBFS is where both are live, and it is also where a rider is.
    #[test]
    fn an_impulse_with_a_vowel_behind_it_is_not_a_tap() {
        let mut audio = Vec::new();
        room(&mut audio, 1_500, 0.01);
        for _ in 0..3 {
            room(&mut audio, 3, 0.158);
            room(&mut audio, 57, 0.063);
            room(&mut audio, 160, 0.01);
        }
        room(&mut audio, 400, 0.01);

        let mut d = HelmetTapDetector::new();
        let mut banked = false;
        d.push(&audio, &mut banked);
        assert_eq!(
            d.stats().gestures,
            0,
            "speech completed a gesture on its onsets: {:?}",
            d.stats()
        );
        assert!(
            d.stats().discarded_ringing > 0,
            "rejected for some other reason, so this proves nothing: {:?}",
            d.stats()
        );
    }

    /// **Speaking on a helmet was tapping it.**
    #[test]
    fn taps_are_not_looked_for_while_the_rider_is_talking() {
        let mut d = HelmetTapDetector::new();
        let mut banked = false;
        let mut quiet_audio = Vec::new();
        quiet(&mut quiet_audio, 400);
        d.push(&quiet_audio, &mut banked);

        for _ in 0..3 {
            d.hear_speech();
            let mut audio = Vec::new();
            tap(&mut audio, 0.4);
            quiet(&mut audio, 160);
            d.push(&audio, &mut banked);
        }
        assert_eq!(
            d.stats().gestures,
            0,
            "speech was a gesture: {:?}",
            d.stats()
        );
        assert!(d.stats().speech_blocks > 0, "the speech gate never engaged");
    }

    /// **A helmet hears this app's own cue a round trip later.**
    ///
    /// The 120 ms tail covers a phone's own speaker. Over Bluetooth the sound
    /// crosses the link, comes out of the helmet, is picked up by the helmet's
    /// microphone and crosses back, so it lands after a phone-sized tail has
    /// expired — which a rider reported as the acknowledging tick registering
    /// as a tap.
    #[test]
    fn a_helmet_does_not_hear_this_app_a_round_trip_later() {
        let mut d = HelmetTapDetector::new();
        let mut banked = false;
        let mut quiet_audio = Vec::new();
        quiet(&mut quiet_audio, 400);
        d.push(&quiet_audio, &mut banked);

        for _ in 0..3 {
            d.hear_own_output(-20.0);
            let mut audio = Vec::new();
            quiet(&mut audio, 300);
            tap(&mut audio, 0.5);
            quiet(&mut audio, 100);
            d.push(&audio, &mut banked);
        }
        assert_eq!(
            d.stats().gestures,
            0,
            "the app heard its own cue through the helmet: {:?}",
            d.stats()
        );

        // The control: the phone's path, with its shorter tail, does see that
        // audio — so the test above is testing the Bluetooth tail and not the
        // margin.
        let mut p = super::super::tap_phone::PhoneTapDetector::new();
        p.push(&quiet_audio, &mut banked);
        for _ in 0..3 {
            p.hear_own_output(-20.0);
            let mut audio = Vec::new();
            quiet(&mut audio, 300);
            tap(&mut audio, 0.5);
            quiet(&mut audio, 100);
            p.push(&audio, &mut banked);
        }
        assert!(
            p.stats().candidates > 0,
            "nothing got through even on the short tail, so this proves nothing"
        );
    }
}
