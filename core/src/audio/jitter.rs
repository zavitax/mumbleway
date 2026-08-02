//! Per-speaker jitter buffer.
//!
//! Packets arrive late, early, out of order or not at all — especially over
//! cellular while moving. The buffer trades a little latency for continuity:
//! it holds a small backlog so a late packet still arrives before its turn, and
//! uses Opus FEC/PLC to fill genuine gaps rather than clicking.

use std::collections::BTreeMap;

use super::codec::{VoiceDecoder, FRAME_SAMPLES};
use super::dsp::LevelNormalizer;
use crate::error::Result;

/// Loudness every speaker is brought towards, in dBFS.
const NORMALISE_TARGET_DB: f32 = -20.0;

/// Starting backlog, in 20 ms frames. Three frames is 60 ms, a reasonable
/// compromise between mouth-to-ear delay and cellular jitter.
pub const DEFAULT_TARGET_FRAMES: usize = 3;

/// Never buffer more than this; beyond it we are adding delay, not resilience.
pub const MAX_TARGET_FRAMES: usize = 15;

/// Drop the whole buffer if it somehow exceeds this (a stuck or hostile sender).
const HARD_CAP_FRAMES: usize = 60;

/// Buffers and decodes one speaker's stream.
pub struct SpeakerBuffer {
    decoder: VoiceDecoder,
    /// Pending Opus packets keyed by sequence number.
    pending: BTreeMap<u64, Vec<u8>>,
    /// Sequence we expect to play next.
    next_seq: Option<u64>,
    target: usize,
    /// Consecutive concealed frames, used to grow the buffer under bad jitter.
    recent_losses: u32,
    /// Frames played since the last loss, used to shrink it again.
    clean_run: u32,
    /// True once the sender has signalled end of transmission.
    finished: bool,
    /// Per-speaker loudness correction, so everyone arrives at a similar level.
    normalizer: LevelNormalizer,
}

impl SpeakerBuffer {
    pub fn new() -> Result<Self> {
        Ok(Self {
            decoder: VoiceDecoder::new()?,
            pending: BTreeMap::new(),
            next_seq: None,
            target: DEFAULT_TARGET_FRAMES,
            recent_losses: 0,
            clean_run: 0,
            finished: false,
            normalizer: LevelNormalizer::new(NORMALISE_TARGET_DB),
        })
    }

    /// Current loudness correction for this speaker, in dB.
    pub fn normalisation_gain_db(&self) -> f32 {
        self.normalizer.gain_db()
    }

    pub fn target_frames(&self) -> usize {
        self.target
    }

    pub fn buffered(&self) -> usize {
        self.pending.len()
    }

    pub fn is_finished(&self) -> bool {
        self.finished && self.pending.is_empty()
    }

    /// Accepts a packet. Packets older than the play head are discarded.
    pub fn push(&mut self, sequence: u64, opus: Vec<u8>, terminator: bool) {
        if terminator {
            self.finished = true;
        }
        if let Some(next) = self.next_seq {
            if sequence < next {
                // Too late to be useful; its slot has already been played.
                return;
            }
        }
        self.pending.insert(sequence, opus);

        if self.pending.len() > HARD_CAP_FRAMES {
            // Something is badly wrong; resynchronise rather than grow forever.
            self.pending.clear();
            self.next_seq = None;
        }
    }

    /// True when enough backlog exists to start (or continue) playback.
    pub fn ready(&self) -> bool {
        self.pending.len() >= self.target || (self.finished && !self.pending.is_empty())
    }

    /// Produces the next frame of PCM, concealing losses.
    ///
    /// Returns `None` when there is nothing to play at all.
    pub fn pop(&mut self, out: &mut [f32]) -> Option<usize> {
        debug_assert_eq!(out.len(), FRAME_SAMPLES);

        if self.pending.is_empty() {
            return None;
        }

        // Establish the play head on the first packet we ever see.
        let next = match self.next_seq {
            Some(n) => n,
            None => {
                let first = *self.pending.keys().next()?;
                self.next_seq = Some(first);
                first
            }
        };

        if let Some(packet) = self.pending.remove(&next) {
            self.next_seq = Some(next + 1);
            self.clean_run += 1;
            if self.clean_run > 250 && self.target > DEFAULT_TARGET_FRAMES {
                // Sustained clean playback: give the latency back.
                self.target -= 1;
                self.clean_run = 0;
            }
            let n = self.decoder.decode(&packet, out).ok()?;
            // Normalise here rather than in the mixer so each speaker's gain
            // tracks that speaker, not whatever the mix happens to contain.
            self.normalizer.process(&mut out[..n]);
            return Some(n);
        }

        // The expected packet is missing. If a later one exists, conceal this
        // slot; the FEC copy inside the next packet gives a far better result
        // than interpolation.
        let next_available = self.pending.keys().next().copied();
        self.next_seq = Some(next + 1);
        self.recent_losses += 1;
        self.clean_run = 0;
        if self.recent_losses > 3 && self.target < MAX_TARGET_FRAMES {
            // Jitter is getting worse; buffer more.
            self.target += 1;
            self.recent_losses = 0;
        }

        let fec_source = next_available.and_then(|s| self.pending.get(&s)).cloned();
        let n = self.decoder.decode_lost(fec_source.as_deref(), out).ok()?;
        self.normalizer.process(&mut out[..n]);
        Some(n)
    }

    pub fn reset(&mut self) {
        self.pending.clear();
        self.next_seq = None;
        self.finished = false;
        self.recent_losses = 0;
        self.clean_run = 0;
        self.normalizer.reset();
        let _ = self.decoder.reset();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::codec::{Quality, VoiceEncoder};

    fn frames(n: usize) -> Vec<Vec<u8>> {
        let mut enc = VoiceEncoder::new(Quality::Balanced).unwrap();
        (0..n)
            .map(|i| {
                let pcm: Vec<f32> = (0..FRAME_SAMPLES)
                    .map(|s| {
                        let t = (i * FRAME_SAMPLES + s) as f32 / 48000.0;
                        (2.0 * std::f32::consts::PI * 220.0 * t).sin() * 0.4
                    })
                    .collect();
                enc.encode(&pcm).unwrap()
            })
            .collect()
    }

    #[test]
    fn plays_frames_in_order() {
        let mut b = SpeakerBuffer::new().unwrap();
        for (i, f) in frames(5).into_iter().enumerate() {
            b.push(i as u64, f, false);
        }
        let mut out = vec![0.0f32; FRAME_SAMPLES];
        for _ in 0..5 {
            assert_eq!(b.pop(&mut out), Some(FRAME_SAMPLES));
        }
        assert_eq!(b.pop(&mut out), None, "buffer should be drained");
    }

    #[test]
    fn reorders_out_of_order_arrivals() {
        let mut b = SpeakerBuffer::new().unwrap();
        let f = frames(4);
        // Deliver 0, 2, 1, 3.
        b.push(0, f[0].clone(), false);
        b.push(2, f[2].clone(), false);
        b.push(1, f[1].clone(), false);
        b.push(3, f[3].clone(), false);

        let mut out = vec![0.0f32; FRAME_SAMPLES];
        // All four should play without any concealment.
        for _ in 0..4 {
            assert_eq!(b.pop(&mut out), Some(FRAME_SAMPLES));
        }
        assert_eq!(b.buffered(), 0);
    }

    #[test]
    fn conceals_a_missing_frame_instead_of_stalling() {
        let mut b = SpeakerBuffer::new().unwrap();
        let f = frames(4);
        b.push(0, f[0].clone(), false);
        // frame 1 never arrives
        b.push(2, f[2].clone(), false);
        b.push(3, f[3].clone(), false);

        let mut out = vec![0.0f32; FRAME_SAMPLES];
        assert_eq!(b.pop(&mut out), Some(FRAME_SAMPLES)); // 0
                                                          // Slot 1 is concealed rather than skipped or stalled.
        assert_eq!(b.pop(&mut out), Some(FRAME_SAMPLES));
        assert!(out.iter().all(|s| s.is_finite()));
        assert_eq!(b.pop(&mut out), Some(FRAME_SAMPLES)); // 2
        assert_eq!(b.pop(&mut out), Some(FRAME_SAMPLES)); // 3
    }

    #[test]
    fn discards_packets_that_arrive_far_too_late() {
        let mut b = SpeakerBuffer::new().unwrap();
        let f = frames(6);
        for (i, p) in f.iter().enumerate().take(4) {
            b.push(i as u64, p.clone(), false);
        }
        let mut out = vec![0.0f32; FRAME_SAMPLES];
        for _ in 0..4 {
            b.pop(&mut out);
        }
        // Sequence 1 turning up now is useless; it must not rewind playback.
        b.push(1, f[1].clone(), false);
        assert_eq!(b.buffered(), 0, "stale packet was buffered");
    }

    #[test]
    fn grows_the_buffer_under_sustained_loss() {
        let mut b = SpeakerBuffer::new().unwrap();
        let start = b.target_frames();
        let f = frames(40);
        // Deliver only every other frame, forcing repeated concealment.
        for (i, p) in f.iter().enumerate() {
            if i % 2 == 0 {
                b.push(i as u64, p.clone(), false);
            }
        }
        let mut out = vec![0.0f32; FRAME_SAMPLES];
        for _ in 0..40 {
            b.pop(&mut out);
        }
        assert!(
            b.target_frames() > start,
            "buffer should deepen when the link is jittery"
        );
        assert!(b.target_frames() <= MAX_TARGET_FRAMES);
    }

    #[test]
    fn terminator_marks_the_stream_finished() {
        let mut b = SpeakerBuffer::new().unwrap();
        let f = frames(2);
        b.push(0, f[0].clone(), false);
        b.push(1, f[1].clone(), true);
        assert!(!b.is_finished(), "still has audio to play");

        let mut out = vec![0.0f32; FRAME_SAMPLES];
        b.pop(&mut out);
        b.pop(&mut out);
        assert!(b.is_finished(), "should be finished once drained");
    }

    #[test]
    fn survives_a_runaway_sender_without_growing_without_bound() {
        let mut b = SpeakerBuffer::new().unwrap();
        let f = frames(1);
        for i in 0..500u64 {
            b.push(i, f[0].clone(), false);
        }
        assert!(
            b.buffered() <= HARD_CAP_FRAMES,
            "buffer grew to {} frames",
            b.buffered()
        );
    }

    #[test]
    fn reset_clears_everything() {
        let mut b = SpeakerBuffer::new().unwrap();
        for (i, f) in frames(3).into_iter().enumerate() {
            b.push(i as u64, f, true);
        }
        b.reset();
        assert_eq!(b.buffered(), 0);
        assert!(!b.is_finished());
        let mut out = vec![0.0f32; FRAME_SAMPLES];
        assert_eq!(b.pop(&mut out), None);
    }
}
