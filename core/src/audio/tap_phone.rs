//! Taps on the phone's own microphone, by amplitude alone.
//!
//! **A separate path from [`super::tap_helmet`] at a rider's request**, and
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
//! in [`super::tap_helmet`], which keeps its own
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

use super::gesture::{Banked, Candidate, GesturePattern, TapGesture};
use super::tap_stats::{
    tuned, AcousticStats, StageLamps, FRAME, METER_FALL_DB_PER_MS, QUANTISATION_AMPLITUDE,
    QUANTISATION_DBFS,
};

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
/// How long after the detector starts listening it ignores everything, in ms.
///
/// **Asked for by a rider, and it closes a fault with teeth.** A stream's first
/// moments are not audio: opening an input produces clicks and a burst of
/// whatever the device had buffered, and a route moving under it produces more.
/// Those are broadband transients with fast attacks — the exact shape this
/// detector is built to find — so the artefacts of starting to listen read as
/// taps.
///
/// What that cost: in pocket mode a false gesture sets the rider's claim on the
/// microphone, which opens capture and starts the thirty-second silence
/// counting. A rider watched precisely that happen with a headset they had just
/// connected, and the microphone they were trying to hand over to the helmet
/// stayed open on the phone instead.
///
/// Three seconds also covers the trailing reference's own convergence:
/// [`REFERENCE_ALPHA`] is 0.001, so the floor needs about a thousand frames
/// before a margin measured against it means anything, and before that a
/// quiet-to-loud transition looks like a strike.
const SETTLE_MS: u64 = 3_000;

const SELF_NOISE_GATE_DBFS: f32 = -60.0;
const SELF_NOISE_TAIL_MS: u32 = 120;

/// How fast the reference level follows the audio, per 1 ms frame.
///
/// **A slow average, not a minimum statistic.** The quietest frame in the last
/// second sits near silence between words while a typical frame sits far
/// above it, so a minimum statistic let every word clear the margin. A
/// trailing average cannot be dragged down that way, and moves slowly enough
/// that a tap cannot drag it up behind itself either.
const REFERENCE_ALPHA: f32 = 0.001;

/// How far above the trailing average a frame must reach, in dB.
/// The stages a frame passes, in the order it passes them.
///
/// **This path's own set.** The helmet detector has a different rule and
/// therefore different lamps — a crest factor and a sustain limit where this
/// has a margin and a per-millisecond attack — and
/// `UiAcousticStats::on_helmet` is what tells a panel which it is reading.
pub mod stage {
    /// Audio arrived at all: the raw frame is above one 16-bit step.
    pub const INPUT: usize = 0;
    /// Something survived the band-pass.
    pub const BAND: usize = 1;
    /// Not deaf — this app was not making a noise of its own.
    pub const HEARING: usize = 2;
    /// The frame stood above the trailing floor at all. Dark here with the
    /// others lit means the floor has risen onto the signal.
    pub const OVER_FLOOR: usize = 3;
    /// It stood [`MARGIN_DB`] above it.
    pub const MARGIN: usize = 4;
    /// It rose [`ATTACK_DB`] inside the millisecond. The discriminator.
    pub const ATTACK: usize = 5;
    /// **Both of those in the same millisecond**, so an impulse was armed.
    /// [`MARGIN`] and [`ATTACK`] answer for themselves, and both being lit
    /// does not mean one frame did both.
    pub const ARMED: usize = 6;
    /// It fell back inside [`MAX_DECAY_MS`] and outside the dead time, so a
    /// strike was formed.
    pub const FELL_BACK: usize = 7;
    /// There is no tail test on this path, so a strike passes this stage by
    /// arriving. Lit rather than dark, because dark means "stopped here".
    pub const TAIL: usize = 8;
    /// It was banked towards a gesture.
    pub const BANKED: usize = 9;
    pub const COUNT: usize = 10;
}

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
    /// Frames left in which nothing may arm. See [`SETTLE_MS`].
    settle_frames: u64,
    lamps: StageLamps<{ stage::COUNT }>,
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
            // **Settling from the moment it exists**, because the first thing
            // any detector sees is a stream being opened.
            settle_frames: SETTLE_MS,
            lamps: StageLamps::new(),
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
        let mut s = self.stats;
        s.stage_mask = self.lamps.mask(self.frame_ms);
        s.recent_attack_db = self.lamps.recent(0, self.frame_ms);
        s.recent_over_db = self.lamps.recent(1, self.frame_ms);
        s
    }

    /// Starts the settling window again.
    ///
    /// Called when the detector is switched on and whenever the input stream is
    /// built, which are the two moments artefacts arrive. Everything that
    /// describes an event in progress is dropped with it: a run half performed
    /// across a stream rebuild is a run whose taps were heard by a device that
    /// no longer exists.
    pub fn begin_settling(&mut self) {
        self.settle_frames = SETTLE_MS;
        self.rising_at_ms = None;
        self.peak_over = f32::NEG_INFINITY;
        self.pattern.reset();
        self.stats.run = 0;
        // Re-learned rather than carried over: the floor belongs to the device
        // and the route that produced it.
        self.reference_db = f32::NEG_INFINITY;
        self.previous_db = f32::NEG_INFINITY;
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

        let mut raw_peak = 0.0f32;
        let mut peak = 0.0f32;
        for &sample in frame {
            raw_peak = raw_peak.max(sample.abs());
            let mut v = sample;
            for pole in self.low.iter_mut() {
                *pole += (v - *pole) * self.low_coeff;
                v = *pole;
            }
            self.high += (v - self.high) * self.high_coeff;
            peak = peak.max((v - self.high).abs());
        }
        if raw_peak > QUANTISATION_AMPLITUDE {
            self.lamps.pass(stage::INPUT, self.frame_ms);
        }
        if peak > QUANTISATION_AMPLITUDE {
            self.lamps.pass(stage::BAND, self.frame_ms);
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

        // **Settling, and ahead of the hearing lamp**: the detector is not
        // listening, which is what that lamp says. The level, the meter and the
        // floor below all keep running, so the panel shows a live microphone
        // and the reference converges — it is only arming that waits.
        if self.settle_frames > 0 {
            self.settle_frames -= 1;
            self.rising_at_ms = None;
            self.peak_over = f32::NEG_INFINITY;
            if !self.reference_db.is_finite() {
                self.reference_db = level_db;
            }
            self.reference_db += (level_db - self.reference_db) * REFERENCE_ALPHA;
            self.stats.floor_db = self.reference_db;
            return None;
        }
        self.lamps.pass(stage::HEARING, self.frame_ms);

        if !self.reference_db.is_finite() {
            self.reference_db = level_db;
        }
        self.reference_db += (level_db - self.reference_db) * REFERENCE_ALPHA;
        let floor_db = self.reference_db;
        let over = level_db - floor_db;
        // **The first frame has no previous frame, and that is not an
        // infinite attack.** `previous_db` starts at minus infinity, so the
        // first millisecond of every stream reported a rise of infinity — the
        // same fault the deafness branch above was fixed for, left standing
        // at construction because it could not arm anything on its own: the
        // margin is zero on that frame, since the reference is seeded from
        // it. Harmless for detection and a lie on the panel, which is how it
        // was found — the attack lamp lit on digital silence.
        let attack = if self.previous_db.is_finite() {
            level_db - self.previous_db
        } else {
            0.0
        };
        self.previous_db = level_db;
        // Lit separately, whatever the state: the arming test wants both, and
        // one lamp for the pair would not say which was missing.
        if over > 0.0 {
            self.lamps.pass(stage::OVER_FLOOR, self.frame_ms);
        }
        if over >= self.margin_db {
            self.lamps.pass(stage::MARGIN, self.frame_ms);
        }
        if attack >= self.attack_db {
            self.lamps.pass(stage::ATTACK, self.frame_ms);
        }
        self.lamps.offer(0, attack, self.frame_ms);
        self.lamps.offer(1, over, self.frame_ms);
        self.stats.floor_db = floor_db;
        let at_ms = self.frame_ms;

        match self.rising_at_ms {
            None => {
                // Loud against the room, and sharp. Both, because speech and
                // music reach the margin too and neither arrives this fast.
                if over >= self.margin_db && attack >= self.attack_db {
                    self.lamps.pass(stage::ARMED, self.frame_ms);
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
                    self.lamps.pass(stage::FELL_BACK, self.frame_ms);
                    // **No tail test on this path**, so the strike passes the
                    // stage by arriving. Lit rather than left dark, because
                    // dark means "stopped here" on the panel and nothing
                    // stops here — the helmet path is the one that can throw a
                    // strike away for ringing.
                    self.lamps.pass(stage::TAIL, self.frame_ms);
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
                self.lamps.pass(stage::BANKED, self.frame_ms);
                self.stats.banked += 1;
                self.stats.run = run;
                *banked = true;
                None
            }
            Banked::RestartedOnMagnitude => {
                self.lamps.pass(stage::BANKED, self.frame_ms);
                self.stats.banked += 1;
                self.stats.run = 1;
                *banked = true;
                None
            }
            Banked::Completed(g) => {
                self.lamps.pass(stage::BANKED, self.frame_ms);
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
    use crate::audio::tap_stats::STAGE_HOLD_MS;

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

    /// A detector that has already settled.
    ///
    /// **Every test that offers a tap needs this**, because a real detector
    /// spends its first three seconds ignoring everything — see `SETTLE_MS`,
    /// and the fault it exists for. Fed through the real path rather than
    /// reaching inside: the window is only over when the frames say so.
    fn settled() -> PhoneTapDetector {
        let mut d = PhoneTapDetector::new();
        let mut audio = Vec::new();
        quiet(&mut audio, SETTLE_MS as usize + 50);
        let mut banked = false;
        d.push(&audio, &mut banked);
        d
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

        let mut d = settled();
        let mut banked = false;
        assert!(
            d.push(&audio, &mut banked).is_some(),
            "no gesture: {:?}",
            d.stats()
        );
        assert_eq!(d.stats().banked, 3, "a tick for each tap");
    }

    /// Lamp *n* is lit when stage *n* passed something recently.
    fn lit(d: &PhoneTapDetector, s: usize) -> bool {
        d.stats().stage_mask & (1 << s) != 0
    }

    /// **The row a rider reads to see where it stops.** A tap passes every
    /// stage, so every lamp is lit directly after one.
    #[test]
    fn a_tap_lights_the_whole_row() {
        let mut audio = Vec::new();
        quiet(&mut audio, 400);
        tap(&mut audio, 0.4);

        let mut d = settled();
        let mut banked = false;
        d.push(&audio, &mut banked);
        assert!(banked, "the tap did not bank: {:?}", d.stats());
        for s in 0..stage::COUNT {
            assert!(lit(&d, s), "stage {s} dark after a tap: {:?}", d.stats());
        }
    }

    /// And the row is a *diagnosis*, which means the lamps have to go dark in
    /// the right place. Quiet room: audio is arriving and in band, the
    /// detector is listening, nothing stands over the floor.
    #[test]
    fn a_quiet_room_stops_at_the_floor() {
        let mut audio = Vec::new();
        quiet(&mut audio, 2_000);
        let mut d = settled();
        let mut banked = false;
        d.push(&audio, &mut banked);

        assert!(lit(&d, stage::INPUT), "a room is not digital silence");
        assert!(lit(&d, stage::BAND));
        assert!(lit(&d, stage::HEARING));
        assert!(!lit(&d, stage::MARGIN), "quiet reached the margin");
        assert!(!lit(&d, stage::ARMED));
        assert!(!lit(&d, stage::FELL_BACK));
        assert!(!lit(&d, stage::BANKED));
    }

    /// **The reading this lamp exists for, and it is a real one.** A rider
    /// hitting a helmet reported the first six lamps lit and the last three
    /// dark. That was meant to say "it armed and never fell back in time" and
    /// could equally have said "the two tests passed in different
    /// milliseconds and nothing ever armed" — two causes with opposite fixes.
    ///
    /// So here is a signal that does exactly the second: a blip that rises
    /// sharply but not far, and then a ramp that climbs far but not sharply.
    /// Margin lit, attack lit, armed dark.
    #[test]
    fn two_tests_passed_in_different_frames_do_not_arm() {
        /// One millisecond at an amplitude, in the band taps occupy.
        fn ms(into: &mut Vec<f32>, amp: f32, at: usize) {
            for i in 0..FRAME {
                let x = (at * FRAME + i) as f32;
                into.push(amp * ((x * 0.209).sin() + (x * 0.406).sin()) * 0.5);
            }
        }

        let base = 0.0008;
        let mut audio = Vec::new();
        let mut at = 0;
        for _ in 0..1_500 {
            ms(&mut audio, base, at);
            at += 1;
        }
        // Sharp and short: a tenth of the floor, then three times it. About
        // 20 dB in one millisecond, reaching only 10 dB over the floor.
        ms(&mut audio, base * 0.3, at);
        at += 1;
        ms(&mut audio, base * 3.2, at);
        at += 1;
        for _ in 0..40 {
            ms(&mut audio, base, at);
            at += 1;
        }
        // Far but not sharp: 5 dB a millisecond up to 30 dB over the floor.
        let mut amp = base;
        for _ in 0..6 {
            amp *= 1.78;
            ms(&mut audio, amp, at);
            at += 1;
        }
        for _ in 0..20 {
            ms(&mut audio, amp, at);
            at += 1;
        }

        let mut d = settled();
        let mut banked = false;
        d.push(&audio, &mut banked);

        assert!(
            lit(&d, stage::MARGIN),
            "the ramp missed the margin: {:?}",
            d.stats()
        );
        assert!(
            lit(&d, stage::ATTACK),
            "the blip missed the attack: {:?}",
            d.stats()
        );
        assert!(
            !lit(&d, stage::ARMED),
            "armed on two tests that passed in different frames: {:?}",
            d.stats()
        );
        assert!(!lit(&d, stage::FELL_BACK));
        assert!(!banked);
    }

    /// **Deafness is the one that has actually caught people out**, because
    /// the tick this app plays comes back through a helmet's own microphone
    /// and shuts the detector for 600 ms. The row says so: audio in, band in,
    /// hearing dark, nothing after it.
    #[test]
    fn our_own_noise_puts_the_hearing_lamp_out() {
        let mut d = settled();
        let mut banked = false;

        let mut settle = Vec::new();
        quiet(&mut settle, 400);
        d.push(&settle, &mut banked);

        // **Reported every block, which is how the engine reports it**, and
        // the stretch has to outlast [`STAGE_HOLD_MS`] before the lamp can be
        // expected to go out — a single [`SELF_NOISE_TAIL_MS`] is shorter than
        // the hold, so the lamp would still be lit and right to be.
        for _ in 0..200 {
            let mut block = Vec::new();
            quiet(&mut block, 10);
            d.hear_own_output(-20.0);
            d.push(&block, &mut banked);
        }

        let mut tapped = Vec::new();
        tap(&mut tapped, 0.4);
        d.hear_own_output(-20.0);
        d.push(&tapped, &mut banked);

        assert!(!banked, "banked a tap while deaf");
        assert!(lit(&d, stage::INPUT), "the stream is still arriving");
        assert!(lit(&d, stage::BAND));
        assert!(!lit(&d, stage::HEARING), "heard a tap through its own cue");
        assert!(!lit(&d, stage::MARGIN), "a stage past the deafness is lit");
        assert!(!lit(&d, stage::BANKED));
    }

    /// Digital silence puts out the first lamp and every one after it, which
    /// is the reading that tells a dead stream from a quiet room — the fault
    /// that took 8.5 minutes of bit-exact zero to notice once.
    #[test]
    fn a_dead_stream_lights_nothing() {
        let audio = vec![0.0f32; FRAME * 500];
        // **Not `settled()`**: that leaves three seconds of audio behind it,
        // and the input lamp holds for 1.5 s, so the row would still be lit
        // from before the stream died. A fresh detector has never heard
        // anything, which is the state this test is about.
        let mut d = PhoneTapDetector::new();
        let mut banked = false;
        d.push(&audio, &mut banked);
        assert_eq!(
            d.stats().stage_mask & !(1 << stage::HEARING),
            0,
            "something passed on digital silence: {:?}",
            d.stats()
        );
    }

    /// A lamp goes out again on its own, or a row read a minute later
    /// describes a tap nobody is performing.
    #[test]
    fn the_lamps_go_out_again() {
        let mut audio = Vec::new();
        quiet(&mut audio, 400);
        tap(&mut audio, 0.4);
        let mut d = settled();
        let mut banked = false;
        d.push(&audio, &mut banked);
        assert!(lit(&d, stage::BANKED));

        let mut after = Vec::new();
        quiet(&mut after, STAGE_HOLD_MS as usize + 10);
        d.push(&after, &mut banked);
        assert!(!lit(&d, stage::BANKED), "the tap lamp stayed lit");
        assert!(lit(&d, stage::HEARING), "and this one should still be lit");
    }

    /// **The window itself, which a rider asked for in these terms**: "when in
    /// pocket mode and listening for taps, skip the first 3 seconds of
    /// capture: they contain artifacts which we do not want to process".
    ///
    /// The same tap, twice: inside the window it is ignored, after it, it
    /// banks. Anything weaker than this passes on a detector that ignores
    /// everything for ever.
    #[test]
    fn the_first_three_seconds_are_ignored() {
        let mut early = Vec::new();
        quiet(&mut early, 400);
        tap(&mut early, 0.4);
        quiet(&mut early, 100);

        let mut d = PhoneTapDetector::new();
        let mut banked = false;
        d.push(&early, &mut banked);
        assert!(!banked, "a tap inside the settling window banked");
        assert_eq!(d.stats().candidates, 0, "{:?}", d.stats());

        // Out of the window, and the same tap again.
        let mut rest = Vec::new();
        quiet(&mut rest, SETTLE_MS as usize);
        tap(&mut rest, 0.4);
        quiet(&mut rest, 100);
        d.push(&rest, &mut banked);
        assert!(banked, "the window never ended: {:?}", d.stats());
    }

    /// And it starts again on demand, because a stream being rebuilt is the
    /// other moment artefacts arrive.
    #[test]
    fn settling_can_be_restarted() {
        let mut d = settled();
        let mut banked = false;
        d.begin_settling();

        let mut audio = Vec::new();
        quiet(&mut audio, 400);
        tap(&mut audio, 0.4);
        d.push(&audio, &mut banked);
        assert!(!banked, "a tap right after a rebuild banked");
    }

    #[test]
    fn a_quiet_microphone_hears_nothing() {
        let mut audio = Vec::new();
        quiet(&mut audio, 4_000);
        let mut d = settled();
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

        let mut d = settled();
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

        let mut d = settled();
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

        let mut d = settled();
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
        let mut d = settled();
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
        let mut d = settled();
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
        let mut d = settled();
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
