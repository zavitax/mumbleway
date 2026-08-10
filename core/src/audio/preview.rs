//! Playing a recording back through the capture chain.
//!
//! # Why it exists
//!
//! A diagnostic recording is the **microphone**, deliberately — the recorder
//! takes its copy above the enhancer so the corpus is what the device heard
//! rather than what the chain made of it. That is the right file to keep, and
//! it is the wrong file to answer *"is this what the others hear?"* with.
//!
//! Until now, answering that meant two clients, two devices and two accounts,
//! with a rider trying to judge their own voice arriving back at them. This
//! runs the recording through the same stages the live audio goes through, so
//! the question is a button.
//!
//! # What it reproduces, and what it deliberately does not
//!
//! **The processing**: the speech enhancer, then the suppression profile, the
//! gate, the levelling and the limiter — every stage that changes the sound.
//!
//! **Not the transmit envelope.** Which stretches went out is a separate
//! question with its own control, and it is answered from the decision log
//! rather than by re-deciding: the log records what the chain actually did on
//! the day, and re-running the gate now would answer with what it would decide
//! today, on a different profile, with a noise floor learned from a file. The
//! two controls compose — processing on, transmitted-only on, and what is left
//! is what the far end got.
//!
//! # A separate chain, never the live one
//!
//! Every stage here adapts: the noise floor, the AGC, the expander's learned
//! spectrum, the enhancer's recurrent state. Feeding yesterday's ride through
//! the live chain would teach all of them about a file and leave a rider's
//! actual microphone tuned to it. So this owns its own instances, and throws
//! them away when the listener stops.

use super::deepfilter::Enhancer;
use super::denoise::{CaptureProcessor, NoiseProfile, FRAME_SIZE};

/// A capture chain with nothing on the other end of it.
pub struct PreviewChain {
    enhancer: Enhancer,
    processor: CaptureProcessor,
    /// Input not yet a whole block. The caller hands over whatever the
    /// transport read, which is a buffer length rather than a multiple of the
    /// chain's block, so the remainder waits here for the rest of itself.
    pending: Vec<f32>,
}

impl PreviewChain {
    /// Builds the chain. **Slow** — tens of milliseconds, and seconds on a
    /// low-end phone, because the enhancer loads a model. Never on the audio
    /// thread and never on the platform thread.
    pub fn new(profile: NoiseProfile) -> Self {
        Self {
            enhancer: Enhancer::new(),
            processor: CaptureProcessor::new(profile),
            pending: Vec::with_capacity(FRAME_SIZE * 2),
        }
    }

    /// Follows the rider's setting, so switching profiles while listening does
    /// what it looks like it does.
    pub fn set_profile(&mut self, profile: NoiseProfile) {
        self.processor.set_profile(profile);
    }

    /// Runs whole blocks through the chain, appending to `out`.
    ///
    /// Returns having consumed all of `input`; anything short of a block is
    /// held for the next call, so `out` can be up to one block shorter than
    /// what went in. The transport allows for that — 10 ms at the very worst,
    /// and it comes back on the following call.
    ///
    /// No echo reference. There is no far end in a preview, so there is no
    /// echo to cancel, and the canceller becomes a pass-through of its own
    /// accord when handed nothing.
    pub fn process(&mut self, input: &[f32], out: &mut Vec<f32>) {
        self.pending.extend_from_slice(input);
        let whole = self.pending.len() / FRAME_SIZE;
        if whole == 0 {
            return;
        }
        out.reserve(whole * FRAME_SIZE);
        for i in 0..whole {
            let mut block: [f32; FRAME_SIZE] = [0.0; FRAME_SIZE];
            block.copy_from_slice(&self.pending[i * FRAME_SIZE..(i + 1) * FRAME_SIZE]);
            // The same order as the capture worker, and it matters: everything
            // below reads a level, so the enhancer has to have run first or
            // they are all reading the microphone the chain no longer hears.
            self.enhancer.process(&mut block);
            let _ = self.processor.process(&mut block);
            out.extend_from_slice(&block);
        }
        self.pending.drain(..whole * FRAME_SIZE);
    }
}

// **Deliberately not `Send`, and not made so.** `tract` holds its tensors in
// `Rc`, so `DfTract` — and therefore `Enhancer` and this — cannot cross a
// thread boundary. An `unsafe impl Send` behind a mutex would be sound and is
// the obvious shortcut; it is not taken. `preview_worker` in `engine.rs` owns
// one of these outright on a thread of its own and communicates by channel, so
// the chain never moves at all and there is nothing to justify. The live
// capture chain is arranged the same way, for the same reason.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whole_blocks_come_out_and_the_remainder_waits() {
        let mut chain = PreviewChain::new(NoiseProfile::Standard);
        let mut out = Vec::new();

        // Less than a block: nothing can come out yet.
        chain.process(&vec![0.1; 200], &mut out);
        assert!(out.is_empty());

        // The rest of that block plus a bit: exactly one block comes out.
        chain.process(&vec![0.1; 400], &mut out);
        assert_eq!(out.len(), FRAME_SIZE);

        // And the 120 left over are not lost -- they lead the next block.
        out.clear();
        chain.process(&vec![0.1; FRAME_SIZE - 120], &mut out);
        assert_eq!(out.len(), FRAME_SIZE);
    }

    #[test]
    fn it_actually_changes_the_audio() {
        // A preview that returns the file unchanged would answer "is this what
        // they hear" with a confident lie, and look identical on screen.
        let mut chain = PreviewChain::new(NoiseProfile::Helmet);
        let mut out = Vec::new();
        let mut seed = 99u32;
        let input: Vec<f32> = (0..FRAME_SIZE * 40)
            .map(|i| {
                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                let noise = (seed >> 16) as f32 / 32768.0 - 1.0;
                0.25 * (i as f32 * 0.06).sin() + 0.15 * noise
            })
            .collect();
        chain.process(&input, &mut out);
        assert_eq!(out.len(), input.len());
        assert!(
            out.iter().zip(&input).any(|(a, b)| (a - b).abs() > 1e-4),
            "the preview chain passed the audio through untouched"
        );
        assert!(
            out.iter().all(|s| s.is_finite() && s.abs() <= 1.5),
            "the preview chain produced something unplayable"
        );
    }

    #[test]
    fn silence_stays_silent() {
        let mut chain = PreviewChain::new(NoiseProfile::Standard);
        let mut out = Vec::new();
        chain.process(&vec![0.0; FRAME_SIZE * 20], &mut out);
        assert!(out.iter().all(|s| s.abs() < 1e-3), "silence gained signal");
    }
}
