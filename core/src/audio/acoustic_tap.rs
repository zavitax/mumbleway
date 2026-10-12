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

/// The band a tap actually lives in, measured rather than assumed.
///
/// **Two recordings, two completely different spectra**, which is the finding
/// that shaped this. A fingernail on the phone's body rings at 1–4 kHz — 93%
/// of its energy — while the same gesture with the phone held in a hand is a
/// thud, 87% of it under 500 Hz. There is no narrow band that catches both.
///
/// What they agree on is that **neither has anything above 4 kHz** — 0.3% of
/// the energy on one, 2.2% on the other — so cutting there throws away none of
/// the signal. It earns its place on *recall* rather than on rejection: at the
/// attack threshold below, the band finds 8 of 10 phone-tap gestures against 6
/// of 10 wideband, and narrowing it further to 2 kHz drops back to 6. The top
/// corner is where the measurement put it.
///
/// The high-pass end is cheaper to justify: engine rumble, wind and the
/// handling noise of a phone moving in a pocket are all below 100 Hz, and the
/// hand-held taps keep 87% of their energy above it.
///
/// # What this band does *not* do, and what was tried
///
/// **Band-limiting is not what keeps music out, and a band ratio is not
/// either.** Both were tried on 2026-10-11, against the request that "hi-hats
/// or high-mid / high frequencies coming out of the phone's speaker or
/// external music" must not register, and the measurement refused both.
///
/// A steeper band first. That fails because the detector arms on a margin
/// above its own *trailing average*: on a quiet floor anything audible clears
/// it, so a hi-hat 22 dB down — what three poles buy at 9 kHz — still stands
/// some 37 dB over a floor at one quantisation step, against a margin of 18.
/// Each pole bought 7 dB and the margin was never the binding constraint.
///
/// Then a scale-free ratio between the band and everything above it, on the
/// reasoning that a strike puts its energy in the band and a cymbal above it.
/// **Measured against `music_shipping/music_only__input.wav` — real music
/// through a real microphone — it is backwards.** Music's armed impulses are
/// the *most* in-band things in the corpus, median +34 dB, because what clears
/// an 18 dB margin in a dense mix is the kick and the snare, not the hi-hat.
/// The real taps are the lowest ratios measured anywhere: median +0.8 dB held
/// in a hand and +15.9 dB on the phone body, because a contact click is
/// broadband. A 6 dB gate would have kept 100% of the music and thrown away
/// half the taps.
///
/// What separates them is [`ATTACK_DB`] — contact against airborne — and that
/// is where the rule now lives. `core/tests/acoustic_tap_corpus.rs` prints
/// both distributions and is how either claim can be re-checked.
const BAND_LOW_HZ: f32 = 100.0;
const BAND_HIGH_HZ: f32 = 4_000.0;

/// How loud this app's own output has to be before the detector stops
/// listening, in dBFS.
///
/// **The detector runs on the raw microphone, before the canceller**, so
/// everything the phone plays reaches it — including its own tick, which is a
/// 2 kHz click sitting dead centre in the band a phone-body tap occupies. Tap,
/// tick, tick heard as a tap, tick: a feedback loop, and it was found by
/// measuring where taps live rather than by hearing it.
///
/// The canceller cannot help here: in the listening state there is no chain
/// running at all. So the rule is blunt and reliable — while this app is
/// making a noise, it is not listening for taps. It costs nothing, because a
/// rider is not tapping during a cue, and it covers other riders' voices
/// coming out of the speaker as well as our own cues.
const SELF_NOISE_GATE_DBFS: f32 = -60.0;

/// The same, on a Bluetooth route, where our own sound comes back late.
///
/// **A round trip, not a cue length.** A cue is mixed into the output, crosses
/// the link to the helmet, comes out of its speaker, is picked up by its own
/// microphone and crosses back: two hops of a hundred-odd milliseconds, plus
/// whatever the helmet rings for. The 120 ms that covers a phone's own speaker
/// has expired long before any of it arrives, so the detector was listening
/// again exactly in time to hear itself — which a rider reported as the
/// acknowledging tick registering as a tap.
///
/// It is also why no tick is played into a helmet at all; see the engine's
/// input callback. A tail this long is longer than the gap between two taps,
/// so a per-tap tick and tap detection cannot both be had on one link.
const SELF_NOISE_TAIL_BLUETOOTH_MS: u32 = 600;

/// How long after this app's own sound the detector stays deaf, in frames.
///
/// A cue is 18 ms to one and a half seconds; this covers the ringing after
/// the longest of them and the room's own tail.
const SELF_NOISE_TAIL_MS: u32 = 120;

/// How long after speech the detector stays deaf, in frames.
///
/// Short, because speech is reported per block and the flag is already one
/// block behind the callback: this only has to bridge the gaps *within* an
/// utterance, not outlast it. A rider who stops talking to tap is heard a
/// tenth of a second later.
const SPEECH_TAIL_MS: u32 = 100;

/// How far an impulse must have fallen 15 to 30 ms after its peak, in dB.
///
/// **What separates a plosive from a tap, measured.** /p/, /t/ and /k/ at a
/// boom microphone two centimetres from a mouth are small impacts: the attack
/// test cannot tell them from a fingertip and should not be expected to,
/// because acoustically there is no difference. What differs is what follows —
/// a tap is followed by nothing, a consonant by its vowel.
///
/// Measured 2026-10-12 as the tail-to-peak energy ratio of every transient in
/// the corpus, which is the one feature of a proposed classifier that scored
/// well on helmet taps:
///
/// | | p5 / median / p95 |
/// |---|---|
/// | taps on the helmet | 0.001 / **0.002** / **0.006** |
/// | taps on the phone body | 0.000 / 0.001 / 0.149 |
/// | voice with motor noise | **0.055** / 0.098 / 0.195 |
/// | voice over motorcycle noise | 0.000 / 0.105 / 0.454 |
///
/// Helmet taps are an order of magnitude tighter than speech, which is the
/// case this exists for.
///
/// **Those are energy ratios and this constant is a peak-level drop, which is
/// not the same unit.** A 10 dB drop in level is about a tenth in amplitude
/// and a hundredth in energy, so −10 dB here lands near 0.01 as a ratio —
/// between the helmet taps at 0.002 and the voice median at 0.098, which is
/// where it belongs. Worth saying because the two numbers look comparable and
/// are not.
///
/// Swept in the detector rather than inferred from the table, because the
/// band-pass and the windows differ from the harness the table came from:
///
/// | drop | phone taps | helmet | hand | corpus false arms |
/// |---|---|---|---|---|
/// | off | 10/10 | 5/7 | 5/7 | 152/h |
/// | **−10 dB** | **10/10** | **4/7** | 2/7 | **60/h** |
/// | −14 dB | 10/10 | 4/7 | 0/7 | 57/h |
/// | −18 dB | 9/10 | 3/7 | 0/7 | 57/h |
///
/// −10 dB keeps every phone-body gesture and removes three fifths of the false
/// arms. What it costs is the hand-held recording, which is a thud under
/// 500 Hz rather than the gesture this ships — a phone tapped through a
/// pocket, or a helmet. Below −10 the cost rises and nothing more is bought.
///
/// **The first attempt at this failed and the difference is instructive.** It
/// required the level never to come back over the margin for 90 ms, and real
/// taps ring: the ringing crosses the margin and recall on two of three tap
/// recordings went to zero. A maximum over a window ignores the bumps that a
/// never-exceeds test trips on.
///
/// `MW_TAP_DECAY_DROP_DB` overrides it; 0 disables the test.
const DECAY_DROP_DB: f32 = -10.0;

/// The window the drop is measured over, after the peak, in frames.
const DECAY_FROM_MS: u64 = 15;
const DECAY_TO_MS: u64 = 30;

/// How fast the meter falls back, in dB per millisecond.
///
/// **A meter needs ballistics or it shows nothing.** The level here is the
/// peak of a one-millisecond frame, and a panel reading it ten times a second
/// samples one frame in a hundred — so a tap, which is over in forty, is
/// missed about ninety-nine times out of a hundred. A rider watching that
/// would see a meter that never moves while they tap.
///
/// Thirty decibels a second: a tap forty decibels over the floor stays
/// visible for a second and a bit, which is long enough to read and short
/// enough not to smear three taps into one.
const METER_FALL_DB_PER_MS: f32 = 0.03;

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

/// How sharply it must arrive: this frame against the one before, in dB.
///
/// **This is the test that rejects music, and it is the only one that does.**
/// It separates *contact* from *airborne*, which turns out to be the real
/// distinction: a fingertip on the phone's body is coupled to the microphone
/// through the case, so the level steps up inside a single millisecond, while
/// anything arriving through air has its onset shaped by the speaker, the room
/// and the distance. No amount of filtering finds that difference, because it
/// is not in the spectrum — see [`BAND_LOW_HZ`] for the two attempts that were
/// measured and abandoned.
///
/// Measured 2026-10-11 over 1175 s of corpus, as p5 / median / p95 of the
/// attack of every impulse sharp enough to be judged:
///
/// | | attack, dB |
/// |---|---|
/// | taps held in a hand | +10.8 / **+25.1** / +34.5 |
/// | taps on the phone body | +9.2 / **+23.4** / +29.1 |
/// | real music, real microphone | +7.0 / **+7.9** / +11.6 |
/// | every road and voice ride | ~+7.1 / +8…+13 / — |
///
/// Twelve decibels of daylight between music's p95 and the taps' median, so
/// the threshold goes in the gap rather than on either edge. Swept against
/// both halves: at 7 dB real music completes **52 gestures an hour**; at 12 it
/// completes none; at 14 it still completes none while phone-tap recall is at
/// its best measured value, 8 of 10. Above 16 recall starts falling without
/// buying anything, since music is already at zero.
///
/// It costs the "taps on a desk" recording, which goes from 3 of 7 to none —
/// a desk is a surface the phone is resting *on* rather than a body being
/// struck, so the coupling is the airborne kind. That gesture is not the one
/// the feature is for and it is not worth 52 false arms an hour.
///
/// Override with `MW_TAP_ATTACK_DB` to re-sweep; the rig is
/// `core/tests/acoustic_tap_corpus.rs`.
const ATTACK_DB: f32 = 14.0;

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
    /// Impulses whose tail did not fall away — a consonant with its vowel
    /// behind it, rather than a tap. See `DECAY_DROP_DB`.
    pub discarded_ringing: u64,
    /// Blocks in which the chain reported speech, so the detector did not
    /// look. **Zero while a rider is talking means the gate never engaged**,
    /// which is a different fault from the gate being too weak.
    pub speech_blocks: u64,
    pub banked: u64,
    pub gestures: u64,
    pub level_db: f32,
    pub floor_db: f32,
    /// The same level with a meter's fall-back, so something sampling this
    /// from outside sees the taps. See `METER_FALL_DB_PER_MS`.
    pub meter_db: f32,
    /// The last impulse that arrived sharply enough to be judged, as its
    /// attack and its height over the floor — the two numbers the decision is
    /// actually made on, kept so the rig can sweep [`ATTACK_DB`] and
    /// [`MARGIN_DB`] against real recordings rather than against a generator.
    pub impulse_attack_db: f32,
    pub impulse_over_db: f32,
    /// How far the last impulse's tail had fallen 15 to 30 ms after its peak.
    pub impulse_decay_db: f32,
    pub peak_db: f32,
    pub run: u8,
}

/// An impulse that has peaked and is having its tail measured.
struct Held {
    candidate: Candidate,
    peak_db: f32,
    peak_at_ms: u64,
    tail_db: f32,
}

/// Listens for N taps in whatever microphone it is given.
pub struct AcousticTapDetector {
    /// A band-pass built from one-poles: three for the upper skirt, one for
    /// the lower.
    low: [f32; 3],
    high: f32,
    low_coeff: f32,
    high_coeff: f32,
    /// Frames remaining in which this app's own output is still sounding.
    deaf_frames: u32,
    /// The trailing average the margin is measured against.
    reference_db: f32,
    /// [`MARGIN_DB`] and [`ATTACK_DB`], read once so a sweep costs no rebuild
    /// and the audio callback reads no environment.
    margin_db: f32,
    attack_db: f32,
    decay_drop_db: f32,
    pattern: GesturePattern,
    previous_db: f32,
    /// When the frame that started a candidate arrived, and how loud it was.
    rising_at_ms: Option<u64>,
    peak_db: f32,
    /// The absolute level at the peak, and when it was, for the tail measure.
    peak_level_db: f32,
    peak_at_ms: u64,
    latent_until_ms: u64,
    /// An impulse waiting for its tail to be measured: the candidate, the peak
    /// level it reached, when that peak was, and the loudest frame seen so far
    /// in the tail window.
    held: Option<Held>,
    frame_ms: u64,
    stats: AcousticStats,
}

impl Default for AcousticTapDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl AcousticTapDetector {
    pub fn new() -> Self {
        Self {
            low: [0.0; 3],
            high: 0.0,
            // One-pole coefficients for a 48 kHz frame rate.
            low_coeff: Self::coeff(Self::tuned("MW_TAP_BAND_HIGH_HZ", BAND_HIGH_HZ)),
            high_coeff: Self::coeff(Self::tuned("MW_TAP_BAND_LOW_HZ", BAND_LOW_HZ)),
            deaf_frames: 0,
            reference_db: f32::NEG_INFINITY,
            margin_db: Self::tuned("MW_TAP_MARGIN_DB", MARGIN_DB),
            attack_db: Self::tuned("MW_TAP_ATTACK_DB", ATTACK_DB),
            decay_drop_db: Self::tuned("MW_TAP_DECAY_DROP_DB", DECAY_DROP_DB),
            held: None,
            pattern: GesturePattern::new(),
            previous_db: f32::NEG_INFINITY,
            rising_at_ms: None,
            peak_db: f32::NEG_INFINITY,
            peak_level_db: f32::NEG_INFINITY,
            peak_at_ms: 0,
            latent_until_ms: 0,
            frame_ms: 0,
            stats: AcousticStats {
                // **Not the derived zero**, which is full scale: the meter
                // would start pinned and take three seconds to fall, so a
                // rider opening the panel would see a microphone that looks
                // deafeningly loud and settles for no reason.
                meter_db: QUANTISATION_DBFS,
                ..AcousticStats::default()
            },
        }
    }

    /// A threshold, with an environment override for sweeping it.
    ///
    /// **Read here rather than where it is used**, so the audio callback never
    /// touches the environment. A sweep is a rig concern; a bad value in the
    /// environment of a shipped app would be a silent behaviour change, so an
    /// unparseable one is ignored rather than honoured.
    fn tuned(name: &str, default: f32) -> f32 {
        std::env::var(name)
            .ok()
            .and_then(|v| v.parse::<f32>().ok())
            .filter(|v| v.is_finite())
            .unwrap_or(default)
    }

    fn coeff(hz: f32) -> f32 {
        let x = (-2.0 * std::f32::consts::PI * hz / 48_000.0).exp();
        1.0 - x
    }

    /// Tells the detector how loud this app's own output is, so it can stop
    /// listening while the phone is making a noise. See
    /// [`SELF_NOISE_GATE_DBFS`].
    pub fn hear_own_output(&mut self, level_dbfs: f32, bluetooth: bool) {
        if level_dbfs > SELF_NOISE_GATE_DBFS {
            self.deaf_frames = self.deaf_frames.max(if bluetooth {
                SELF_NOISE_TAIL_BLUETOOTH_MS
            } else {
                SELF_NOISE_TAIL_MS
            });
        }
    }

    /// Tells the detector the chain is hearing speech, so it should not look.
    ///
    /// **A plosive is a small impact.** /p/, /t/ and /k/ at a boom microphone
    /// two centimetres from a mouth have onsets as sharp as a fingertip on a
    /// phone's case, which is what the attack test measures — so a rider
    /// talking on a helmet was tapping it, as far as this could tell.
    ///
    /// Suppressed rather than filtered, because there is nothing in the sound
    /// to filter on: the two really are the same kind of event, and what tells
    /// them apart is that one happens while somebody is talking. Tapping
    /// mid-word is not a thing a rider does — they stop, then tap — so the
    /// gesture loses nothing it was going to get.
    pub fn hear_speech(&mut self) {
        self.stats.speech_blocks += 1;
        self.deaf_frames = self.deaf_frames.max(SPEECH_TAIL_MS);
    }

    pub fn stats(&self) -> AcousticStats {
        self.stats
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

        // Band-limited first, and only then measured. The band is for recall
        // — see `BAND_LOW_HZ`, which also records the two ways band-limiting
        // was tried as a way of rejecting music, and why neither worked.
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

        // While this app is making a noise it is not listening for taps.
        //
        // **It keeps tracking the level, though, and that is not a detail.**
        // This block used to set `previous_db` to minus infinity, which made
        // the attack of the first frame after hearing resumed *infinite* — so
        // every recovery from a cue armed a candidate, whatever the audio was
        // doing. The rig found it the moment it began simulating the engine's
        // own ticks: false arms went from 184 an hour to 233, with the only
        // change being that the detector could now hear itself.
        //
        // The trailing reference is deliberately left alone: our own cue is
        // forty decibels over the room, and letting it into the floor would
        // raise the bar for the tap that follows it.
        if self.deaf_frames > 0 {
            self.deaf_frames -= 1;
            self.held = None;
            self.rising_at_ms = None;
            self.peak_db = f32::NEG_INFINITY;
            self.previous_db = (20.0 * (peak + 1e-7).log10()).max(QUANTISATION_DBFS);
            return None;
        }
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
        // A peak that falls back slowly enough to be seen. See
        // `METER_FALL_DB_PER_MS`: the raw level is a one-millisecond frame and
        // anything sampling it from outside would miss every tap.
        self.stats.meter_db = level_db.max(self.stats.meter_db - METER_FALL_DB_PER_MS);

        // A held impulse gathers the loudest frame in its tail window and is
        // judged at the end of it. A maximum rather than a never-exceeds test:
        // taps ring, and the first version of this tripped on the ringing and
        // threw away every tap on two of three recordings.
        if let Some(h) = self.held.as_mut() {
            let since_peak = at_ms.saturating_sub(h.peak_at_ms);
            if since_peak >= DECAY_FROM_MS {
                h.tail_db = h.tail_db.max(level_db);
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
                // **Both tests, and the attack is the one doing the work.**
                // Speech and music both cross the margin; neither crosses it
                // inside a millisecond, because only contact with the phone
                // does. See `ATTACK_DB` for the measurement.
                if over >= self.margin_db && attack >= self.attack_db {
                    self.stats.impulse_attack_db = attack;
                    self.stats.impulse_over_db = over;
                    self.rising_at_ms = Some(at_ms);
                    self.peak_db = over;
                    self.peak_level_db = level_db;
                    self.peak_at_ms = at_ms;
                }
            }
            Some(since) => {
                if over > self.peak_db {
                    self.peak_db = over;
                    self.peak_level_db = level_db;
                    self.peak_at_ms = at_ms;
                }
                if over >= self.margin_db {
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
                    let candidate = Candidate {
                        at_ms: since,
                        strength_db,
                    };
                    if self.decay_drop_db >= 0.0 {
                        return self.offer(candidate, banked);
                    }
                    // Held while its tail is measured. See `DECAY_DROP_DB`: a
                    // vowel arriving behind this is what says it was a
                    // consonant and not a tap.
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

    /// A tap as it was measured: full level in the first frame, then a decay
    /// to a tenth in about forty milliseconds, in the band taps occupy.
    ///
    /// **The frequencies here used to be wrong, and wrong in the direction
    /// that mattered.** This generator ran at `2.399` and `7.13` radians a
    /// sample, which is 18.3 kHz and — past Nyquist — an aliased 6.5 kHz. So
    /// for as long as it existed, the fixture proving taps are *found* was
    /// spectrally a cymbal, and the fixture proving a hi-hat is *rejected* was
    /// the same signal with a different name. No rule could ever have
    /// separated them, which is why one of the two tests always failed.
    ///
    /// 1.6 kHz and 3.1 kHz are inside the measured band — see `BAND_LOW_HZ`,
    /// where a phone-body tap is 93% between 1 and 4 kHz. The lesson is the
    /// one in `CLAUDE.md`: a synthetic signal agrees with whoever wrote it,
    /// and this one was agreeing with a spectrum nobody had checked.
    fn tap(into: &mut Vec<f32>, peak: f32) {
        for ms in 0..60 {
            let a = peak * (-(ms as f32) / 14.0).exp();
            for i in 0..FRAME {
                // Broadband within the band, which is what a strike is.
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

        let mut d = AcousticTapDetector::new();
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
        let mut d = AcousticTapDetector::new();
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
                audio.push(0.3 * ((t * 0.209).sin() + (t * 0.406).sin()) * 0.5);
            }
        }
        quiet(&mut audio, 400);

        let mut d = AcousticTapDetector::new();
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
                audio.push(a * (t * 0.209).sin());
            }
        }
        quiet(&mut audio, 400);

        let mut d = AcousticTapDetector::new();
        let mut banked = false;
        assert!(d.push(&audio, &mut banked).is_none());
        assert_eq!(d.stats().gestures, 0);
    }

    /// Music on a beat, which is the false arm that matters.
    ///
    /// **This replaces a test that asserted something untrue.** The old one
    /// synthesised a 9 kHz hi-hat and claimed the band-pass rejected it; it
    /// could not, for a reason that is now written up under `BAND_LOW_HZ`, and
    /// it was never going to, because the `tap()` fixture beside it was
    /// generating 18 kHz and the two signals were the same kind of thing.
    ///
    /// What is true, and measured on real music through a real microphone, is
    /// that a drum is **struck somewhere else and arrives through air**: its
    /// onset is shaped by the speaker, the room and the distance, and comes
    /// out at a median of 7.9 dB a millisecond against 23–25 for a fingertip
    /// on the phone's own body. So the envelope here ramps over 8 ms — fast
    /// for anything acoustic, ten times slower than contact.
    ///
    /// Three of them on an even beat, because that is the hard case and the
    /// reason the gesture pattern cannot carry this alone: **a bar of music is
    /// matched impulses at matched intervals**, which is exactly what the
    /// pattern is looking for, and better matched than a human hand manages.
    /// The attack is the only thing standing between a backbeat and a stop
    /// command.
    #[test]
    fn music_on_a_beat_is_not_a_gesture() {
        let mut audio = Vec::new();
        quiet(&mut audio, 400);
        for _ in 0..3 {
            for ms in 0..200 {
                // Up over 8 ms, then a drum's own decay. In band, loud, and
                // on time — everything but the onset of a strike.
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

        let mut d = AcousticTapDetector::new();
        let mut banked = false;
        assert!(d.push(&audio, &mut banked).is_none());
        assert_eq!(
            d.stats().gestures,
            0,
            "music on a beat completed a gesture: {:?}",
            d.stats()
        );
    }

    /// **A consonant with its vowel behind it is not a tap.**
    ///
    /// The onset of /p/ is an impact and the attack test cannot tell it from a
    /// fingertip, because there is nothing to tell: acoustically they are the
    /// same event. What differs is the next thirty milliseconds — a tap has
    /// nothing in them, a consonant has a vowel. Measured on the corpus at
    /// 0.002 against 0.098 as an energy ratio; see `DECAY_DROP_DB`.
    ///
    /// **It needs a real noise floor to be a fair test**, which took one
    /// failure to notice. Over near-silence the two tests cannot both be
    /// reached: the margin is eighteen decibels over the floor, so a tail
    /// quiet enough to fall under it is necessarily more than ten decibels
    /// below any loud peak, and the decay gate never gets a say. A room at
    /// −40 dBFS is where both are live, and it is also where a rider is.
    #[test]
    fn an_impulse_with_a_vowel_behind_it_is_not_a_tap() {
        // A floor the margin is measurable against: −40 dBFS, so an impulse
        // at −16 is 24 dB over it and a tail at −24 is under the 18 dB margin
        // while being only 8 dB below the peak.
        fn room(into: &mut Vec<f32>, ms: usize, level: f32) {
            for m in 0..ms {
                for i in 0..FRAME {
                    let t = (m * FRAME + i) as f32;
                    into.push(level * ((t * 0.37).sin() + (t * 1.1).sin()) * 0.5);
                }
            }
        }

        let mut audio = Vec::new();
        room(&mut audio, 1_500, 0.01);
        for _ in 0..3 {
            room(&mut audio, 3, 0.158);
            room(&mut audio, 57, 0.063);
            room(&mut audio, 160, 0.01);
        }
        room(&mut audio, 400, 0.01);

        let mut d = AcousticTapDetector::new();
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
    ///
    /// A plosive at a boom microphone is a small impact and the attack test
    /// cannot tell it from a fingertip, because acoustically it is the same
    /// kind of event. What tells them apart is that one happens while somebody
    /// is talking, so the chain's own speech decision is what suppresses it.
    #[test]
    fn taps_are_not_looked_for_while_the_rider_is_talking() {
        let mut d = AcousticTapDetector::new();
        let mut banked = false;
        let mut quiet_audio = Vec::new();
        quiet(&mut quiet_audio, 400);
        d.push(&quiet_audio, &mut banked);

        // Three plosives on a beat, each announced as speech the way the
        // worker announces it.
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
            "speech completed a gesture: {:?}",
            d.stats()
        );
    }

    /// **A helmet hears this app's own cue a round trip later.**
    ///
    /// The 120 ms tail covers a phone's own speaker. Over Bluetooth the sound
    /// crosses the link, comes out of the helmet, is picked up by the helmet's
    /// microphone and crosses back, so it lands after the detector has started
    /// listening again — which a rider reported as the acknowledging tick
    /// registering as a tap.
    #[test]
    fn a_helmet_does_not_hear_this_app_a_round_trip_later() {
        let mut d = AcousticTapDetector::new();
        let mut banked = false;
        let mut quiet_audio = Vec::new();
        quiet(&mut quiet_audio, 400);
        d.push(&quiet_audio, &mut banked);

        for _ in 0..3 {
            // The cue is mixed now, and arrives at the microphone 300 ms
            // later, which the phone-sized tail would have let through.
            d.hear_own_output(-20.0, true);
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

        // And the phone-sized tail really is too short for that, so the test
        // above is testing the Bluetooth tail and not the margin.
        let mut p = AcousticTapDetector::new();
        p.push(&quiet_audio, &mut banked);
        for _ in 0..3 {
            p.hear_own_output(-20.0, false);
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

    /// **The feedback loop this nearly shipped with.** The tick played to
    /// acknowledge a tap is a 2 kHz click, which is exactly the band a tap on
    /// the phone's body occupies, and the detector runs on the raw microphone
    /// before any canceller. Without the gate, the acknowledgement is heard
    /// as the next tap and acknowledged in turn.
    #[test]
    fn the_app_does_not_hear_its_own_noise() {
        let mut d = AcousticTapDetector::new();
        let mut banked = false;
        let mut quiet_audio = Vec::new();
        quiet(&mut quiet_audio, 400);
        d.push(&quiet_audio, &mut banked);

        for _ in 0..3 {
            // The engine says it is playing, loudly, as it would while a cue
            // sounds.
            d.hear_own_output(-20.0, false);
            let mut audio = Vec::new();
            tap(&mut audio, 0.4);
            quiet(&mut audio, 160);
            d.push(&audio, &mut banked);
        }
        assert_eq!(
            d.stats().candidates,
            0,
            "the app heard itself: {:?}",
            d.stats()
        );
        assert_eq!(d.stats().gestures, 0);
    }
}
