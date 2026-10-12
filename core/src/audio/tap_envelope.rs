//! The measurement every tap detector starts from, and nothing it decides.
//!
//! **Split out because the two microphones are not the same problem**, and a
//! rider asked for them to stop sharing a rule. A phone tapped through a
//! jacket pocket is a contact impact arriving on a 48 kHz built-in
//! microphone, in a room or a slipstream. A helmet tapped while a rider talks
//! into it is a contact impact arriving on a narrowband hands-free link,
//! competing with plosives from two centimetres away and with this app's own
//! cues coming back a round trip later. Three days of trying to serve both
//! with one threshold produced a rule that was too strict for the phone and
//! too loose for the helmet at the same time.
//!
//! What they genuinely share is the *measurement*: band-limit the signal, take
//! a peak per millisecond, and say how far that is above a trailing average
//! and above the frame before. Everything after that differs, so everything
//! after that lives in [`super::tap_phone`] and [`super::tap_helmet`].
//!
//! Nothing here decides anything. It has no thresholds beyond the ones that
//! describe the microphone rather than the gesture, which is the line the
//! split is drawn on.

/// One millisecond at 48 kHz: the frame the envelope is measured over.
///
/// A tap decays to a tenth within forty of these, measured on recordings whose
/// taps a rider confirmed were audible, so a frame is fine enough to see the
/// shape and coarse enough to cost nothing on the audio thread.
pub const FRAME: usize = 48;

/// The band a tap lives in, measured rather than assumed.
///
/// **Two recordings, two completely different spectra.** A fingernail on the
/// phone's body rings at 1–4 kHz — 93% of its energy — while the same gesture
/// with the phone held in a hand is a thud, 87% of it under 500 Hz. There is
/// no narrow band that catches both.
///
/// What they agree on is that **neither has anything above 4 kHz** — 0.3% of
/// the energy on one, 2.2% on the other — so cutting there throws away none of
/// the signal. It earns its place on *recall*: at the phone's attack
/// threshold the band finds 8 of 10 gestures against 6 of 10 wideband, and
/// narrowing it to 2 kHz drops back to 6.
///
/// The high-pass end is cheaper to justify: engine rumble, wind and the
/// handling noise of a phone moving in a pocket are all below 100 Hz, and the
/// hand-held taps keep 87% of their energy above it.
///
/// # What this band does *not* do
///
/// **Band-limiting is not what keeps music out, and a band ratio is not
/// either.** Both were tried on 2026-10-11 and the measurement refused both.
/// A steeper band fails because arming is a margin over a *trailing average*:
/// on a quiet floor anything audible clears it, so a hi-hat 22 dB down still
/// stands 37 dB over a floor at one quantisation step. And a scale-free ratio
/// between the band and everything above it is backwards — music's armed
/// impulses are the *most* in-band things in the corpus, median +34 dB,
/// because what clears the margin in a dense mix is the kick and the snare.
/// The real taps are the lowest ratios measured anywhere.
const BAND_LOW_HZ: f32 = 100.0;
const BAND_HIGH_HZ: f32 = 4_000.0;

/// How loud this app's own output has to be before the detector stops
/// listening, in dBFS.
///
/// **The detector runs on the raw microphone, before the canceller**, so
/// everything the phone plays reaches it — including its own tick, which is a
/// 2 kHz click sitting dead centre in the band a phone-body tap occupies.
/// While this app is making a noise, it is not listening for taps.
const SELF_NOISE_GATE_DBFS: f32 = -60.0;

/// How long after this app's own sound the detector stays deaf, in frames.
const SELF_NOISE_TAIL_MS: u32 = 120;

/// The same, on a Bluetooth route, where our own sound comes back late.
///
/// **A round trip, not a cue length.** A cue is mixed into the output, crosses
/// the link to the helmet, comes out of its speaker, is picked up by its own
/// microphone and crosses back: two hops of a hundred-odd milliseconds, plus
/// whatever the helmet rings for. The 120 ms that covers a phone's own speaker
/// has expired long before any of it arrives, so the detector was listening
/// again exactly in time to hear itself — which a rider reported as the
/// acknowledging tick registering as a tap.
const SELF_NOISE_TAIL_BLUETOOTH_MS: u32 = 600;

/// The lowest level the floor may believe in, in dBFS.
///
/// One least-significant bit of sixteen-bit audio. **Load-bearing, and the
/// accelerometer detector shipped the identical fault**: a clamp set to a
/// number chosen for being safely low rather than for meaning anything let a
/// digitally silent block drag the floor down to it, after which every frame
/// was a candidate and nothing was ever a tap. Below one bit there is nothing
/// a microphone can have told us.
const QUANTISATION_DBFS: f32 = -90.0;

/// How fast the reference level follows the audio, per 1 ms frame.
///
/// **A slow average, not a minimum statistic, and that distinction cost a
/// measurement to learn.** `NoiseFloorTracker` tracks the quietest recent
/// moment, which is right for a steady accelerometer signal and quite wrong
/// for a spiky audio envelope: the quietest frame in the last second sits near
/// silence between words while a typical frame sits far above it, so every
/// word cleared the margin. A trailing average cannot be dragged down that
/// way, and it moves slowly enough that a tap cannot drag it *up* behind
/// itself either.
const REFERENCE_ALPHA: f32 = 0.001;

/// How fast the meter falls back, in dB per millisecond.
///
/// **A meter needs ballistics or it shows nothing.** The level is the peak of
/// a one-millisecond frame, and a panel reading it ten times a second samples
/// one frame in a hundred — so a tap, over in forty, is missed about
/// ninety-nine times out of a hundred, and a rider would watch a meter that
/// never moves while they tap. Thirty decibels a second keeps a tap visible
/// for a second and a bit: long enough to read, short enough not to smear
/// three taps into one.
const METER_FALL_DB_PER_MS: f32 = 0.03;

/// What the detector is seeing, for the panel and the rig.
///
/// One shape for both microphones, so the panel needs no cases and a reading
/// sent back by a rider means the same thing whichever path produced it. Some
/// fields are only ever written by one of them, and say so.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TapStats {
    /// Whether these came from the helmet's path rather than the phone's.
    pub on_helmet: bool,
    pub frames: u64,
    pub candidates: u64,
    /// Impulses that arrived sharply but never fell back in time.
    pub discarded_long: u64,
    /// Impulses whose tail did not fall away — a consonant with its vowel
    /// behind it rather than a tap. **Helmet path only.**
    pub discarded_ringing: u64,
    /// Blocks in which the chain reported speech, so the detector did not
    /// look. **Helmet path only**, and zero while a rider is talking means
    /// that gate never engaged, which is a different fault from it being too
    /// weak.
    pub speech_blocks: u64,
    pub banked: u64,
    pub gestures: u64,
    pub level_db: f32,
    pub floor_db: f32,
    /// The level with a meter's fall-back, so something sampling it from
    /// outside sees the taps. See [`METER_FALL_DB_PER_MS`].
    pub meter_db: f32,
    /// The last impulse judged, as its attack and its height over the floor —
    /// the numbers the decision is actually made on, kept so the rig can sweep
    /// them against real recordings rather than against a generator.
    pub impulse_attack_db: f32,
    pub impulse_over_db: f32,
    /// How far that impulse's tail had fallen 15 to 30 ms after its peak.
    /// **Helmet path only.**
    pub impulse_decay_db: f32,
    pub peak_db: f32,
    pub run: u8,
}

impl Default for TapStats {
    fn default() -> Self {
        Self {
            on_helmet: false,
            frames: 0,
            candidates: 0,
            discarded_long: 0,
            discarded_ringing: 0,
            speech_blocks: 0,
            banked: 0,
            gestures: 0,
            level_db: 0.0,
            floor_db: 0.0,
            // **Not the derived zero**, which is full scale: the meter would
            // open pinned and take three seconds to fall, so a rider opening
            // the panel would see a microphone that looks deafeningly loud and
            // settles for no reason.
            meter_db: QUANTISATION_DBFS,
            impulse_attack_db: 0.0,
            impulse_over_db: 0.0,
            impulse_decay_db: 0.0,
            peak_db: 0.0,
            run: 0,
        }
    }
}

/// What one frame of audio came to.
#[derive(Clone, Copy, Debug)]
pub struct Reading {
    /// Milliseconds on the arrival clock, which is what gesture timing uses.
    pub at_ms: u64,
    /// The band-limited peak of this frame, in dBFS.
    pub level_db: f32,
    /// The trailing average the margin is measured against.
    pub floor_db: f32,
    /// How far this frame stands above that average.
    pub over: f32,
    /// How far it rose from the frame before — the sharpness of arrival.
    pub attack: f32,
    /// Whether the detector should not be looking at all: this app is making
    /// a noise, or the rules have asked for quiet.
    pub deaf: bool,
}

/// Band-limits, measures and tracks, and decides nothing.
pub struct Envelope {
    /// A band-pass from one-poles: three for the upper skirt, one for the
    /// lower. Cheap enough for the audio callback and sharp enough for a
    /// decision about energy rather than fidelity.
    low: [f32; 3],
    high: f32,
    low_coeff: f32,
    high_coeff: f32,
    /// Frames remaining in which the detector should not look.
    deaf_frames: u32,
    reference_db: f32,
    previous_db: f32,
    frame_ms: u64,
    meter_db: f32,
}

impl Default for Envelope {
    fn default() -> Self {
        Self::new()
    }
}

impl Envelope {
    pub fn new() -> Self {
        Self {
            low: [0.0; 3],
            high: 0.0,
            low_coeff: Self::coeff(tuned("MW_TAP_BAND_HIGH_HZ", BAND_HIGH_HZ)),
            high_coeff: Self::coeff(tuned("MW_TAP_BAND_LOW_HZ", BAND_LOW_HZ)),
            deaf_frames: 0,
            reference_db: f32::NEG_INFINITY,
            previous_db: f32::NEG_INFINITY,
            frame_ms: 0,
            meter_db: QUANTISATION_DBFS,
        }
    }

    /// A one-pole coefficient for a 48 kHz frame rate.
    fn coeff(hz: f32) -> f32 {
        1.0 - (-2.0 * std::f32::consts::PI * hz / 48_000.0).exp()
    }

    /// Measures one frame. Advances the clock whether or not anybody looks.
    pub fn frame(&mut self, frame: &[f32]) -> Reading {
        self.frame_ms += 1;

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
        self.meter_db = level_db.max(self.meter_db - METER_FALL_DB_PER_MS);

        // **The level is tracked through deafness, and that is not a detail.**
        // An earlier version set the previous level to minus infinity while
        // this app was making a noise, which made the attack of the first
        // frame afterwards *infinite* — so every recovery from a cue armed a
        // candidate whatever the audio was doing. The rig found it the moment
        // it began simulating the engine's own ticks: false arms went from 184
        // an hour to 233, with the only change being that the detector could
        // hear itself.
        //
        // The trailing reference is deliberately *not* updated while deaf: our
        // own cue is forty decibels over the room, and letting it into the
        // floor would raise the bar for the tap that follows it.
        let deaf = self.deaf_frames > 0;
        if deaf {
            self.deaf_frames -= 1;
            self.previous_db = level_db;
            return Reading {
                at_ms: self.frame_ms,
                level_db,
                floor_db: self.reference_db,
                over: 0.0,
                attack: 0.0,
                deaf: true,
            };
        }

        if !self.reference_db.is_finite() {
            self.reference_db = level_db;
        }
        self.reference_db += (level_db - self.reference_db) * REFERENCE_ALPHA;
        let attack = level_db - self.previous_db;
        self.previous_db = level_db;

        Reading {
            at_ms: self.frame_ms,
            level_db,
            floor_db: self.reference_db,
            over: level_db - self.reference_db,
            attack,
            deaf: false,
        }
    }

    /// Tells the envelope how loud this app's own output is.
    ///
    /// `bluetooth` chooses the tail, which is a round trip rather than a cue
    /// length. See [`SELF_NOISE_TAIL_BLUETOOTH_MS`].
    pub fn hear_own_output(&mut self, level_dbfs: f32, bluetooth: bool) {
        if level_dbfs > SELF_NOISE_GATE_DBFS {
            self.deaf_frames = self.deaf_frames.max(if bluetooth {
                SELF_NOISE_TAIL_BLUETOOTH_MS
            } else {
                SELF_NOISE_TAIL_MS
            });
        }
    }

    /// Asks for quiet for a while, whatever the reason.
    pub fn stop_looking_for(&mut self, ms: u32) {
        self.deaf_frames = self.deaf_frames.max(ms);
    }

    pub fn meter_db(&self) -> f32 {
        self.meter_db
    }
}

/// A threshold, with an environment override for sweeping it.
///
/// **Read where the detector is built rather than where it is used**, so the
/// audio callback never touches the environment. A sweep is a rig concern; an
/// unparseable value in the environment of a shipped app would be a silent
/// behaviour change, so it is ignored rather than honoured.
pub fn tuned(name: &str, default: f32) -> f32 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse::<f32>().ok())
        .filter(|v| v.is_finite())
        .unwrap_or(default)
}
