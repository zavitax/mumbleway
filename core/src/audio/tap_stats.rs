//! What a tap detector reports, and the lamp bookkeeping behind it.
//!
//! **Shared because it is plumbing, not policy.** The two detectors —
//! [`super::tap_helmet`] and [`super::tap_phone`] — share no thresholds, no
//! filters and no rule, deliberately: they listen through different
//! microphones with different faults, and a shared front end is a shared blast
//! radius. What they do share is the shape of a report, a frame size and the
//! arithmetic of "this stage passed something recently", none of which carries
//! an opinion about what a tap is.
//!
//! The *names* of the stages are not here, because they differ: the helmet
//! path has a crest factor and a sustain limit where the phone path has a
//! margin and a per-millisecond attack. Each detector names its own, and
//! `UiAcousticStats::on_helmet` is what tells a panel which set it is reading.

/// The envelope's frame, in samples at 48 kHz. One millisecond.
///
/// Fine enough to time an attack that takes a few milliseconds, and coarse
/// enough that a ride is a few hundred thousand frames rather than tens of
/// millions.
pub const FRAME: usize = 48;

/// The floor a level is clamped at, and it is load-bearing.
///
/// **The accelerometer detector shipped the identical fault and the first
/// microphone one was written with it in hand and still got it wrong**: the
/// clamp was −140 dBFS, a number chosen for being safely low rather than for
/// meaning anything. A digitally silent block then dragged the trailing
/// reference down there, and on real recordings the floor sat at −137 while
/// the signal sat at −40 — ninety-seven decibels over a margin of twenty-six,
/// so every frame was a candidate and nothing ever fell back.
///
/// −90 dBFS is one least-significant bit of sixteen-bit audio. Below that
/// there is nothing a microphone can have told us, so a floor has no business
/// going there.
pub const QUANTISATION_DBFS: f32 = -90.0;

/// The same floor as an amplitude, so a frame can be tested without a log.
pub const QUANTISATION_AMPLITUDE: f32 = 3.162_278e-5;

/// How long a stage lamp stays lit after the last frame that passed it, in ms.
///
/// **A gate that passes for one millisecond is invisible.** A panel samples
/// once or ten times a second, so without a hold every lamp except the ones
/// that pass continuously would read dark for ever — the same reason the meter
/// has a fall-back rate beside the raw level.
///
/// A second and a half, because the slower of the two panels samples once a
/// second. That makes the row a latch rather than an instant: it shows the
/// furthest point a frame reached recently, which is what somebody tapping a
/// helmet and then looking at the screen actually wants.
pub const STAGE_HOLD_MS: u64 = 1_500;

/// How quickly the displayed level may fall, in dB per millisecond.
///
/// The raw level is a one-millisecond frame, so anything sampling it from
/// outside would miss every tap. 0.03 dB/ms is 30 dB a second.
pub const METER_FALL_DB_PER_MS: f32 = 0.03;

/// A threshold, with an environment override for sweeping it.
///
/// Read where a detector is built rather than where it is used, so the audio
/// callback never touches the environment.
pub fn tuned(name: &str, default: f32) -> f32 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse::<f32>().ok())
        .filter(|v| v.is_finite())
        .unwrap_or(default)
}

/// What a tap detector is hearing, whichever one it is.
///
/// Every field is written by the detector on the audio thread except
/// [`Self::stage_mask`], [`Self::recent_attack_db`] and
/// [`Self::recent_over_db`], which are functions of how long ago something
/// happened and are therefore filled in when the stats are read.
#[derive(Clone, Copy, Debug, Default)]
pub struct AcousticStats {
    pub frames: u64,
    /// Events that passed every test the detector has, short of the gesture.
    pub candidates: u64,
    /// Events rejected for lasting too long to be a strike.
    pub discarded_long: u64,
    /// Events rejected on the test that separates a tap from a voice: the
    /// tail on the phone path, the crest factor on the helmet path.
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
    /// from outside sees the taps. See [`METER_FALL_DB_PER_MS`].
    pub meter_db: f32,
    /// The last event's two decisive readings, whatever they are on this path.
    /// On the helmet: its rise time in milliseconds and its crest factor in
    /// dB. On the phone: its attack in dB per millisecond and its height over
    /// the floor.
    pub impulse_attack_db: f32,
    pub impulse_over_db: f32,
    /// How far the last event's tail had fallen when it was judged.
    pub impulse_decay_db: f32,
    pub peak_db: f32,
    /// The sharpest rise and the greatest height over the floor seen in the
    /// last [`STAGE_HOLD_MS`], whether or not anything was accepted.
    ///
    /// **The impulse readings are written only when an event is judged**, so a
    /// microphone whose taps never get that far reports nothing at all and
    /// there is no telling a rider who is two decibels short from one who is
    /// twenty.
    pub recent_attack_db: f32,
    pub recent_over_db: f32,
    pub run: u8,
    /// Which stages have passed something recently, lowest bit first. The
    /// stages are named by whichever detector produced this.
    pub stage_mask: u32,
}

/// When each stage last passed something, and the maxima that expire with it.
///
/// One per detector. `N` is that detector's stage count.
pub struct StageLamps<const N: usize> {
    /// Zero means never, which is why [`Self::mask`] tests it.
    at_ms: [u64; N],
    /// A running maximum with the frame it was seen on, so it can expire.
    recent: [(f32, u64); 2],
}

impl<const N: usize> Default for StageLamps<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize> StageLamps<N> {
    pub fn new() -> Self {
        Self {
            at_ms: [0; N],
            recent: [(f32::NEG_INFINITY, 0); 2],
        }
    }

    /// Records that a stage passed something, on the frame it happened.
    pub fn pass(&mut self, stage: usize, now_ms: u64) {
        self.at_ms[stage] = now_ms;
    }

    /// Offers a value to one of the two expiring maxima, 0 or 1.
    pub fn offer(&mut self, which: usize, value: f32, now_ms: u64) {
        let (held, at) = self.recent[which];
        if value > held || now_ms.saturating_sub(at) > STAGE_HOLD_MS {
            self.recent[which] = (value, now_ms);
        }
    }

    /// One of the maxima, or zero when there is nothing recent — so a reader
    /// has one "nothing here" value rather than two.
    pub fn recent(&self, which: usize, now_ms: u64) -> f32 {
        let (value, at) = self.recent[which];
        if at > 0 && now_ms.saturating_sub(at) <= STAGE_HOLD_MS {
            value
        } else {
            0.0
        }
    }

    /// Which stages have passed something within [`STAGE_HOLD_MS`].
    ///
    /// Measured on the frame clock, which only advances while audio arrives —
    /// so a dead stream freezes this rather than clearing it. The panel is
    /// what notices: it compares `frames` between polls and draws every lamp
    /// dark when nothing has moved, which is the truthful reading and needs no
    /// clock on the audio thread.
    pub fn mask(&self, now_ms: u64) -> u32 {
        let mut mask = 0u32;
        for (i, &at) in self.at_ms.iter().enumerate() {
            if at > 0 && now_ms.saturating_sub(at) <= STAGE_HOLD_MS {
                mask |= 1 << i;
            }
        }
        mask
    }
}
