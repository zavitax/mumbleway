//! Per-speaker jitter buffer.
//!
//! Packets arrive late, early, out of order or not at all — especially over
//! cellular while moving. The buffer trades a little latency for continuity:
//! it holds a small backlog so a late packet still arrives before its turn, and
//! uses Opus FEC/PLC to fill genuine gaps rather than clicking.

use std::collections::BTreeMap;

use super::codec::{VoiceDecoder, FRAME_SAMPLES, SAMPLE_RATE};
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

/// How far out of step a packet must be before it counts as a new stream
/// rather than a late or reordered one.
///
/// A minute of frames. Real lateness is milliseconds; anything beyond this is
/// a talker who stopped and started again, and treating the two the same is
/// how the start of an utterance goes missing.
const RESTART_GAP_FRAMES: u64 = 3_000;

/// How many frames in a row may be invented before the buffer gives up and
/// skips to the next real packet.
///
/// Opus concealment extrapolates convincingly for a frame or two and then
/// decays into a wind-like hiss. The silence between two sentences is a gap of
/// exactly this kind — hundreds of slots that were never sent — so without a
/// bound the buffer fills every pause with noise until the next word arrives.
const MAX_CONSECUTIVE_CONCEALS: u32 = 5;

/// Mixer rounds a short burst may wait for company before playing anyway.
///
/// The target backlog assumes more is coming. A burst shorter than it never
/// arrives at that threshold, so without this it waits for a transmission that
/// already ended and is simply never heard — which is what a voice hovering at
/// the sender's activation threshold produces, over and over.
const STALL_ROUNDS_BEFORE_START: u32 = 40;

/// Level reported for a speaker producing nothing.
pub const SILENT_DB: f32 = -120.0;

/// How quickly a meter falls once a speaker stops, in dB per idle round.
///
/// Fast enough that the meter empties as speech ends, slow enough that the
/// gaps between words do not make it flicker.
const LEVEL_DECAY_DB: f32 = 1.5;

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
    /// Frames invented since the last real one, bounding a concealment run.
    concealed_run: u32,
    /// Frames played since the last loss, used to shrink it again.
    clean_run: u32,
    /// True once the sender has signalled end of transmission.
    finished: bool,
    /// How far the sequence advances per packet, in 10 ms units.
    ///
    /// Sequence counts 10 ms units, so a sender using 20 ms frames steps by
    /// two and one using 60 ms by six. Assuming one leaves a phantom gap
    /// between every packet, which the buffer then fills with invented audio —
    /// most of the stream, synthesised.
    ///
    /// Taken from the packet rather than inferred from the numbering, because
    /// the two cannot be told apart by numbering: every other packet lost from
    /// a 10 ms sender looks exactly like an intact 20 ms one. The packet says
    /// how long it is, so there is nothing to guess.
    stride: Option<u64>,
    /// Whether playback is under way, as opposed to still filling up.
    ///
    /// The target backlog is a threshold for *starting*, not a condition for
    /// every frame. Requiring it continuously means the cushion is never spent
    /// — the buffer holds frames back while the speaker goes silent for want
    /// of them — which turns ordinary jitter into a hole in the audio.
    playing: bool,
    /// Per-speaker loudness correction, so everyone arrives at a similar level.
    normalizer: LevelNormalizer,
    /// Whether that correction is applied at all.
    normalise: bool,
    /// Frames handed out that were invented rather than decoded, cumulative.
    ///
    /// Synthesised audio and real audio sound alike enough to argue about;
    /// counting them apart settles whether a hiss is something the buffer made
    /// up or something the far end actually sent.
    concealed_total: u64,
    /// Frames handed out that came from a real packet, cumulative.
    decoded_total: u64,
    /// Mixer rounds spent holding audio that is not yet judged ready.
    stalled_rounds: u32,
    /// Smoothed output level in dBFS, for this speaker's meter.
    ///
    /// Measured after decoding rather than taken from the roster, because the
    /// server never says who is talking — that is only knowable from the audio
    /// actually arriving.
    level_db: f32,
}

impl SpeakerBuffer {
    pub fn new() -> Result<Self> {
        Ok(Self {
            decoder: VoiceDecoder::new()?,
            pending: BTreeMap::new(),
            next_seq: None,
            target: DEFAULT_TARGET_FRAMES,
            recent_losses: 0,
            concealed_run: 0,
            clean_run: 0,
            finished: false,
            stride: None,
            playing: false,
            normalizer: LevelNormalizer::new(NORMALISE_TARGET_DB),
            normalise: true,
            concealed_total: 0,
            decoded_total: 0,
            stalled_rounds: 0,
            level_db: SILENT_DB,
        })
    }

    /// Turns loudness correction on or off.
    ///
    /// Worth being able to remove: an automatic gain that adapts on the gaps
    /// between sentences raises the far end's room tone towards the target and
    /// is heard as a hiss that comes up after every sentence. Being able to
    /// take it out of the chain settles whether it is the cause in seconds.
    pub fn set_normalisation(&mut self, on: bool) {
        self.normalise = on;
    }

    /// `(invented, decoded)` frames handed out so far.
    pub fn frame_counts(&self) -> (u64, u64) {
        (self.concealed_total, self.decoded_total)
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
        // Read before this packet can change it: a packet arriving *after* a
        // terminator belongs to the next transmission, whatever its number.
        let ended = self.finished;
        if terminator {
            self.finished = true;
        }

        if let Some(next) = self.next_seq {
            if sequence < next {
                // A sender that numbers each transmission from zero puts every
                // burst after the first behind the play head, where it would
                // be discarded in full as arriving too late.
                //
                // Three ways to recognise it: the end of the last transmission
                // said so, or nothing is playing so there is nothing this could
                // arrive too late for, or the distance is beyond any real
                // lateness. The middle one matters because the terminator is a
                // single packet and losing it must not cost a whole word.
                if ended || !self.playing || next - sequence > RESTART_GAP_FRAMES {
                    self.pending.clear();
                    self.next_seq = None;
                    self.finished = terminator;
                    self.playing = false;
                    self.recent_losses = 0;
                    self.concealed_run = 0;
                    self.clean_run = 0;
                } else {
                    // Too late to be useful; its slot has already been played.
                    return;
                }
            }
        }
        self.stalled_rounds = 0;
        self.pending.insert(sequence, opus);

        if self.pending.len() > HARD_CAP_FRAMES {
            // Something is badly wrong; resynchronise rather than grow forever.
            self.pending.clear();
            self.next_seq = None;
        }
    }

    /// How far the sequence advances per packet, once known.
    fn step(&self) -> u64 {
        self.stride.unwrap_or(1).max(1)
    }

    /// Reads the packet's own duration and records it as the sequence step.
    fn observe_stride(&mut self, packet: &[u8]) {
        let Ok(samples) = opus::packet::get_nb_samples(packet, SAMPLE_RATE) else {
            return;
        };
        // 10 ms at 48 kHz is 480 samples, and that is the unit the sequence
        // counts in.
        let units = (samples / 480) as u64;
        if units > 0 {
            self.stride = Some(units);
        }
    }

    /// True when there is something to play.
    ///
    /// The target backlog gates the *start* of a burst only. Once playing,
    /// anything held is played: holding a frame back because the backlog is
    /// thinner than we would like produces the silence it was meant to
    /// prevent.
    pub fn ready(&self) -> bool {
        if self.pending.is_empty() {
            return false;
        }
        self.playing
            || self.finished
            || self.pending.len() >= self.target
            || self.stalled_rounds >= STALL_ROUNDS_BEFORE_START
    }

    /// Records a round in which this speaker held audio but was not played.
    ///
    /// Waiting for the backlog to reach the target is right while a burst is
    /// still arriving and wrong once it has ended, and the difference is only
    /// visible as time passing with nothing new turning up.
    pub fn note_waiting(&mut self) {
        self.stalled_rounds = self.stalled_rounds.saturating_add(1);
        self.decay_level();
    }

    /// Current smoothed level in dBFS.
    pub fn level_db(&self) -> f32 {
        self.level_db
    }

    /// Lets the meter fall while this speaker produces nothing.
    pub fn decay_level(&mut self) {
        self.level_db = (self.level_db - LEVEL_DECAY_DB).max(SILENT_DB);
    }

    /// Produces the next frame of PCM, concealing genuine losses.
    ///
    /// Returns `None` only when nothing is held at all. `out` is resized to
    /// whatever the packet contains: frame length is the sender's choice, and
    /// Opus allows 10 through 60 ms.
    ///
    /// Every call either plays a packet or conceals a bounded number of times,
    /// so the buffer always moves forward. An earlier version predicted the
    /// next sequence number and waited for it to arrive; when the prediction
    /// stepped over a packet — trivially possible, since the step is the frame
    /// length and a stream can resume on any boundary — the head marched past
    /// audio it was still holding and concealed for ever, producing endless
    /// hiss with a perfectly good packet sitting in the queue.
    pub fn pop(&mut self, out: &mut Vec<f32>) -> Option<usize> {
        let available = match self.pending.keys().next().copied() {
            Some(seq) => seq,
            None => {
                // Genuinely run dry: refill to the target before starting
                // again, rather than stuttering along one frame at a time.
                self.playing = false;
                return None;
            }
        };
        self.playing = true;
        self.stalled_rounds = 0;
        let next = self.next_seq.unwrap_or(available);

        // Conceal only for a packet or two that plausibly went missing in
        // transit, and never for long. A wider gap is the silence between two
        // utterances — not damage to repair, but audio that was never sent —
        // and extrapolating across it is heard as a hiss before every word.
        let gap = available.saturating_sub(next);
        let conceal = available > next
            && gap <= self.step() * 2
            && self.concealed_run < MAX_CONSECUTIVE_CONCEALS;

        if conceal {
            self.next_seq = Some(next + self.step());
            self.clean_run = 0;
            // One run of concealment is one event, however many frames it
            // spans. Counting each frame separately lets a single pause look
            // like a burst of loss and deepen the buffer for a link that is
            // behaving perfectly.
            if self.concealed_run == 0 {
                self.recent_losses += 1;
            }
            self.concealed_run += 1;
            if self.recent_losses > 3 && self.target < MAX_TARGET_FRAMES {
                // Jitter is getting worse; buffer more.
                self.target += 1;
                self.recent_losses = 0;
            }

            self.concealed_total += 1;
            out.resize(FRAME_SAMPLES, 0.0);
            // The FEC copy inside the next packet beats interpolation.
            let fec = self.pending.get(&available).cloned();
            let n = self.decoder.decode_lost(fec.as_deref(), out).ok()?;
            if self.normalise {
                self.normalizer.process(&mut out[..n]);
            }
            return Some(n);
        }

        // Otherwise play what is actually held, wherever it sits. Removing a
        // packet on every non-concealing call is what guarantees progress.
        let packet = self.pending.remove(&available)?;
        self.observe_stride(&packet);
        self.next_seq = Some(available + self.step());
        self.concealed_run = 0;
        self.decoded_total += 1;
        self.clean_run += 1;
        if self.clean_run > 250 && self.target > DEFAULT_TARGET_FRAMES {
            // Sustained clean playback: give the latency back.
            self.target -= 1;
            self.clean_run = 0;
        }

        // Ask the packet how much room it needs before handing it a buffer.
        let needed = opus::packet::get_nb_samples(&packet, SAMPLE_RATE)
            .unwrap_or(FRAME_SAMPLES)
            .max(FRAME_SAMPLES);
        out.resize(needed, 0.0);
        let n = self.decoder.decode(&packet, out).ok()?;
        // Normalise here rather than in the mixer so each speaker's gain tracks
        // that speaker, not whatever the mix happens to contain.
        if self.normalise {
            self.normalizer.process(&mut out[..n]);
        }
        // Rises immediately and falls gradually, so a meter tracks speech
        // rather than flickering on every syllable boundary.
        let level = super::dsp::to_dbfs(super::dsp::rms(&out[..n]));
        self.level_db = if level > self.level_db {
            level
        } else {
            (self.level_db - LEVEL_DECAY_DB).max(level)
        };

        // That was the last frame held: rebuild a cushion before starting again
        // rather than stuttering along one frame at a time.
        self.playing = !self.pending.is_empty();
        Some(n)
    }

    pub fn reset(&mut self) {
        self.pending.clear();
        self.next_seq = None;
        self.finished = false;
        self.playing = false;
        self.recent_losses = 0;
        self.concealed_run = 0;
        self.clean_run = 0;
        self.normalizer.reset();
        let _ = self.decoder.reset();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::codec::{Quality, VoiceEncoder};

    /// Sequence numbers count 10 ms units and every test frame is 20 ms, so a
    /// conforming sender numbers them in twos.
    fn seq(index: usize) -> u64 {
        index as u64 * 2
    }

    /// Counts frames that actually decode to audio, not concealment.
    fn drain(b: &mut SpeakerBuffer, limit: usize) -> usize {
        let mut out = vec![0.0f32; FRAME_SAMPLES];
        let mut heard = 0;
        for _ in 0..limit {
            match b.pop(&mut out) {
                Some(_) => heard += 1,
                None => break,
            }
        }
        heard
    }

    #[test]
    fn a_second_utterance_is_heard_after_the_sender_restarts_its_counter() {
        // The failure this guards against is not silence, it is the *start* of
        // every utterance after the first going missing, and more of it the
        // longer the session runs — which sounds like a choppy network and is
        // not one.
        let packets = frames(12);
        let mut b = SpeakerBuffer::new().unwrap();

        for (i, p) in packets.iter().take(5).enumerate() {
            b.push(seq(i), p.clone(), false);
        }
        // Releasing the key sends a terminator, exactly as the engine does.
        b.push(seq(5), packets[5].clone(), true);
        assert!(drain(&mut b, 6) > 0, "the first burst should be heard");

        // The talker starts again, and the counter begins at zero.
        for (i, p) in packets.iter().skip(6).enumerate() {
            b.push(seq(i), p.clone(), false);
        }
        assert!(
            drain(&mut b, 6) > 0,
            "the second burst was discarded as arriving too late"
        );
    }

    #[test]
    fn a_sender_stepping_in_ten_millisecond_units_is_not_treated_as_loss() {
        // Mumble numbers sequences in 10 ms units, so 20 ms frames step by two
        // and 60 ms frames by six. Assuming one leaves a phantom gap between
        // every packet, and the buffer fills each one with invented audio —
        // most of the stream, synthesised, which sounds like a broken link on
        // a link that is fine.
        // Our own frames are 20 ms, so a conforming sender numbers them 0, 2,
        // 4, ... The buffer must read that from the packets, not assume 1.
        let stride = 2u64;
        let packets = frames(8);
        let mut b = SpeakerBuffer::new().unwrap();
        for (i, p) in packets.iter().enumerate() {
            b.push(i as u64 * stride, p.clone(), false);
        }

        let mut out = vec![0.0f32; FRAME_SAMPLES];
        for _ in 0..packets.len() {
            assert!(b.pop(&mut out).is_some());
        }
        assert_eq!(b.buffered(), 0, "real packets were skipped or concealed");
        assert_eq!(
            b.target_frames(),
            DEFAULT_TARGET_FRAMES,
            "phantom gaps were counted as loss and grew the buffer"
        );
    }

    #[test]
    fn a_burst_shorter_than_the_target_is_still_played() {
        // The target backlog assumes more is coming. A voice hovering at the
        // sender's activation threshold — a growl, typically — chatters into
        // bursts of one or two frames, and without a way out those wait for a
        // transmission that already ended and are never heard at all. It comes
        // through perfectly once the sender transmits continuously, which is
        // what points at the burst length rather than the audio.
        let packets = frames(2);
        let mut b = SpeakerBuffer::new().unwrap();
        b.push(0, packets[0].clone(), false);
        b.push(seq(1), packets[1].clone(), false);

        assert!(
            !b.ready(),
            "should wait a little for the rest of the burst first"
        );
        for _ in 0..STALL_ROUNDS_BEFORE_START {
            b.note_waiting();
        }
        assert!(b.ready(), "a short burst was never played");

        let mut out = Vec::new();
        assert!(b.pop(&mut out).is_some());
    }

    #[test]
    fn waiting_resets_when_more_of_the_burst_turns_up() {
        // The wait is for a burst that has ended. While one is still arriving,
        // the backlog should be allowed to build as intended.
        let packets = frames(3);
        let mut b = SpeakerBuffer::new().unwrap();
        b.push(0, packets[0].clone(), false);
        for _ in 0..STALL_ROUNDS_BEFORE_START - 1 {
            b.note_waiting();
        }
        b.push(seq(1), packets[1].clone(), false);
        assert!(!b.ready(), "the wait should have restarted");
    }

    #[test]
    fn a_packet_the_step_would_skip_is_still_played() {
        // With a 20 ms frame the play head moves in twos, so a stream that
        // resumes on an odd boundary lands between steps. Predicting the next
        // number and waiting for it marched the head straight past a packet
        // still sitting in the queue, and concealed for ever — endless hiss
        // with perfectly good audio held and unreachable. One finger snap was
        // enough to trigger it.
        let packets = frames(2);
        let mut b = SpeakerBuffer::new().unwrap();
        b.push(0, packets[0].clone(), false);

        let mut out = Vec::new();
        assert!(b.pop(&mut out).is_some(), "first packet should play");

        // Head is now at 2; this one lands at 1, between steps.
        b.push(1, packets[1].clone(), false);

        let (invented_before, _) = b.frame_counts();
        for _ in 0..50 {
            if b.buffered() == 0 {
                break;
            }
            b.pop(&mut out);
        }
        let (invented_after, decoded) = b.frame_counts();

        assert_eq!(b.buffered(), 0, "the packet was never reached");
        assert_eq!(decoded, 2, "the odd-numbered packet was never decoded");
        assert!(
            invented_after - invented_before <= MAX_CONSECUTIVE_CONCEALS as u64,
            "invented {} frames rather than playing what was held",
            invented_after - invented_before
        );
    }

    #[test]
    fn frames_longer_than_twenty_milliseconds_are_played_not_dropped() {
        // Frame length is the sender's choice, and Mumble exposes it as a
        // setting. Handing a 40 or 60 ms packet a 20 ms buffer makes the decode
        // fail for want of room, and the packet is then dropped in silence —
        // heard as a word that simply is not there, only from the people whose
        // client happens to be configured that way.
        // The raw encoder, because ours deliberately accepts only 20 ms.
        let mut enc =
            opus::Encoder::new(SAMPLE_RATE, opus::Channels::Mono, opus::Application::Voip).unwrap();
        for units in [4usize, 6] {
            let samples = units * 480;
            let pcm: Vec<f32> = (0..samples)
                .map(|i| {
                    let t = i as f32 / 48_000.0;
                    (2.0 * std::f32::consts::PI * 180.0 * t).sin() * 0.3
                })
                .collect();
            let packet = enc.encode_vec_float(&pcm, 4000).unwrap();

            let mut b = SpeakerBuffer::new().unwrap();
            b.push(0, packet, true);
            let mut out = vec![0.0f32; FRAME_SAMPLES];
            let decoded = b.pop(&mut out);
            assert_eq!(
                decoded,
                Some(samples),
                "a {}ms packet was dropped instead of played",
                units * 10
            );
        }
    }

    #[test]
    fn crossing_a_pause_invents_nothing_at_all() {
        // Not merely bounded: zero. Even a few frames of extrapolation across
        // a pause is heard as a hiss immediately before every word, because it
        // lands in the silence right where the ear is waiting for speech.
        let packets = frames(6);
        let mut b = SpeakerBuffer::new().unwrap();
        for (i, p) in packets.iter().take(3).enumerate() {
            b.push(seq(i), p.clone(), false);
        }
        let mut out = vec![0.0f32; FRAME_SAMPLES];
        for _ in 0..3 {
            b.pop(&mut out);
        }

        // The next word, after a pause far longer than any lost packet.
        let resume = seq(3) + 120;
        for (i, p) in packets.iter().skip(3).enumerate() {
            b.push(resume + seq(i), p.clone(), false);
        }

        let before = b.buffered();
        b.pop(&mut out);
        assert_eq!(
            b.buffered(),
            before - 1,
            "a frame was invented crossing the pause instead of playing the word"
        );
    }

    #[test]
    fn a_word_survives_a_lost_terminator() {
        // The terminator is one packet. If losing it costs the whole next
        // word — discarded as arriving too late — then every so often a word
        // simply is not there, which is far worse than the tone it marks.
        let packets = frames(6);
        let mut b = SpeakerBuffer::new().unwrap();
        for (i, p) in packets.iter().take(3).enumerate() {
            b.push(seq(i), p.clone(), false);
        }
        let mut out = vec![0.0f32; FRAME_SAMPLES];
        while b.ready() {
            b.pop(&mut out);
        }

        // No terminator arrived, and the talker starts again from zero.
        for (i, p) in packets.iter().skip(3).enumerate() {
            b.push(seq(i), p.clone(), false);
        }
        assert!(
            b.buffered() > 0,
            "the next word was discarded because the terminator went missing"
        );
    }

    #[test]
    fn a_pause_between_sentences_is_skipped_not_synthesised() {
        // A pause is hundreds of slots that were never sent. Concealing them
        // all fills the silence with the hiss Opus decays into, and counts
        // every one as loss — which drives the target to its ceiling, where a
        // short utterance can never reach it and is never heard at all.
        let packets = frames(8);
        let mut b = SpeakerBuffer::new().unwrap();
        for (i, p) in packets.iter().take(4).enumerate() {
            b.push(seq(i), p.clone(), false);
        }
        let mut out = vec![0.0f32; FRAME_SAMPLES];
        for _ in 0..4 {
            b.pop(&mut out);
        }

        // A second and a half of silence, then talking resumes.
        let resume = seq(4) + 150;
        for (i, p) in packets.iter().skip(4).enumerate() {
            b.push(resume + seq(i), p.clone(), false);
        }

        let mut invented = 0;
        for _ in 0..40 {
            if b.buffered() == 0 {
                break;
            }
            b.pop(&mut out);
            invented += 1;
        }
        assert!(
            invented <= 4 + MAX_CONSECUTIVE_CONCEALS as usize,
            "spent {invented} frames crossing a pause"
        );
        assert_eq!(
            b.target_frames(),
            DEFAULT_TARGET_FRAMES,
            "a pause was mistaken for packet loss and deepened the buffer"
        );
    }

    #[test]
    fn the_backlog_is_spent_rather_than_hoarded() {
        // The target is a threshold for starting, not a condition for every
        // frame. Holding frames back whenever the backlog dips below it means
        // the cushion is never actually used, and every late packet becomes a
        // hole — heard as continuous choppiness on an otherwise fine link.
        let packets = frames(6);
        let mut b = SpeakerBuffer::new().unwrap();

        for (i, p) in packets.iter().take(DEFAULT_TARGET_FRAMES).enumerate() {
            b.push(seq(i), p.clone(), false);
        }
        assert!(b.ready(), "should start once the target backlog is there");

        let mut out = vec![0.0f32; FRAME_SAMPLES];
        assert!(b.pop(&mut out).is_some());

        // Now thinner than the target, but playing: it must keep going.
        assert!(b.buffered() < DEFAULT_TARGET_FRAMES);
        assert!(
            b.ready(),
            "a playing buffer refused to spend the backlog it exists to hold"
        );
        assert!(b.pop(&mut out).is_some());
    }

    #[test]
    fn running_dry_refills_before_starting_again() {
        // Once genuinely empty, stuttering along one frame at a time is worse
        // than pausing to rebuild a cushion.
        let packets = frames(4);
        let mut b = SpeakerBuffer::new().unwrap();
        for (i, p) in packets.iter().take(DEFAULT_TARGET_FRAMES).enumerate() {
            b.push(seq(i), p.clone(), false);
        }
        let mut out = vec![0.0f32; FRAME_SAMPLES];
        while b.ready() {
            b.pop(&mut out);
        }
        assert_eq!(b.buffered(), 0);

        b.push(1000, packets[3].clone(), false);
        assert!(
            !b.ready(),
            "one frame after running dry should rebuild, not restart instantly"
        );
    }

    #[test]
    fn a_straggler_is_dropped_while_a_stream_is_playing() {
        // Mid-stream, a packet whose slot has already played is useless and
        // must not rewind playback.
        //
        // Once the stream has drained the buffer accepts it instead: a low
        // number then is far more likely to be a talker starting again after a
        // lost terminator than a straggler, and replaying a few milliseconds
        // is a much smaller harm than dropping a whole word.
        let packets = frames(6);
        let mut b = SpeakerBuffer::new().unwrap();
        for (i, p) in packets.iter().enumerate() {
            b.push(1000 + seq(i), p.clone(), false);
        }
        let mut out = vec![0.0f32; FRAME_SAMPLES];
        for _ in 0..3 {
            b.pop(&mut out);
        }
        assert!(b.buffered() > 0, "should still be mid-stream");

        let before = b.buffered();
        b.push(1000, packets[0].clone(), false);
        assert_eq!(b.buffered(), before, "a straggler should not be accepted");
    }

    #[test]
    fn a_long_pause_does_not_have_to_be_concealed_frame_by_frame() {
        // Between two utterances the sequence jumps by however long the pause
        // was. Concealing every slot would invent audio for a silence nobody
        // talked through and delay the next burst until it finished.
        let packets = frames(4);
        let mut b = SpeakerBuffer::new().unwrap();
        for (i, p) in packets.iter().take(2).enumerate() {
            b.push(seq(i), p.clone(), false);
        }
        drain(&mut b, 2);

        for (i, p) in packets.iter().skip(2).enumerate() {
            b.push(50_000 + seq(i), p.clone(), false);
        }
        let mut out = vec![0.0f32; FRAME_SAMPLES];
        assert!(b.pop(&mut out).is_some());
        assert!(
            b.buffered() <= 1,
            "the pause was concealed instead of skipped"
        );
    }

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
            b.push(seq(i), f, false);
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
        b.push(seq(0), f[0].clone(), false);
        b.push(seq(2), f[2].clone(), false);
        b.push(seq(1), f[1].clone(), false);
        b.push(seq(3), f[3].clone(), false);

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
        b.push(seq(0), f[0].clone(), false);
        // frame 1 never arrives
        b.push(seq(2), f[2].clone(), false);
        b.push(seq(3), f[3].clone(), false);

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
            b.push(seq(i), p.clone(), false);
        }
        let mut out = vec![0.0f32; FRAME_SAMPLES];
        for _ in 0..2 {
            b.pop(&mut out);
        }
        // Turning up mid-stream, its slot already played: it must not rewind.
        let before = b.buffered();
        b.push(seq(0), f[0].clone(), false);
        assert_eq!(b.buffered(), before, "stale packet was buffered");
    }

    #[test]
    fn grows_the_buffer_under_sustained_loss() {
        let mut b = SpeakerBuffer::new().unwrap();
        let start = b.target_frames();
        let f = frames(40);
        // Deliver only every other frame, forcing repeated concealment.
        for (i, p) in f.iter().enumerate() {
            if i % 2 == 0 {
                b.push(seq(i), p.clone(), false);
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
        b.push(seq(0), f[0].clone(), false);
        b.push(seq(1), f[1].clone(), true);
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
            b.push(seq(i), f, true);
        }
        b.reset();
        assert_eq!(b.buffered(), 0);
        assert!(!b.is_finished());
        let mut out = vec![0.0f32; FRAME_SAMPLES];
        assert_eq!(b.pop(&mut out), None);
    }
}
