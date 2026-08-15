//! Signal processing stages that run before the noise suppressor and codec.
//!
//! Tuned for the hard case in the requirements: a microphone inside a
//! motorcycle helmet at speed. That environment is dominated by
//!
//! * **wind and engine rumble** — high energy below ~200 Hz, which masks speech
//!   and wastes codec bits, so it is filtered out aggressively;
//! * **broadband road noise** — handled by the RNNoise stage in [`super::denoise`];
//! * **wildly varying speech level** — the rider shouts on the motorway and
//!   murmurs at a red light, so a slow AGC keeps the far end comfortable;
//! * **wind gusts hitting the capsule** — brief huge transients that a limiter
//!   must catch before they clip.

/// One biquad section in direct form I.
#[derive(Debug, Clone, Copy)]
pub struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
}

impl Biquad {
    /// Second-order Butterworth high-pass.
    pub fn high_pass(sample_rate: f32, cutoff_hz: f32, q: f32) -> Self {
        let w0 = 2.0 * std::f32::consts::PI * cutoff_hz / sample_rate;
        let (sin_w0, cos_w0) = w0.sin_cos();
        let alpha = sin_w0 / (2.0 * q);

        let b0 = (1.0 + cos_w0) / 2.0;
        let b1 = -(1.0 + cos_w0);
        let b2 = (1.0 + cos_w0) / 2.0;
        let a0 = 1.0 + alpha;
        let a1 = -2.0 * cos_w0;
        let a2 = 1.0 - alpha;

        Self {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: a1 / a0,
            a2: a2 / a0,
            x1: 0.0,
            x2: 0.0,
            y1: 0.0,
            y2: 0.0,
        }
    }

    /// Second-order Butterworth low-pass — the mirror of [`Self::high_pass`].
    ///
    /// Only the numerator differs, and only in the sign of the middle term and
    /// which side of the cosine is taken. Worth writing out rather than
    /// deriving from the high-pass at run time: the two are used in a cascade
    /// together and a transcription error in one shows up as the other quietly
    /// doing nothing.
    pub fn low_pass(sample_rate: f32, cutoff_hz: f32, q: f32) -> Self {
        let w0 = 2.0 * std::f32::consts::PI * cutoff_hz / sample_rate;
        let (sin_w0, cos_w0) = w0.sin_cos();
        let alpha = sin_w0 / (2.0 * q);

        let b0 = (1.0 - cos_w0) / 2.0;
        let b1 = 1.0 - cos_w0;
        let b2 = (1.0 - cos_w0) / 2.0;
        let a0 = 1.0 + alpha;
        let a1 = -2.0 * cos_w0;
        let a2 = 1.0 - alpha;

        Self {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: a1 / a0,
            a2: a2 / a0,
            x1: 0.0,
            x2: 0.0,
            y1: 0.0,
            y2: 0.0,
        }
    }

    /// RBJ peaking EQ: a bell of `gain_db` centred on `centre_hz`.
    ///
    /// **The only filter here that can boost**, and the only one whose gain is
    /// not 0 dB somewhere. `q` sets the width — at 1.0 the bell is a little
    /// under two octaves at half its height, which is wide enough to cover a
    /// vowel's first two formants and narrow enough to leave the top of the
    /// band alone.
    ///
    /// Note `A` is `10^(g/40)` and not `10^(g/20)`: the peak gain of this
    /// design is `A²`. Getting that wrong gives a filter that is quietly twice
    /// as strong in decibels as asked for, which looks like a working feature
    /// with badly chosen constants.
    pub fn peaking(sample_rate: f32, centre_hz: f32, q: f32, gain_db: f32) -> Self {
        let a = 10f32.powf(gain_db / 40.0);
        let w0 = 2.0 * std::f32::consts::PI * centre_hz / sample_rate;
        let (sin_w0, cos_w0) = w0.sin_cos();
        let alpha = sin_w0 / (2.0 * q);

        let b0 = 1.0 + alpha * a;
        let b1 = -2.0 * cos_w0;
        let b2 = 1.0 - alpha * a;
        let a0 = 1.0 + alpha / a;
        let a1 = -2.0 * cos_w0;
        let a2 = 1.0 - alpha / a;

        Self {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: a1 / a0,
            a2: a2 / a0,
            x1: 0.0,
            x2: 0.0,
            y1: 0.0,
            y2: 0.0,
        }
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.b1 * self.x1 + self.b2 * self.x2
            - self.a1 * self.y1
            - self.a2 * self.y2;
        self.x2 = self.x1;
        self.x1 = x;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }

    pub fn reset(&mut self) {
        self.x1 = 0.0;
        self.x2 = 0.0;
        self.y1 = 0.0;
        self.y2 = 0.0;
    }
}

/// Cascaded high-pass used to strip wind and engine rumble.
#[derive(Debug, Clone)]
pub struct RumbleFilter {
    sections: Vec<Biquad>,
}

impl RumbleFilter {
    /// A 4th-order Butterworth high-pass (two cascaded biquads).
    pub fn new(sample_rate: f32, cutoff_hz: f32) -> Self {
        // Butterworth Q values for a 4th-order cascade.
        Self {
            sections: vec![
                Biquad::high_pass(sample_rate, cutoff_hz, 0.541_196),
                Biquad::high_pass(sample_rate, cutoff_hz, 1.306_563),
            ],
        }
    }

    pub fn process(&mut self, buf: &mut [f32]) {
        for s in buf.iter_mut() {
            let mut v = *s;
            for section in self.sections.iter_mut() {
                v = section.process(v);
            }
            *s = v;
        }
    }

    pub fn reset(&mut self) {
        for s in self.sections.iter_mut() {
            s.reset();
        }
    }
}

/// Cascaded low-pass that closes the top of the band.
///
/// The chain has always had a high-pass and never a low-pass, which leaves
/// everything above the voice in the signal: wind hiss, tyre roar, chain and
/// sprocket noise, and the top octave of a helmet's own turbulence. None of it
/// carries a word, and all of it is counted — by the level meter, by the noise
/// floor tracker, by the gate that compares one against the other, and by the
/// AGC deciding how much gain a "quiet" block needs.
///
/// So it is not only bandwidth spent on nothing. It is noise that moves the
/// thresholds the transmit decision is made against, which is why this belongs
/// *before* those measurements rather than on the way to the encoder.
#[derive(Debug, Clone)]
pub struct SpeechBand {
    sections: Vec<Biquad>,
}

impl SpeechBand {
    /// A 4th-order Butterworth low-pass (two cascaded biquads).
    ///
    /// Same order as [`RumbleFilter`], and for the same reason: a 2nd-order
    /// skirt is still 6 dB down only an octave out, which at these corners
    /// leaves most of what it was meant to remove.
    pub fn new(sample_rate: f32, cutoff_hz: f32) -> Self {
        Self {
            sections: vec![
                Biquad::low_pass(sample_rate, cutoff_hz, 0.541_196),
                Biquad::low_pass(sample_rate, cutoff_hz, 1.306_563),
            ],
        }
    }

    pub fn process(&mut self, buf: &mut [f32]) {
        for s in buf.iter_mut() {
            let mut v = *s;
            for section in self.sections.iter_mut() {
                v = section.process(v);
            }
            *s = v;
        }
    }

    pub fn reset(&mut self) {
        for s in self.sections.iter_mut() {
            s.reset();
        }
    }
}

/// Root-mean-square of a block.
pub fn rms(buf: &[f32]) -> f32 {
    if buf.is_empty() {
        return 0.0;
    }
    let sum = energy(buf);
    (sum / buf.len() as f32).sqrt()
}

/// Peak absolute value of a block.
pub fn peak(buf: &[f32]) -> f32 {
    buf.iter().fold(0.0f32, |m, s| m.max(s.abs()))
}

/// Converts a linear amplitude to dBFS.
pub fn to_dbfs(amplitude: f32) -> f32 {
    if amplitude <= 1e-9 {
        -120.0
    } else {
        20.0 * amplitude.log10()
    }
}

/// Tracks the background noise level using minimum statistics.
///
/// This exists because RNNoise's speech probability cannot be trusted on its own
/// in a helmet: the harmonic structure of engine and wind noise reads as voiced
/// speech, and it happily reports VAD > 0.8 on a steady 55 Hz drone. Gating on
/// that alone keys the transmitter continuously at speed.
///
/// The fix is to decide on *signal-to-noise ratio* instead. Speech is
/// intermittent, so the minimum level over a couple of seconds is a good estimate
/// of the steady background; anything that fails to rise clearly above it is
/// noise no matter what the network says.
#[derive(Debug, Clone)]
pub struct NoiseFloorTracker {
    /// Minimum seen so far in the sub-window being filled.
    sub_min: f32,
    /// Completed sub-window minima.
    mins: [f32; Self::SUB_WINDOWS],
    idx: usize,
    count: u32,
    sub_len: u32,
    /// Added to the raw minimum, since a minimum under-estimates the mean level.
    bias_db: f32,
    /// The floor actually reported, which may lag the minimum on the way up.
    ///
    /// `INFINITY` until the first update, so a fresh tracker still reports the
    /// permissive default rather than a rate-limited climb from nowhere.
    published_db: f32,
    /// Whether voice was present on the previous block, so the rising edge can
    /// be told from a continued utterance. Only the edge rolls the floor back.
    was_voice: bool,
    /// A short history of the reported floor, one sample per
    /// [`Self::HISTORY_STRIDE`] blocks, so a late verdict can be answered with
    /// the floor as it stood *before* the speech that provoked it.
    history: [f32; Self::HISTORY_SLOTS],
    history_idx: usize,
    history_count: u32,
    /// Consecutive blocks the floor has been held down for.
    held_blocks: u32,
    /// How many times the watchdog has overruled the freeze this session.
    ///
    /// Published to the diagnostics panel rather than kept private: a
    /// non-zero count means something is holding the freeze on for a minute at
    /// a time, which is either a very long phrase or a trigger keyed on the
    /// wrong thing, and the panel is where that becomes visible instead of
    /// being inferred from audio that sounds slightly wrong.
    watchdog_trips: u32,
}

impl NoiseFloorTracker {
    /// More, shorter sub-windows mean a stale minimum expires sooner. A single
    /// anomalously quiet block would otherwise pin the floor for the whole
    /// window, which is exactly what a codec warm-up transient produces.
    const SUB_WINDOWS: usize = 6;

    /// The most the reported floor may **rise** in one 10 ms block: 0.06 dB,
    /// which is 6 dB/s. Falling is not limited at all.
    ///
    /// **This is the fix for a floor that climbed onto the voice and cut it
    /// off**, reported from two recordings on 2026-08-14 — one of long spoken
    /// phrases, one of singing. Minimum statistics assumes what the comment
    /// above says it assumes: *speech is intermittent*, so some sub-window
    /// inside the memory holds a gap. Hold a phrase past 1.5 s and every
    /// sub-window is full of voice, the minimum **is** the voice, and the SNR
    /// the gate runs on collapses to nothing. In the sung clip the floor rose
    /// 53 dB in half a second, went 0.6 dB *above* the singing, and 58% of the
    /// voiced blocks fell under the gate's margin. Singing is the worst case
    /// only because a held note has no gaps at all; the spoken clip showed the
    /// same climb, 56 dB of it.
    ///
    /// A rise limit fixes it without trusting anybody's speech detector, which
    /// matters: this estimator exists precisely *because* RNNoise's VAD cannot
    /// be trusted on a helmet, so freezing on that VAD would undo the reason
    /// for the class. It needs no extra signal at all — it simply declines to
    /// believe that background noise changed faster than background noise can.
    ///
    /// 6 dB/s was chosen by replaying both recordings through the candidates,
    /// not by taste. 12 dB/s still lost 17% of the sung voice; 6 and 3 both
    /// lost none, and 6 is the faster of the two, so it keeps as much of the
    /// original responsiveness as the fix allows.
    ///
    /// **The cost, stated plainly:** a genuine background rise is now followed
    /// at 6 dB/s once the sub-windows have flushed, so hard acceleration under
    /// a 30 dB swing takes about five seconds to track instead of arriving with
    /// the window. During that lag the floor under-reads and the gate is too
    /// permissive rather than too strict — noise gets through for a moment
    /// instead of speech being cut, which is the better way round, and the VAD
    /// and harmonicity gates still have to agree before anything transmits.
    const MAX_RISE_DB_PER_BLOCK: f32 = 0.06;

    /// Blocks between samples of the reported floor, and how many are kept.
    ///
    /// 10 blocks is 100 ms; 30 slots is three seconds. Three because that is
    /// how stale a verdict from the background classifier can be: it infers
    /// every two seconds on a window of just under one, so speech may have
    /// begun almost three seconds before anything says so.
    const HISTORY_STRIDE: u32 = 10;
    const HISTORY_SLOTS: usize = 30;

    /// How long the floor may be held down before the freeze is overruled.
    ///
    /// **The freeze can latch, and this is what stops it.** Held on a signal
    /// derived from the floor -- the gate opens above it, so freezing while the
    /// gate is open is exactly that -- a loud steady noise opens the gate, the
    /// freeze pins the floor beneath it, and neither can ever recover: the gate
    /// stays open because the floor cannot rise, and the floor cannot rise
    /// because the gate is open. The chain already carries a scar from the same
    /// shape, in `denoise.rs`: an arm that let the VAD refresh the hangover
    /// kept a transmission open for ever on an engine drone.
    ///
    /// Sixty seconds is chosen against the two failures it sits between. Below
    /// it, an engine drone holds the channel for that long, which is a long
    /// time on a shared frequency. Above it, a rider singing one long phrase
    /// crosses it and the floor starts climbing back onto them mid-performance
    /// -- the fault this whole mechanism exists to prevent. Sixty is
    /// comfortably past the longest phrase measured (13.8 s sung) and short
    /// enough that a drone is not indefinite.
    ///
    /// It is a backstop, not the mechanism. Anything relying on it firing
    /// regularly is keyed on the wrong signal.
    /// Sixty seconds, at the 100 blocks a second the whole pipeline runs at
    /// (10 ms of 48 kHz). Written as a literal rather than derived: the frame
    /// size lives in `denoise`, which imports this module, and importing it
    /// back to save one multiplication is not worth a cycle between them.
    const FREEZE_WATCHDOG_BLOCKS: u32 = 6_000;

    /// `sub_len_blocks` sub-windows of this length span the tracking window;
    /// six 0.25 s sub-windows give a ~1.5 s memory, longer than a normal breath
    /// pause but short enough to follow changing road speed.
    pub fn new(sub_len_blocks: u32) -> Self {
        Self {
            sub_min: f32::INFINITY,
            mins: [f32::INFINITY; Self::SUB_WINDOWS],
            idx: 0,
            count: 0,
            sub_len: sub_len_blocks.max(1),
            bias_db: 3.0,
            published_db: f32::INFINITY,
            was_voice: false,
            history: [f32::INFINITY; Self::HISTORY_SLOTS],
            history_idx: 0,
            history_count: 0,
            held_blocks: 0,
            watchdog_trips: 0,
        }
    }

    /// Feeds one block level and returns the current floor estimate in dBFS.
    ///
    /// Equivalent to [`Self::update_gated`] with nothing saying there is a
    /// voice, which is the right default for a caller that has no such signal.
    pub fn update(&mut self, level_db: f32) -> f32 {
        self.update_gated(level_db, false)
    }

    /// Feeds one block, with `voice` saying whether something is speaking or
    /// singing right now.
    ///
    /// **While `voice` holds, the floor may fall but never rise.** A minimum
    /// statistic assumes the quietest thing in its memory is background; a held
    /// phrase makes that assumption false, and the rate limit above only slows
    /// the resulting climb rather than stopping it. Nothing but "there is a
    /// voice in this" can stop it, because the estimator cannot tell the
    /// difference on level alone — that is the whole reason it was fooled.
    ///
    /// **Falling is still allowed**, deliberately. A floor that dropped during
    /// a gap and is now held down is the safe direction: it opens the gate too
    /// readily rather than closing it on the voice, and the suppressor ahead of
    /// it is what deals with whatever comes through. Between phrases the freeze
    /// lifts and the floor climbs to wherever the room actually is.
    ///
    /// On the **rising edge** the floor is rolled back to the lowest value it
    /// held in the last three seconds. A verdict from the classifier can be
    /// most of three seconds old, so by the time it says "speech" the floor may
    /// already have climbed onto the very speech being reported. Freezing where
    /// it *is* would preserve that damage; freezing where it was before the
    /// speech began is what was actually asked for.
    pub fn update_gated(&mut self, level_db: f32, voice: bool) -> f32 {
        self.sub_min = self.sub_min.min(level_db);
        self.count += 1;
        if self.count >= self.sub_len {
            self.mins[self.idx] = self.sub_min;
            self.idx = (self.idx + 1) % Self::SUB_WINDOWS;
            self.sub_min = f32::INFINITY;
            self.count = 0;
        }

        // The floor as it stood before whatever provoked this verdict. Taken
        // before the history is advanced, so it cannot include this block.
        if voice && !self.was_voice {
            let before = self.oldest_floor_db();
            if before.is_finite() && before < self.published_db {
                self.published_db = before;
            }
        }
        self.was_voice = voice;

        // The watchdog. Counted here so that it covers every path below,
        // including the one where the floor is falling anyway.
        if voice {
            self.held_blocks = self.held_blocks.saturating_add(1);
        } else {
            self.held_blocks = 0;
        }
        let overruled = self.held_blocks > Self::FREEZE_WATCHDOG_BLOCKS;
        if overruled && self.held_blocks == Self::FREEZE_WATCHDOG_BLOCKS + 1 {
            self.watchdog_trips = self.watchdog_trips.saturating_add(1);
        }

        let raw = self.raw_floor_db();
        self.published_db = if !self.published_db.is_finite() {
            // First block: nothing to rate-limit against, and starting the
            // climb from the permissive default would mute the opening word.
            raw
        } else if raw < self.published_db {
            // **Down is instant.** A room that went quiet is a fact, and a
            // floor that lags downward would hold the gate shut on a voice
            // that is now well clear of it.
            raw
        } else if voice && !overruled {
            // Held. Not slowed — held. See the note on this method.
            self.published_db
        } else {
            (self.published_db + Self::MAX_RISE_DB_PER_BLOCK).min(raw)
        };

        self.remember_floor();
        self.published_db
    }

    /// Samples the reported floor into the rolling history, every
    /// [`Self::HISTORY_STRIDE`] blocks.
    fn remember_floor(&mut self) {
        self.history_count += 1;
        if self.history_count < Self::HISTORY_STRIDE {
            return;
        }
        self.history_count = 0;
        self.history[self.history_idx] = self.published_db;
        self.history_idx = (self.history_idx + 1) % Self::HISTORY_SLOTS;
    }

    /// The lowest floor held in the last three seconds.
    ///
    /// The minimum rather than the oldest entry: what is wanted is a value from
    /// before the speech, and speech only ever pushes the floor *up*, so the
    /// lowest recent value is the one least contaminated by it. Taking the
    /// oldest slot alone would be at the mercy of where in the ring the phrase
    /// happened to start.
    fn oldest_floor_db(&self) -> f32 {
        self.history.iter().copied().fold(f32::INFINITY, f32::min)
    }

    /// The reported floor: the minimum statistic, held back on the way up by
    /// [`Self::MAX_RISE_DB_PER_BLOCK`].
    pub fn floor_db(&self) -> f32 {
        if self.published_db.is_finite() {
            self.published_db
        } else {
            self.raw_floor_db()
        }
    }

    /// How long the floor has been held down, in blocks.
    pub fn held_blocks(&self) -> u32 {
        self.held_blocks
    }

    /// How many times the watchdog has overruled the freeze.
    pub fn watchdog_trips(&self) -> u32 {
        self.watchdog_trips
    }

    /// Whether the watchdog is currently overruling the freeze.
    pub fn freeze_overruled(&self) -> bool {
        self.held_blocks > Self::FREEZE_WATCHDOG_BLOCKS
    }

    /// The minimum statistic itself, before the rise limit.
    ///
    /// Kept separate so the limit has something to be limited against, and so a
    /// test can tell "the estimator is wrong" from "the estimator is right and
    /// has not been allowed there yet".
    fn raw_floor_db(&self) -> f32 {
        let completed = self.mins.iter().copied().fold(f32::INFINITY, f32::min);
        let m = completed.min(self.sub_min);
        if m.is_finite() {
            m + self.bias_db
        } else {
            -100.0
        }
    }

    /// How far `level_db` sits above the estimated noise floor.
    pub fn snr_db(&self, level_db: f32) -> f32 {
        level_db - self.floor_db()
    }

    pub fn reset(&mut self) {
        self.sub_min = f32::INFINITY;
        self.mins = [f32::INFINITY; Self::SUB_WINDOWS];
        self.idx = 0;
        self.count = 0;
        // Back to "not yet set" rather than to a number, so the first block
        // after a reset lands on the estimate instead of climbing to it at
        // 6 dB/s from wherever the previous device left it.
        self.published_db = f32::INFINITY;
        self.was_voice = false;
        self.history = [f32::INFINITY; Self::HISTORY_SLOTS];
        self.history_idx = 0;
        self.history_count = 0;
        self.held_blocks = 0;
        // `watchdog_trips` deliberately survives a reset: it counts what has
        // happened this session, and a device change is not a reason to forget
        // that the freeze had to be overruled.
    }
}

/// A noise gate with separate open/close thresholds and a hold time.
///
/// Hysteresis matters here: with a single threshold, road noise hovering right at
/// the limit would chatter the gate open and closed several times a second.
#[derive(Debug, Clone)]
pub struct NoiseGate {
    /// Level at which the gate opens, dBFS.
    pub open_db: f32,
    /// Level below which it closes, dBFS. Must be below `open_db`.
    pub close_db: f32,
    /// How long to stay open after the signal drops, in blocks.
    pub hold_blocks: u32,
    open: bool,
    hold_left: u32,
    gain: f32,
    attack: f32,
    release: f32,
}

impl NoiseGate {
    pub fn new(open_db: f32, close_db: f32, hold_blocks: u32) -> Self {
        Self {
            open_db,
            close_db: close_db.min(open_db - 1.0),
            hold_blocks,
            open: false,
            hold_left: 0,
            gain: 0.0,
            // Fast enough not to clip word onsets, slow enough to avoid clicks.
            attack: 0.35,
            release: 0.08,
        }
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Processes one block, returning whether the gate is open.
    pub fn process(&mut self, buf: &mut [f32], level_db: f32) -> bool {
        if self.open {
            if level_db < self.close_db {
                if self.hold_left == 0 {
                    self.open = false;
                } else {
                    self.hold_left -= 1;
                }
            } else {
                self.hold_left = self.hold_blocks;
            }
        } else if level_db > self.open_db {
            self.open = true;
            self.hold_left = self.hold_blocks;
        }

        let target = if self.open { 1.0 } else { 0.0 };
        let rate = if target > self.gain {
            self.attack
        } else {
            self.release
        };
        // Ramp the gain across the block rather than stepping, to avoid clicks.
        for s in buf.iter_mut() {
            self.gain += (target - self.gain) * rate;
            *s *= self.gain;
        }
        self.open
    }

    pub fn reset(&mut self) {
        self.open = false;
        self.hold_left = 0;
        self.gain = 0.0;
    }
}

/// Slow automatic gain control.
#[derive(Debug, Clone)]
pub struct Agc {
    /// Desired output level, dBFS.
    pub target_db: f32,
    /// Never amplify beyond this, to avoid lifting the noise floor.
    pub max_gain_db: f32,
    pub min_gain_db: f32,
    gain_db: f32,
    /// dB of correction per block.
    step_db: f32,
}

impl Agc {
    pub fn new(target_db: f32, max_gain_db: f32) -> Self {
        Self {
            target_db,
            max_gain_db,
            min_gain_db: -12.0,
            gain_db: 0.0,
            step_db: 0.6,
        }
    }

    pub fn gain_db(&self) -> f32 {
        self.gain_db
    }

    /// Adapts towards the target and applies the gain. `speaking` gates
    /// adaptation so the AGC does not wind up during silence.
    pub fn process(&mut self, buf: &mut [f32], level_db: f32, speaking: bool) {
        if speaking && level_db > -60.0 {
            let error = self.target_db - level_db;
            let step = error.clamp(-self.step_db, self.step_db);
            self.gain_db = (self.gain_db + step).clamp(self.min_gain_db, self.max_gain_db);
        }
        let g = 10f32.powf(self.gain_db / 20.0);
        for s in buf.iter_mut() {
            *s *= g;
        }
    }

    pub fn reset(&mut self) {
        self.gain_db = 0.0;
    }
}

/// Brings every speaker to a similar loudness.
///
/// Riders arrive with wildly different microphones, gains and helmet acoustics,
/// so without this one person is inaudible under the engine while the next is
/// painfully loud. Adapting slowly is the point: fast adaptation would pump
/// audibly and chase the shape of individual words.
#[derive(Debug, Clone)]
pub struct LevelNormalizer {
    gain_db: f32,
    target_db: f32,
    max_gain_db: f32,
    min_gain_db: f32,
    step_db: f32,
    /// Level below which a block is treated as silence and ignored.
    floor_db: f32,
}

impl LevelNormalizer {
    pub fn new(target_db: f32) -> Self {
        Self {
            gain_db: 0.0,
            target_db,
            max_gain_db: 18.0,
            min_gain_db: -18.0,
            step_db: 0.35,
            floor_db: -55.0,
        }
    }

    pub fn gain_db(&self) -> f32 {
        self.gain_db
    }

    pub fn reset(&mut self) {
        self.gain_db = 0.0;
    }

    /// Adapts towards the target and applies the gain in place.
    pub fn process(&mut self, buf: &mut [f32]) {
        let level = to_dbfs(rms(buf));

        // Only adapt on actual speech. Adapting during the gaps would wind the
        // gain up on silence and then blast the next word.
        if level > self.floor_db {
            let error = self.target_db - level;
            let step = error.clamp(-self.step_db, self.step_db);
            self.gain_db = (self.gain_db + step).clamp(self.min_gain_db, self.max_gain_db);
        }

        let g = 10f32.powf(self.gain_db / 20.0);
        for s in buf.iter_mut() {
            *s = (*s * g).clamp(-1.0, 1.0);
        }
    }
}

/// Peak limiter that catches wind-gust transients before they clip.
#[derive(Debug, Clone)]
pub struct Limiter {
    ceiling: f32,
    gain: f32,
    release: f32,
}

impl Limiter {
    pub fn new(ceiling: f32) -> Self {
        Self {
            ceiling,
            gain: 1.0,
            release: 0.0005,
        }
    }

    pub fn process(&mut self, buf: &mut [f32]) {
        for s in buf.iter_mut() {
            let target = *s * self.gain;
            if target.abs() > self.ceiling {
                // Clamp instantly on the way down; recover slowly.
                self.gain *= self.ceiling / target.abs();
            } else {
                self.gain += (1.0 - self.gain) * self.release;
            }
            let out = *s * self.gain;
            // Belt and braces: never emit out of range even mid-adaptation.
            *s = out.clamp(-self.ceiling, self.ceiling);
        }
    }

    pub fn reset(&mut self) {
        self.gain = 1.0;
    }
}

/// Downmixes interleaved multi-channel audio to mono in place, returning the
/// number of mono samples written.
pub fn interleaved_to_mono(input: &[f32], channels: usize, out: &mut Vec<f32>) {
    out.clear();
    if channels <= 1 {
        out.extend_from_slice(input);
        return;
    }
    let scale = 1.0 / channels as f32;
    for frame in input.chunks_exact(channels) {
        out.push(frame.iter().sum::<f32>() * scale);
    }
}

/// A short room reverberation for incoming voices.
///
/// Voice activation and noise gates cut a talker off the instant they stop,
/// which is unnatural: a real room keeps ringing for a moment, and without
/// that tail every utterance ends like a switch being thrown. A little decay
/// smooths the cut and makes several people on one channel sound like they
/// share a space rather than arriving from separate boxes.
///
/// Schroeder's arrangement: parallel comb filters make the dense repeats, and
/// series allpass sections smear them so the repeats stop being individually
/// audible. Cheap enough for the mix thread, which matters because this runs
/// on every frame of everyone's audio.
#[derive(Debug, Clone)]
pub struct Reverb {
    combs: Vec<Comb>,
    allpasses: Vec<Allpass>,
    wet: f32,
}

#[derive(Debug, Clone)]
struct Comb {
    buf: Vec<f32>,
    idx: usize,
    feedback: f32,
    /// One-pole lowpass in the feedback path: a real room absorbs treble
    /// faster than bass, and without it the tail sounds metallic.
    damp: f32,
    last: f32,
}

impl Comb {
    fn new(len: usize, feedback: f32, damp: f32) -> Self {
        Self {
            buf: vec![0.0; len.max(1)],
            idx: 0,
            feedback,
            damp,
            last: 0.0,
        }
    }

    #[inline]
    fn process(&mut self, x: f32) -> f32 {
        let y = self.buf[self.idx];
        self.last = y * (1.0 - self.damp) + self.last * self.damp;
        self.buf[self.idx] = x + self.last * self.feedback;
        self.idx = (self.idx + 1) % self.buf.len();
        y
    }
}

#[derive(Debug, Clone)]
struct Allpass {
    buf: Vec<f32>,
    idx: usize,
    gain: f32,
}

impl Allpass {
    fn new(len: usize, gain: f32) -> Self {
        Self {
            buf: vec![0.0; len.max(1)],
            idx: 0,
            gain,
        }
    }

    #[inline]
    fn process(&mut self, x: f32) -> f32 {
        let buffered = self.buf[self.idx];
        let y = -x + buffered;
        self.buf[self.idx] = x + buffered * self.gain;
        self.idx = (self.idx + 1) % self.buf.len();
        y
    }
}

impl Reverb {
    /// Delays in samples at 48 kHz, from Schroeder's ratios. Mutually prime so
    /// their repeats do not line up into an audible pitch.
    const COMB_DELAYS: [usize; 4] = [1687, 1601, 2053, 2251];
    const ALLPASS_DELAYS: [usize; 2] = [601, 199];

    /// `decay_secs` is the time to fall 60 dB; `wet` is how much of the tail is
    /// mixed in, `0.0..=1.0`.
    pub fn new(decay_secs: f32, wet: f32) -> Self {
        let combs = Self::COMB_DELAYS
            .iter()
            .map(|&len| {
                // Feedback that reaches -60 dB after `decay_secs`, given how
                // often this delay line recirculates in that time.
                let laps = decay_secs * SAMPLE_RATE_HZ / len as f32;
                let feedback = 10f32.powf(-3.0 / laps.max(0.001)).clamp(0.0, 0.98);
                Comb::new(len, feedback, 0.25)
            })
            .collect();
        let allpasses = Self::ALLPASS_DELAYS
            .iter()
            .map(|&len| Allpass::new(len, 0.5))
            .collect();
        Self {
            combs,
            allpasses,
            wet: wet.clamp(0.0, 1.0),
        }
    }

    pub fn reset(&mut self) {
        for c in self.combs.iter_mut() {
            c.buf.fill(0.0);
            c.last = 0.0;
        }
        for a in self.allpasses.iter_mut() {
            a.buf.fill(0.0);
        }
    }

    /// Adds the tail to `buf` in place, leaving the dry signal at full level.
    pub fn process(&mut self, buf: &mut [f32]) {
        if self.wet <= 0.0 {
            return;
        }
        for sample in buf.iter_mut() {
            let dry = *sample;
            let mut tail: f32 = self.combs.iter_mut().map(|c| c.process(dry)).sum();
            tail /= self.combs.len() as f32;
            for a in self.allpasses.iter_mut() {
                tail = a.process(tail);
            }
            // Dry stays at unity: this is a room around the voice, not an
            // effect applied to it, and attenuating speech to make space for
            // its own echo is the wrong trade on a motorcycle.
            *sample = (dry + tail * self.wet).clamp(-1.0, 1.0);
        }
    }
}

/// Sample rate the delay lengths above are chosen for.
const SAMPLE_RATE_HZ: f32 = 48_000.0;

/// In-place radix-2 FFT.
///
/// Written out rather than pulled in: the only transform this crate needs is a
/// power-of-two of one fixed size, and a dependency for that would be more
/// code to audit than the twenty lines it replaces.
///
/// It lives here rather than beside its first caller because it now has two:
/// the spectral de-hisser, which subtracts a learned noise floor, and the
/// diagnostics analyser, which draws what the chain is doing. One transform in
/// the crate is easier to reason about than two that drift apart.
pub(crate) fn fft(re: &mut [f32], im: &mut [f32], inverse: bool) {
    use std::f32::consts::PI;

    let n = re.len();
    debug_assert!(n.is_power_of_two());
    debug_assert_eq!(n, im.len());

    // Bit-reversal permutation.
    let mut j = 0usize;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }

    let sign = if inverse { 1.0 } else { -1.0 };
    let mut len = 2;
    while len <= n {
        let angle = sign * 2.0 * PI / len as f32;
        let (wr, wi) = (angle.cos(), angle.sin());
        for start in (0..n).step_by(len) {
            let (mut cr, mut ci) = (1.0f32, 0.0f32);
            for k in 0..len / 2 {
                let (ar, ai) = (re[start + k], im[start + k]);
                let (br, bi) = (re[start + k + len / 2], im[start + k + len / 2]);
                let (tr, ti) = (br * cr - bi * ci, br * ci + bi * cr);
                re[start + k] = ar + tr;
                im[start + k] = ai + ti;
                re[start + k + len / 2] = ar - tr;
                im[start + k + len / 2] = ai - ti;
                let next = (cr * wr - ci * wi, cr * wi + ci * wr);
                cr = next.0;
                ci = next.1;
            }
        }
        len <<= 1;
    }

    if inverse {
        let scale = 1.0 / n as f32;
        for i in 0..n {
            re[i] *= scale;
            im[i] *= scale;
        }
    }
}

/// Sum of `a[i] * b[i]`, in four independent lanes.
///
/// **The lanes are the point.** A running total written the obvious way is a
/// serial dependency: floating-point addition is not associative, so the
/// compiler is not allowed to reorder it, and every add waits for the one
/// before whatever the machine could do in parallel. The multiplies cannot get
/// ahead of the adds either, so the whole loop runs one element at a time.
///
/// Summing into four accumulators and combining at the end is a *different
/// order of summation*, which is why it cannot be done for us and has to be
/// asked for. On the echo canceller's filter it was worth 4.8x on its own —
/// more than removing a branch from the inner loop was.
///
/// The reordering is immaterial to everything here: these are energies and
/// correlations of audio, where the inputs carry far more uncertainty than
/// float ordering does.
#[inline]
pub(crate) fn dot(a: &[f32], b: &[f32]) -> f32 {
    let mut acc = [0.0f32; 4];
    let mut a4 = a.chunks_exact(4);
    let mut b4 = b.chunks_exact(4);
    for (x, y) in a4.by_ref().zip(b4.by_ref()) {
        acc[0] += x[0] * y[0];
        acc[1] += x[1] * y[1];
        acc[2] += x[2] * y[2];
        acc[3] += x[3] * y[3];
    }
    let mut sum = (acc[0] + acc[1]) + (acc[2] + acc[3]);
    for (x, y) in a4.remainder().iter().zip(b4.remainder()) {
        sum += x * y;
    }
    sum
}

/// Sum of `(a[i] - b[i])^2`, in four lanes. See [`dot`].
#[inline]
pub(crate) fn sq_diff(a: &[f32], b: &[f32]) -> f32 {
    let mut acc = [0.0f32; 4];
    let mut a4 = a.chunks_exact(4);
    let mut b4 = b.chunks_exact(4);
    for (x, y) in a4.by_ref().zip(b4.by_ref()) {
        let e = [x[0] - y[0], x[1] - y[1], x[2] - y[2], x[3] - y[3]];
        acc[0] += e[0] * e[0];
        acc[1] += e[1] * e[1];
        acc[2] += e[2] * e[2];
        acc[3] += e[3] * e[3];
    }
    let mut sum = (acc[0] + acc[1]) + (acc[2] + acc[3]);
    for (x, y) in a4.remainder().iter().zip(b4.remainder()) {
        let e = x - y;
        sum += e * e;
    }
    sum
}

/// Sum of `(x[i] - c)^2` against a constant, in four lanes. See [`dot`].
///
/// Variance about a known mean, which is a reduction like the others and was
/// costing a serial add chain in the same places.
#[inline]
pub(crate) fn sq_diff_const(x: &[f32], c: f32) -> f32 {
    let mut acc = [0.0f32; 4];
    let mut x4 = x.chunks_exact(4);
    for v in x4.by_ref() {
        let e = [v[0] - c, v[1] - c, v[2] - c, v[3] - c];
        acc[0] += e[0] * e[0];
        acc[1] += e[1] * e[1];
        acc[2] += e[2] * e[2];
        acc[3] += e[3] * e[3];
    }
    let mut sum = (acc[0] + acc[1]) + (acc[2] + acc[3]);
    for v in x4.remainder() {
        let e = v - c;
        sum += e * e;
    }
    sum
}

/// Sum of squares, in four lanes. See [`dot`].
#[inline]
pub(crate) fn energy(x: &[f32]) -> f32 {
    let mut acc = [0.0f32; 4];
    let mut x4 = x.chunks_exact(4);
    for c in x4.by_ref() {
        acc[0] += c[0] * c[0];
        acc[1] += c[1] * c[1];
        acc[2] += c[2] * c[2];
        acc[3] += c[3] * c[3];
    }
    let mut sum = (acc[0] + acc[1]) + (acc[2] + acc[3]);
    for v in x4.remainder() {
        sum += v * v;
    }
    sum
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The lanes change the order of summation, so they have to be shown to
    /// give the same answer as the obvious loop, not assumed to.
    #[test]
    fn the_lane_reductions_agree_with_the_obvious_loop() {
        let mut seed = 12345u32;
        let mut next = || {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (seed >> 8) as f32 / 8_388_608.0 - 1.0
        };
        // Deliberately not a multiple of four, so the remainder is exercised.
        let a: Vec<f32> = (0..1023).map(|_| next()).collect();
        let b: Vec<f32> = (0..1023).map(|_| next()).collect();

        let naive_dot: f32 = a.iter().zip(&b).map(|(x, y)| x * y).sum();
        let naive_sq: f32 = a.iter().zip(&b).map(|(x, y)| (x - y) * (x - y)).sum();
        let naive_e: f32 = a.iter().map(|x| x * x).sum();

        assert!(
            (dot(&a, &b) - naive_dot).abs() < 1e-3,
            "{} vs {naive_dot}",
            dot(&a, &b)
        );
        assert!((sq_diff(&a, &b) - naive_sq).abs() < 1e-3);
        assert!((energy(&a) - naive_e).abs() < 1e-3);

        // Shorter than one lane, and empty.
        assert!((dot(&a[..3], &b[..3]) - (a[0] * b[0] + a[1] * b[1] + a[2] * b[2])).abs() < 1e-6);
        assert_eq!(energy(&[]), 0.0);
    }

    #[test]
    fn a_bell_lifts_its_own_centre_and_leaves_the_ends_alone() {
        // Measured by running a sine through it, which is the definition
        // rather than a restatement of the algebra above — the failure this
        // catches is `10^(g/20)` for `A`, and that produces coefficients that
        // are self-consistent and twice as strong as asked for.
        let sr = 48_000.0;
        let gain_at = |hz: f32| {
            let mut f = Biquad::peaking(sr, 1_000.0, 1.0, 9.0);
            let n = 4_800;
            let mut peak_in = 0.0f32;
            let mut peak_out = 0.0f32;
            for i in 0..n {
                let t = i as f32 / sr;
                let x = (2.0 * std::f32::consts::PI * hz * t).sin();
                let y = f.process(x);
                // The second half only, so the filter has settled.
                if i > n / 2 {
                    peak_in = peak_in.max(x.abs());
                    peak_out = peak_out.max(y.abs());
                }
            }
            20.0 * (peak_out / peak_in).log10()
        };

        assert!(
            (gain_at(1_000.0) - 9.0).abs() < 0.5,
            "asked for 9 dB at the centre, got {}",
            gain_at(1_000.0)
        );
        assert!(gain_at(60.0).abs() < 1.0, "a bell must not move the bottom");
        assert!(
            gain_at(12_000.0).abs() < 1.0,
            "a bell must not move the top"
        );
    }

    #[test]
    fn fft_round_trips() {
        // Forward then inverse must return what went in. Everything built on
        // the transform — the de-hisser's noise subtraction, the diagnostics
        // analyser's bands — is wrong in a way that is hard to see if this is
        // wrong, so it is checked directly.
        let mut re = tone(1000.0, 512, 0.5);
        let original = re.clone();
        let mut im = vec![0.0; 512];
        fft(&mut re, &mut im, false);
        fft(&mut re, &mut im, true);
        for (a, b) in original.iter().zip(re.iter()) {
            assert!((a - b).abs() < 1e-3, "{a} vs {b}");
        }
    }

    #[test]
    fn reverb_leaves_the_dry_voice_at_full_level() {
        // A room around the voice, not an effect applied to it: attenuating
        // speech to make space for its own echo is the wrong trade here.
        let mut r = Reverb::new(0.3, 0.2);
        let mut buf = vec![0.0f32; 64];
        buf[0] = 1.0;
        r.process(&mut buf);
        assert!(
            buf[0] >= 1.0 - 1e-6,
            "first sample dropped to {}, the dry signal was attenuated",
            buf[0]
        );
    }

    #[test]
    fn reverb_tail_decays_to_nothing() {
        // The whole point is a tail that fades. One that sustains would turn
        // an open channel into a howl.
        let mut r = Reverb::new(0.3, 0.3);
        let mut buf = vec![0.0f32; 512];
        buf[0] = 1.0;
        r.process(&mut buf);

        let mut silence = vec![0.0f32; 48_000]; // one second
        r.process(&mut silence);

        let early = peak(&silence[..4_800]);
        let late = peak(&silence[43_200..]);
        assert!(early > 0.0, "no tail at all");
        assert!(
            late < early * 0.1,
            "tail only fell from {early} to {late} in a second"
        );
    }

    #[test]
    fn reverb_stays_finite_and_bounded_under_sustained_input() {
        // Feedback loops are where instability hides, and an audio path that
        // produces NaN takes the output stream down with it.
        let mut r = Reverb::new(0.5, 0.35);
        for _ in 0..200 {
            let mut buf: Vec<f32> = (0..480).map(|i| ((i as f32) * 0.3).sin() * 0.9).collect();
            r.process(&mut buf);
            assert!(buf.iter().all(|s| s.is_finite() && s.abs() <= 1.0));
        }
    }

    #[test]
    fn reverb_is_inert_when_fully_dry() {
        let mut r = Reverb::new(0.3, 0.0);
        let input: Vec<f32> = (0..256).map(|i| (i as f32 * 0.1).sin()).collect();
        let mut buf = input.clone();
        r.process(&mut buf);
        assert_eq!(buf, input, "a dry reverb should change nothing");
    }

    #[test]
    fn reverb_reset_clears_the_tail() {
        // Switching it off mid-sentence must not leave the room ringing when
        // it is switched back on.
        let mut r = Reverb::new(0.3, 0.3);
        let mut buf = vec![0.0f32; 256];
        buf[0] = 1.0;
        r.process(&mut buf);
        r.reset();

        let mut silence = vec![0.0f32; 4_800];
        r.process(&mut silence);
        assert_eq!(peak(&silence), 0.0, "tail survived a reset");
    }

    const SR: f32 = 48_000.0;

    fn tone(freq: f32, len: usize, amp: f32) -> Vec<f32> {
        (0..len)
            .map(|i| (2.0 * std::f32::consts::PI * freq * i as f32 / SR).sin() * amp)
            .collect()
    }

    /// Measures steady-state gain, skipping the filter's settling transient.
    fn steady_gain(filter: &mut RumbleFilter, freq: f32) -> f32 {
        let mut buf = tone(freq, 12_000, 0.5);
        filter.process(&mut buf);
        rms(&buf[6_000..]) / 0.3536 // RMS of a 0.5-amplitude sine
    }

    #[test]
    fn rumble_filter_kills_wind_and_keeps_speech() {
        let mut f = RumbleFilter::new(SR, 150.0);
        let g50 = steady_gain(&mut f, 50.0);
        f.reset();
        let g1000 = steady_gain(&mut f, 1000.0);

        // 50 Hz engine/wind rumble must be heavily attenuated...
        assert!(
            g50 < 0.05,
            "50 Hz gain was {g50}, expected strong rejection"
        );
        // ...while speech-band content passes essentially untouched.
        assert!(
            g1000 > 0.9,
            "1 kHz gain was {g1000}, speech must pass through"
        );
    }

    #[test]
    fn rumble_filter_rolls_off_steeply() {
        // A 4th-order section should fall much faster than a single pole.
        let mut f = RumbleFilter::new(SR, 150.0);
        let g40 = steady_gain(&mut f, 40.0);
        f.reset();
        let g80 = steady_gain(&mut f, 80.0);
        assert!(g40 < g80, "attenuation must increase as frequency drops");
        assert!(g40 < 0.02, "40 Hz gain {g40} not rejected hard enough");
    }

    #[test]
    fn gate_opens_on_speech_and_closes_on_noise_floor() {
        let mut g = NoiseGate::new(-40.0, -50.0, 2);

        // Quiet noise floor: stays shut.
        let mut quiet = tone(300.0, 480, 0.001);
        let quiet_db = to_dbfs(rms(&quiet));
        g.process(&mut quiet, quiet_db);
        assert!(!g.is_open(), "gate must stay shut on the noise floor");

        // Speech-level signal: opens.
        for _ in 0..5 {
            let mut loud = tone(300.0, 480, 0.3);
            let loud_db = to_dbfs(rms(&loud));
            g.process(&mut loud, loud_db);
        }
        assert!(g.is_open(), "gate must open on speech");
    }

    #[test]
    fn gate_hysteresis_prevents_chatter() {
        // A level sitting between the two thresholds must not toggle the gate.
        let mut g = NoiseGate::new(-40.0, -50.0, 0);
        for _ in 0..5 {
            let mut loud = tone(300.0, 480, 0.3);
            let loud_db = to_dbfs(rms(&loud));
            g.process(&mut loud, loud_db);
        }
        assert!(g.is_open());

        // -45 dBFS is below the open threshold but above the close threshold.
        for _ in 0..10 {
            let mut mid = vec![0.0f32; 480];
            g.process(&mut mid, -45.0);
            assert!(g.is_open(), "gate closed inside the hysteresis band");
        }

        // Drop clearly below the close threshold and it should shut.
        for _ in 0..10 {
            let mut low = vec![0.0f32; 480];
            g.process(&mut low, -70.0);
        }
        assert!(!g.is_open(), "gate must close well below the threshold");
    }

    #[test]
    fn gate_attenuates_audio_when_closed() {
        let mut g = NoiseGate::new(-40.0, -50.0, 0);
        // Drive it closed.
        for _ in 0..50 {
            let mut b = vec![0.0f32; 480];
            g.process(&mut b, -90.0);
        }
        let mut noise = tone(300.0, 480, 0.2);
        let before = rms(&noise);
        g.process(&mut noise, -90.0);
        assert!(
            rms(&noise) < before * 0.05,
            "closed gate must actually mute the signal"
        );
    }

    #[test]
    fn agc_lifts_quiet_speech_and_tames_loud_speech() {
        let mut agc = Agc::new(-18.0, 24.0);
        // Quiet talker: gain should climb.
        for _ in 0..200 {
            let mut b = tone(300.0, 480, 0.01);
            let lvl = to_dbfs(rms(&b));
            agc.process(&mut b, lvl, true);
        }
        assert!(agc.gain_db() > 6.0, "AGC should boost a quiet talker");

        // Loud talker: gain should fall back.
        let mut agc = Agc::new(-18.0, 24.0);
        for _ in 0..200 {
            let mut b = tone(300.0, 480, 0.9);
            let lvl = to_dbfs(rms(&b));
            agc.process(&mut b, lvl, true);
        }
        assert!(agc.gain_db() < 0.0, "AGC should attenuate a loud talker");
    }

    #[test]
    fn agc_does_not_wind_up_during_silence() {
        // Without this, the AGC would amplify the noise floor between sentences
        // and the far end would hear the engine surge up during pauses.
        let mut agc = Agc::new(-18.0, 24.0);
        for _ in 0..500 {
            let mut b = vec![0.0f32; 480];
            agc.process(&mut b, -80.0, false);
        }
        assert_eq!(agc.gain_db(), 0.0, "AGC must not adapt while not speaking");
    }

    #[test]
    fn agc_respects_its_ceiling() {
        let mut agc = Agc::new(-18.0, 12.0);
        for _ in 0..1000 {
            let mut b = tone(300.0, 480, 0.0005);
            let lvl = to_dbfs(rms(&b));
            agc.process(&mut b, lvl, true);
        }
        assert!(agc.gain_db() <= 12.0 + 1e-3, "AGC exceeded its max gain");
    }

    #[test]
    fn normaliser_brings_quiet_and_loud_speakers_together() {
        // The whole point: two people arriving at very different levels should
        // end up close to each other after adaptation.
        let mut quiet = LevelNormalizer::new(-20.0);
        let mut loud = LevelNormalizer::new(-20.0);

        let mut quiet_out = 0.0;
        let mut loud_out = 0.0;
        for _ in 0..400 {
            let mut a = tone(300.0, 480, 0.01); // very quiet talker
            let mut b = tone(300.0, 480, 0.6); // very loud talker
            quiet.process(&mut a);
            loud.process(&mut b);
            quiet_out = to_dbfs(rms(&a));
            loud_out = to_dbfs(rms(&b));
        }

        assert!(
            (quiet_out - loud_out).abs() < 6.0,
            "levels still {:.1} dB apart (quiet {quiet_out:.1}, loud {loud_out:.1})",
            (quiet_out - loud_out).abs()
        );
        assert!(quiet.gain_db() > 0.0, "quiet speaker should be boosted");
        assert!(loud.gain_db() < 0.0, "loud speaker should be attenuated");
    }

    #[test]
    fn normaliser_ignores_silence() {
        // Adapting during gaps would wind the gain up and blast the next word.
        let mut n = LevelNormalizer::new(-20.0);
        for _ in 0..500 {
            let mut silence = vec![0.0f32; 480];
            n.process(&mut silence);
        }
        assert_eq!(n.gain_db(), 0.0);
    }

    #[test]
    fn normaliser_output_never_clips() {
        let mut n = LevelNormalizer::new(-6.0);
        for _ in 0..600 {
            let mut b = tone(300.0, 480, 0.95);
            n.process(&mut b);
            assert!(peak(&b) <= 1.0, "normaliser clipped at {}", peak(&b));
        }
    }

    #[test]
    fn limiter_prevents_clipping_on_gusts() {
        let mut lim = Limiter::new(0.98);
        // A sudden 4x-over-full-scale transient, like wind hitting the capsule.
        let mut buf = vec![0.1f32; 200];
        buf.extend(vec![4.0f32; 200]);
        buf.extend(vec![0.1f32; 200]);
        lim.process(&mut buf);

        assert!(
            peak(&buf) <= 0.98 + 1e-4,
            "limiter let {} through",
            peak(&buf)
        );
    }

    #[test]
    fn limiter_leaves_normal_audio_alone() {
        let mut lim = Limiter::new(0.98);
        let original = tone(440.0, 4800, 0.3);
        let mut buf = original.clone();
        lim.process(&mut buf);
        let diff: f32 = buf
            .iter()
            .zip(&original)
            .map(|(a, b)| (a - b).abs())
            .fold(0.0, f32::max);
        assert!(diff < 0.01, "limiter distorted in-range audio by {diff}");
    }

    #[test]
    fn downmix_averages_channels() {
        let stereo = vec![1.0, 0.0, 0.5, 0.5, -1.0, 1.0];
        let mut mono = Vec::new();
        interleaved_to_mono(&stereo, 2, &mut mono);
        assert_eq!(mono, vec![0.5, 0.5, 0.0]);

        // Mono input passes through untouched.
        let mut out = Vec::new();
        interleaved_to_mono(&[0.1, 0.2], 1, &mut out);
        assert_eq!(out, vec![0.1, 0.2]);
    }

    #[test]
    fn noise_floor_converges_on_a_steady_background() {
        let mut t = NoiseFloorTracker::new(50);
        for _ in 0..400 {
            t.update(-35.0);
        }
        // The floor should sit at the background level (plus the small bias).
        assert!(
            (t.floor_db() - -35.0).abs() < 4.0,
            "floor was {}, expected about -35",
            t.floor_db()
        );
        // And steady noise therefore has no meaningful SNR.
        assert!(
            t.snr_db(-35.0) < 4.0,
            "steady noise must not look like speech"
        );
    }

    #[test]
    fn noise_floor_ignores_speech_bursts() {
        let mut t = NoiseFloorTracker::new(50);
        // Background at -50, with a loud burst every second.
        for i in 0..600 {
            let level = if i % 100 < 25 { -15.0 } else { -50.0 };
            t.update(level);
        }
        assert!(
            t.floor_db() < -40.0,
            "speech dragged the floor up to {}",
            t.floor_db()
        );
        // A burst therefore stands well clear of the floor.
        assert!(t.snr_db(-15.0) > 25.0);
    }

    #[test]
    fn noise_floor_follows_a_rising_background() {
        // Accelerating: the background climbs from -60 to -30 dBFS.
        let mut t = NoiseFloorTracker::new(50);
        for _ in 0..400 {
            t.update(-60.0);
        }
        assert!(t.floor_db() < -50.0);

        // It still gets there, and this is the assertion that matters: a floor
        // that could not follow the road would let a helmet at speed key the
        // transmitter continuously, which is the fault the estimator exists to
        // prevent.
        //
        // **It is allowed to take its time.** 400 blocks was enough before the
        // rise limit and is not now: the sub-windows have to flush (300 blocks
        // here) and then 30 dB at 6 dB/s is 500 more. Eight seconds is the
        // documented cost of not cutting a held note in half.
        for _ in 0..1000 {
            t.update(-30.0);
        }
        assert!(
            t.floor_db() > -36.0,
            "floor failed to follow the louder background: {}",
            t.floor_db()
        );
    }

    /// The rise limit is real, and this is what stops the floor climbing onto a
    /// voice. Half a second of anything cannot move the floor more than 3 dB.
    #[test]
    fn the_floor_cannot_leap_upward() {
        let mut t = NoiseFloorTracker::new(25);
        for _ in 0..600 {
            t.update(-90.0);
        }
        let quiet = t.floor_db();

        // The step the sung recording actually produced: silence, then a held
        // note 70 dB louder.
        for _ in 0..50 {
            t.update(-20.0);
        }
        let after_half_a_second = t.floor_db();
        assert!(
            after_half_a_second - quiet <= 3.0 + 1e-3,
            "floor rose {:.1} dB in half a second; the limit is 6 dB/s",
            after_half_a_second - quiet
        );
    }

    /// The regression from the 2026-08-14 recordings, in the shape that caused
    /// it: a phrase held far longer than the 1.5 s memory.
    ///
    /// Before the rise limit the minimum statistic *became* the voice — every
    /// sub-window was full of it — and the floor ended up level with, or above,
    /// the thing it was supposed to be measuring underneath.
    #[test]
    fn a_long_held_note_does_not_drag_the_floor_onto_the_voice() {
        // The gate's own tracker: six 0.25 s sub-windows, a 1.5 s memory.
        let mut t = NoiseFloorTracker::new(25);
        for _ in 0..300 {
            t.update(-90.0);
        }

        // Six seconds of unbroken voice at -20 dBFS, which is roughly what the
        // sung clip did and four times the tracker's memory.
        let mut worst_snr = f32::INFINITY;
        for _ in 0..600 {
            let floor = t.update(-20.0);
            worst_snr = worst_snr.min(-20.0 - floor);
        }

        // The gate needs a clear margin. Before the limit this went negative --
        // the floor passed *above* the voice -- and the note was cut.
        assert!(
            worst_snr > 6.0,
            "the floor climbed onto a held note: worst SNR {worst_snr:.1} dB"
        );
    }

    /// The rule the recordings asked for: while something is speaking, the
    /// floor does not climb at all. Not slower — not at all.
    #[test]
    fn a_voice_stops_the_floor_climbing_outright() {
        let mut t = NoiseFloorTracker::new(25);
        for _ in 0..300 {
            t.update_gated(-90.0, false);
        }
        let quiet = t.floor_db();

        // Thirty seconds of unbroken voice, twenty times the tracker's memory.
        for _ in 0..3000 {
            t.update_gated(-20.0, true);
        }
        assert!(
            (t.floor_db() - quiet).abs() < 1e-3,
            "floor moved {:.2} dB while a voice was present",
            t.floor_db() - quiet
        );
        // And the voice therefore still stands clear of it.
        assert!(t.snr_db(-20.0) > 60.0);
    }

    /// The freeze is not a latch. Between phrases the floor finds the room
    /// again, or a rider who moves from a quiet street to a motorway would keep
    /// a floor measured on the street.
    #[test]
    fn the_floor_climbs_again_once_the_voice_stops() {
        let mut t = NoiseFloorTracker::new(25);
        for _ in 0..300 {
            t.update_gated(-90.0, false);
        }
        for _ in 0..600 {
            t.update_gated(-20.0, true);
        }
        let held = t.floor_db();

        // The voice stops and the background is genuinely louder than it was.
        for _ in 0..2000 {
            t.update_gated(-40.0, false);
        }
        assert!(
            t.floor_db() > held + 10.0,
            "floor stayed frozen after the voice stopped: {} vs {held}",
            t.floor_db()
        );
    }

    /// Rule for a late verdict: the classifier can be most of three seconds
    /// behind, so by the time it says "speech" the floor has already climbed
    /// onto it. The rising edge rolls back to before that happened.
    #[test]
    fn a_late_verdict_rolls_the_floor_back_to_before_the_speech() {
        let mut t = NoiseFloorTracker::new(25);
        for _ in 0..600 {
            t.update_gated(-90.0, false);
        }
        let before = t.floor_db();

        // Two and a half seconds of speech that nothing has recognised yet:
        // the floor climbs, exactly as it did in the recordings.
        for _ in 0..250 {
            t.update_gated(-20.0, false);
        }
        let climbed = t.floor_db();
        assert!(
            climbed > before + 5.0,
            "the setup did not reproduce the climb: {before} -> {climbed}"
        );

        // Now the verdict lands.
        t.update_gated(-20.0, true);
        assert!(
            t.floor_db() < climbed - 5.0,
            "a late verdict left the climb in place: {} vs {climbed}",
            t.floor_db()
        );
        assert!(
            (t.floor_db() - before).abs() < 1.0,
            "expected roughly the pre-speech floor {before}, got {}",
            t.floor_db()
        );
    }

    /// Only the edge rolls back. A continuing phrase must not keep dragging the
    /// floor down towards silence, which would make the gate open on anything.
    #[test]
    fn only_the_start_of_a_phrase_rolls_back() {
        let mut t = NoiseFloorTracker::new(25);
        for _ in 0..600 {
            t.update_gated(-50.0, false);
        }
        t.update_gated(-20.0, true);
        let at_onset = t.floor_db();
        for _ in 0..1000 {
            t.update_gated(-20.0, true);
        }
        assert!(
            (t.floor_db() - at_onset).abs() < 1e-3,
            "the floor kept moving during a held phrase: {at_onset} -> {}",
            t.floor_db()
        );
    }

    /// `update` is `update_gated` with no voice signal, which is what a caller
    /// that has none should get.
    #[test]
    fn the_plain_update_is_the_ungated_one() {
        let mut a = NoiseFloorTracker::new(25);
        let mut b = NoiseFloorTracker::new(25);
        for i in 0..500 {
            let lvl = -60.0 + (i % 7) as f32;
            assert_eq!(a.update(lvl), b.update_gated(lvl, false));
        }
    }

    /// Down is not rate-limited: a room that goes quiet is a fact, and lagging
    /// on the way down would hold the gate shut on a voice already clear of it.
    #[test]
    fn the_floor_still_drops_immediately() {
        let mut t = NoiseFloorTracker::new(25);
        for _ in 0..600 {
            t.update(-30.0);
        }
        assert!(t.floor_db() > -34.0);

        // One sub-window of silence is enough to bring it down.
        for _ in 0..25 {
            t.update(-95.0);
        }
        assert!(
            t.floor_db() < -85.0,
            "floor should fall at once, was {}",
            t.floor_db()
        );
    }

    #[test]
    fn noise_floor_starts_permissive_rather_than_muting_everything() {
        // Before it has seen anything, the floor must not be so high that genuine
        // speech is suppressed during the first moments after connecting.
        let t = NoiseFloorTracker::new(50);
        assert!(
            t.snr_db(-30.0) > 10.0,
            "a fresh tracker must let speech through"
        );
    }

    #[test]
    fn level_helpers_behave() {
        assert!((to_dbfs(1.0) - 0.0).abs() < 1e-4);
        assert!((to_dbfs(0.5) + 6.02).abs() < 0.05);
        assert_eq!(to_dbfs(0.0), -120.0);
        assert!((rms(&[1.0, -1.0, 1.0, -1.0]) - 1.0).abs() < 1e-6);
        assert_eq!(peak(&[0.1, -0.7, 0.3]), 0.7);
        assert_eq!(rms(&[]), 0.0);
    }
}
