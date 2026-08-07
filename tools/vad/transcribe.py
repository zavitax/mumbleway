"""Find the speech with an ASR model, because it knows what words sound like.

A VAD asks "is this speech-shaped". Whisper asks "what was said", and that is a
far stronger question to bring to audio where the voice is buried: a model that
can only recognise the *shape* of speech has nothing left to work with once the
wind is louder than the talker, while one carrying a language model can still
resolve words from fragments. Two VADs have now been run over these recordings
and the rider says both are still missing speech.

Nothing here ships. Whisper is orders of magnitude too heavy for a phone in a
helmet. It is used as an ORACLE: to produce labels good enough that the
real-time candidates can finally be scored against something complete, instead
of against three seconds of stopwatch work.

Run over the suppressed audio as well as the raw, because the chain's own
suppression was measured to help a neural VAD substantially, and there is no
reason to expect an ASR model to differ.
"""

import os
import sys
import json
import numpy as np


def _add_cuda_dlls():
    """Make the pip-installed CUDA libraries findable on Windows.

    Python 3.8 stopped using PATH to resolve dependent DLLs, so putting the
    nvidia wheel directories on PATH does nothing at all and the failure is
    `cublas64_12.dll is not found` while the file sits right there. The
    directories have to be registered explicitly.
    """
    if not hasattr(os, "add_dll_directory"):
        return
    try:
        import nvidia
    except ImportError:
        return
    # A namespace package: __file__ is None, __path__ is the real answer.
    for root in list(getattr(nvidia, "__path__", [])):
        for dirpath, _dirs, files in os.walk(root):
            if any(f.endswith(".dll") for f in files):
                try:
                    os.add_dll_directory(dirpath)
                except OSError:
                    pass


_add_cuda_dlls()

from faster_whisper import WhisperModel  # noqa: E402  (must follow the DLL setup)

RATE = 48_000
DECIMATE = 3


def to_16k(path):
    x = np.fromfile(path, dtype=np.float32)
    n = (len(x) // DECIMATE) * DECIMATE
    return x[:n].reshape(-1, DECIMATE).mean(axis=1).astype(np.float32)


def main():
    src = sys.argv[1]
    size = sys.argv[2] if len(sys.argv) > 2 else "large-v3"
    device = "cuda" if os.environ.get("WHISPER_CPU") is None else "cpu"
    compute = "float16" if device == "cuda" else "int8"

    print(f"loading {size} on {device}/{compute}", file=sys.stderr)
    model = WhisperModel(size, device=device, compute_type=compute)

    report = {}
    for name in sorted(os.listdir(src)):
        if not name.endswith(".raw"):
            continue
        audio = to_16k(os.path.join(src, name))
        stem = name[:-4]

        segments, info = model.transcribe(
            audio,
            beam_size=5,
            # No VAD filter: the point is to find what a VAD would miss, and
            # letting one gate the input would build the very blind spot this
            # is meant to see around.
            vad_filter=False,
            word_timestamps=True,
            condition_on_previous_text=False,
            # Wind produces confident nonsense at high temperature; falling
            # back through temperatures lets a segment be reconsidered rather
            # than committed to.
            temperature=[0.0, 0.2, 0.4],
        )

        found = []
        for seg in segments:
            found.append(
                {
                    "start": round(seg.start, 2),
                    "end": round(seg.end, 2),
                    "text": seg.text.strip(),
                    "logprob": round(seg.avg_logprob, 3),
                    "no_speech": round(seg.no_speech_prob, 3),
                }
            )

        total = sum(s["end"] - s["start"] for s in found)
        print(f"\n=== {stem}  ({len(audio) / 16000:.1f}s, lang={info.language} "
              f"p={info.language_probability:.2f}) ===")
        print(f"    {len(found)} segments, {total:.1f}s of speech")
        for s in found:
            flag = " " if s["no_speech"] < 0.5 else "?"
            print(f"   {flag}{s['start']:8.2f} - {s['end']:8.2f}  "
                  f"lp={s['logprob']:6.2f} ns={s['no_speech']:.2f}  {s['text'][:70]}")
        report[stem] = found

    out = os.path.join(src, "whisper_segments.json")
    with open(out, "w", encoding="utf-8") as f:
        json.dump(report, f, indent=1, ensure_ascii=False)
    print(f"\nwrote {out}")


if __name__ == "__main__":
    main()
