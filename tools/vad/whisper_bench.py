# -*- coding: utf-8 -*-
"""Whisper on the same labelled audio, scored like every other detector.

An ASR model asks a stronger question than a VAD -- "what was said" rather than
"is this speech-shaped" -- so it should be the ceiling. It is also the one
detector with a known failure mode on exactly this material: Whisper is famous
for inventing text over music, and the music-only clip is the cleanest possible
test of that, since every word it emits there is a hallucination.

`vad_filter` is OFF on purpose. faster-whisper's filter is Silero, and leaving
it on would score Silero again under another name.

Nothing here ships -- Whisper is orders of magnitude too heavy for a helmet.
This is the oracle, and the point is to find out how much headroom the
real-time candidates are leaving.

    python whisper_bench.py [model]        # default: base
"""
import io
import os
import sys

import numpy as np

RATE = 48_000
BLOCK = 480
RIDES = r'C:\ml_data\rides'
CLIPS = [
    ('voice over music', '20260809-1201-000', '20260809-1201-000.speech'),
    ('music only', '20260809-0142-000', None),
]


def load16k(stem):
    x = np.fromfile(os.path.join(RIDES, stem + '.raw'), dtype='<f4')
    n = (len(x) // 3) * 3
    return x[:n].reshape(-1, 3).mean(axis=1).astype(np.float32), len(x) // BLOCK


def labels(path, n):
    y = np.zeros(n, dtype=bool)
    if path is None:
        return y
    with io.open(os.path.join(RIDES, path), encoding='utf-8') as f:
        for line in f:
            if line.startswith('#'):
                continue
            p = line.split()
            if len(p) >= 2:
                try:
                    y[int(float(p[0]) * 100):int(float(p[1]) * 100)] = True
                except ValueError:
                    pass
    return y


def main():
    from faster_whisper import WhisperModel
    size = sys.argv[1] if len(sys.argv) > 1 else 'base'
    model = WhisperModel(size, device='cpu', compute_type='int8')
    print('whisper %s, vad_filter OFF\n' % size)

    for title, stem, label_file in CLIPS:
        audio, n = load16k(stem)
        truth = labels(label_file, n)
        segments, _ = model.transcribe(audio, language='en', vad_filter=False,
                                       condition_on_previous_text=False)
        said = np.zeros(n, dtype=bool)
        conf = np.zeros(n)
        text, count = [], 0
        for s in segments:
            count += 1
            a, b = int(s.start * 100), min(n, int(s.end * 100))
            said[a:b] = True
            conf[a:b] = 1.0 - float(getattr(s, 'no_speech_prob', 0.0))
            t = s.text.strip()
            if t:
                text.append(t)
        print('=== %s (%s) ===' % (title, stem))
        print('  %d segments, marked %.1f%% of the clip as speech'
              % (count, 100 * said.mean()))
        if truth.any():
            print('  keeps %.1f%% of labelled speech; %.1f%% of what it marked '
                  'was speech'
                  % (100 * said[truth].mean(),
                     100 * truth[said].mean() if said.any() else float('nan')))
        else:
            print('  every one of those blocks is a false positive: %.1f%%'
                  % (100 * said.mean()))
            joined = ' '.join(text)
            print('  it transcribed %d characters from music: %r'
                  % (len(joined), joined[:300]))
        print()


if __name__ == '__main__':
    main()
