//! How much of what the speaker plays comes back into the microphone.
//!
//! **Echo return loss, measured before anything cancels it.** The chain has an
//! echo canceller and reports how well it is doing, but nothing measures the
//! path it is cancelling — and that number answers a question the project has
//! carried unanswered: whether the *platform* is running a canceller of its own
//! underneath ours.
//!
//! It matters because Android's `VOICE_COMMUNICATION` capture preset switches
//! on the device's own echo cancellation, noise suppression and gain control on
//! most phones, and there is no reliable way to ask whether it did.
//! `AcousticEchoCanceler.getEnabled` sees only effects the *framework* attached
//! to a session; pre-processing applied inside the audio HAL — which is where
//! most of it happens — is invisible to that API. So a negative answer means
//! nothing, which is the answer most devices would give.
//!
//! Measuring is not ambiguous in the same way. A phone's speaker and microphone
//! are inches apart: play something and a large fraction of it comes back. If
//! it does not, something removed it.
//!
//! # Why a minimum, and which way it is wrong
//!
//! The ratio is only the echo path when the near end is silent. Somebody
//! talking over the top raises the microphone and nothing else, so a
//! contaminated block reads as *less* loss than the truth — and the minimum
//! therefore latches onto exactly those blocks.
//!
//! **That is chosen rather than tolerated, because the alternative is wrong in
//! the dangerous direction.** The maximum would be the least contaminated by
//! speech, and would instead latch onto the moment playback starts and the echo
//! has not yet made the trip back — reading enormous loss from a path that is
//! simply late, which is indistinguishable from the platform cancelling.
//!
//! So this understates the loss, never overstates it. It can fail to notice
//! that something upstream is cancelling; it cannot invent one. A conclusion
//! drawn from a high reading is safe, and the way to get a clean reading is the
//! deliberate one — play the test tone and stay quiet, where there is no
//! near-end speech to contaminate anything.

use super::dsp::{rms, to_dbfs};

/// How long a window the minimum is taken over, in blocks. Five seconds, which
/// is long enough to contain a gap in almost any speech and short enough to
/// follow a phone being taken out of a pocket.
const WINDOW_BLOCKS: usize = 500;

/// How loud the reference must be for a block to count, in dBFS.
///
/// Below this the far end is not really playing and the ratio is two noise
/// floors divided by each other, which is a number about nothing.
const REFERENCE_FLOOR_DB: f32 = -45.0;

/// The measurement, and what it is worth.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Coupling {
    /// Echo return loss in dB: how much quieter the microphone is than the
    /// signal that was played. Positive is loss, so larger means less echo.
    ///
    /// **Around 0 to 20 dB is a phone.** The speaker and the microphone are
    /// inches apart and the case couples them mechanically as well as through
    /// the air. A helmet with the phone in a pocket is more.
    ///
    /// **Beyond about 40 dB nothing acoustic explains it.** Either the platform
    /// is cancelling underneath us or the reference is not the signal that
    /// actually reached the speaker — and the second is worth ruling out
    /// before believing the first, because a muted output produces exactly the
    /// same reading.
    pub erl_db: f32,

    /// How many blocks the answer rests on.
    ///
    /// **Reported because a confident number from four blocks is the failure
    /// mode here.** The window only fills while the far end is playing, and on
    /// a quiet call that can take minutes. Anything under a second or so is a
    /// measurement in progress rather than a result.
    pub blocks: usize,
}

/// Tracks the loudest echo path seen recently.
pub struct EchoCoupling {
    /// The last [`WINDOW_BLOCKS`] ratios, oldest first.
    ratios: Vec<f32>,
    next: usize,
    filled: usize,
}

impl Default for EchoCoupling {
    fn default() -> Self {
        Self::new()
    }
}

impl EchoCoupling {
    pub fn new() -> Self {
        Self {
            ratios: vec![0.0; WINDOW_BLOCKS],
            next: 0,
            filled: 0,
        }
    }

    /// Offers one block of microphone audio and the reference it should be
    /// judged against.
    ///
    /// Both taken **before** the canceller, which is the whole point: after it,
    /// the number measures our own work rather than the room's.
    pub fn push(&mut self, mic: &[f32], reference: &[f32]) {
        let ref_db = to_dbfs(rms(reference));
        if !ref_db.is_finite() || ref_db < REFERENCE_FLOOR_DB {
            return;
        }
        let mic_db = to_dbfs(rms(mic));
        if !mic_db.is_finite() {
            return;
        }
        // Loss, so the sign reads the way the name does.
        self.ratios[self.next] = ref_db - mic_db;
        self.next = (self.next + 1) % WINDOW_BLOCKS;
        self.filled = (self.filled + 1).min(WINDOW_BLOCKS);
    }

    /// The measurement so far, or `None` until anything has been heard.
    pub fn get(&self) -> Option<Coupling> {
        if self.filled == 0 {
            return None;
        }
        // The minimum loss, which is the maximum echo — the closest reading to
        // the path itself, for the reason in the module comment.
        let erl_db = self.ratios[..self.filled]
            .iter()
            .copied()
            .fold(f32::INFINITY, f32::min);
        Some(Coupling {
            erl_db,
            blocks: self.filled,
        })
    }

    /// Forgets everything, for a route change.
    ///
    /// A headset connected mid-call is a different speaker and a different
    /// microphone, so the old path says nothing about the new one — and the
    /// minimum would carry the old, closer coupling forward for five seconds
    /// into a measurement of something else.
    pub fn reset(&mut self) {
        self.next = 0;
        self.filled = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(len: usize, amp: f32) -> Vec<f32> {
        (0..len)
            .map(|i| amp * (i as f32 * 0.13).sin())
            .collect::<Vec<_>>()
    }

    /// A microphone hearing the speaker at half its amplitude is 6 dB of loss.
    #[test]
    fn it_measures_the_loss_between_the_two() {
        let mut c = EchoCoupling::new();
        let reference = tone(480, 0.5);
        let mic = tone(480, 0.25);
        for _ in 0..10 {
            c.push(&mic, &reference);
        }
        let got = c.get().expect("ten loud blocks is a measurement");
        assert!(
            (got.erl_db - 6.0).abs() < 0.5,
            "half the amplitude is 6 dB, got {}",
            got.erl_db
        );
        assert_eq!(got.blocks, 10);
    }

    /// Silence from the far end is not a measurement of anything.
    ///
    /// Without this the ratio would be two noise floors divided by each other,
    /// which on a quiet call is a confident number about nothing — and it would
    /// be the *lowest* one, so the minimum would latch onto it and stay.
    #[test]
    fn a_quiet_far_end_is_not_counted() {
        let mut c = EchoCoupling::new();
        let quiet = vec![0.0f32; 480];
        for _ in 0..100 {
            c.push(&quiet, &quiet);
        }
        assert_eq!(c.get(), None);
    }

    /// Somebody talking over the echo makes this read *more* echo, not less.
    ///
    /// **The direction is the point, and it was got backwards first.** Near-end
    /// speech raises the microphone, which lowers the apparent loss, and the
    /// minimum then latches onto those blocks. So the measurement is pulled
    /// towards "there is plenty of echo here" — the conservative end, because
    /// the conclusion it guards is "something upstream cancelled it", and that
    /// must never be reached by accident.
    #[test]
    fn near_end_speech_can_only_understate_the_loss() {
        let reference = tone(480, 0.5);
        let echo_only = tone(480, 0.25);
        let with_voice: Vec<f32> = echo_only.iter().map(|s| s * 8.0).collect();

        let mut clean = EchoCoupling::new();
        let mut noisy = EchoCoupling::new();
        for i in 0..40 {
            clean.push(&echo_only, &reference);
            if i % 4 == 0 {
                noisy.push(&echo_only, &reference);
            } else {
                noisy.push(&with_voice, &reference);
            }
        }
        let clean = clean.get().unwrap().erl_db;
        let noisy = noisy.get().unwrap().erl_db;
        assert!(
            noisy <= clean,
            "speech must never raise the reading: {noisy} against {clean}"
        );
    }

    /// A path that cannot be acoustic reads as one.
    #[test]
    fn a_cancelled_path_reads_as_implausible_loss() {
        let mut c = EchoCoupling::new();
        let reference = tone(480, 0.5);
        // A thousandth of the amplitude: 60 dB, which no phone's case gives.
        let mic = tone(480, 0.0005);
        for _ in 0..10 {
            c.push(&mic, &reference);
        }
        assert!(c.get().unwrap().erl_db > 40.0);
    }

    /// A route change throws the old path away.
    #[test]
    fn a_reset_forgets_the_old_speaker() {
        let mut c = EchoCoupling::new();
        let reference = tone(480, 0.5);
        for _ in 0..10 {
            c.push(&tone(480, 0.25), &reference);
        }
        c.reset();
        assert_eq!(c.get(), None);
    }
}
