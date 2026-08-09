# -*- coding: utf-8 -*-
"""Picks the bar the app ships, from frames rather than from medians.

`yamnet_bench.py` reports a median per clip, which is enough to show the model
answers the right question and not enough to choose a threshold: what the app
does with a frame is take `Helmet` on it, so what matters is how often each clip
crosses a given bar, not where its middle sits.

The target is **"the background is loud and structured"**, not "music" -- see
`docs/MUSIC_GATE.md`. Under that target a ride with engine and wind is a
positive, which is the correction that retired the earlier false-positive
reading.

    python yamnet_threshold.py                  # the ride corpus
    python yamnet_threshold.py --librispeech    # and 40 clean-speech negatives
    python yamnet_threshold.py foo.wav bar.raw  # anything else, as positives

`--librispeech` is the answer to "one speaker, one room" on the negative side:
`dev-clean` is hundreds of speakers recorded by somebody else, and every frame
of it should score near zero. It cannot say anything about the positive side —
for that there is no substitute for another genre, and another room.
"""
import os
import subprocess
import sys
import zipfile

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
MODEL = os.path.join(HERE, 'yamnet.tflite')
RIDES = r'C:\ml_data'
FRAME = 15600

# Wanted, under the retargeted label. The name of the class is the model's; the
# meaning is ours.
CLIPS = [
    ('music only',        'rides/20260809-0142-000.raw', True),
    ('voice over music',  'rides/20260809-1201-000.raw', True),
    ('ride A, engine',    'rides/20260808-0512-000.raw', True),
    ('ride B, quiet',     'rides/20260808-0524-000.raw', False),
]
BARS = (0.05, 0.10, 0.20, 0.30, 0.40, 0.50)


def labels():
    with zipfile.ZipFile(MODEL) as z:
        name = [n for n in z.namelist() if n.endswith('.txt')][0]
        return z.read(name).decode('utf-8').splitlines()


def to16k(x):
    """The same three-tap box average the app's tap uses, deliberately."""
    n = (len(x) // 3) * 3
    return x[:n].reshape(-1, 3).mean(axis=1).astype(np.float32)


def load(path):
    """Reads a clip at 16 kHz mono, whatever it started as.

    The corpus `.raw` files are 48 kHz f32 and go through the app's own
    decimation; anything else is handed to ffmpeg, which resamples properly.
    That difference is deliberate — the corpus has to match what the app will
    see, and an outside file only has to be right.
    """
    if path.endswith('.raw'):
        return to16k(np.fromfile(path, dtype='<f4'))
    out = subprocess.run(
        ['ffmpeg', '-v', 'quiet', '-i', path, '-ac', '1', '-ar', '16000',
         '-f', 'f32le', '-'],
        stdout=subprocess.PIPE, check=True).stdout
    return np.frombuffer(out, dtype='<f4')


def librispeech(root, want=40):
    """A spread of `dev-clean` utterances, one per speaker where possible."""
    base = os.path.join(root, 'LibriSpeech', 'dev-clean')
    if not os.path.isdir(base):
        return []
    picked = []
    for speaker in sorted(os.listdir(base)):
        sdir = os.path.join(base, speaker)
        if not os.path.isdir(sdir):
            continue
        for chapter in sorted(os.listdir(sdir)):
            cdir = os.path.join(sdir, chapter)
            flacs = sorted(f for f in os.listdir(cdir) if f.endswith('.flac'))
            if flacs:
                picked.append(os.path.join(cdir, flacs[0]))
                break
        if len(picked) >= want:
            break
    return picked


def main():
    from ai_edge_litert.interpreter import Interpreter
    names = labels()
    music = [i for i, n in enumerate(names) if n.strip().lower() == 'music'][0]

    it = Interpreter(model_path=MODEL)
    it.allocate_tensors()
    inp = it.get_input_details()[0]['index']
    out = it.get_output_details()[0]['index']

    def score(x):
        s = []
        for i in range(0, len(x) - FRAME + 1, FRAME):
            it.set_tensor(inp, x[i:i + FRAME])
            it.invoke()
            s.append(it.get_tensor(out)[0][music])
        return np.array(s)

    args = [a for a in sys.argv[1:] if not a.startswith('--')]
    scored = []
    for title, rel, wanted in CLIPS:
        p = os.path.join(RIDES, rel.replace('/', os.sep))
        if not os.path.exists(p):
            print('%-18s missing' % title)
            continue
        scored.append((title, wanted, score(load(p))))

    # Hundreds of speakers, recorded by somebody else, in rooms none of this
    # was tuned in. Pooled into one row: what matters is whether *any* frame of
    # clean speech crosses the bar, not which utterance it came from.
    if '--librispeech' in sys.argv:
        files = librispeech(RIDES)
        if not files:
            print('LibriSpeech not found under %s' % RIDES)
        else:
            pooled = [score(load(f)) for f in files]
            pooled = [p for p in pooled if len(p)]
            if pooled:
                scored.append(('LibriSpeech x%d' % len(pooled), False,
                               np.concatenate(pooled)))

    for path in args:
        scored.append((os.path.basename(path)[:18], True, score(load(path))))

    head = '%-18s %-8s %5s  ' % ('clip', 'wanted', 'n')
    head += '  '.join('>=%.2f' % b for b in BARS)
    print(head)
    for title, wanted, s in scored:
        row = '%-18s %-8s %5d  ' % (title, 'Helmet' if wanted else 'no', len(s))
        row += '  '.join('%5.1f%%' % (100 * (s >= b).mean()) for b in BARS)
        print(row)

    print('\nA bar is safe when every Helmet clip fires often and the quiet one'
          ' never does.\nTaking Helmet costs some naturalness; releasing it at'
          ' speed loses the rider.')


if __name__ == '__main__':
    main()
