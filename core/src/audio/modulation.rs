//! Does this sound like somebody talking, in the shape of its loudness?
//!
//! Two discriminators have now been tried on the transmit decision and both
//! judge a block on what is *in* it: level, and periodicity. Level cannot
//! separate a rider from the wind they are talking through, because the two
//! overlap. Periodicity looked decisive on synthetic signals and carried
//! almost no information on real helmet audio — see [`super::pitch`].
//!
//! This is a third axis, and it is different in kind from both: it does not
//! look at a block at all. It looks at how the loudness of the last second has
//! been *moving*.
//!
//! Speech is syllables. A talker produces three to eight of them a second and
//! the loudness rises and falls with them, which puts a strong peak in the
//! spectrum of the envelope somewhere around 4 Hz. It is one of the most
//! stable things about speech across languages, speakers and accents, and it
//! survives being buried in noise far better than the fine structure does,
//! because it is a property of a whole second rather than of any moment in it.
//!
//! Wind does not do that. It gusts, but on a scale of seconds, which puts its
//! envelope energy below 1 Hz. An engine at constant throttle barely modulates
//! at all. Road and tyre noise varies with the surface, again slowly.
//!
//! # It costs nothing extra
//!
//! The envelope is the sequence of per-block levels the chain already computes
//! for its own meter and its own gate. Nothing here touches the audio: it is a
//! 128-point transform of 128 numbers, once per block, on a signal sampled at
//! the block rate. Against the FFTs the analyser already runs it does not
//! register.
//!
//! # What it cannot do
//!
//! It cannot tell speech from music, and it never will: music is modulated at
//! very much the same rate, which is part of why people can sing along to it.
//! Music is the known gap in `core/tests/suppression.rs` and this does not
//! close it.
//!
//! It is also, unavoidably, a *slow* answer. A second of history has to
//! accumulate before there is a modulation spectrum to look at, so this can say
//! "somebody has been talking recently" long before it can say "this 10 ms is
//! speech". Anything using it has to treat it as context for the level tests
//! rather than as a replacement for them.
//!
//! **It is not in the transmit decision.** It is computed and published so it
//! can be scored against hand-labelled recordings first — `core/tests/road.rs`
//! reports how much information each feature actually carries. That order is
//! deliberate and is the lesson of [`super::pitch`], which went in on
//! synthetic evidence and had to come out again.

use super::dsp::fft;

/// Blocks of history the modulation spectrum is taken over.
///
/// 128 blocks of 10 ms is 1.28 seconds, which holds four or five syllables —
/// enough for a rate to mean something. A power of two because the transform
/// wants one, and the next size down would hold two syllables and measure
/// mostly the accident of where the window fell.
const WINDOW: usize = 128;

/// The block rate, which is the rate the envelope is sampled at.
const ENVELOPE_HZ: f32 = 100.0;

/// The syllabic band. Three to eight syllables a second is conversational
/// speech; below it is gusting and above it is nothing anybody articulates.
const SYLLABIC_LOW_HZ: f32 = 3.0;
const SYLLABIC_HIGH_HZ: f32 = 8.0;

/// The band the syllabic share is taken against.
///
/// Not the whole spectrum. Below 0.8 Hz sits the drift of the level itself —
/// riding into a headwind, opening the throttle — which is large, has nothing
/// to do with talking, and would swamp the ratio if it were counted.
const CONTEXT_LOW_HZ: f32 = 0.8;
const CONTEXT_HIGH_HZ: f32 = 20.0;

/// How much of the recent loudness is moving at a talking rate.
pub struct ModulationTracker {
    /// Recent block levels in dB, oldest first.
    history: [f32; WINDOW],
    filled: usize,
    re: [f32; WINDOW],
    im: [f32; WINDOW],
    window: [f32; WINDOW],
}

impl Default for ModulationTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl ModulationTracker {
    pub fn new() -> Self {
        let mut window = [0.0f32; WINDOW];
        for (i, w) in window.iter_mut().enumerate() {
            // Hann. Without it the transform sees the window edges as a step
            // and smears energy across every bin, which on a ratio between two
            // bands is not a cosmetic problem — it moves the answer.
            *w = 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / WINDOW as f32).cos();
        }
        Self {
            history: [f32::NAN; WINDOW],
            filled: 0,
            re: [0.0; WINDOW],
            im: [0.0; WINDOW],
            window,
        }
    }

    pub fn reset(&mut self) {
        self.history = [f32::NAN; WINDOW];
        self.filled = 0;
    }

    /// Adds a block's level and reports the syllabic share, 0..1.
    ///
    /// Zero until there is a window of history, because a modulation rate
    /// measured over a quarter of a second is a guess about which quarter.
    pub fn push(&mut self, level_db: f32) -> f32 {
        self.history.copy_within(1.., 0);
        // Silence arrives as a very large negative number, and a couple of
        // those dominate the variance of the window and read as an enormous
        // modulation. Floored well below any speech but within sight of it.
        self.history[WINDOW - 1] = level_db.max(-90.0);
        self.filled = (self.filled + 1).min(WINDOW);
        if self.filled < WINDOW {
            return 0.0;
        }

        // Mean removed before windowing: the average level is a DC term that
        // says how loud it is, and this is a question about change.
        let mean: f32 = self.history.iter().sum::<f32>() / WINDOW as f32;
        for i in 0..WINDOW {
            self.re[i] = (self.history[i] - mean) * self.window[i];
            self.im[i] = 0.0;
        }
        fft(&mut self.re, &mut self.im, false);

        let bin_hz = ENVELOPE_HZ / WINDOW as f32;
        let (mut syllabic, mut context) = (0.0f32, 0.0f32);
        for bin in 1..WINDOW / 2 {
            let hz = bin as f32 * bin_hz;
            if !(CONTEXT_LOW_HZ..=CONTEXT_HIGH_HZ).contains(&hz) {
                continue;
            }
            let power = self.re[bin] * self.re[bin] + self.im[bin] * self.im[bin];
            context += power;
            if (SYLLABIC_LOW_HZ..=SYLLABIC_HIGH_HZ).contains(&hz) {
                syllabic += power;
            }
        }

        if context <= 1e-12 {
            return 0.0;
        }
        (syllabic / context).clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Feeds an envelope that rises and falls `hz` times a second.
    fn modulated_at(hz: f32, blocks: usize) -> f32 {
        let mut t = ModulationTracker::new();
        let mut last = 0.0;
        for i in 0..blocks {
            let phase = 2.0 * std::f32::consts::PI * hz * i as f32 / ENVELOPE_HZ;
            last = t.push(-40.0 + 12.0 * phase.sin());
        }
        last
    }

    #[test]
    fn a_talking_rate_scores_high() {
        for hz in [3.5f32, 4.0, 5.0, 7.0] {
            let share = modulated_at(hz, 400);
            assert!(share > 0.7, "{hz} Hz modulation scored only {share}");
        }
    }

    #[test]
    fn a_gusting_rate_scores_low() {
        // Wind varies over seconds, not syllables.
        for hz in [0.2f32, 0.5] {
            let share = modulated_at(hz, 400);
            assert!(share < 0.3, "{hz} Hz modulation scored {share}");
        }
    }

    #[test]
    fn a_steady_level_scores_low() {
        // An engine at constant throttle. Nothing is moving, so nothing is
        // moving at a talking rate.
        let mut t = ModulationTracker::new();
        let mut last = 0.0;
        for _ in 0..400 {
            last = t.push(-30.0);
        }
        assert!(last < 0.3, "a flat level scored {last}");
    }

    #[test]
    fn nothing_is_reported_until_there_is_a_window() {
        // A modulation rate measured over a quarter of a second is a guess
        // about which quarter.
        let mut t = ModulationTracker::new();
        for i in 0..WINDOW - 1 {
            assert_eq!(t.push(-40.0 + (i as f32).sin()), 0.0);
        }
        t.push(-40.0);
    }

    #[test]
    fn silence_does_not_read_as_enormous_modulation() {
        // Silence arrives as a very large negative dBFS, and a couple of those
        // in a window dominate its variance. Without the floor a pause between
        // two words looks like the most articulate thing in the recording.
        let mut t = ModulationTracker::new();
        let mut last = 0.0;
        for i in 0..400 {
            last = t.push(if i % 50 == 0 { -120.0 } else { -30.0 });
        }
        assert!(last < 0.5, "punctuated silence scored {last}");
    }
}
