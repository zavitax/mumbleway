//! What a voice sounds like after it has crossed a bad network.
//!
//! The unit tests around the codec and the jitter buffer check that each piece
//! behaves: that FEC recovers a frame, that concealment produces something
//! finite, that the buffer grows when it should. None of them answers the
//! question a rider actually has, which is whether the person at the other end
//! could make out the words.
//!
//! This runs the whole path — encode, lose packets, deliver them late and out
//! of order, buffer, decode — and scores what comes out against what went in.
//!
//! Everything is deterministic and in-process. No sockets, no sleeping, no
//! clock: loss and jitter come from a seeded generator, so a failure here can
//! be re-run and will fail the same way.
//!
//! # Why the loss is bursty
//!
//! Uniform random loss is unrealistically kind, and kind in exactly the way
//! that flatters the two mechanisms under test. Opus's in-band FEC carries a
//! coarse copy of the previous frame in the next one, so it recovers a single
//! dropped packet almost perfectly and recovers nothing at all when the packet
//! carrying the copy is dropped too. Scatter 10% loss uniformly and almost
//! every loss is isolated; put the same 10% in bursts, as real mobile links do,
//! and FEC is defeated exactly when it is most needed.
//!
//! So the model is Gilbert–Elliott: a good state that rarely drops, a bad state
//! that mostly drops, and a small chance of moving between them each packet.

use std::collections::HashSet;

use mumbleway_core::audio::codec::{Quality, VoiceEncoder, FRAME_SAMPLES, SEQ_UNITS_PER_FRAME};
use mumbleway_core::audio::quality::intelligibility;
use mumbleway_core::audio::testsig::{self, Rng};
use mumbleway_core::audio::SpeakerBuffer;

/// A two-state loss model.
///
/// `bad_to_good` being much larger than `good_to_bad` is what makes bursts
/// short and rare rather than the link spending half its life broken.
struct Network {
    rng: Rng,
    in_bad: bool,
    good_to_bad: f32,
    bad_to_good: f32,
    /// Drop chance in each state.
    good_loss: f32,
    bad_loss: f32,
    /// How many packets a delivery may be delayed by.
    jitter_packets: usize,
    /// A fixed impairment instead of the random one, when set.
    pattern: Option<Pattern>,
    /// Packets offered so far, which the pattern counts against.
    offered: usize,
}

/// A deterministic impairment, for comparisons that must differ in exactly one
/// way.
///
/// The random model answers "how bad is a 15% link". It cannot answer "is a
/// late packet worth more than a lost one", because two draws from it differ
/// in *which* packets were hit as well as in what happened to them, and the
/// answer that comes back is a mixture of the effect and the luck. Here the
/// same packets are hit both times and only their fate changes.
#[derive(Clone, Copy)]
struct Pattern {
    /// Packets between the start of one impairment and the next.
    period: usize,
    /// Consecutive packets impaired at the start of each period.
    ///
    /// Three, in practice, rather than one. A single missing frame is the case
    /// Opus FEC was built for and recovers almost perfectly, so losing one in
    /// every seven barely differs from losing none — the contrast this is
    /// meant to draw only appears once the gap is wider than the buffer will
    /// conceal.
    span: usize,
    /// How late the impaired packets are, or `None` if they never arrive.
    delay: Option<usize>,
}

impl Network {
    /// A link that loses roughly `loss` of packets, in bursts.
    fn new(seed: u64, loss: f32, jitter_packets: usize) -> Self {
        // Bursts of about five packets, which is 100 ms — the length of a
        // fade under a bridge or behind a lorry.
        let bad_to_good = 0.2;
        let bad_loss = 0.9;
        let good_loss = 0.001;
        // Solve for the good->bad rate that gives the requested average, given
        // how long a burst lasts once it starts.
        let good_to_bad = if loss <= good_loss {
            0.0
        } else {
            (loss * bad_to_good) / (bad_loss - loss).max(0.01)
        };

        Self {
            rng: Rng::new(seed),
            in_bad: false,
            good_to_bad,
            bad_to_good,
            good_loss,
            bad_loss,
            jitter_packets,
            pattern: None,
            offered: 0,
        }
    }

    /// A link that impairs the same packets every time, and only those.
    fn patterned(period: usize, span: usize, delay: Option<usize>) -> Self {
        let mut net = Self::new(0, 0.0, 0);
        net.pattern = Some(Pattern {
            period,
            span,
            delay,
        });
        net
    }

    /// Whether this packet is lost, and how many packets late it arrives.
    fn deliver(&mut self) -> Option<usize> {
        if let Some(p) = self.pattern {
            let i = self.offered;
            self.offered += 1;
            return if i % p.period < p.span {
                p.delay
            } else {
                Some(0)
            };
        }

        self.in_bad = if self.in_bad {
            self.rng.unit() >= self.bad_to_good
        } else {
            self.rng.unit() < self.good_to_bad
        };

        let loss = if self.in_bad {
            self.bad_loss
        } else {
            self.good_loss
        };
        if self.rng.unit() < loss {
            return None;
        }
        Some(if self.jitter_packets == 0 {
            0
        } else {
            (self.rng.unit() * self.jitter_packets as f32) as usize
        })
    }
}

/// What the far end heard, next to what it should have heard.
///
/// # Alignment is recorded, not recovered
///
/// The obvious way to line the two recordings up is to cross-correlate them
/// and take the best offset, and that was the first attempt. It cannot work,
/// and the reason is worth keeping: there is no single offset. The buffer
/// gives up on a gap it cannot plausibly conceal and jumps to whatever it is
/// holding, so a burst of loss *skips slots* — after it, output frame n is no
/// longer stream slot n, and the lag has changed partway through the
/// recording. A search over a signal that also has silence in its holes then
/// locks on to the wrong lag and reports near-total unintelligibility for a
/// link that was merely poor. That is what produced scores of 0.99, 0.65,
/// 0.04, 0.38 for steadily worsening loss.
///
/// So the buffer is asked instead. It reports the slot each frame it hands out
/// belongs to, and the reference is assembled frame by frame from the source
/// audio that should have been playing at that moment. The two recordings are
/// aligned by construction and there is nothing left to search for.
struct Outcome {
    /// What should have been heard, in the order it was heard.
    reference: Vec<f32>,
    /// What was heard.
    played: Vec<f32>,
    /// Frames the sender encoded.
    sent_frames: usize,
    /// Source frames the play head reached at all, however they were filled.
    ///
    /// The interesting members of this set are the ones missing from it.
    /// Concealment fills a slot; an underrun fills it with silence; both are
    /// still moments the listener spent on that part of the sentence, and the
    /// clarity score judges how well. What is *not* here is the slots the
    /// buffer gave up on and jumped over — audio that did not merely arrive
    /// badly but never happened, taking its share of the timeline with it.
    covered: HashSet<u64>,
}

impl Outcome {
    /// How much of what was said arrived at all, 0..1.
    ///
    /// Deliberately not the buffer's decoded-frame count, which would invert
    /// the result this file exists to demonstrate. Opus reconstructs an
    /// isolated loss from the FEC copy carried in the following packet, and
    /// what comes out is real audio by any standard a listener would apply —
    /// but the buffer files it under concealment, because from its side that
    /// is what happened. Score by decode count and scattered loss, where FEC
    /// works, is punished harder than bursty loss, where it does not.
    fn delivered(&self) -> f32 {
        if self.sent_frames == 0 {
            return 0.0;
        }
        (self.covered.len() as f32 / self.sent_frames as f32).min(1.0)
    }

    /// How clearly the audio that did arrive came through.
    fn clarity(&self) -> f32 {
        intelligibility(&self.reference, &self.played)
    }

    /// Both together, which is the thing a listener actually experiences.
    ///
    /// They have to be measured separately and then combined, because either
    /// alone is high on a call nobody could hold. Clarity is scored against
    /// the audio the play head actually reached, so a link that skipped a
    /// third of the words scores well on it — the two thirds that arrived were
    /// perfect. Delivery counts how much of the sentence was reached without
    /// caring whether it was audible. A link is only good when both are.
    fn score(&self) -> f32 {
        self.clarity() * self.delivered()
    }
}

/// Encodes `pcm`, sends it across `net`, and reports what the far end played.
///
/// Reordering falls out of the delay rather than being modelled separately: a
/// packet delayed by two arrives after packets that were sent later, which is
/// what reordering is.
fn across(pcm: &[f32], net: &mut Network) -> Outcome {
    let mut encoder = VoiceEncoder::new(Quality::Balanced).expect("encoder");
    let mut buffer = SpeakerBuffer::new().expect("buffer");
    let mut scratch = Vec::new();

    let frames = pcm.len() / FRAME_SAMPLES;
    let mut out = Outcome {
        reference: Vec::with_capacity(pcm.len()),
        played: Vec::with_capacity(pcm.len()),
        sent_frames: frames,
        covered: HashSet::new(),
    };
    // The slot the listener's clock says is due next, which keeps moving
    // whether or not the buffer has anything for it.
    let mut due: Option<u64> = None;

    let mut encoded: Vec<(u64, Vec<u8>)> = Vec::with_capacity(frames);
    for f in 0..frames {
        let start = f * FRAME_SAMPLES;
        let opus = encoder
            .encode(&pcm[start..start + FRAME_SAMPLES])
            .expect("encode");
        encoded.push((f as u64 * SEQ_UNITS_PER_FRAME, opus));
    }

    // Packets waiting to be delivered, as (arrive_at_index, sequence, payload).
    let mut in_flight: Vec<(usize, u64, Vec<u8>)> = Vec::new();

    // Two things have to be right here, and each was wrong once.
    //
    // The output device is the clock, and it does not wait. It asks for a
    // period every period, and plays silence when the buffer has nothing ready.
    // Letting the consumer wait for the buffer instead hid a real effect and
    // invented one: SpeakerBuffer raises its target backlog under loss, as it
    // should, and once the target exceeded what the harness could sustain, pop
    // returned None for ever and the recording stopped — so middling loss
    // scored worse than severe loss.
    //
    // And the device starts late; that lateness *is* the backlog. Queuing
    // frames up front while still popping from the first iteration consumes the
    // queue before the first real packet arrives, leaving the run one-in-one-out
    // with nothing in hand. A buffer with no backlog absorbs nothing, so every
    // reordered packet underran and reordering measured as several times worse
    // than losing the same packets outright — the opposite of what a jitter
    // buffer is for.
    for (f, (seq, payload)) in encoded.iter().enumerate() {
        if let Some(delay) = net.deliver() {
            in_flight.push((f + delay, *seq, payload.clone()));
        }

        // Hand over everything that has come due.
        in_flight.retain(|(arrives, seq, payload)| {
            if *arrives <= f {
                buffer.push(*seq, payload.clone(), false);
                false
            } else {
                true
            }
        });

        if f < PRIME_FRAMES {
            continue;
        }
        device_period(&mut buffer, &mut scratch, pcm, &mut out, &mut due, true);
    }

    // Whatever is still in the air arrives. The sender running out of things
    // to say is not the network dropping the last few packets.
    for (_, seq, payload) in in_flight.drain(..) {
        buffer.push(seq, payload, false);
    }

    // Drain what the buffer is still holding, so the tail is scored rather
    // than truncated.
    let mut idle = 0;
    while idle < 60 {
        if device_period(&mut buffer, &mut scratch, pcm, &mut out, &mut due, false) {
            idle = 0;
        } else {
            idle += 1;
        }
    }

    out
}

/// One period of the output device: play whatever is ready, or silence.
///
/// Returns whether a frame was played, which is how the drain loop knows the
/// buffer has finished. `record_silence` is off during the drain, where an
/// empty buffer means the call ended rather than that the listener is sitting
/// through a hole.
fn device_period(
    buffer: &mut SpeakerBuffer,
    scratch: &mut Vec<f32>,
    pcm: &[f32],
    out: &mut Outcome,
    due: &mut Option<u64>,
    record_silence: bool,
) -> bool {
    // Through `ready` rather than straight to `pop`, because that is how the
    // mixer asks: a buffer that has run dry rebuilds a cushion before starting
    // again instead of stuttering along one frame at a time, and a harness
    // that skipped the gate would not see the rebuild at all.
    let played = if buffer.ready() {
        buffer.pop(scratch)
    } else {
        if buffer.buffered() > 0 {
            buffer.note_waiting();
        }
        None
    };

    match played {
        Some(n) => {
            let slot = buffer.play_slot().expect("a frame was played but no slot");
            push_reference(out, pcm, slot, n);
            out.played.extend_from_slice(&scratch[..n]);
            *due = Some(slot + SEQ_UNITS_PER_FRAME);
            true
        }
        None => {
            // The device asked and got nothing, so it plays silence — and time
            // moves on regardless, so the next slot falls due whether the
            // buffer is ready for it or not. Recording that against the audio
            // that should have been there is what makes an underrun cost
            // something instead of quietly shortening both recordings by the
            // same amount, which would score as no damage at all.
            if record_silence {
                if let Some(slot) = *due {
                    push_reference(out, pcm, slot, FRAME_SAMPLES);
                    let len = out.played.len();
                    out.played.resize(len + FRAME_SAMPLES, 0.0);
                    *due = Some(slot + SEQ_UNITS_PER_FRAME);
                }
            }
            false
        }
    }
}

/// Appends the source audio belonging to one sequence slot, and records that
/// the listener's timeline reached it.
///
/// Past the end is silence rather than a panic, and is not counted as reached:
/// the play head can run beyond the last frame sent while the buffer conceals
/// its way to a stop, and those slots are not part of what was said.
fn push_reference(out: &mut Outcome, pcm: &[f32], slot: u64, n: usize) {
    let start = (slot / SEQ_UNITS_PER_FRAME) as usize * FRAME_SAMPLES;
    match pcm.get(start..start + n) {
        Some(frame) => {
            out.covered.insert(slot);
            out.reference.extend_from_slice(frame);
        }
        None => {
            let len = out.reference.len();
            out.reference.resize(len + n, 0.0);
        }
    }
}

/// Periods queued before the output device starts, so the buffer is not asked
/// for audio it has not been given yet.
const PRIME_FRAMES: usize = 20;

/// Mean score over several links with the same character but different draws.
///
/// One seed is one sample, and a burst model is high-variance by construction —
/// whether a burst lands on a vowel or on a gap between words changes the score
/// more than a few percent of loss does. Averaging a handful of draws measures
/// the link rather than the luck.
fn score_over_seeds(sent: &[f32], loss: f32, jitter: usize, seeds: &[u64]) -> f32 {
    let mut total = 0.0;
    for &seed in seeds {
        let mut net = Network::new(seed, loss, jitter);
        total += across(sent, &mut net).score();
    }
    total / seeds.len() as f32
}

fn speech_sample(seconds: usize) -> Vec<f32> {
    testsig::speech(48_000 * seconds, 130.0, 0.5)
}

#[test]
fn a_clean_link_delivers_the_voice_intact() {
    // The control. If this does not score high, nothing below means anything —
    // a low score everywhere would look like network damage and be a bug in the
    // harness.
    let sent = speech_sample(3);
    let mut net = Network::new(1, 0.0, 0);
    let heard = across(&sent, &mut net);

    assert!(
        !heard.played.is_empty(),
        "nothing came out of a perfect link"
    );
    assert!(
        heard.delivered() > 0.99,
        "a perfect link delivered only {:.3} of what was sent",
        heard.delivered()
    );
    let score = heard.clarity();
    assert!(score > 0.85, "a perfect link scored {score}");
}

#[test]
fn intelligibility_falls_as_the_link_gets_worse() {
    // The assertion every other test in this file leans on: if the measure is
    // not ordered, nothing built on it means anything.
    //
    // It was parked for a while, and the two causes are both worth naming
    // because they were both harness bugs wearing the costume of a result.
    // First the consumer waited on the buffer instead of running to its own
    // clock, which made middling loss score exactly zero and severe loss score
    // well. Then `aligned` searched for one offset when there is no one offset
    // to find — the buffer skips slots after a burst it cannot conceal — and
    // gave 0.99, 0.65, 0.04, 0.38 for steadily worsening loss.
    //
    // Neither was a property of the audio. Both looked like one.
    let sent = speech_sample(3);
    const SEEDS: [u64; 4] = [7, 31, 104, 512];

    let mut previous = 1.0f32;
    let mut scores = Vec::new();
    for loss in [0.0f32, 0.05, 0.15, 0.30] {
        let score = score_over_seeds(&sent, loss, 0, &SEEDS);
        scores.push((loss, score));
        assert!(
            score <= previous + 0.03,
            "{:.0}% loss scored {score}, above the {previous} before it — {scores:?}",
            loss * 100.0
        );
        previous = score;
    }

    let worst = scores.last().unwrap().1;
    let best = scores.first().unwrap().1;
    assert!(
        best - worst > 0.1,
        "30% loss cost only {:.3} against a clean link — the model is not biting: {scores:?}",
        best - worst
    );
}

#[test]
fn a_burst_loses_more_than_the_same_loss_spread_out() {
    // The reason the model is Gilbert-Elliott rather than a coin flip. Opus
    // carries a copy of each frame in the next one, so isolated losses are
    // nearly free and consecutive ones are not. A suite that only tested
    // uniform loss would report FEC as working far better than it does on the
    // links riders actually use.
    let sent = speech_sample(3);

    let mut bursty = Network::new(11, 0.15, 0);
    let burst_score = across(&sent, &mut bursty).score();

    // Same average loss, scattered: no bad state to get stuck in.
    let mut even = Network::new(11, 0.15, 0);
    even.good_to_bad = 0.0;
    even.good_loss = 0.15;
    let even_score = across(&sent, &mut even).score();

    assert!(
        burst_score <= even_score + 0.02,
        "bursty loss scored {burst_score}, no worse than scattered {even_score}"
    );
}

#[test]
fn late_packets_are_waited_for_rather_than_lost() {
    // What the jitter buffer is for. Packets arriving a few frames late and out
    // of order should cost far less than losing them would, because the buffer
    // holds a backlog precisely so they can be put back in place.
    //
    // The same fifteen percent of packets in all three runs, so the only thing
    // that differs is what became of them.
    let sent = speech_sample(3);
    let steady = across(&sent, &mut Network::patterned(20, 3, Some(0))).score();
    let jittery = across(&sent, &mut Network::patterned(20, 3, Some(4))).score();
    let lost = across(&sent, &mut Network::patterned(20, 3, None)).score();

    // Reordering inside the backlog costs nothing, and that is the correct
    // result rather than a weak one: a four-frame spread against a twenty-frame
    // backlog is exactly the case a jitter buffer exists to absorb, and if it
    // cost anything the buffer would not be doing its job.
    //
    // An earlier version of this test asserted the opposite — that jitter must
    // show a cost — and it "passed" only because the harness had no backlog at
    // all. The assertion was measuring the harness's bug.
    assert!(
        (jittery - steady).abs() < 0.05,
        "reordering within the backlog cost {:.3}: {steady} to {jittery}",
        steady - jittery
    );
    assert!(
        jittery > lost + 0.1,
        "reordering ({jittery}) was no better than losing outright ({lost})"
    );
}

#[test]
fn the_link_model_loses_roughly_what_it_was_asked_to() {
    // A model that silently lost nothing would make every test above pass for
    // the wrong reason, and it would look exactly like a very good codec.
    for wanted in [0.05f32, 0.15, 0.30] {
        let mut net = Network::new(23, wanted, 0);
        let dropped = (0..4_000).filter(|_| net.deliver().is_none()).count();
        let got = dropped as f32 / 4_000.0;
        assert!(
            (got - wanted).abs() < wanted * 0.6 + 0.02,
            "asked for {wanted}, lost {got}"
        );
    }
}

#[test]
fn the_same_seed_gives_the_same_link() {
    let a: Vec<bool> = {
        let mut n = Network::new(99, 0.2, 3);
        (0..500).map(|_| n.deliver().is_none()).collect()
    };
    let b: Vec<bool> = {
        let mut n = Network::new(99, 0.2, 3);
        (0..500).map(|_| n.deliver().is_none()).collect()
    };
    assert_eq!(a, b, "the same seed produced a different link");
}
