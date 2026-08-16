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
//! # What this is not calibrated against
//!
//! Between the reference and the microphone sit the output volume, the
//! speaker, the air, the microphone and the input gain. Only the middle of
//! that is the echo path. The input gain is a number the chain knows and takes
//! back out; **the output volume is not**, so the absolute figure moves with
//! the device's volume setting and is not comparable between phones.
//!
//! What it is good for is comparing one configuration against another on the
//! same phone at the same volume — which is exactly the question it was built
//! for: whether asking Android for the telephony capture preset switches on a
//! canceller underneath ours. Read it, change the setting, read it again.
//!
//! # Near-end speech is excluded rather than tolerated
//!
//! The minimum is chosen to latch onto whichever block holds the most
//! microphone signal, and during a call that is the rider talking rather than
//! the echo — so blocks the chain called speech are dropped. It costs nothing:
//! there is always more far-end audio than near-end speech to measure against,
//! and a block spent talking was never going to be a measurement of the room.
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
    /// **A change of 20 dB or more between two settings is the signal.** The
    /// absolute value is not calibrated — see the module comment — so a low
    /// reading may only mean the output volume is low, and a phone at half
    /// volume reads 6 dB more loss than the same phone at full.
    ///
    /// A reading that *moves sharply* when nothing physical moved is the thing
    /// worth acting on: same phone, same volume, same distance, and 20 or
    /// 30 dB more loss after a setting changed means something in the platform
    /// started cancelling.
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
    ///
    /// `input_gain_db` is whatever was applied to `mic` on the way in — the
    /// rider's slider plus the clip guard's trim — and is taken back out here.
    /// **Leaving it in was the first version and it read −13 dB on a phone**,
    /// because a rider running +30 dB of input gain measures their own slider
    /// and calls it a room. It is a known number, so there is no reason to
    /// carry it.
    ///
    /// `near_end_speaking` is the chain's own verdict on the *previous* block —
    /// this one has not been judged yet — and blocks where it is true are
    /// dropped outright. **Without that this reads mostly the near end during a
    /// call**: the minimum is chosen to latch onto whichever block has the most
    /// microphone in it, and on a call that is the rider talking, not the echo.
    /// Measured on a phone it read −25 dB mid-conversation and −13 dB with only
    /// a test tone playing, which is a 12 dB difference made entirely of
    /// somebody's voice.
    pub fn push(
        &mut self,
        mic: &[f32],
        reference: &[f32],
        input_gain_db: f32,
        near_end_speaking: bool,
    ) {
        if near_end_speaking {
            return;
        }
        let ref_db = to_dbfs(rms(reference));
        if !ref_db.is_finite() || ref_db < REFERENCE_FLOOR_DB {
            return;
        }
        let mic_db = to_dbfs(rms(mic));
        if !mic_db.is_finite() {
            return;
        }
        // Loss, so the sign reads the way the name does, with the gain the
        // chain itself added taken back off the microphone.
        self.ratios[self.next] = ref_db - (mic_db - input_gain_db);
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
            c.push(&mic, &reference, 0.0, false);
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
            c.push(&quiet, &quiet, 0.0, false);
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
            clean.push(&echo_only, &reference, 0.0, false);
            if i % 4 == 0 {
                noisy.push(&echo_only, &reference, 0.0, false);
            } else {
                noisy.push(&with_voice, &reference, 0.0, false);
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
            c.push(&mic, &reference, 0.0, false);
        }
        assert!(c.get().unwrap().erl_db > 40.0);
    }

    /// Blocks the chain called speech are not measurements of a room.
    ///
    /// **The failure this closes was found on a phone, not in a test.** The
    /// figure read −13 dB with a test tone playing and −25 mid-conversation:
    /// the extra 12 dB was the rider's own voice, because the minimum is chosen
    /// to latch onto whichever block holds the most microphone signal and
    /// during a call that is them talking.
    #[test]
    fn the_near_end_talking_is_not_a_measurement() {
        let reference = tone(480, 0.5);
        let echo_only = tone(480, 0.25);
        let with_voice: Vec<f32> = echo_only.iter().map(|s| s * 8.0).collect();

        let mut c = EchoCoupling::new();
        for i in 0..40 {
            if i % 4 == 0 {
                c.push(&echo_only, &reference, 0.0, false);
            } else {
                // Loud near-end speech, correctly labelled. Dropped.
                c.push(&with_voice, &reference, 0.0, true);
            }
        }
        let got = c.get().unwrap();
        assert!(
            (got.erl_db - 6.0).abs() < 0.5,
            "only the quiet blocks should count, got {}",
            got.erl_db
        );
        assert_eq!(got.blocks, 10, "the talking blocks must not be counted");
    }

    /// The gain the chain added is not the room's doing.
    ///
    /// A rider on +30 dB of input gain measured their own slider and called it
    /// an echo path: the figure read −13 dB on a phone whose acoustic path is
    /// around +17.
    #[test]
    fn the_input_gain_is_taken_back_out() {
        let reference = tone(480, 0.5);
        // The microphone as the chain sees it: the echo, times 30 dB of gain.
        let amplified = tone(480, 0.25 * 31.62);
        let mut c = EchoCoupling::new();
        for _ in 0..10 {
            c.push(&amplified, &reference, 30.0, false);
        }
        assert!(
            (c.get().unwrap().erl_db - 6.0).abs() < 0.5,
            "30 dB of slider must not read as 30 dB less room"
        );
    }

    /// A route change throws the old path away.
    #[test]
    fn a_reset_forgets_the_old_speaker() {
        let mut c = EchoCoupling::new();
        let reference = tone(480, 0.5);
        for _ in 0..10 {
            c.push(&tone(480, 0.25), &reference, 0.0, false);
        }
        c.reset();
        assert_eq!(c.get(), None);
    }
}
