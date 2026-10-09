# Scoring the tap detector

`score.py` runs a tap detector over the recorder's **motion track** and reports
what it found, what it missed, and — the number that actually decides anything —
**false arms per hour**.

```bash
python tools/tap/score.py <ride-dir> --taps 3
python tools/tap/score.py <ride-dir> --taps 4 --sweep
```

## What it has already found

**First corpus, 2026-10-09: two iPhone recordings, 55 seconds, 100 Hz, taps
audible in the audio of the same segment.** It settled three constants that
had been guessed, and the first one was wrong by eighteen decibels.

| | was | now |
|---|---|---|
| `MARGIN_DB` | 12 | **30** |
| `MAX_PULSE_MS` | 60 | **150** |
| `GAP_MAX_MS` | 400 | **600** |

The failure the guess produced is worth keeping, because no amount of staring
at a still phone would have shown it. On a hand-held phone ψ's *median* sits
about 15 dB above its own tracked floor — the floor is a minimum statistic and
real movement is nowhere near the minimum — so **59% and 69% of the two
recordings were above a 12 dB margin.** The detector was mid-candidate almost
always, every real tap arrived while it was already counting, and the
pulse-width test threw the pair away. It fired 21 and 23 times in 40 and 15
seconds and matched not one of the gestures.

The taps were never faint: every audible one reached 22–60 dB over the floor.
There was no sensitivity problem, only a margin that admitted everything.

At 30 dB the corpus gives **zero false arms**, and it is the only margin tried
that does. Recall is about two thirds — a gesture sometimes needs repeating,
which is the right way round for this trade.

**What 55 seconds cannot tell you**: both recordings are a phone in a hand and
on a desk, indoors, stationary. The target below is per hour and this is a
sixty-fifth of one. Nothing here says anything about a bike.

## Why this exists before the detector ships

`core/src/audio/tap.rs` had a threshold, a pulse width, a dead time, a gap
window and two tolerances, and **every one of them was marked provisional** in
that file. None was chosen by argument, because none can be: whether a
deliberate tap through a padded bag can be told from a pothole at 100 km/h is a
question about signals nobody in this project had looked at.

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
