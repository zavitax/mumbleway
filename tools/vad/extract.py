"""Cut out everything a neural VAD thinks might be speech, for a human to judge.

Deliberately generous: the threshold is well below the one Silero ships with,
because the question here is not "what is confidently speech" but "what did the
current chain throw away that it should not have". A false alarm costs the
listener three seconds; a miss costs the whole point of the exercise.

Segments are found on the SUPPRESSED audio, where the model works far better,
and cut from the ORIGINAL, so what comes out sounds like a recording rather
than like the inside of a denoiser.
"""

import os
import sys
import subprocess
import numpy as np

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from evaluate import Silero, load_raw, to_16k, segments, VAD_RATE

THRESHOLD = 0.30
SOURCES = {
    "New_Recording": "New Recording.m4a",
    "New_Recording_2": "New Recording 2.m4a",
    "New_Recording_3": "New Recording 3.m4a",
    "New_Recording_4": "New Recording 4.m4a",
}


def main():
    denoised, original, out, model = sys.argv[1:5]
    os.makedirs(out, exist_ok=True)
    vad = Silero(model)

    total_found = 0.0
    for stem, src in SOURCES.items():
        path = os.path.join(denoised, f"{stem}__Helmet.raw")
        if not os.path.exists(path):
            continue
        prob = vad.run(to_16k(load_raw(path)))
        hop = Silero.WINDOW / VAD_RATE
        spans = segments(prob, hop, threshold=THRESHOLD, min_speech_s=0.15, pad_s=0.4)

        print(f"\n{stem}: {len(spans)} candidate segments at p>={THRESHOLD}")
        for i, (s, e) in enumerate(spans):
            lo = int(s / hop)
            hi = max(lo + 1, int(e / hop))
            peak = float(prob[lo:hi].max()) if hi <= len(prob) else 0.0
            total_found += e - s
            name = f"{stem}_{i:02d}_{s:07.2f}s_p{int(peak * 100):03d}.m4a"
            subprocess.run(
                ["ffmpeg", "-y", "-loglevel", "error",
                 "-ss", f"{max(0.0, s):.2f}", "-t", f"{e - s:.2f}",
                 "-i", os.path.join(original, src), "-c", "copy",
                 os.path.join(out, name)],
                check=False,
            )
            print(f"    {s:7.2f} - {e:7.2f}s  peak p={peak:.2f}  -> {name}")

    print(f"\ntotal candidate speech: {total_found:.1f}s across {len(SOURCES)} recordings")
    print(f"clips in: {out}")


if __name__ == "__main__":
    main()
