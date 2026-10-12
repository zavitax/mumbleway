//! Taps on the phone's own microphone, by amplitude alone.
//!
//! **A separate path from [`super::acoustic_tap`] at a rider's request**, and
//! the request was narrow: separate the code, and on the phone let amplitude
//! be enough. A sharp peak with a relatively fast decline is a tap; three
//! inside three seconds is the gesture. Nothing else is consulted.
//!
//! # Why the two are not one rule
//!
//! The phone is the easy problem and was being solved with the hard problem's
//! tools. Its microphone is full-band, the thing striking it is in contact
//! with the case, and nothing else in the room is. Every difficult case
//! belongs to the helmet: plosives from two centimetres away, this app's own
//! cues coming back a round trip later, and a narrowband link. The tests that
//! deal with those — a tail measurement and the chain's speech verdict — stay
//! in [`super::acoustic_tap`], which keeps them *and* keeps its own
//! thresholds, so helmet work cannot move the phone under it.
//!
//! What a tap has to be here:
//!
//! - **loud, against the room**: [`MARGIN_DB`] over the trailing average,
//!   which is what makes the rule work in a car park and in a slipstream
//!   without a number for either;
//! - **sharp**: [`ATTACK_DB`] in one millisecond, which is what rejects
//!   speech and music — both reach the margin, neither crosses it that fast,
//!   because only contact with the case does;
//! - **brief**: back under the margin inside [`MAX_DECAY_MS`], which rejects
//!   a shout, a horn, or anything else loud that stays.
//!
//! The band-pass, the trailing average, the meter and the self-noise gate are
//! deliberately duplicated from the other path rather than shared. They are
//! forty lines, they describe the microphone rather than the gesture, and the
//! point of the split is that one path can be changed without touching the
//! other.

use super::acoustic_tap::{AcousticStats, FRAME};
use super::gesture::{Banked, Candidate, GesturePattern, TapGesture};

/// The band a tap lives in, measured rather than assumed.
///
/// A fingernail on the phone's body rings at 1–4 kHz — 93% of its energy —
/// while the same gesture with the phone held in a hand is a thud, 87% of it
/// under 500 Hz. Neither has anything above 4 kHz, so cutting there throws
/// away none of the signal, and it earns its place on recall: 8 of 10
/// gestures against 6 of 10 wideband, and 6 again if narrowed to 2 kHz.
const BAND_LOW_HZ: f32 = 100.0;
const BAND_HIGH_HZ: f32 = 4_000.0;

/// How loud this app's own output has to be before the detector stops
/// listening, in dBFS, and for how long afterwards.
///
/// **The detector runs on the raw microphone, before the canceller**, so the
/// acknowledging tick — a 2 kHz click, dead centre in the band a phone-body
/// tap occupies — would otherwise be heard as the next tap. On this path the
/// round trip is a handful of milliseconds, so a short tail covers it; the
/// helmet's path needs six hundred, which is why it has its own.
const SELF_NOISE_GATE_DBFS: f32 = -60.0;
const SELF_NOISE_TAIL_MS: u32 = 120;

/// The lowest level the floor may believe in, in dBFS.
///
/// One least-significant bit of sixteen-bit audio. **Load-bearing**: a clamp
/// set to a number chosen for being safely low rather than for meaning
/// anything let a digitally silent block drag the floor to it, after which
/// every frame was a candidate and nothing was ever a tap.
const QUANTISATION_DBFS: f32 = -90.0;

/// How fast the reference level follows the audio, per 1 ms frame.
///
/// **A slow average, not a minimum statistic.** The quietest frame in the last
/// second sits near silence between words while a typical frame sits far
/// above it, so a minimum statistic let every word clear the margin. A
/// trailing average cannot be dragged down that way, and moves slowly enough
/// that a tap cannot drag it up behind itself either.
const REFERENCE_ALPHA: f32 = 0.001;

/// How fast the meter falls back, in dB per millisecond.
///
/// **A meter needs ballistics or it shows nothing.** The level is the peak of
/// a one-millisecond frame, and a panel reading it ten times a second samples
/// one frame in a hundred — so a tap, over in forty, is missed about
/// ninety-nine times out of a hundred.
const METER_FALL_DB_PER_MS: f32 = 0.03;

/// How far above the trailing average a frame must reach, in dB.
const MARGIN_DB: f32 = 18.0;

/// How sharply it must arrive: this frame against the one before, in dB.
///
/// **The test that does the work**, separating *contact* from *airborne*. A
/// fingertip on the case is coupled through the case, so the level steps up
/// inside a millisecond; anything arriving through air has its onset shaped by
/// the speaker, the room and the distance.
///
/// Measured over 1175 s of corpus as p5 / median / p95:
///
/// | | attack, dB |
/// |---|---|
/// | taps on the phone body | +9.2 / **+23.4** / +29.1 |
/// | real music, real microphone | +7.0 / **+7.9** / +11.6 |
///
/// Swept against both halves: at 7 dB real music completes 52 gestures an
/// hour; at 12 none; at 14 still none while phone-tap recall is at its best
/// measured value, ten of ten.
const ATTACK_DB: f32 = 14.0;

/// How long an impulse may stay over the margin before it is not a tap.
///
/// The decline half of the rule. Measured at a tenth within forty
/// milliseconds, p90 of 46 ms on one recording and 107 on another that was a
/// desk and rang.
const MAX_DECAY_MS: u32 = 140;

/// Dead time after a tap, in ms.
///
/// **Not a tuning knob.** A tap rings, and the ringing crosses the threshold
/// again on the way down, so one strike offers several candidates tens of
/// milliseconds apart. This is what makes a tap one tap.
const LATENT_MS: u32 = 70;

/// Listens for taps on the phone's own microphone.
pub struct PhoneTapDetector {
    low: [f32; 3],
    high: f32,
    low_coeff: f32,
    high_coeff: f32,
    deaf_frames: u32,
    reference_db: f32,
    previous_db: f32,
    margin_db: f32,
    attack_db: f32,
    pattern: GesturePattern,
    rising_at_ms: Option<u64>,
    peak_over: f32,
    latent_until_ms: u64,
    frame_ms: u64,
    stats: AcousticStats,
}

impl Default for PhoneTapDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl PhoneTapDetector {
    pub fn new() -> Self {
        Self {
            low: [0.0; 3],
            high: 0.0,
            low_coeff: Self::coeff(tuned("MW_TAP_BAND_HIGH_HZ", BAND_HIGH_HZ)),
            high_coeff: Self::coeff(tuned("MW_TAP_BAND_LOW_HZ", BAND_LOW_HZ)),
            deaf_frames: 0,
            reference_db: f32::NEG_INFINITY,
            previous_db: f32::NEG_INFINITY,
            margin_db: tuned("MW_TAP_MARGIN_DB", MARGIN_DB),
            attack_db: tuned("MW_TAP_ATTACK_DB", ATTACK_DB),
            pattern: GesturePattern::new(),
            rising_at_ms: None,
            peak_over: f32::NEG_INFINITY,
            latent_until_ms: 0,
            frame_ms: 0,
            stats: AcousticStats {
                // **Not the derived zero**, which is full scale: the meter
                // would open pinned and take three seconds to fall.
                meter_db: QUANTISATION_DBFS,
                ..AcousticStats::default()
            },
        }
    }

    fn coeff(hz: f32) -> f32 {
        1.0 - (-2.0 * std::f32::consts::PI * hz / 48_000.0).exp()
    }

    pub fn stats(&self) -> AcousticStats {
        self.stats
    }

    /// Tells the detector how loud this app's own output is.
    pub fn hear_own_output(&mut self, level_dbfs: f32) {
        if level_dbfs > SELF_NOISE_GATE_DBFS {
            self.deaf_frames = self.deaf_frames.max(SELF_NOISE_TAIL_MS);
        }
    }

    /// Feeds a block of 48 kHz mono audio, however long it happens to be.
    ///
    /// Answers `Some` on the frame that completes a gesture, and reports
    /// through `banked` whether a single tap landed — which is what the
    /// acknowledging tick is played from, because three taps heard and no
    /// gesture is a different diagnosis from three taps not heard.
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

        // **A half-performed gesture gives up on its own**, whether or not
        // anybody is listening to this frame. Before this the window was only
        // checked when a tap arrived, so a count stood for ever if none did:
        // tap twice, wait, and the pips went on claiming two were banked long
        // after those two could complete anything — and the next tap dropped
        // the display from two to one, which reads as a lost tap rather than
        // an expired pair. Ahead of the deafness check, because a run has to
        // expire while this app is making a noise too.
        if self.pattern.expire(self.frame_ms) {
            self.stats.run = 0;
        }

        let mut peak = 0.0f32;
        for &sample in frame {
            let mut v = sample;
            for pole in self.low.iter_mut() {
                *pole += (v - *pole) * self.low_coeff;
                v = *pole;
            }
            self.high += (v - self.high) * self.high_coeff;
            peak = peak.max((v - self.high).abs());
        }
        let level_db = (20.0 * (peak + 1e-7).log10()).max(QUANTISATION_DBFS);
        self.stats.meter_db = level_db.max(self.stats.meter_db - METER_FALL_DB_PER_MS);
        self.stats.level_db = level_db;

        // **The level is tracked through deafness, and that is not a detail.**
        // Setting the previous level to minus infinity while this app makes a
        // noise made the attack of the first frame afterwards *infinite*, so
        // every recovery from a cue armed a candidate whatever the audio was
        // doing. The trailing reference is deliberately not updated here: our
        // own cue is forty decibels over the room, and letting it into the
        // floor would raise the bar for the tap that follows.
        if self.deaf_frames > 0 {
            self.deaf_frames -= 1;
            self.previous_db = level_db;
            self.rising_at_ms = None;
            self.peak_over = f32::NEG_INFINITY;
            return None;
        }

        if !self.reference_db.is_finite() {
            self.reference_db = level_db;
        }
        self.reference_db += (level_db - self.reference_db) * REFERENCE_ALPHA;
        let floor_db = self.reference_db;
        let over = level_db - floor_db;
        let attack = level_db - self.previous_db;
        self.previous_db = level_db;
        self.stats.floor_db = floor_db;
        let at_ms = self.frame_ms;

        match self.rising_at_ms {
            None => {
                // Loud against the room, and sharp. Both, because speech and
                // music reach the margin too and neither arrives this fast.
                if over >= self.margin_db && attack >= self.attack_db {
                    self.stats.impulse_attack_db = attack;
                    self.stats.impulse_over_db = over;
                    self.rising_at_ms = Some(at_ms);
                    self.peak_over = over;
                }
            }
            Some(since) => {
                self.peak_over = self.peak_over.max(over);
                if over >= self.margin_db {
                    if at_ms.saturating_sub(since) > MAX_DECAY_MS as u64 {
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

/// A threshold, with an environment override for sweeping it.
///
/// Read where the detector is built rather than where it is used, so the audio
/// callback never touches the environment.
fn tuned(name: &str, default: f32) -> f32 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse::<f32>().ok())
        .filter(|v| v.is_finite())
        .unwrap_or(default)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tap as it was measured: full level in the first frame, then a decay
    /// to a tenth in about forty milliseconds, in the band taps occupy.
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
        assert!(
            d.stats().discarded_long > 0,
            "a sustained sound was rejected for the wrong reason"
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

    /// Music on a beat, which is the false arm that matters: **a bar of music
    /// is matched impulses at matched intervals**, exactly what the gesture
    /// pattern looks for and better matched than a human hand manages. The
    /// attack is the only thing between a backbeat and a stop command.
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
    /// acknowledge a tap is a 2 kHz click, dead centre in the band a
    /// phone-body tap occupies, and the detector runs on the raw microphone
    /// before any canceller.
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

    /// **The pips have to clear themselves**, which they did not.
    ///
    /// A rider asked how long three dots took to reset with no further taps,
    /// and the answer was that they never did: the window was only examined
    /// when a tap arrived. So the panel claimed two taps were banked for as
    /// long as the app was open, and the next tap — starting a fresh run —
    /// dropped the count from two to one, which reads as a lost tap rather
    /// than an expired pair.
    ///
    /// Asserted through `stats.run`, because that is the number the panel
    /// draws; the pattern's own expiry has its own test in `gesture`.
    #[test]
    fn a_half_performed_gesture_clears_itself() {
        let mut d = PhoneTapDetector::new();
        let mut banked = false;
        let mut lead = Vec::new();
        quiet(&mut lead, 400);
        d.push(&lead, &mut banked);

        let mut two = Vec::new();
        tap(&mut two, 0.4);
        quiet(&mut two, 160);
        tap(&mut two, 0.4);
        quiet(&mut two, 160);
        d.push(&two, &mut banked);
        assert_eq!(d.stats().run, 2, "two taps did not bank: {:?}", d.stats());

        // Well inside the window: still counted, because they still count.
        let mut soon = Vec::new();
        quiet(&mut soon, 500);
        d.push(&soon, &mut banked);
        assert_eq!(d.stats().run, 2, "gave up inside the window");

        // Past it, with nothing offered and nothing to hear.
        let mut later = Vec::new();
        quiet(&mut later, 3_000);
        d.push(&later, &mut banked);
        assert_eq!(
            d.stats().run,
            0,
            "the count outlived the taps: {:?}",
            d.stats()
        );
    }

    /// The meter has to move when a tap arrives, because a panel samples it a
    /// hundred times more slowly than the frames go by.
    #[test]
    fn the_meter_rises_on_a_tap_and_falls_back() {
        let mut d = PhoneTapDetector::new();
        let mut banked = false;
        let mut quiet_audio = Vec::new();
        quiet(&mut quiet_audio, 400);
        d.push(&quiet_audio, &mut banked);
        let idle = d.stats().meter_db;

        let mut audio = Vec::new();
        tap(&mut audio, 0.5);
        d.push(&audio, &mut banked);
        let struck = d.stats().meter_db;
        assert!(struck > idle + 20.0, "the meter did not see the tap");

        let mut after = Vec::new();
        quiet(&mut after, 2_000);
        d.push(&after, &mut banked);
        assert!(
            d.stats().meter_db < struck - 20.0,
            "the meter never fell back"
        );
    }
}
