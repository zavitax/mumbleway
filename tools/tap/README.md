# Scoring the tap detector

`score.py` runs a tap detector over the recorder's **motion track** and reports
what it found, what it missed, and — the number that actually decides anything —
**false arms per hour**.

```bash
python tools/tap/score.py <ride-dir> --taps 3
python tools/tap/score.py <ride-dir> --taps 4 --sweep
```

## Why this exists before the detector ships

`core/src/audio/tap.rs` has a threshold, a pulse width, a dead time, a gap
window and two tolerances, and **every one of them is marked provisional** in
that file. None was chosen by argument, because none can be: whether a
deliberate tap through a padded bag can be told from a pothole at 100 km/h is a
question about signals nobody in this project has yet looked at.

That is the same position the voice gate was in, and `tools/vad/` is how it got
out of it — a corpus, hand labels, and candidates scored against both. The
retraction at the top of `tools/vad/README.md` is what happens without it.

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

## Labels

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
