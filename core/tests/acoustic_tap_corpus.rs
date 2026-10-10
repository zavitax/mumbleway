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

        let mut d = AcousticTapDetector::new(3);
        let mut fired: Vec<u64> = Vec::new();
        let mut banked = false;
        for block in pcm.chunks(480) {
            if let Some(g) = d.push(block, &mut banked) {
                fired.push(g.at_ms);
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
