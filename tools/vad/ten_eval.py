"""TEN VAD over the same audio, scored the same way, with the same control.

The control is not a formality. The first Silero run here reported zero speech
in everything including clean synthesised speech, and the cause was a missing
64-sample context that the API accepts the absence of without complaint. Any
model whose harness has not been shown to work on audio that is obviously
speech is reporting on the harness, not the audio.
"""

import os
import sys
import inspect
import numpy as np
from ten_vad import TenVad

RATE = 48_000
VAD_RATE = 16_000
DECIMATE = 3
HOP = 256  # 16 ms at 16 kHz, one of the two configurations it is tuned for


def to_16k_int16(x):
    n = (len(x) // DECIMATE) * DECIMATE
    y = x[:n].reshape(-1, DECIMATE).mean(axis=1)
    return np.clip(y * 32768.0, -32768, 32767).astype(np.int16)


def run(audio_i16, threshold=0.5):
    vad = TenVad(HOP, threshold)
    probs = []
    for i in range(0, len(audio_i16) - HOP + 1, HOP):
        p, _flag = vad.process(audio_i16[i : i + HOP])
        probs.append(float(p))
    return np.array(probs, dtype=np.float32)


def spans_from(prob, hop_s, threshold=0.5, min_s=0.20, pad_s=0.15):
    on = prob >= threshold
    out, start = [], None
    for i, v in enumerate(on):
        if v and start is None:
            start = i
        elif not v and start is not None:
            out.append((start * hop_s, i * hop_s))
            start = None
    if start is not None:
        out.append((start * hop_s, len(on) * hop_s))
    merged = []
    for s, e in out:
        if e - s < min_s:
            continue
        s, e = max(0.0, s - pad_s), e + pad_s
        if merged and s <= merged[-1][1]:
            merged[-1] = (merged[-1][0], max(merged[-1][1], e))
        else:
            merged.append((s, e))
    return merged


def main():
    print("TenVad signature:", inspect.signature(TenVad.__init__), file=sys.stderr)
    hop_s = HOP / VAD_RATE
    for src in sys.argv[1:]:
        for name in sorted(os.listdir(src)):
            if not name.endswith(".raw"):
                continue
            x = np.fromfile(os.path.join(src, name), dtype=np.float32)
            prob = run(to_16k_int16(x))
            spans = spans_from(prob, hop_s)
            total = sum(e - s for s, e in spans)
            seconds = len(x) / RATE
            print(f"\n=== {name[:-4]}  {seconds:.1f}s ===")
            print(f"    speech found: {total:.1f}s in {len(spans)} segments "
                  f"({total / seconds * 100:.1f}%)")
            print(f"    p50 {np.percentile(prob, 50):.3f}  p90 {np.percentile(prob, 90):.3f}  "
                  f"max {prob.max():.3f}")
            if spans:
                print("    segments: " + ", ".join(f"{s:.1f}-{e:.1f}" for s, e in spans[:12]))


if __name__ == "__main__":
    main()
