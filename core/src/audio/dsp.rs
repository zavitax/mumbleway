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

/// Root-mean-square of a block.
pub fn rms(buf: &[f32]) -> f32 {
    if buf.is_empty() {
        return 0.0;
    }
    let sum: f32 = buf.iter().map(|s| s * s).sum();
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
}

impl NoiseFloorTracker {
    /// More, shorter sub-windows mean a stale minimum expires sooner. A single
    /// anomalously quiet block would otherwise pin the floor for the whole
    /// window, which is exactly what a codec warm-up transient produces.
    const SUB_WINDOWS: usize = 6;

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
        }
    }

    /// Feeds one block level and returns the current floor estimate in dBFS.
    pub fn update(&mut self, level_db: f32) -> f32 {
        self.sub_min = self.sub_min.min(level_db);
        self.count += 1;
        if self.count >= self.sub_len {
            self.mins[self.idx] = self.sub_min;
            self.idx = (self.idx + 1) % Self::SUB_WINDOWS;
            self.sub_min = f32::INFINITY;
            self.count = 0;
        }
        self.floor_db()
    }

    pub fn floor_db(&self) -> f32 {
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

#[cfg(test)]
mod tests {
    use super::*;

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

        for _ in 0..400 {
            t.update(-30.0);
        }
        assert!(
            t.floor_db() > -36.0,
            "floor failed to follow the louder background: {}",
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
