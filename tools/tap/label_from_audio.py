"""Write `labels.csv` for a ride by finding the taps in its own audio.

    python tools/tap/label_from_audio.py C:\\ml_data\\rides
    python tools/tap/label_from_audio.py C:\\ml_data\\rides --write

**The label comes from the other track, not from anybody's memory.** A tap on
the phone body is a broadband click, and the microphone is recording at the
same instant the accelerometer is — so a recording made with the taps audible
labels itself, to the 10 ms block, with no stopwatch and nothing to misremember.

That is what makes a corpus cheap. `score.py` needed rides labelled by hand,
which is the step that does not happen; this turns every recording where the
taps can be heard into a scored ride for nothing. It is also *better* than a
hand label, because the two tracks share the `block` column: an audio click at
block 2475 and a motion impulse at block 2475 are the same event by
construction, not by a judgement about which bump was which.

## What it will not do

It cannot label a ride recorded at speed, because a tap on a phone in a pocket
is not audible over wind — and that is exactly the corpus the false-arm number
has to come from. So this answers *recall* on quiet rides and says nothing
about false arms on loud ones. Both are needed and only one of them is cheap.

## The detector

Loud relative to the trailing second, and brief. The reference is the median
of the previous ~1 s of block peaks rather than a fixed level, for the same
reason the tap detector tracks a floor: a fixed threshold is deaf in a car park
and hair-triggered in a quiet room. Speech is loud but sustained, so it does
not clear a ratio test against its own recent past.
"""

from __future__ import annotations

import argparse
import array
import sys
from pathlib import Path

RATE = 48000
BLOCK = 480  # 10 ms, the recorder's own block, and the `.csv` row rate

#: How far above the trailing median a block must peak to be a click.
OVER_MEDIAN = 8.0

#: And an absolute floor, so a ratio against near-silence cannot qualify.
MIN_PEAK = 0.02

#: Blocks within this of each other are one physical click.
MERGE_BLOCKS = 5

#: Clicks more than this apart belong to different gestures.
GESTURE_GAP_MS = 700


def block_peaks(path: Path) -> list[float]:
    """Peak amplitude per 10 ms block of a headerless mono s16 recording."""
    a = array.array("h")
    a.frombytes(path.read_bytes())
    if sys.byteorder == "big":
        a.byteswap()
    out = []
    for i in range(0, len(a) - BLOCK, BLOCK):
        out.append(max(abs(v) for v in a[i : i + BLOCK]) / 32768.0)
    return out


def clicks(peaks: list[float]) -> list[int]:
    """Block indices that are a sharp, brief burst above the trailing median."""
    hits: list[int] = []
    for i in range(100, len(peaks)):
        window = sorted(peaks[i - 100 : i - 2])
        reference = window[len(window) // 2] + 1e-6
        if peaks[i] > reference * OVER_MEDIAN and peaks[i] > MIN_PEAK:
            hits.append(i)
    merged: list[int] = []
    for i in hits:
        if merged and i - merged[-1] <= MERGE_BLOCKS:
            continue
        merged.append(i)
    return merged


def gestures(hits: list[int]) -> list[list[int]]:
    """Runs of clicks close enough together to be one performance."""
    if not hits:
        return []
    groups, current = [], [hits[0]]
    for b in hits[1:]:
        if (b - current[-1]) * 10 <= GESTURE_GAP_MS:
            current.append(b)
        else:
            groups.append(current)
            current = [b]
    groups.append(current)
    return groups


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("ride", type=Path, help="directory of .s16 segments")
    ap.add_argument(
        "--write",
        action="store_true",
        help="write labels.csv; without it, report only",
    )
    args = ap.parse_args()

    rows = []
    for audio in sorted(args.ride.glob("*.s16")):
        # Only rides that have a motion track to score against. The corpus
        # holds plenty of older recordings made before the recorder had one.
        if not (audio.parent / f"{audio.stem}.motion.csv").exists():
            continue
        segment = audio.stem.split("-")[-1]
        peaks = block_peaks(audio)
        hits = clicks(peaks)
        groups = gestures(hits)
        print(
            f"{audio.name}: {len(peaks) * 0.01:.1f} s, {len(hits)} clicks "
            f"in {len(groups)} groups "
            f"({sum(1 for g in groups if len(g) >= 3)} of three or more)"
        )
        for g in groups:
            for n, b in enumerate(g, 1):
                rows.append((segment, b, f"tap {n} of {len(g)}"))

    if not rows:
        print("nothing to label — no .s16 with a .motion.csv beside it")
        return 1

    out = args.ride / "labels.csv"
    text = "segment,block,note\n" + "".join(
        f"{s},{b},{n}\n" for s, b, n in rows
    )
    if not args.write:
        print(f"\n{len(rows)} labels. Re-run with --write to put them in {out}.")
        return 0
    out.write_text(text, encoding="utf-8")
    print(f"\nwrote {len(rows)} labels to {out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
