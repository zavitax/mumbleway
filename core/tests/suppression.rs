//! What the chain transmits when nobody is talking.
//!
//! This is the fault the whole suppression effort exists to fix, stated as a
//! number. A rider at speed, or beside a row of idling motorcycles at a
//! junction, is not talking — and everyone else on the channel hears them
//! anyway, because every stage of the capture chain judges the signal by
//! *level* and wind is loud. The gate opens for a gust exactly as it opens for
//! a word.
//!
//! So: noise and nothing else, through the real chain, in voice-activated
//! mode, swept over kind and level. Every block it decides to transmit is a
//! block the channel spends on weather.
//!
//! # What it found
//!
//! Not what it was written to find. The chain rejects synthesised wind,
//! engines, traffic and randomly-shaped unknown noise *completely*, at every
//! level up to 0.85 of full scale — so the argument that a level-based chain
//! cannot tell a gust from a word, which is what the speech-focused gate was
//! to be built on, is not supported by anything measured here.
//!
//! The one leak is loud music, at a couple of percent of blocks, and it is the
//! case the proposed gate could not have fixed: music is harmonic, at a human
//! pitch, modulated at something near speech rhythm.
//!
//! Read the caveat in [`noise_alone_transmits_nothing`] before treating that
//! as the end of the matter. Synthesised wind is not recorded wind.
//!
//! # Why the noise is synthesised rather than recorded
//!
//! A corpus of three wav files can be passed by a chain that has learnt those
//! three wav files, and there is no way to tell from inside the suite that
//! that is what happened. Every case here is generated from a seed, and the
//! seeded sweeps run the same case at levels and shapes nobody chose by hand —
//! including [`testsig::unknown`], which exists specifically so that surviving
//! wind, engines and music is not the same as passing.

use mumbleway_core::audio::denoise::CaptureProcessor;
use mumbleway_core::audio::testsig;
use mumbleway_core::audio::{Enhancer, NoiseProfile};

/// RNNoise works on fixed 10 ms blocks at 48 kHz, and so does the chain.
const BLOCK: usize = 480;
const RATE: usize = 48_000;

/// Share of blocks the chain would put on the wire, 0..1.
///
/// Warm-up blocks are excluded because the chain says itself that it does not
/// yet trust its own answer, and counting them would put a fixed 150 ms of
/// noise into every measurement regardless of what was being measured.
fn transmitted_share(profile: NoiseProfile, signal: &[f32]) -> f32 {
    share_through(profile, signal, false)
}

/// The same, with the enhancer in front — which is what the app runs.
///
/// **This suite measured the suppressor alone until 2026-08-15**, so its clean
/// zero described a chain nobody ships. `engine.rs` runs `Enhancer::process`
/// immediately before `CaptureProcessor::suppress`.
fn shipping_share(profile: NoiseProfile, signal: &[f32]) -> f32 {
    share_through(profile, signal, true)
}

fn share_through(profile: NoiseProfile, signal: &[f32], enhance: bool) -> f32 {
    let mut chain = CaptureProcessor::new(profile);
    let mut enhancer = Enhancer::new();
    let mut block = [0.0f32; BLOCK];
    let mut sent = 0usize;
    let mut counted = 0usize;

    for chunk in signal.chunks_exact(BLOCK) {
        block.copy_from_slice(chunk);
        if enhance {
            enhancer.process(&mut block);
        }
        let analysis = chain.process(&mut block);
        if analysis.warming_up {
            continue;
        }
        counted += 1;
        if analysis.speaking {
            sent += 1;
        }
    }
    if counted == 0 {
        return 0.0;
    }
    sent as f32 / counted as f32
}

/// The noises a rider is surrounded by, at the levels they arrive at.
///
/// Named as loudly as they are because that is the case that fails: a chain
/// that rejects quiet wind and transmits a gust has not solved anything. The
/// amplitudes are peak-ish rather than RMS, and 0.3 is a long way from quiet.
fn cases(seconds: usize, seed: u64, amp: f32) -> Vec<(String, Vec<f32>)> {
    let len = RATE * seconds;
    let wind = testsig::wind(len, amp, seed);
    let engine = testsig::engine(len, 42.0, amp * 1.15, seed + 1);
    let traffic = testsig::traffic(len, amp, seed + 2);
    let music = testsig::music(len, amp * 0.85, seed + 3);
    let unknown = testsig::unknown(len, amp, seed + 4);

    // The junction: several bikes idling and the rider's own helmet noise.
    let mut junction = testsig::traffic(len, amp, seed + 5);
    for (j, w) in junction
        .iter_mut()
        .zip(testsig::wind(len, amp * 0.5, seed + 6))
    {
        *j = (*j + w).clamp(-1.0, 1.0);
    }

    // Everything at once, which is a motorway service station.
    let mut everything = junction.clone();
    for (e, m) in everything
        .iter_mut()
        .zip(testsig::music(len, amp * 0.65, seed + 7))
    {
        *e = (*e + m).clamp(-1.0, 1.0);
    }

    [
        ("wind", wind),
        ("engine", engine),
        ("traffic", traffic),
        ("music", music),
        ("unknown", unknown),
        ("junction", junction),
        ("everything", everything),
    ]
    .into_iter()
    .map(|(name, signal)| (format!("{name}@{amp:.2}"), signal))
    .collect()
}

/// The kinds this suite has measured the chain as already rejecting outright.
///
/// Music is not here. See [`loud_music_is_not_mistaken_for_a_voice`].
const REJECTED_TODAY: [&str; 4] = ["wind", "engine", "traffic", "unknown"];

#[test]
fn noise_alone_transmits_nothing() {
    // This was written as the acceptance test for work that has not been done
    // — the pitch-constrained harmonicity gate — on the argument that the
    // transmit decision is RNNoise's VAD plus an SNR margin above a tracked
    // floor, and that neither can tell a gust from a word.
    //
    // It passed. Not marginally: zero blocks transmitted, for wind, engines,
    // traffic and randomly-shaped unknown noise, at every level from a quarter
    // of full scale to 0.85, which is a genuinely loud gust. So the argument
    // was wrong, or at least unproven, and the gate was about to be built on
    // it. RNNoise turns out to reject aperiodic broadband noise perfectly well
    // on its own, and the floor tracker handles the rest.
    //
    // What this is now is a regression guard, which is worth more than an
    // acceptance test would have been: it holds the line while the chain
    // changes underneath it.
    //
    // The caveat is load-bearing and must not be lost. Synthesised wind is not
    // recorded wind, and the generator was written by the same hand that
    // formed the hypothesis, which is exactly the bias that produces a suite
    // agreeing with its author. This says the fault does not reproduce here;
    // it cannot say it does not happen on a bike.
    let mut worst = 0.0f32;
    let mut report = Vec::new();
    // Swept over level as well as kind, because "loud" is the whole of the
    // complaint: a chain that rejects quiet wind and transmits a gust has not
    // solved anything, and one hand-picked amplitude cannot tell the two apart.
    for amp in [0.25f32, 0.5, 0.85] {
        for (name, signal) in cases(15, 7, amp) {
            if !REJECTED_TODAY.iter().any(|k| name.starts_with(k)) {
                continue;
            }
            let share = transmitted_share(NoiseProfile::Helmet, &signal);
            if share > 0.0 {
                report.push(format!("{name} {:.1}%", share * 100.0));
            }
            worst = worst.max(share);
        }
    }
    assert_eq!(worst, 0.0, "noise was transmitted: {}", report.join(", "));
}

/// The same noise, through the chain the app actually runs.
///
/// **This is a defect being recorded, not a bar being met.** The test above
/// puts zero blocks of wind, engine, traffic and unknown noise on the wire, and
/// did so for months — but it measured the suppressor without the enhancer in
/// front of it, which is not what anybody runs. With the enhancer where
/// `engine.rs` has it, a quarter to a half of the same noise transmits.
///
/// The mechanism is in [`what_the_enhancer_does_to_the_shape_of_noise`] and is
/// not what it looks like. Removing noise does not make noise leak; collapsing
/// the *floor* does. A speech enhancer fed no speech emits near-silence between
/// its residues, the minimum-statistics floor latches onto the silence, and
/// everything above it reads as signal. Measured on wind at half scale: the
/// level fell 15 dB when the enhancer went in and the floor fell 54, so the SNR
/// the gate keys on went from 9 dB to 43.
///
/// The number here is therefore a ceiling to drive down, and this test exists
/// to stop it getting worse in the meantime. **Do not read a pass as good
/// news.** The target is the zero above.
#[test]
fn how_much_noise_the_shipping_chain_leaks() {
    let mut worst = 0.0f32;
    let mut report = Vec::new();
    // A narrower sweep than the suppressor-alone test above, because the
    // enhancer costs about seven times the runtime and the leak does not
    // depend on level: 22, 26 and 27% for wind at a quarter, a half and 0.85
    // of full scale. The two extremes are kept and the middle dropped.
    for amp in [0.25f32, 0.85] {
        for (name, signal) in cases(10, 7, amp) {
            if !REJECTED_TODAY.iter().any(|k| name.starts_with(k)) {
                continue;
            }
            let share = shipping_share(NoiseProfile::Helmet, &signal);
            if share > 0.0 {
                report.push(format!("{name} {:.1}%", share * 100.0));
            }
            worst = worst.max(share);
        }
    }
    // Measured at 58.7% on 2026-08-15. The margin is for seed and platform
    // wobble, not for headroom to regress into.
    assert!(
        worst <= 0.65,
        "the shipping chain leaked more noise than the recorded defect: {}",
        report.join(", ")
    );
}

#[test]
#[ignore = "the one noise the chain does leak; see the comment"]
fn loud_music_is_not_mistaken_for_a_voice() {
    // The single real false positive this suite found, and it is the awkward
    // one. At 0.85 amplitude, 2.2% of blocks of music go out — a couple of
    // seconds a minute of somebody's stereo on the channel.
    //
    // Awkward because the fix the plan proposed does not fix it. A
    // pitch-constrained harmonicity gate rejects wind for being aperiodic and
    // engines for firing below the human pitch range; music is harmonic, sits
    // squarely inside that range, and is modulated at something close to
    // speech rhythm. Every discriminator that catches wind waves music
    // through, which is why RNNoise lets it past in the first place.
    //
    // So this stays ignored and stays named, rather than being folded into the
    // test above at a threshold that would quietly accept it. Whatever closes
    // it will not be the work that was planned.
    let mut worst = 0.0f32;
    let mut report = Vec::new();
    for amp in [0.25f32, 0.5, 0.85] {
        for (name, signal) in cases(15, 7, amp) {
            if !name.starts_with("music") {
                continue;
            }
            let share = transmitted_share(NoiseProfile::Helmet, &signal);
            report.push(format!("{name} {:.1}%", share * 100.0));
            worst = worst.max(share);
        }
    }
    assert_eq!(worst, 0.0, "music was transmitted: {}", report.join(", "));
}

#[test]
fn the_suite_can_tell_noise_from_speech_in_the_first_place() {
    // The control, and it is not a formality: `noise_alone_transmits_nothing`
    // can be made to pass by a chain that transmits nothing at all, and that
    // chain would be far worse than the one we have. This is the assertion
    // that stops the target above being reached the wrong way.
    let speech = testsig::speech(RATE * 5, 130.0, 0.5);
    let share = transmitted_share(NoiseProfile::Helmet, &speech);
    assert!(
        share > 0.5,
        "only {:.1}% of clear speech was transmitted",
        share * 100.0
    );
}

#[test]
fn speech_survives_the_noise_it_is_buried_in() {
    // The other half of the same guard. A gate hard enough to reject a gust
    // must still let a rider through while the gust is happening, which is the
    // whole difficulty: they are not alternatives, they are simultaneous.
    let speech = testsig::speech(RATE * 5, 130.0, 0.5);
    for snr_db in [12.0f32, 6.0] {
        let noisy = testsig::mix(&speech, &testsig::wind(speech.len(), 1.0, 21), snr_db);
        let share = transmitted_share(NoiseProfile::Helmet, &noisy);
        assert!(
            share > 0.35,
            "at {snr_db} dB over wind only {:.1}% of speech got through",
            share * 100.0
        );
    }
}

#[test]
fn how_much_speech_gets_through_over_wind() {
    // A recorded curve, not a claim about the margin relief.
    //
    // It was written as one — named for periodicity buying back the speech the
    // wind was taking — and the A/B says it does no such thing here. With the
    // Helmet relief at 6 dB the curve is 97.7 / 97.7 / 97.3 / 97.5; with it at
    // zero it is 97.5 / 97.5 / 96.9 / 96.9. Two tenths of a percent is not an
    // effect, and a test named for one would have asserted something false
    // every time it passed.
    //
    // Why it cannot see the effect is the useful part. `testsig::mix` scales
    // the wind against the whole utterance, silences included, so during the
    // speech itself the voice sits comfortably above the wind and the tracked
    // floor never climbs over it. The condition the relief exists for — the
    // floor lifted above the rider's own voice for a whole sentence — is not
    // what this signal contains, at any SNR the mix can be asked for.
    //
    // So this stands as a baseline: a change that takes any of it away fails
    // here rather than on a motorway.
    let speech = testsig::speech(RATE * 5, 130.0, 0.5);
    let mut curve = Vec::new();
    for snr_db in [12.0f32, 8.0, 5.0, 2.0] {
        let noisy = testsig::mix(&speech, &testsig::wind(speech.len(), 1.0, 21), snr_db);
        let share = transmitted_share(NoiseProfile::Helmet, &noisy);
        curve.push((snr_db, share));
        println!(
            "{snr_db:>5} dB over wind -> {:.1}% transmitted",
            share * 100.0
        );
    }

    for (snr_db, share) in &curve {
        assert!(
            *share > 0.90,
            "at {snr_db} dB over wind only {:.1}% got through: {curve:?}",
            share * 100.0
        );
    }
}

#[test]
fn a_whisper_is_not_thrown_away() {
    // The case a harmonicity gate is most likely to lose, recorded here
    // *before* that gate exists so it cannot be introduced as a regression
    // that nobody notices. A rider talking quietly is still talking.
    let quiet = testsig::whisper(RATE * 5, 0.25, 4);
    let share = transmitted_share(NoiseProfile::Standard, &quiet);
    assert!(
        share > 0.15,
        "only {:.1}% of a whisper was transmitted",
        share * 100.0
    );
}

#[test]
fn the_answer_does_not_depend_on_the_draw() {
    // Every generator here is seeded, and a suite that only ever ran one seed
    // would be a corpus of seven files with extra steps. If the chain's
    // behaviour swings wildly between draws of the same *kind* of noise, the
    // numbers above mean nothing and this is what says so.
    let mut shares = Vec::new();
    for seed in [3u64, 91, 500] {
        let signal = testsig::wind(RATE * 10, 0.30, seed);
        shares.push(transmitted_share(NoiseProfile::Helmet, &signal));
    }
    let spread = shares.iter().cloned().fold(0.0f32, f32::max)
        - shares.iter().cloned().fold(1.0f32, f32::min);
    assert!(
        spread < 0.5,
        "three draws of the same wind behaved quite differently: {shares:?}"
    );
}

/// Why enhanced noise leaks when raw noise does not.
///
/// **The finding this file exists to record, as of 2026-08-15.** Adding the
/// enhancer — which the shipping chain has and this suite did not — turned a
/// clean zero into 22 to 58% of blocks transmitted on wind and unknown noise.
/// That is the opposite of what removing noise ought to do, so the mechanism is
/// worth having written down rather than inferred.
///
/// The SNR test is a *ratio*: level against a floor tracked from the same
/// signal. Take 20 dB out of steady wind and both fall together, so the ratio
/// should not move. It moves because what the enhancer leaves behind is not
/// steady. A speech enhancer fed no speech emits intermittent residue, and an
/// intermittent signal clears its own tracked floor by construction — the floor
/// follows the quiet parts and the bursts stand above it. Steady wind is
/// rejected precisely *because* it is steady.
///
/// So the enhancer converts a signal the floor tracker handles into one it
/// cannot, and the margin that rejected 100% of raw wind passes a quarter of
/// enhanced wind.
#[test]
#[ignore = "diagnostic; prints the mechanism behind the enhanced-noise leak"]
fn what_the_enhancer_does_to_the_shape_of_noise() {
    use mumbleway_core::audio::dsp::{rms, to_dbfs, NoiseFloorTracker};

    println!(
        "\n{:<22} {:>8} {:>8} {:>8} {:>8}",
        "case", "lvl p50", "flr p50", "snr p50", "snr p90"
    );
    for (name, signal) in cases(15, 7, 0.5) {
        if !name.starts_with("wind") && !name.starts_with("unknown") {
            continue;
        }
        for enhanced in [false, true] {
            let mut sig = signal.clone();
            if enhanced {
                let mut e = Enhancer::new();
                for c in sig.chunks_exact_mut(BLOCK) {
                    e.process(c);
                }
            }
            let mut floor = NoiseFloorTracker::new(25);
            let (mut levels, mut floors, mut snrs) = (Vec::new(), Vec::new(), Vec::new());
            for c in sig.chunks_exact(BLOCK) {
                let l = to_dbfs(rms(c));
                let f = floor.update(l);
                levels.push(l);
                floors.push(f);
                snrs.push(l - f);
            }
            let p = |v: &mut Vec<f32>, q: f32| {
                v.sort_by(|a, b| a.partial_cmp(b).unwrap());
                v[((v.len() - 1) as f32 * q) as usize]
            };
            let mut l = levels.clone();
            let mut f = floors.clone();
            let mut s1 = snrs.clone();
            let mut s2 = snrs.clone();
            println!(
                "{:<22} {:>8.1} {:>8.1} {:>8.1} {:>8.1}",
                format!("{name} {}", if enhanced { "enhanced" } else { "raw" }),
                p(&mut l, 0.5),
                p(&mut f, 0.5),
                p(&mut s1, 0.5),
                p(&mut s2, 0.9)
            );
        }
    }
}
