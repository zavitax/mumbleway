"""Score a tap detector against hand-labelled rides.

    python tools/tap/score.py <ride-dir> [--taps 3] [--sweep]

Where `<ride-dir>` holds the recorder's output — `*.motion.csv` from a ride,
beside the `.s16` and decision `.csv` of the same segment — and a `labels.csv`
naming where the taps actually are.

**The number this exists to produce is false arms per hour**, not accuracy.
A detector that finds every tap and fires twice an hour on its own is useless:
each false arm drags a headset onto the hands-free profile, costs a second or
two of negotiation, and tells the rider through their music that something they
did not do has happened. Recall matters second, because a missed gesture is
retried by a rider who felt it not work.

Written against the motion track rather than against a phone, for the same
reason `tools/vad/` scores recordings rather than live audio: the corpus is the
only thing two candidate parameter sets can be compared on, and a ride cannot be
repeated.

## What it does NOT establish

- **Nothing about iOS from an Android ride, or the reverse.** Android delivers
  200–500 Hz and iOS 100, and a tap's transient is 20–80 ms, so the shape is
  visible on one and a step on the other. Score the two corpora separately;
  `--rate` reports what a file actually contains so a mixed set is obvious.
- **Nothing about a bag it was not recorded in.** Padding, how tightly the bag
  is strapped and where it sits on the thigh all change the impulse. This is
  why the rig takes rides rather than a synthetic generator — see the
  measurement-discipline section of CLAUDE.md, which this project earned.

## Labels

`labels.csv` is `segment,block,note` — the segment index from the filename and
the block column from the motion track, which is the audio block *within that
segment*. One row per tap performed, taken from the rider's own count. A label
within `--tolerance-ms` of a detection counts as found.

A ride with **no taps at all** needs no labels file and is the most valuable
kind: it is the only thing that measures false arms, and it can only be
collected while the feature is switched off.
"""

from __future__ import annotations

import argparse
import csv
import math
import sys
from dataclasses import dataclass
from pathlib import Path

# Mirrors `core/src/audio/tap.rs`. Kept in step by hand, which is a cost worth
# paying: a rig that imports the shipping code can only ever agree with it, and
# the point here is to disagree until the constants are right.
DEFAULTS = dict(
    margin_db=12.0,
    max_pulse_ms=60,
    latent_ms=70,
    gap_min_ms=80,
    gap_max_ms=400,
    magnitude_tolerance=2.5,
    interval_tolerance=1.8,
    floor_sub_blocks=25,
)


@dataclass
class Sample:
    block: int
    arrival_us: int
    accel: tuple[float, float, float]


class Floor:
    """Minimum statistics with a rate-limited rise, as `NoiseFloorTracker` is.

    The rise limit is the part that matters and the part that was learned the
    hard way on audio: hold a signal past the memory and every sub-window is
    full of it, so the minimum *is* the signal and the margin collapses.
    """

    def __init__(self, sub_len: int, sub_windows: int = 6, rise_db_per_step: float = 0.06):
        self.sub_len = sub_len
        self.sub_windows = sub_windows
        self.rise = rise_db_per_step
        self.mins: list[float] = []
        self.current = math.inf
        self.n = 0
        self.value = -120.0

    def update(self, level_db: float, frozen: bool) -> float:
        if not frozen:
            self.current = min(self.current, level_db)
            self.n += 1
            if self.n >= self.sub_len:
                self.mins.append(self.current)
                self.mins = self.mins[-self.sub_windows :]
                self.current = math.inf
                self.n = 0
        target = min(self.mins) if self.mins else level_db
        if target > self.value:
            self.value = min(target, self.value + self.rise)
        else:
            self.value = target
        return self.value


def detect(samples: list[Sample], taps_wanted: int, **p) -> list[int]:
    """Returns the arrival time, in ms, of each completed gesture."""
    cfg = {**DEFAULTS, **p}
    floor = Floor(int(cfg["floor_sub_blocks"]))
    hist = [[0.0] * 3 for _ in range(3)]
    seen = 0
    above_since = None
    peak = -math.inf
    latent_until = 0
    pending: list[tuple[int, float]] = []
    out: list[int] = []

    for s in samples:
        for k in range(3):
            hist[k][0], hist[k][1], hist[k][2] = hist[k][1], hist[k][2], s.accel[k]
        seen += 1
        if seen < 3:
            continue

        psi = sum(abs(h[1] * h[1] - h[2] * h[0]) for h in hist)
        level = 10.0 * math.log10(psi + 1e-12)
        at_ms = s.arrival_us // 1000
        f = floor.update(level, above_since is not None)
        over = level - f

        if above_since is None:
            if over >= cfg["margin_db"]:
                above_since, peak = at_ms, over
            continue

        peak = max(peak, over)
        if over >= cfg["margin_db"]:
            if at_ms - above_since > cfg["max_pulse_ms"]:
                above_since, peak = None, -math.inf
            continue

        strength, start = peak, above_since
        above_since, peak = None, -math.inf
        if at_ms < latent_until:
            continue
        latent_until = at_ms + cfg["latent_ms"]

        if pending and start - pending[-1][0] > cfg["gap_max_ms"]:
            pending = []
        if not pending:
            pending = [(start, strength)]
            continue

        gap = start - pending[-1][0]
        if gap < cfg["gap_min_ms"]:
            continue
        if _ratio(abs(strength), abs(pending[-1][1])) > cfg["magnitude_tolerance"]:
            pending = [(start, strength)]
            continue
        if len(pending) >= 2:
            prev_gap = max(1, pending[-1][0] - pending[-2][0])
            if _ratio(gap, prev_gap) > cfg["interval_tolerance"]:
                pending = [(start, strength)]
                continue

        pending.append((start, strength))
        if len(pending) >= taps_wanted:
            out.append(start)
            pending = []
    return out


def _ratio(a: float, b: float) -> float:
    a, b = abs(a), abs(b)
    if a <= 0 or b <= 0:
        return math.inf
    return a / b if a > b else b / a


def load(path: Path) -> list[Sample]:
    rows: list[Sample] = []
    with path.open(encoding="utf-8") as f:
        for line in f:
            if line.startswith("#") or line.startswith("block,"):
                continue
            parts = line.rstrip("\n").split(",")
            if len(parts) < 12:
                continue
            rows.append(
                Sample(
                    block=int(parts[0]),
                    arrival_us=int(parts[2]),
                    accel=(float(parts[3]), float(parts[4]), float(parts[5])),
                )
            )
    return rows


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("ride", type=Path)
    ap.add_argument("--taps", type=int, default=3)
    ap.add_argument("--tolerance-ms", type=int, default=600)
    ap.add_argument("--sweep", action="store_true", help="try a grid of margins")
    args = ap.parse_args()

    tracks = sorted(args.ride.glob("*.motion.csv"))
    if not tracks:
        print(f"no motion tracks in {args.ride}", file=sys.stderr)
        return 2

    labels: dict[int, list[int]] = {}
    label_file = args.ride / "labels.csv"
    if label_file.exists():
        with label_file.open(encoding="utf-8") as f:
            for row in csv.reader(f):
                if not row or row[0].startswith("#") or row[0] == "segment":
                    continue
                labels.setdefault(int(row[0]), []).append(int(row[1]))

    margins = (
        [6.0, 9.0, 12.0, 15.0, 18.0, 21.0] if args.sweep else [DEFAULTS["margin_db"]]
    )
    print(f"{'margin':>7} {'found':>6} {'missed':>7} {'false':>6} {'false/h':>8}")
    print("-" * 40)

    for margin in margins:
        found = missed = false = 0
        seconds = 0.0
        for track in tracks:
            seg = int(track.name.split("-")[-1].split(".")[0])
            samples = load(track)
            if len(samples) < 10:
                continue
            seconds += (samples[-1].arrival_us - samples[0].arrival_us) / 1e6

            hits = detect(samples, args.taps, margin_db=margin)
            # Labels are block numbers; the motion track carries the block each
            # sample fell in, so a label becomes a time by looking it up.
            wanted_ms = []
            for b in labels.get(seg, []):
                at = next((s for s in samples if s.block >= b), None)
                if at:
                    wanted_ms.append(at.arrival_us // 1000)

            unmatched = list(hits)
            for w in wanted_ms:
                near = [h for h in unmatched if abs(h - w) <= args.tolerance_ms]
                if near:
                    found += 1
                    unmatched.remove(near[0])
                else:
                    missed += 1
            false += len(unmatched)

        hours = seconds / 3600.0 if seconds else 0.0
        rate = false / hours if hours else float("nan")
        print(f"{margin:7.1f} {found:6d} {missed:7d} {false:6d} {rate:8.2f}")

    if not labels:
        print(
            "\nNo labels.csv, so every detection is counted as a false arm — "
            "which is exactly right for a ride recorded with no taps in it, and "
            "that is the ride this number needs."
        )
    print(
        "\nUnder one false arm per hour is the target worth arguing about "
        "before building anything on top of this."
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
