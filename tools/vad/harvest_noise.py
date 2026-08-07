"""Carve motorcycle noise out of the rider's own recordings.

The training pipeline needs helmet noise with *nobody talking*, and until a
dedicated noise-only ride is recorded the only source is the four recordings we
have -- which do contain speech. So the speech has to come out.

The threshold used here is deliberately paranoid, and in the opposite direction
from everywhere else in this project. Elsewhere a low threshold costs a false
alarm somebody listens to and discards. Here a missed speech segment is
poisoned training data: the mixing pipeline will label it as noise, and the
network will be taught that this particular voice is something to suppress.
One second of speech smuggled into the noise pool is worse than throwing away
a minute of usable wind.

So: anything TEN VAD scores above 0.15 is excluded, with a generous margin on
both sides, and only what is left counts as noise. At that threshold the model
calls most of a recording speech, which is exactly the caution wanted.
"""

import os
import sys
import numpy as np

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from ten_eval import HOP, VAD_RATE, run, to_16k_int16
from evaluate import Silero, to_16k

RATE = 48_000

# Anything at or above this in EITHER model is treated as possibly-speech.
#
# Both, because they disagree in both directions on this material and each
# finds speech the other misses. For discarding, a union of two imperfect
# detectors is strictly safer than the better one alone.
#
# 0.15 was the first value tried and it kept nothing at all: at that threshold
# TEN calls most of a recording speech, so a one-second margin around every
# suspicion swallowed all four recordings. Caution that discards the entire
# corpus is not caution, it is a different way of failing.
SUSPICION = 0.30
# Seconds thrown away either side of anything suspicious.
MARGIN = 1.0
# Shorter runs of noise than this are not worth keeping.
MIN_RUN = 2.0


def noise_spans(x, silero):
    """Spans of the 48 kHz signal that neither model thinks are speech."""
    hop_s = HOP / VAD_RATE
    seconds = len(x) / RATE
    suspicious = np.zeros(int(seconds / hop_s) + 2, dtype=bool)
    grow = int(round(MARGIN / hop_s))

    def flag(at_s):
        i = int(at_s / hop_s)
        lo = max(0, i - grow)
        hi = min(len(suspicious), i + grow + 1)
        suspicious[lo:hi] = True

    for i, p in enumerate(run(to_16k_int16(x), SUSPICION)):
        if p >= SUSPICION:
            flag(i * hop_s)

    # Silero runs on its own window length, so it is mapped by time rather
    # than by index -- the two models do not share a frame grid.
    s_hop = Silero.WINDOW / VAD_RATE
    for i, p in enumerate(silero.run(to_16k(x))):
        if p >= SUSPICION:
            flag(i * s_hop)

    spans, start = [], None
    for i, bad in enumerate(suspicious):
        if not bad and start is None:
            start = i
        elif bad and start is not None:
            spans.append((start * hop_s, i * hop_s))
            start = None
    if start is not None:
        spans.append((start * hop_s, min(seconds, len(suspicious) * hop_s)))
    return [(s, e) for s, e in spans if e - s >= MIN_RUN]


def main():
    src, out, model = sys.argv[1], sys.argv[2], sys.argv[3]
    os.makedirs(out, exist_ok=True)
    silero = Silero(model)
    kept = 0.0
    total = 0.0

    for name in sorted(os.listdir(src)):
        if not name.endswith(".raw"):
            continue
        x = np.fromfile(os.path.join(src, name), dtype=np.float32)
        total += len(x) / RATE
        spans = noise_spans(x, silero)
        pieces = []
        for s, e in spans:
            pieces.append(x[int(s * RATE) : int(e * RATE)])
            kept += e - s
        if pieces:
            np.concatenate(pieces).tofile(os.path.join(out, f"noise_{name}"))
        print(f"{name[:-4]:<20} {len(spans):>3} runs, "
              f"{sum(e - s for s, e in spans):>6.1f}s of noise kept")

    print(f"\nkept {kept:.1f}s of {total:.1f}s ({kept / total * 100:.0f}%) as noise")
    print(f"in: {out}")


if __name__ == "__main__":
    main()
