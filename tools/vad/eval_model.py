"""Score the trained VAD on the real recordings, against the hand labels.

The only number that matters. The model is trained on synthetic mixtures, and
mixtures are not a helmet -- so its training loss says nothing at all about
whether it works. This runs it on audio recorded from a real helmet at road
speed and scores it against spans the rider labelled by hand.

The bars to clear, both measured on the same audio in core/tests/road.rs:

    RNNoise's VAD, on labelled speech blocks      38%
    the whole chain, recall / precision       57% / 54%
"""

import os
import sys
import glob
import numpy as np
import torch

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from train_vad import Features, HelmetVad, HOP, RATE, load_raw48_to16


def labels_for(path, frames):
    """Read the .speech sidecar onto the model's frame grid."""
    sidecar = path[:-4] + ".speech"
    if not os.path.exists(sidecar):
        return None
    spans = []
    for line in open(sidecar):
        parts = line.split()
        if len(parts) >= 2:
            spans.append((float(parts[0]), float(parts[1])))
    y = np.zeros(frames, dtype=np.float32)
    for i in range(frames):
        t = i * HOP / RATE
        if any(a <= t <= b for a, b in spans):
            y[i] = 1.0
    return y


def main():
    ckpt_path, audio_dir = sys.argv[1], sys.argv[2]
    dev = "cuda" if torch.cuda.is_available() else "cpu"

    ckpt = torch.load(ckpt_path, map_location=dev)
    model = HelmetVad().to(dev)
    model.load_state_dict(ckpt["state"])
    model.eval()
    feats = Features().to(dev)

    for path in sorted(glob.glob(os.path.join(audio_dir, "*.raw"))):
        audio = load_raw48_to16(path)
        with torch.no_grad():
            x = torch.from_numpy(audio).unsqueeze(0).to(dev)
            prob = torch.sigmoid(model(feats(x))).squeeze(0).cpu().numpy()

        name = os.path.basename(path)[:-4]
        y = labels_for(path, len(prob))
        print(f"\n=== {name}  {len(audio) / RATE:.1f}s ===")
        if y is None:
            active = (prob > 0.5).mean()
            print(f"    no labels; model calls {active * 100:.1f}% of it speech "
                  f"(p50 {np.percentile(prob, 50):.3f} p90 {np.percentile(prob, 90):.3f})")
            continue

        # The whole curve, because a single threshold is a policy and this is a
        # measurement. Recall is what the loss was weighted for; precision is
        # what it was weighted against.
        print(f"    {'thresh':>7} {'recall':>8} {'precision':>10} {'sent':>7}")
        for t in (0.9, 0.7, 0.5, 0.3, 0.1):
            p = prob > t
            hit = float((p & (y > 0.5)).sum())
            miss = float((~p & (y > 0.5)).sum())
            fa = float((p & (y <= 0.5)).sum())
            rec = hit / max(1.0, hit + miss)
            pre = hit / max(1.0, hit + fa)
            print(f"    {t:>7.1f} {rec * 100:>7.1f}% {pre * 100:>9.1f}% "
                  f"{p.mean() * 100:>6.1f}%")


if __name__ == "__main__":
    main()
