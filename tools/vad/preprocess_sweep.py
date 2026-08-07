"""Does the preprocessing explain what the models are missing?

Three faults in this project so far have been preprocessing rather than
audio -- a missing 64-sample context that made Silero detect nothing, a
harness clock that made loss scores meaningless, a patch left behind by a
timed-out script. "The model misses speech" is a claim about a pipeline until
the pipeline has been varied.

Two things in it are guesses worth testing:

  * The 48 -> 16 kHz step is a mean of three samples. That is a crude
    anti-alias filter with real droop from about 4 kHz, which is where the
    consonants a detector keys on live.

  * Nothing normalises the level. Both models are trained on speech at ordinary
    recording levels, and a rider's voice inside a helmet arrives far below
    that -- so the model may be being asked about audio quieter than anything
    in its training set.
"""

import os
import subprocess
import sys
import numpy as np

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from ten_eval import HOP, VAD_RATE, run, spans_from

DECIMATE = 3


def mean_of_three(x):
    n = (len(x) // DECIMATE) * DECIMATE
    return x[:n].reshape(-1, DECIMATE).mean(axis=1).astype(np.float32)


def proper_resample(path):
    """ffmpeg's polyphase resampler, which is what should have been used."""
    out = path + ".16k.f32"
    subprocess.run(
        ["ffmpeg", "-y", "-loglevel", "error", "-f", "f32le", "-ar", "48000",
         "-ac", "1", "-i", path, "-ar", "16000", "-f", "f32le", out],
        check=True,
    )
    return np.fromfile(out, dtype=np.float32)


def peak_normalise(x, target=0.5):
    p = float(np.abs(x).max())
    return x if p < 1e-6 else (x * (target / p)).astype(np.float32)


def sliding_agc(x, window_s=1.0, target=0.1, ceiling_db=30.0):
    """Lift quiet passages towards a normal level, bounded.

    A rider's voice does not fade in and out; the wind does. A per-window gain
    keyed on the loudest thing in the window would just track the wind, so this
    keys on a low percentile -- the quiet parts -- and lifts those.
    """
    w = int(window_s * VAD_RATE)
    if w < 2 or len(x) < w:
        return peak_normalise(x)
    ceiling = 10 ** (ceiling_db / 20.0)
    out = np.empty_like(x)
    for i in range(0, len(x), w):
        seg = x[i : i + w]
        rms = float(np.sqrt((seg**2).mean())) if len(seg) else 0.0
        gain = 1.0 if rms < 1e-7 else min(target / rms, ceiling)
        out[i : i + w] = seg * gain
    return np.clip(out, -1.0, 1.0).astype(np.float32)


def to_i16(x):
    return np.clip(x * 32768.0, -32768, 32767).astype(np.int16)


VARIANTS = {
    "mean-of-3 (current)": lambda p, x: mean_of_three(x),
    "proper resample": lambda p, x: proper_resample(p),
    "resample + peak norm": lambda p, x: peak_normalise(proper_resample(p)),
    "resample + sliding AGC": lambda p, x: sliding_agc(proper_resample(p)),
}


def main():
    src = sys.argv[1]
    threshold = float(sys.argv[2]) if len(sys.argv) > 2 else 0.5
    hop_s = HOP / VAD_RATE

    names = sorted(n for n in os.listdir(src) if n.endswith(".raw"))
    print(f"TEN VAD, threshold {threshold}\n")
    header = f"{'clip':<28}" + "".join(f"{k:>24}" for k in VARIANTS)
    print(header)
    print("-" * len(header))

    for name in names:
        path = os.path.join(src, name)
        x = np.fromfile(path, dtype=np.float32)
        seconds = len(x) / 48_000
        row = f"{name[:-4]:<28}"
        for fn in VARIANTS.values():
            audio = fn(path, x)
            prob = run(to_i16(audio), threshold)
            spans = spans_from(prob, hop_s, threshold=threshold)
            found = sum(e - s for s, e in spans)
            row += f"{found:>10.1f}s {len(spans):>3} seg  "
        print(row + f"   ({seconds:.0f}s clip)")


if __name__ == "__main__":
    main()
