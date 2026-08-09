# -*- coding: utf-8 -*-
"""YAMNet, asked what the sound *is* rather than whether it is speech.

Every other detector here answers "speech or not". YAMNet answers "which of 521
things", two of which are `Speech` and `Music`, so it can say the thing the
profile decision actually wants to know: is there music playing. That is a
different question from the gate's, and a safer place to be wrong -- picking
Helmet needlessly costs some naturalness, where a wrong gate cuts a rider off.

It is also the right size for a phone. 4 MB of TFLite against Whisper's tens of
millions of parameters, one 0.975 s frame at a time.

    python yamnet_bench.py

Needs `ai-edge-litert` and `yamnet.tflite` beside this file (the MediaPipe
build, which carries its own label map).
"""
import io
import os
import sys
import zipfile

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
MODEL = os.path.join(HERE, 'yamnet.tflite')
RIDES = r'C:\ml_data'
FRAME = 15600            # 0.975 s at 16 kHz, what the model was built for


def labels():
    """The class names, out of the model's own metadata.

    Read rather than hardcoded: YAMNet's index for `Music` is widely quoted as
    132 and being wrong about it would not fail, it would quietly score the
    wrong class.
    """
    with zipfile.ZipFile(MODEL) as z:
        name = [n for n in z.namelist() if n.endswith('.txt')][0]
        return z.read(name).decode('utf-8').splitlines()


def to16k(x):
    n = (len(x) // 3) * 3
    return x[:n].reshape(-1, 3).mean(axis=1).astype(np.float32)


def run(path, interp, inp, out):
    x = to16k(np.fromfile(path, dtype='<f4'))
    scores = []
    for i in range(0, len(x) - FRAME + 1, FRAME):
        interp.set_tensor(inp, x[i:i + FRAME])
        interp.invoke()
        scores.append(interp.get_tensor(out)[0].copy())
    return np.array(scores)


def main():
    from ai_edge_litert.interpreter import Interpreter
    names = labels()
    music = [i for i, n in enumerate(names) if n.strip().lower() == 'music'][0]
    speech = [i for i, n in enumerate(names) if n.strip().lower() == 'speech'][0]
    print('Music is class %d, Speech is class %d, of %d\n' % (music, speech, len(names)))

    it = Interpreter(model_path=MODEL)
    it.allocate_tensors()
    inp = it.get_input_details()[0]['index']
    out = it.get_output_details()[0]['index']

    CLIPS = [
        ('music only',       'rides/20260809-0142-000.raw', True),
        ('voice over music', 'rides/20260809-1201-000.raw', True),
        ('ride, no music A', 'rides/20260808-0512-000.raw', False),
        ('ride, no music B', 'rides/20260808-0524-000.raw', False),
    ]
    pos, neg = [], []
    for title, rel, is_music in CLIPS:
        p = os.path.join(RIDES, rel.replace('/', os.sep))
        if not os.path.exists(p):
            print('%-18s missing' % title)
            continue
        s = run(p, it, inp, out)
        m, sp = s[:, music], s[:, speech]
        top = names[int(s.mean(axis=0).argmax())]
        print('%-18s %3d frames  Music p50 %.3f p90 %.3f   Speech p50 %.3f'
              '   loudest class on average: %s'
              % (title, len(s), np.percentile(m, 50), np.percentile(m, 90),
                 np.percentile(sp, 50), top))
        (pos if is_music else neg).extend(m.tolist())

    a, b = np.array(pos), np.array(neg)
    if len(a) and len(b):
        v = np.concatenate([a, b])
        r = np.empty(len(v))
        r[np.argsort(v, kind='mergesort')] = np.arange(1, len(v) + 1)
        auc = (r[:len(a)].sum() - len(a) * (len(a) + 1) / 2.0) / (len(a) * len(b))
        print('\nMusic score, clips with music vs clips without: AUC %.3f' % auc)
        for bar in (0.1, 0.2, 0.3, 0.5):
            print('  bar %.2f: fires on %5.1f%% of music, %5.1f%% of the rest'
                  % (bar, 100 * (a >= bar).mean(), 100 * (b >= bar).mean()))


if __name__ == '__main__':
    main()
