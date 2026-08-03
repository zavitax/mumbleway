//! Guards against the microphone hearing the speaker.
//!
//! The echo canceller already subtracts what it can model. This handles what
//! is left, and the three ways of doing that are genuinely different problems
//! rather than three strengths of one knob:
//!
//! * **Ducking** turns the microphone down while the far end is loud. It is
//!   what intercoms have always done, it costs nothing, and it works exactly
//!   where a canceller struggles — a speaker inches from the microphone inside
//!   a helmet, with a path that changes as the rider's head moves. The price
//!   is that talking over somebody becomes harder, which is the definition of
//!   half duplex and is sometimes the right trade.
//!
//! * **Howl suppression** does nothing at all until a loop starts to run away,
//!   then cuts hard. Feedback of that kind is a tone climbing in level, which
//!   is a shape you can recognise without touching anything else. It leaves
//!   ordinary conversation completely alone, and does nothing about mild bleed.
//!
//! * **Residual suppression** attenuates in proportion to how much of what is
//!   left looks like the far end rather than the near end. It is the
//!   non-linear stage a stronger canceller would have, and it is the gentlest
//!   of the three on a real conversation while being the least effective
//!   against a genuine howl.
//!
//! All three see the same two things: the microphone block after cancellation,
//! and the far-end reference for the same block.

/// Which guard is in use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeedbackMode {
    Off,
    /// Turn the microphone down while the far end is loud.
    Duck,
    /// Cut only when a howl is building.
    HowlGuard,
    /// Attenuate whatever survived cancellation, in proportion.
    Residual,
}

/// How quickly gain falls when it needs to and recovers when it does not.
///
/// Falling fast and recovering slowly is the standard asymmetry: a howl has to
/// be caught before it is audible, while a gain that jumps back up the moment
/// the far end pauses breathes audibly and can restart the loop it just broke.
const ATTACK: f32 = 0.4;
const RELEASE: f32 = 0.02;

/// Below this the far end counts as silent and nothing ducks.
const FAR_END_FLOOR: f32 = 0.003;

/// How tonal a block has to look before it counts as a howl.
///
/// Speech is never this periodic for this long: a vowel comes close for a
/// moment, but it moves. A howl is one note that does not.
const HOWL_CORRELATION: f32 = 0.86;

/// Consecutive blocks of that before it is believed.
const HOWL_RUN: u32 = 3;

/// Blocks to stay clamped once it is. Roughly a third of a second at 20 ms.
const HOWL_HOLD: u32 = 16;

pub struct FeedbackGuard {
    mode: FeedbackMode,
    gain: f32,
    howl_run: u32,
    hold: u32,
}

impl Default for FeedbackGuard {
    fn default() -> Self {
        Self::new(FeedbackMode::Off)
    }
}

impl FeedbackGuard {
    pub fn new(mode: FeedbackMode) -> Self {
        Self {
            mode,
            gain: 1.0,
            howl_run: 0,
            hold: 0,
        }
    }

    pub fn mode(&self) -> FeedbackMode {
        self.mode
    }

    pub fn set_mode(&mut self, mode: FeedbackMode) {
        if mode == self.mode {
            return;
        }
        self.mode = mode;
        // A guard that has just been switched off must not leave the
        // microphone turned down behind it.
        self.gain = 1.0;
        self.howl_run = 0;
        self.hold = 0;
    }

    /// The gain currently being applied, for meters and tests.
    pub fn gain(&self) -> f32 {
        self.gain
    }

    /// Applies the guard in place.
    ///
    /// `reference` is what was played out for this block; an empty or silent
    /// one means there is nothing that could be feeding back and every mode
    /// leaves the microphone alone.
    pub fn process(&mut self, mic: &mut [f32], reference: &[f32]) {
        let target = match self.mode {
            FeedbackMode::Off => 1.0,
            FeedbackMode::Duck => self.duck_target(reference),
            FeedbackMode::HowlGuard => self.howl_target(mic),
            FeedbackMode::Residual => self.residual_target(mic, reference),
        };

        // Smoothed rather than applied outright: a gain that steps produces a
        // click, and a click is worse than the bleed it was hiding.
        let rate = if target < self.gain { ATTACK } else { RELEASE };
        self.gain += (target - self.gain) * rate;
        self.gain = self.gain.clamp(0.0, 1.0);

        if self.gain > 0.999 {
            return;
        }
        for s in mic.iter_mut() {
            *s *= self.gain;
        }
    }

    fn duck_target(&self, reference: &[f32]) -> f32 {
        let far = rms(reference);
        if far <= FAR_END_FLOOR {
            return 1.0;
        }
        // Down to a fifth at the loudest, and proportionally between. Never to
        // silence: a rider shouting a warning has to get through something,
        // and a channel that closes completely is worse than one that is
        // merely quiet.
        let depth = ((far - FAR_END_FLOOR) * 12.0).clamp(0.0, 1.0);
        1.0 - depth * 0.8
    }

    fn howl_target(&mut self, mic: &[f32]) -> f32 {
        if self.hold > 0 {
            self.hold -= 1;
            return 0.1;
        }
        if tonality(mic) >= HOWL_CORRELATION && rms(mic) > FAR_END_FLOOR {
            self.howl_run += 1;
            if self.howl_run >= HOWL_RUN {
                self.howl_run = 0;
                self.hold = HOWL_HOLD;
                return 0.1;
            }
        } else {
            self.howl_run = 0;
        }
        1.0
    }

    fn residual_target(&self, mic: &[f32], reference: &[f32]) -> f32 {
        let far = rms(reference);
        let near = rms(mic);
        if far <= FAR_END_FLOOR || near <= f32::EPSILON {
            return 1.0;
        }
        // How much louder the far end is than what is left on the microphone.
        // A near end well above the far end is somebody talking and is left
        // alone; one well below it is residue and is pushed down.
        let ratio = near / far;
        if ratio >= 1.0 {
            1.0
        } else {
            // Never below a quarter, so a quiet talker under a loud far end is
            // attenuated rather than erased.
            (0.25 + 0.75 * ratio).clamp(0.25, 1.0)
        }
    }
}

fn rms(x: &[f32]) -> f32 {
    if x.is_empty() {
        return 0.0;
    }
    (x.iter().map(|s| s * s).sum::<f32>() / x.len() as f32).sqrt()
}

/// How nearly the block repeats itself, at its most repetitive lag.
///
/// A normalised autocorrelation over the lags a howl lives in — roughly 120 Hz
/// to 1200 Hz, which is where a helmet loop settles. One means a pure tone,
/// zero means noise. Cheaper than a transform and quite enough to tell a note
/// that is not moving from speech, which never holds still this long.
fn tonality(x: &[f32]) -> f32 {
    const MIN_LAG: usize = 40;
    const MAX_LAG: usize = 400;
    if x.len() <= MAX_LAG + 1 {
        return 0.0;
    }
    let energy: f32 = x.iter().map(|s| s * s).sum();
    if energy <= f32::EPSILON {
        return 0.0;
    }

    let mut best = 0.0f32;
    for lag in MIN_LAG..=MAX_LAG {
        let mut sum = 0.0;
        for i in lag..x.len() {
            sum += x[i] * x[i - lag];
        }
        // Normalised by the energy of the overlapping stretch, so a long lag
        // is not penalised for comparing fewer samples.
        let tail: f32 = x[lag..].iter().map(|s| s * s).sum();
        let head: f32 = x[..x.len() - lag].iter().map(|s| s * s).sum();
        let norm = (tail * head).sqrt();
        if norm > f32::EPSILON {
            best = best.max(sum / norm);
        }
    }
    best.clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(hz: f32, len: usize, amp: f32) -> Vec<f32> {
        (0..len)
            .map(|i| amp * (std::f32::consts::TAU * hz * i as f32 / 48_000.0).sin())
            .collect()
    }

    fn noise(len: usize, amp: f32) -> Vec<f32> {
        // Deterministic, and nothing like periodic.
        let mut state = 0x1234_5678u32;
        (0..len)
            .map(|_| {
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                ((state >> 8) as f32 / 8_388_608.0 - 1.0) * amp
            })
            .collect()
    }

    fn settle(guard: &mut FeedbackGuard, mic: &[f32], reference: &[f32], blocks: usize) -> f32 {
        let mut buf = vec![0.0; mic.len()];
        for _ in 0..blocks {
            buf.copy_from_slice(mic);
            guard.process(&mut buf, reference);
        }
        guard.gain()
    }

    #[test]
    fn off_leaves_the_microphone_exactly_as_it_was() {
        let mut guard = FeedbackGuard::new(FeedbackMode::Off);
        let mic = tone(300.0, 960, 0.5);
        let mut buf = mic.clone();
        guard.process(&mut buf, &tone(400.0, 960, 0.9));
        assert_eq!(buf, mic);
    }

    #[test]
    fn ducking_follows_the_far_end_and_not_the_near_one() {
        let mut guard = FeedbackGuard::new(FeedbackMode::Duck);
        let mic = tone(300.0, 960, 0.5);

        let loud = settle(&mut guard, &mic, &tone(400.0, 960, 0.8), 60);
        assert!(loud < 0.35, "a loud far end should duck hard, got {loud}");

        guard.set_mode(FeedbackMode::Off);
        guard.set_mode(FeedbackMode::Duck);
        let quiet = settle(&mut guard, &mic, &vec![0.0; 960], 60);
        assert!(quiet > 0.99, "silence should not duck at all, got {quiet}");
    }

    #[test]
    fn ducking_never_closes_the_channel_completely() {
        // A rider shouting a warning has to get through something.
        let mut guard = FeedbackGuard::new(FeedbackMode::Duck);
        let g = settle(&mut guard, &tone(300.0, 960, 0.5), &tone(400.0, 960, 1.0), 200);
        assert!(g > 0.15, "ducked to {g}, which is effectively muted");
    }

    #[test]
    fn a_howl_is_caught_and_speech_is_not() {
        let mut guard = FeedbackGuard::new(FeedbackMode::HowlGuard);
        let howling = settle(&mut guard, &tone(900.0, 960, 0.6), &[], 12);
        assert!(howling < 0.5, "a sustained tone should be cut, got {howling}");

        let mut guard = FeedbackGuard::new(FeedbackMode::HowlGuard);
        let speech = settle(&mut guard, &noise(960, 0.4), &[], 12);
        assert!(
            speech > 0.95,
            "anything that is not a held note must pass, got {speech}"
        );
    }

    #[test]
    fn residual_spares_the_near_end_and_presses_down_the_rest() {
        // Somebody talking over a quiet far end: left alone.
        let mut guard = FeedbackGuard::new(FeedbackMode::Residual);
        let talking = settle(&mut guard, &noise(960, 0.5), &tone(400.0, 960, 0.05), 60);
        assert!(talking > 0.9, "a near-end talker was attenuated to {talking}");

        // Residue under a loud far end: pushed down.
        let mut guard = FeedbackGuard::new(FeedbackMode::Residual);
        let residue = settle(&mut guard, &noise(960, 0.02), &tone(400.0, 960, 0.8), 60);
        assert!(residue < 0.5, "residue was left at {residue}");
    }

    #[test]
    fn switching_off_hands_the_gain_straight_back() {
        // Otherwise turning the guard off leaves the microphone quiet, and the
        // setting looks as though it has broken something.
        let mut guard = FeedbackGuard::new(FeedbackMode::Duck);
        settle(&mut guard, &tone(300.0, 960, 0.5), &tone(400.0, 960, 0.9), 60);
        assert!(guard.gain() < 0.5);
        guard.set_mode(FeedbackMode::Off);
        assert_eq!(guard.gain(), 1.0);
    }
}
