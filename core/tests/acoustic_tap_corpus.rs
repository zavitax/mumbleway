//! The acoustic detector against real taps.
//!
//! **Run with `MW_TAP_CORPUS=<dir>`**, pointing at recordings whose taps are
//! audible — the same corpus `tools/tap/label_from_audio.py` labels from the
//! audio itself. Ignored by default, because the corpus is not in the
//! repository and never will be: it is a rider's microphone.
//!
//! What it reports is the number that decides shippability, and the same one
//! the accelerometer detector could never reach: gestures found, against
//! gestures performed, and arms that nobody asked for.

use std::path::PathBuf;

use mumbleway_core::audio::acoustic_tap::AcousticTapDetector;

fn read_s16(path: &PathBuf) -> Vec<f32> {
    let raw = std::fs::read(path).expect("read");
    raw.chunks_exact(2)
        .map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0)
        .collect()
}

/// The clicks the audio itself says are there, as 1 ms frame indices.
///
/// The same rule `label_from_audio.py` uses, so the labels and the detector
/// are judged against the same ground truth a rider confirmed by ear.
fn audible_clicks(pcm: &[f32]) -> Vec<usize> {
    const B: usize = 480; // 10 ms, as the labeller uses
    let peaks: Vec<f32> = pcm
        .chunks(B)
        .map(|c| c.iter().fold(0.0f32, |m, s| m.max(s.abs())))
        .collect();
    let mut hits: Vec<usize> = Vec::new();
    for i in 100..peaks.len() {
        let mut w: Vec<f32> = peaks[i - 100..i - 2].to_vec();
        w.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let reference = w[w.len() / 2] + 1e-6;
        if peaks[i] > reference * 8.0 && peaks[i] > 0.02 {
            if hits.last().is_some_and(|&l| i - l <= 5) {
                continue;
            }
            hits.push(i);
        }
    }
    hits
}

#[test]
#[ignore = "needs MW_TAP_CORPUS pointing at recordings with audible taps"]
fn it_finds_the_taps_a_rider_can_hear() {
    let dir = std::env::var("MW_TAP_CORPUS").expect("MW_TAP_CORPUS");
    let mut rides = 0;
    let (mut found_total, mut performed_total, mut false_total) = (0, 0, 0);
    let mut seconds = 0.0f64;

    let mut entries: Vec<_> = std::fs::read_dir(&dir)
        .expect("corpus dir")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "s16"))
        .collect();
    entries.sort();

    for path in entries {
        let pcm = read_s16(&path);
        if pcm.len() < 48_000 {
            continue;
        }
        let clicks = audible_clicks(&pcm);
        if clicks.is_empty() {
            continue;
        }
        // Clicks within 700 ms of each other are one performance.
        let mut groups: Vec<Vec<usize>> = vec![vec![clicks[0]]];
        for &b in &clicks[1..] {
            if (b - groups.last().unwrap().last().unwrap()) * 10 <= 700 {
                groups.last_mut().unwrap().push(b);
            } else {
                groups.push(vec![b]);
            }
        }
        let performed: Vec<&Vec<usize>> = groups.iter().filter(|g| g.len() >= 3).collect();

        let mut d = AcousticTapDetector::new();
        let mut fired: Vec<u64> = Vec::new();
        for block in pcm.chunks(480) {
            let mut banked = false;
            if let Some(g) = d.push(block, &mut banked) {
                fired.push(g.at_ms);
            }
            // **The engine plays a tick for every tap it banks, and the
            // detector hears it**, because it runs on the raw microphone
            // ahead of the canceller. Simulated here so the rig scores the
            // detector that ships rather than one that never hears itself:
            // without this the measurement is blind to the window after each
            // tap in which the next one cannot be heard, which is exactly the
            // window a rider tapping quickly uses.
            if banked {
                d.hear_own_output(-20.0);
            }
        }

        let mut matched = 0;
        let mut used: Vec<u64> = Vec::new();
        for g in &performed {
            let end = (*g.last().unwrap() as u64) * 10;
            if let Some(&hit) = fired
                .iter()
                .find(|f| f.abs_diff(end) <= 600 && !used.contains(f))
            {
                used.push(hit);
                matched += 1;
            }
        }

        rides += 1;
        found_total += matched;
        performed_total += performed.len();
        false_total += fired.len() - used.len();
        seconds += pcm.len() as f64 / 48_000.0;

        println!(
            "{}: {:.1} s, {} performed, {} found, {} false  [{:?}]",
            path.file_name().unwrap().to_string_lossy(),
            pcm.len() as f64 / 48_000.0,
            performed.len(),
            matched,
            fired.len() - used.len(),
            d.stats()
        );
    }

    println!(
        "\n{rides} rides, {seconds:.0} s: {found_total}/{performed_total} gestures, \
         {false_total} false arms ({:.0} per hour)",
        false_total as f64 * 3600.0 / seconds.max(1.0)
    );
    assert!(rides > 0, "no usable rides in {dir}");
}

/// Reads 16-bit mono PCM from a `.wav`, by finding its `data` chunk.
///
/// The corpus holds both: rides arrive as headerless `.s16`, while the music
/// the gate is measured against is a `.wav` written by the analysis tools.
fn read_wav(path: &PathBuf) -> Vec<f32> {
    let raw = std::fs::read(path).expect("read");
    let mut at = 12; // past "RIFF....WAVE"
    let mut channels = 1usize;
    while at + 8 <= raw.len() {
        let id = &raw[at..at + 4];
        let size =
            u32::from_le_bytes([raw[at + 4], raw[at + 5], raw[at + 6], raw[at + 7]]) as usize;
        let body = at + 8;
        if id == b"fmt " {
            channels = u16::from_le_bytes([raw[body + 2], raw[body + 3]]) as usize;
        } else if id == b"data" {
            let end = (body + size).min(raw.len());
            let all: Vec<f32> = raw[body..end]
                .chunks_exact(2)
                .map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0)
                .collect();
            return if channels > 1 {
                all.chunks(channels)
                    .map(|c| c.iter().sum::<f32>() / c.len() as f32)
                    .collect()
            } else {
                all
            };
        }
        at = body + size + (size & 1);
    }
    panic!("{}: no data chunk", path.display());
}

/// One impulse that arrived sharply enough to be judged, on both axes the
/// detector decides it on.
#[derive(Clone, Copy)]
struct Impulse {
    over_db: f32,
    attack_db: f32,
}

/// Every such impulse in a recording.
///
/// **Pushed one frame at a time deliberately**, so the stats can be sampled
/// per frame. Run it with `MW_TAP_ATTACK_DB=0` to see the impulses the shipped
/// threshold *rejects* as well as the ones it keeps — which is the half that
/// says where the threshold should sit, and the half a pass/fail count cannot
/// show.
fn impulses(pcm: &[f32]) -> Vec<Impulse> {
    let mut d = AcousticTapDetector::new();
    let mut banked = false;
    let mut out = Vec::new();
    let mut candidates = 0;
    for frame in pcm.chunks(48) {
        d.push(frame, &mut banked);
        let s = d.stats();
        if s.candidates != candidates {
            candidates = s.candidates;
            out.push(Impulse {
                over_db: s.impulse_over_db,
                attack_db: s.impulse_attack_db,
            });
        }
    }
    out
}

/// `p5 / median / p95` of one column, which is all that is wanted here: the
/// question is whether two distributions overlap, not what shape either is.
fn spread(v: &[f32]) -> String {
    if v.is_empty() {
        return "       none        ".to_string();
    }
    let mut v = v.to_vec();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let at = |q: f64| v[((v.len() - 1) as f64 * q) as usize];
    format!("{:+6.1} {:+6.1} {:+6.1}", at(0.05), at(0.50), at(0.95))
}

fn row(label: &str, v: &[Impulse]) {
    let col = |f: fn(&Impulse) -> f32| spread(&v.iter().map(f).collect::<Vec<f32>>());
    println!(
        "  {label:<40} {:>4}  {}  {}",
        v.len(),
        col(|i| i.over_db),
        col(|i| i.attack_db),
    );
}

/// How a tap arrives, against how music arrives.
///
/// **This is the measurement that set `ATTACK_DB`, and the one that refused
/// two frequency-based rules before it.** Run it with `MW_TAP_ATTACK_DB=0` to
/// see every impulse either way, which is how the threshold was chosen; run it
/// as it stands to see what the shipped one keeps.
///
/// The music half matters more than the tap half, and it is the half that was
/// missing while two spectral rules were proposed and one was nearly shipped.
/// A synthetic hi-hat agrees with whoever wrote it — `tap()` in the unit tests
/// was itself generating 18 kHz, so the fixture that proved taps are found was
/// spectrally a cymbal, and the fixture asserting a cymbal is rejected was the
/// same signal. `music_only__input.wav` is real music through a real
/// microphone and knows nothing about what it is being used for.
#[test]
#[ignore = "needs MW_TAP_CORPUS pointing at recordings with audible taps"]
fn taps_and_music_do_not_arrive_the_same_way() {
    let dir = PathBuf::from(std::env::var("MW_TAP_CORPUS").expect("MW_TAP_CORPUS"));

    println!("every impulse sharp enough to be judged, as p5 / median / p95 in dB");
    println!(
        "  {:<40} {:>4}  {:^20}  {:^20}",
        "", "n", "over floor", "attack"
    );
    let mut rides: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("corpus dir")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "s16"))
        .collect();
    rides.sort();

    for path in rides {
        let pcm = read_s16(&path);
        if pcm.len() < 48_000 || audible_clicks(&pcm).is_empty() {
            continue;
        }
        let name = path.file_stem().unwrap().to_string_lossy().to_string();
        let note = path.with_extension("note");
        let label = std::fs::read_to_string(&note)
            .ok()
            .and_then(|t| t.lines().last().map(|l| l.trim().to_string()))
            .unwrap_or_default();
        row(&format!("{name} {label}"), &impulses(&pcm));
    }

    // The negative side, and the only one recorded without taps in mind.
    let music = dir
        .parent()
        .map(|p| p.join("music_shipping").join("music_only__input.wav"))
        .filter(|p| p.exists());
    match music {
        Some(p) => {
            let pcm = read_wav(&p);
            row("music_only__input  (REAL MUSIC)", &impulses(&pcm));

            // **The number that actually decides it.** There are no taps in
            // this recording, so every gesture it completes is a false arm
            // and no labelling is needed — the same shape of measurement as
            // a ride at speed with the gesture on and nobody tapping.
            let mut d = AcousticTapDetector::new();
            let mut banked = false;
            let mut gestures = 0;
            for block in pcm.chunks(480) {
                if d.push(block, &mut banked).is_some() {
                    gestures += 1;
                }
            }
            println!(
                "
music gestures: {gestures} in {:.0} s ({:.1} per hour)  [{:?}]",
                pcm.len() as f64 / 48_000.0,
                gestures as f64 * 3600.0 / (pcm.len() as f64 / 48_000.0),
                d.stats(),
            );
        }
        None => println!("\nno music_shipping/music_only__input.wav beside the corpus"),
    }
}
