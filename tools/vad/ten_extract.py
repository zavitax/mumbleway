"""Cut out everything TEN VAD finds, from raw and suppressed together.

The union of the two, because they disagree in both directions and neither is
the authority here -- the rider's ear is. On one recording the model finds
speech on the raw microphone that it loses after suppression, and on another
the reverse. Taking either alone would throw away real candidates to keep a
tidier story.
"""

import os
import sys
import subprocess
import numpy as np

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from ten_eval import HOP, VAD_RATE, run, spans_from, to_16k_int16

THRESHOLD = 0.40
SOURCES = {
    "New_Recording": "New Recording.m4a",
    "New_Recording_2": "New Recording 2.m4a",
    "New_Recording_3": "New Recording 3.m4a",
    "New_Recording_4": "New Recording 4.m4a",
}


def spans_for(path):
    if not os.path.exists(path):
        return []
    x = np.fromfile(path, dtype=np.float32)
    prob = run(to_16k_int16(x))
    return spans_from(prob, HOP / VAD_RATE, threshold=THRESHOLD, min_s=0.15, pad_s=0.35)


def merge(a, b):
    both = sorted(a + b)
    out = []
    for s, e in both:
        if out and s <= out[-1][1]:
            out[-1] = (out[-1][0], max(out[-1][1], e))
        else:
            out.append((s, e))
    return out


def main():
    raw_dir, denoised_dir, original, out = sys.argv[1:5]
    os.makedirs(out, exist_ok=True)
    total = 0.0
    count = 0

    for stem, src in SOURCES.items():
        spans = merge(
            spans_for(os.path.join(raw_dir, f"{stem}.raw")),
            spans_for(os.path.join(denoised_dir, f"{stem}__Helmet.raw")),
        )
        print(f"\n{stem}: {len(spans)} candidates")
        for i, (s, e) in enumerate(spans):
            total += e - s
            count += 1
            name = f"{stem}_{i:02d}_{s:07.2f}s.m4a"
            subprocess.run(
                ["ffmpeg", "-y", "-loglevel", "error",
                 "-ss", f"{max(0.0, s):.2f}", "-t", f"{max(0.6, e - s):.2f}",
                 "-i", os.path.join(original, src), "-c", "copy",
                 os.path.join(out, name)],
                check=False,
            )
            print(f"    {s:7.2f} - {e:7.2f}s -> {name}")

    print(f"\n{count} clips, {total:.1f}s of candidate speech")
    print(f"in: {out}")


if __name__ == "__main__":
    main()
