"""Run a neural VAD over the helmet recordings and say what it finds.

Two questions, in order of importance:

  1. Does it find speech the current chain misses? The rider says there is a
     lot of it, and if so every recall number measured so far is optimistic --
     they were computed against three hand-labelled seconds.
  2. Does it agree with the labels we do have?

Output is segments and clips, not a score. A score computed against
incomplete labels is worse than no score, because it looks like an answer.
"""

import os
import sys
import json
import numpy as np
import onnxruntime as ort

RATE = 48_000
VAD_RATE = 16_000
DECIMATE = 3


def load_raw(path):
    x = np.fromfile(path, dtype=np.float32)
    return x


def to_16k(x):
    """Decimate 48k -> 16k with a short moving average, as the Rust does."""
    n = (len(x) // DECIMATE) * DECIMATE
    return x[:n].reshape(-1, DECIMATE).mean(axis=1).astype(np.float32)


class Silero:
    """Silero VAD v5: 512-sample windows at 16 kHz, carrying a 2x1x128 state.

    The 64 samples of CONTEXT are not optional and not documented anywhere you
    would look first. Each call is fed the previous chunk's last 64 samples
    followed by the current 512, so the tensor is 576 wide. Feeding a bare 512
    runs without error, returns plausible-looking probabilities, and detects
    nothing whatsoever -- eleven seconds of clean synthesised speech peaked at
    0.21 against a 0.5 threshold. It looked exactly like a model that could not
    cope with the audio.
    """

    WINDOW = 512
    CONTEXT = 64

    def __init__(self, path):
        opts = ort.SessionOptions()
        opts.inter_op_num_threads = 1
        opts.intra_op_num_threads = 1
        self.sess = ort.InferenceSession(path, opts, providers=["CPUExecutionProvider"])
        self.inputs = [i.name for i in self.sess.get_inputs()]

    def run(self, audio):
        """Speech probability per 512-sample window (32 ms)."""
        state = np.zeros((2, 1, 128), dtype=np.float32)
        context = np.zeros(self.CONTEXT, dtype=np.float32)
        out = []
        for i in range(0, len(audio) - self.WINDOW + 1, self.WINDOW):
            chunk = audio[i : i + self.WINDOW]
            fed = np.concatenate([context, chunk]).reshape(1, -1)
            result = self.sess.run(
                None,
                {
                    "input": fed,
                    "state": state,
                    "sr": np.array(VAD_RATE, dtype=np.int64),
                },
            )
            out.append(float(result[0].item()))
            state = result[1]
            context = chunk[-self.CONTEXT :]
        return np.array(out, dtype=np.float32)


def segments(prob, hop_s, threshold=0.5, min_speech_s=0.20, pad_s=0.15):
    """Turn a probability track into [start, end] spans in seconds."""
    on = prob >= threshold
    spans = []
    start = None
    for i, v in enumerate(on):
        if v and start is None:
            start = i
        elif not v and start is not None:
            spans.append((start * hop_s, i * hop_s))
            start = None
    if start is not None:
        spans.append((start * hop_s, len(on) * hop_s))

    merged = []
    for s, e in spans:
        if e - s < min_speech_s:
            continue
        s, e = max(0.0, s - pad_s), e + pad_s
        if merged and s <= merged[-1][1]:
            merged[-1] = (merged[-1][0], max(merged[-1][1], e))
        else:
            merged.append((s, e))
    return merged


def main():
    src = sys.argv[1]
    model = sys.argv[2]
    vad = Silero(model)
    print(f"model inputs: {vad.inputs}", file=sys.stderr)

    report = {}
    for name in sorted(os.listdir(src)):
        if not name.endswith(".raw"):
            continue
        path = os.path.join(src, name)
        x = load_raw(path)
        audio = to_16k(x)
        prob = vad.run(audio)
        hop = Silero.WINDOW / VAD_RATE

        spans = segments(prob, hop)
        total = sum(e - s for s, e in spans)
        seconds = len(x) / RATE
        stem = name[:-4]
        print(f"\n=== {stem}  {seconds:.1f}s ===")
        print(f"    speech found: {total:.1f}s in {len(spans)} segments "
              f"({total / seconds * 100:.1f}% of the clip)")
        print(f"    p50 prob {np.percentile(prob, 50):.3f}  "
              f"p90 {np.percentile(prob, 90):.3f}  max {prob.max():.3f}")
        shown = ", ".join(f"{s:.1f}-{e:.1f}" for s, e in spans[:14])
        print(f"    segments: {shown}{' ...' if len(spans) > 14 else ''}")
        report[stem] = {"seconds": seconds, "spans": [[round(s, 2), round(e, 2)] for s, e in spans]}

    with open(os.path.join(src, "silero_segments.json"), "w") as f:
        json.dump(report, f, indent=1)
    print(f"\nwrote {os.path.join(src, 'silero_segments.json')}")


if __name__ == "__main__":
    main()
