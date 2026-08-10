//! Finding out what this device can run, before a rider finds out for them.
//!
//! # Why the ladder is not enough on its own
//!
//! [`super::relief::ReliefLadder`] steps down after a hundred consecutive late
//! blocks, which is one second of a call already gone wrong. On a phone that
//! cannot manage the full chain that second happens at the worst possible
//! moment — the first thing anyone says — and it happens again at every rung on
//! the way down, so the first six seconds of a conversation are spent
//! discovering something the device could have been asked about while the app
//! was still opening.
//!
//! So ask first. Run the real chain against blocks engineered to be expensive,
//! give up rungs until a block fits, and start the session there. The ladder
//! stays exactly as it is and remains the authority — this only decides where
//! it begins.
//!
//! # What "worst case" means here, and what it does not
//!
//! **The signal is synthetic and that is defensible here, unusually.**
//! `CLAUDE.md` records that synthetic signals agree with whoever wrote them, and
//! it is right about anything that measures a *decision*. This measures
//! arithmetic: how long the chain takes, which is a property of the code path
//! taken and the CPU running it rather than of whether the audio is really
//! speech.
//!
//! What the signal has to do is force the expensive path, and there is exactly
//! one that matters. DeepFilterNet picks one of four per frame from its own SNR
//! estimate, and `super::deepfilter::MIN_DB` is the reason: below it the frame
//! takes a zero mask and no decoder at all, and **85% of a real clean ride takes
//! that branch**. Measuring on quiet audio would time the cheap path and clear
//! a device that cannot run the expensive one. So the probe feeds voiced speech
//! at a moderate signal-to-noise ratio, which is what lands the model between
//! `MIN_DB` and `MAX_DF_DB` where both decoders run.
//!
//! A test asserts that this is what happens, by checking the top rung costs
//! materially more than the rung that switches the DF decoder off. If the
//! generator ever stops driving the expensive path that assertion fails, rather
//! than the probe quietly clearing every device it is asked about.
//!
//! # Why the worst block is not the worst block
//!
//! The decision uses the **second** highest block time, not the highest. A
//! single 40 ms outlier during app start is the operating system — another app
//! being killed, a page fault, the first touch of a cold cache — and starting a
//! rider three rungs down because of one scheduler event would cost them the
//! enhancer for a whole ride. Two slow blocks is the device. Both figures are
//! reported so the panel can show the one that was thrown away.

use std::sync::atomic::{AtomicU8, Ordering};
use std::time::Instant;

use super::denoise::{CaptureProcessor, NoiseProfile, FRAME_SIZE};
use super::relief::Relief;

/// Where the last probe landed, for the next worker to start from.
///
/// A process-wide static rather than a field on `AudioShared`, because the
/// probe runs while the app is opening and `AudioShared` does not exist until
/// the devices do. It also has to survive the engine being stopped and started
/// again — what a device can run does not change between calls, and paying for
/// the probe twice would be paying to learn the same thing.
///
/// Stored as **index + 1**, so 0 can mean "nothing known yet" while still
/// letting the update below be a plain numeric maximum. The obvious sentinel,
/// `u8::MAX`, is larger than every real rung and would have made "only ever
/// deeper" compare the wrong way round.
///
/// # It only ever deepens
///
/// The ladder's rule is that a rung given up is a fact about this device for
/// the rest of the session, and it never climbs back. That rule was being
/// broken by something outside the ladder: `run_worker` owns its
/// `ReliefLadder` as a local, and the worker restarts every time the devices
/// reopen — which is **per call**. So a phone that struggled its way down to
/// `NoRnnoise` during one conversation began the next one back wherever the
/// probe had left it, and had to spend another six seconds of somebody's
/// speech rediscovering the same thing.
///
/// So this is the floor, and everything that learns something writes to it:
/// the startup probe when it lands, and the ladder every time it gives a rung
/// up. A worker starts at whichever is deeper. Nothing can raise it — not a
/// second probe, not a new call, not the engine being stopped and started —
/// and only restarting the app clears it.
static DEEPEST: AtomicU8 = AtomicU8::new(0);

/// Records a rung reached, if it is deeper than anything reached before.
///
/// Returns what is in force afterwards, which is the deeper of the two — so a
/// caller that reports the return value cannot claim to have raised the floor.
pub fn record_rung(rung: Relief) -> Relief {
    let want = rung.index() + 1;
    let mut seen = DEEPEST.load(Ordering::Relaxed);
    loop {
        if seen >= want {
            // Already at least this deep. Theirs stands.
            return Relief::from_index(seen - 1).unwrap_or(rung);
        }
        match DEEPEST.compare_exchange_weak(seen, want, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return rung,
            Err(actual) => seen = actual,
        }
    }
}

/// The rung a worker should start at, or `None` while nothing is known.
pub fn start_rung() -> Option<Relief> {
    match DEEPEST.load(Ordering::Relaxed) {
        0 => None,
        i => Relief::from_index(i - 1),
    }
}

/// Forgets everything learned, as an app restart would. For tests.
pub fn forget() {
    DEEPEST.store(0, Ordering::Relaxed);
}

/// The deadline the probe dials to, in microseconds.
///
/// Below the block deadline the ladder itself uses, on purpose. Landing exactly
/// on 10 ms means every later jitter is an overrun, and the ladder would spend
/// the ride stepping anyway — which is the thing this exists to avoid. A rung
/// that measures 9 ms has a millisecond of room for the rest of the phone.
pub const PROBE_BUDGET_US: u32 = 9_000;

/// Blocks timed at each rung. 60 is 600 ms of audio.
///
/// Long enough that the expensive branch is exercised many times and a single
/// outlier can be discarded with something left, short enough that a slow phone
/// walking several rungs does not add seconds to app start.
const BLOCKS: usize = 60;

/// Discarded before timing starts, at every rung.
///
/// The first frames through a freshly levelled graph are not representative:
/// tract allocates on the first call of a shape, and the caches are cold. They
/// are also exactly the frames that would produce the outlier the second-worst
/// rule exists to survive, so dropping them is cheaper than tolerating them.
const WARMUP: usize = 10;

/// What the probe found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Probed {
    /// Where the session should start.
    pub rung: Relief,
    /// The block time the decision was made on, in microseconds — the second
    /// worst of [`BLOCKS`], at `rung`.
    pub worst_us: u32,
    /// The single worst block at `rung`, which the decision ignored.
    pub outlier_us: u32,
    /// How many rungs had to be given up. Zero on a device that keeps up.
    pub steps: u8,
    /// True when the bottom of the ladder still did not fit. The session starts
    /// there anyway — there is nothing further to give — and this is what the
    /// panel needs in order to say so rather than implying the device is fine.
    pub gave_up: bool,
}

/// Voiced speech at a moderate SNR: what keeps the enhancer on its expensive
/// path. See the module docs.
///
/// Deterministic, and not because it is tidier — a probe that measured a
/// different signal each launch would dial a different rung each launch, and a
/// rider would get a different-sounding app every time they opened it.
fn worst_case_block(at: usize) -> Vec<f32> {
    let mut out = Vec::with_capacity(FRAME_SIZE);
    let mut noise = (at as u32)
        .wrapping_mul(1_664_525)
        .wrapping_add(1_013_904_223);
    for i in 0..FRAME_SIZE {
        let t = (at + i) as f32 / 48_000.0;
        // A 120 Hz glottal-ish stack. Harmonics matter: the model is looking
        // for speech structure, and a bare sine is not it.
        let mut s = 0.0;
        for h in 1..=8 {
            let amp = 0.35 / h as f32;
            s += amp * (t * 120.0 * h as f32 * std::f32::consts::TAU).sin();
        }
        // Syllabic envelope, so the frame-to-frame SNR moves the way real
        // speech moves it rather than sitting on one branch for 600 ms.
        s *= 0.55 + 0.45 * (t * 4.0 * std::f32::consts::TAU).sin();
        // And a noise floor to keep the estimate off the ceiling. A clean
        // signal reads as high SNR and takes the *cheap* ERB-only path, which
        // is the exact mistake this generator exists to avoid.
        noise = noise.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let n = (noise >> 9) as f32 / (1 << 22) as f32 - 1.0;
        out.push(s * 0.28 + n * 0.03);
    }
    out
}

/// Applies a rung to a chain that is already built.
///
/// Neither stage is rebuilt: the enhancer's rungs are threshold changes and the
/// processor's are flags. That is what makes walking the ladder affordable at
/// start — the model is loaded once and levelled in place.
fn level(enhancer: &mut super::deepfilter::Enhancer, processor: &mut CaptureProcessor, r: Relief) {
    // How far down the enhancer already is, in the same units `enhancer_rungs`
    // counts. Read from its own state rather than tracked here, so the two
    // cannot disagree about a step that did not take.
    let done = |e: super::deepfilter::Effort| match e {
        super::deepfilter::Effort::Full => 0u8,
        super::deepfilter::Effort::Reduced => 1,
        super::deepfilter::Effort::ErbOnly => 2,
        super::deepfilter::Effort::Bypassed => 3,
    };
    while done(enhancer.effort()) < r.enhancer_rungs() {
        enhancer.step_down();
    }
    processor.set_relief(r.skip_pitch(), r.skip_rnnoise());
}

/// Times [`BLOCKS`] worst-case blocks through the chain at its current level.
///
/// Returns (second worst, worst) in microseconds.
fn measure(
    enhancer: &mut super::deepfilter::Enhancer,
    processor: &mut CaptureProcessor,
    at: &mut usize,
) -> (u32, u32) {
    let mut times: Vec<u32> = Vec::with_capacity(BLOCKS);
    for i in 0..(BLOCKS + WARMUP) {
        let mut block = worst_case_block(*at);
        *at += FRAME_SIZE;
        let started = Instant::now();
        enhancer.process(&mut block);
        let _ = processor.process(&mut block);
        let us = started.elapsed().as_micros().min(u32::MAX as u128) as u32;
        if i >= WARMUP {
            times.push(us);
        }
    }
    times.sort_unstable();
    let worst = times.last().copied().unwrap_or(0);
    let second = times
        .get(times.len().saturating_sub(2))
        .copied()
        .unwrap_or(worst);
    (second, worst)
}

/// Walks the ladder until a block fits, and says where to start.
///
/// Slow — it loads the model and runs up to a few hundred blocks — so it
/// belongs off the audio thread and off the UI thread, at app start.
pub fn probe(budget_us: u32) -> Probed {
    let got = measure_ladder(budget_us);
    Probed {
        // Never shallower than anything already reached, so a probe cannot
        // undo a rung an earlier call gave up.
        rung: record_rung(got.rung),
        ..got
    }
}

/// The measurement on its own, without recording it.
///
/// Split out so the tests can assert what the walk decides without fighting
/// each other over a process-wide static that is, by design, writable once.
fn measure_ladder(budget_us: u32) -> Probed {
    let mut enhancer = super::deepfilter::Enhancer::new();
    // The profile the probe runs under is the strictest one, because it is the
    // one with the most stages switched on. Clearing a device on `Light` and
    // then having a rider pick `Helmet` would put them over budget with no
    // measurement having been wrong.
    let mut processor = CaptureProcessor::new(NoiseProfile::Helmet);
    let mut at = 0usize;

    let mut rung = Relief::None;
    let mut steps = 0u8;
    loop {
        level(&mut enhancer, &mut processor, rung);
        let (worst_us, outlier_us) = measure(&mut enhancer, &mut processor, &mut at);
        if worst_us <= budget_us {
            return Probed {
                rung,
                worst_us,
                outlier_us,
                steps,
                gave_up: false,
            };
        }
        let Some(next) = rung.weaker() else {
            // The bottom, and still over. Start there and say so.
            return Probed {
                rung,
                worst_us,
                outlier_us,
                steps,
                gave_up: true,
            };
        };
        rung = next;
        steps += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The generator has to drive the enhancer's expensive path, or the whole
    /// probe measures the cheap one and clears devices it should not.
    ///
    /// Asserted by cost rather than by reading the model's branch: the DF
    /// decoder is 19.2 MB of the model's 20, so a rung that switches it off has
    /// to be visibly cheaper. If this fails, the signal has drifted onto the
    /// zero-mask branch and the probe has quietly stopped measuring anything.
    #[test]
    fn the_probe_signal_reaches_the_expensive_path() {
        let mut enhancer = super::super::deepfilter::Enhancer::new();
        if !enhancer.active() {
            return; // no model in this build; nothing to assert about it
        }
        let mut processor = CaptureProcessor::new(NoiseProfile::Helmet);
        let mut at = 0usize;

        level(&mut enhancer, &mut processor, Relief::None);
        let (full, _) = measure(&mut enhancer, &mut processor, &mut at);

        // ErbOnly is where the DF decoder stops running at all.
        level(&mut enhancer, &mut processor, Relief::EnhancerLight);
        let (light, _) = measure(&mut enhancer, &mut processor, &mut at);

        assert!(
            full > light,
            "the probe signal is not reaching the DF decoder: \
             full effort {full} us against ErbOnly {light} us"
        );
    }

    /// The floor only ever deepens: nothing can put a rider back on a rung
    /// their device has already been taken off.
    ///
    /// **The direction is the whole rule.** Degrading further at runtime is
    /// expected and allowed — the ladder does that whenever a device turns out
    /// to be worse than it looked. What must never happen is the other way:
    /// no probe, no new call, and no stop-and-start of the engine may raise
    /// this. Only restarting the app clears it.
    ///
    /// One test, because the static is process-wide and two tests asserting
    /// against it would race each other by construction.
    #[test]
    fn the_floor_deepens_and_never_lifts() {
        forget();
        assert_eq!(start_rung(), None, "nothing learned yet is not a rung");

        // A probe lands somewhere.
        assert_eq!(probe(u32::MAX).rung, Relief::None);
        assert_eq!(start_rung(), Some(Relief::None));

        // Runtime finds the device worse than the probe did. That is allowed,
        // and it is what the ladder does on a hard call.
        assert_eq!(record_rung(Relief::NoRnnoise), Relief::NoRnnoise);
        assert_eq!(start_rung(), Some(Relief::NoRnnoise));

        // Now nothing may lift it. Not a shallower rung reported by anything…
        assert_eq!(
            record_rung(Relief::EnhancerReduced),
            Relief::NoRnnoise,
            "a shallower rung raised the floor"
        );
        assert_eq!(start_rung(), Some(Relief::NoRnnoise));

        // …and not a whole second probe, however generous the moment it caught.
        assert_eq!(
            probe(u32::MAX).rung,
            Relief::NoRnnoise,
            "a second probe put the rider back on a rung they had lost"
        );
        assert_eq!(start_rung(), Some(Relief::NoRnnoise));

        // Deeper still is fine, all the way to the bottom.
        assert_eq!(record_rung(Relief::EnhancerOff), Relief::EnhancerOff);
        assert_eq!(start_rung(), Some(Relief::EnhancerOff));

        // And only a restart clears it.
        forget();
        assert_eq!(start_rung(), None);
    }

    /// Every rung has to survive the trip through an atomic and the FFI, or a
    /// stored index means a different rung than the one that was measured.
    #[test]
    fn every_rung_round_trips_through_its_index() {
        let mut r = Relief::None;
        loop {
            assert_eq!(Relief::from_index(r.index()), Some(r));
            match r.weaker() {
                Some(next) => r = next,
                None => break,
            }
        }
        assert_eq!(
            Relief::from_index(u8::MAX),
            None,
            "the unset marker is not a rung"
        );
    }

    /// A budget nothing can meet has to come back at the bottom rung and admit
    /// it, rather than looping or reporting a rung it did not reach.
    #[test]
    fn an_impossible_budget_stops_at_the_bottom_and_says_so() {
        let got = measure_ladder(0);
        assert!(got.gave_up);
        assert_eq!(got.rung, Relief::EnhancerOff);
        assert_eq!(
            got.rung.weaker(),
            None,
            "it stopped somewhere with a rung below it"
        );
    }

    /// And a budget anything can meet must not give anything up.
    #[test]
    fn a_generous_budget_keeps_the_whole_chain() {
        let got = measure_ladder(u32::MAX);
        assert_eq!(got.rung, Relief::None);
        assert_eq!(got.steps, 0);
        assert!(!got.gave_up);
    }
}
