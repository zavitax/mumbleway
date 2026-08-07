"""Score TEN VAD against the hand labels, at the operating point that matters.

Stage 0 -- moving the transmit decision off RNNoise's VAD and onto this one --
was proposed on the strength of TEN finding speech the chain misses. The rider
has since listened to those finds and judged them not speech, so that
justification is gone and the swap needs re-establishing on something else.

This is that something else, and it is the same test the existing chain was put
through: recall and precision against spans the rider labelled by hand, on real
helmet audio. The numbers to beat, from core/tests/road.rs:

    the whole chain, Helmet    57.4% recall / 53.7% precision
    RNNoise's VAD alone        fires on 38% of labelled speech blocks

A VAD is not the whole decision -- the chain also applies an SNR margin, a
gate, and a hangover -- so this is an upper bound on what swapping it could
buy, not a prediction. If the upper bound does not clear the current figure,
there is nothing to swap for.
"""

import os
import sys
import numpy as np

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from ten_eval import HOP, VAD_RATE, run, to_16k_int16

RATE = 48_000


def labels_on_grid(sidecar, frames, hop_s):
    spans = []
    for line in open(sidecar):
        parts = line.split()
        if len(parts) >= 2:
            spans.append((float(parts[0]), float(parts[1])))
    y = np.zeros(frames, dtype=bool)
    for i in range(frames):
        t = i * hop_s
        if any(a <= t <= b for a, b in spans):
            y[i] = True
    return y, spans


def main():
    audio_dir = sys.argv[1]
    hop_s = HOP / VAD_RATE

    for name in sorted(os.listdir(audio_dir)):
        if not name.endswith(".raw"):
            continue
        sidecar = os.path.join(audio_dir, name[:-4] + ".speech")
        if not os.path.exists(sidecar):
            continue

        x = np.fromfile(os.path.join(audio_dir, name), dtype=np.float32)
        prob = run(to_16k_int16(x), 0.5)
        y, spans = labels_on_grid(sidecar, len(prob), hop_s)

        print(f"\n=== {name[:-4]}  {len(x) / RATE:.1f}s, "
              f"{y.sum() * hop_s:.1f}s labelled speech in {len(spans)} spans ===")
        print(f"    {'thresh':>7} {'recall':>9} {'precision':>10} {'sent':>7}")
        for t in (0.7, 0.5, 0.4, 0.3, 0.2, 0.1):
            p = prob >= t
            hit = float((p & y).sum())
            miss = float((~p & y).sum())
            fa = float((p & ~y).sum())
            rec = hit / max(1.0, hit + miss)
            pre = hit / max(1.0, hit + fa)
            mark = "  <-- beats the chain" if rec > 0.574 and pre > 0.537 else ""
            print(f"    {t:>7.2f} {rec * 100:>8.1f}% {pre * 100:>9.1f}% "
                  f"{p.mean() * 100:>6.1f}%{mark}")


if __name__ == "__main__":
    main()
