# Scoring the tap detector

**The tap is heard, not felt.** The accelerometer detector and its motion
track were removed: at the 100 Hz iOS delivers, a tap is four samples, where a
microphone gives forty. What remains is `label_from_audio.py`, which finds the
taps in a recording's own audio, and `core/tests/acoustic_tap_corpus.rs`, which
scores the shipping detector against them:

```bash
python tools/tap/label_from_audio.py <ride-dir>      # what it hears
cd core && MW_TAP_CORPUS=<ride-dir> cargo test --test acoustic_tap_corpus -- --ignored --nocapture
```

Use forward slashes in the path; Git Bash eats backslashes before Python sees
them.

## What it has found

| recording | performed | found | false |
|---|---|---|---|
| 2026-10-09, taps in the hand | 7 | **7** | **0** |
| 2026-10-11, taps on the phone | 10 | 8 | 10 |

**The second row's "false" is not what it looks like.** That rider tapped 148
times in 36 seconds, so where the labeller sees one performance of six taps the
detector reasonably sees two gestures of three. The corpus cannot tell those
apart, and until a ride is recorded with taps *and* road the false-arm figure
is measuring the labelling rather than the detector.

## A proposal that did not survive measurement

A tap was described as punchy — a sharp rise followed by a drop in volume
**and frequency** — and a frequency test was going to be added for it. It was
measured first, twice, on the 2026-10-11 recording:

- zero crossings per millisecond: **4 at onset, 4 mid-decay**, ratio 1.00 at
  both quartiles;
- high-against-low band energy, onset against mid-decay: **median 1.02×**, and
  only 14% of taps got duller as they decayed.

Volume, over the same window, falls about **25×**. So the punch is in the
envelope — a very fast attack and a fast decay, both of which the detector
already tests — and not in the spectrum. A frequency test would have rejected
roughly 85% of real taps.

## The ride that matters most has no taps in it

Recall is the easier half: a rider who taps and is not heard taps again. **False
arms are the half that makes the feature unusable**, because each one drags the
headset onto the hands-free profile, costs a second or two of negotiation, and
announces through the rider's music that something they did not do has happened.

Measuring them needs rides with **no taps at all**, which is why the motion
track records whether or not tap detection is switched on — see
`docs/CAPTURE_ON_DEMAND.md`. Such a ride needs no `labels.csv`, and every
detection in it is counted as a false arm, which is exactly right.

**Target worth agreeing before building on top of this: under one per hour.**

## Labels come from the audio, not from memory

**A recording whose taps are audible labels itself.** A tap on the phone body
is a broadband click, the microphone is recording at the same instant the
accelerometer is, and both tracks carry the same `block` column — so an audio
click at block 2475 and a motion impulse at block 2475 are the same event by
construction rather than by a judgement about which bump was which.

```bash
python tools/tap/label_from_audio.py <ride-dir>            # report only
python tools/tap/label_from_audio.py <ride-dir> --write    # write labels.csv
```

Use forward slashes in the path. A Windows path with backslashes is eaten by
Git Bash before Python sees it, and the script then reports finding nothing.

This is what makes a corpus cheap: hand-labelling is the step that does not
happen, and the first corpus here existed within minutes of the first recording
that had taps audible in it.

**It cannot label a ride at speed**, because a tap on a phone in a pocket is
not audible over wind — and that is exactly the ride the false-arm number has
to come from. So this answers recall on quiet rides and says nothing about
false arms on loud ones. Both are needed; only one of them is cheap.

## The label format

`labels.csv` beside the motion tracks, as `segment,block,note`:

```
segment,block,note
0,1432,three taps, 60 km/h, smooth
2,890,four taps, standing at lights
```

`segment` is the index from the filename (`ride-002.motion.csv` → 2) and `block`
is the block column from the track, which is the audio block **within that
segment** — the recorder resets it on every rotation, and the audio beside it
starts at sample zero for the same reason.

## Two things it cannot tell you

- **Nothing about iOS from an Android ride, or the reverse.** Android delivers
  200–500 Hz and iOS 100 Hz, and a tap's transient runs 20–80 ms — so the shape
  is visible on one and a bare step on the other. Worse, a twin firing at
  33–67 Hz folds back past a 50 Hz Nyquist as content that moves with the
  throttle. Score the two corpora separately.
- **Nothing about a bag it was not recorded in.** Padding, strap tension and
  where the bag sits on the thigh all change the impulse.

## The algorithm is duplicated on purpose

`score.py` reimplements what `tap.rs` does rather than calling it. That costs
keeping two copies in step, and it buys the only thing worth having here: a rig
that imports the shipping code can only ever agree with it, and the point is to
disagree until the constants are right.

When a measurement contradicts the plan, the measurement wins and the retraction
goes in the file that made the claim — `tap.rs`'s provisional markers are where
these numbers are finally written down.
