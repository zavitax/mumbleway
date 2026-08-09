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

    python yamnet_threshold.py
"""
import os
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


def main():
    from ai_edge_litert.interpreter import Interpreter
    names = labels()
    music = [i for i, n in enumerate(names) if n.strip().lower() == 'music'][0]

    it = Interpreter(model_path=MODEL)
    it.allocate_tensors()
    inp = it.get_input_details()[0]['index']
    out = it.get_output_details()[0]['index']

    scored = []
    for title, rel, wanted in CLIPS:
        p = os.path.join(RIDES, rel.replace('/', os.sep))
        if not os.path.exists(p):
            print('%-18s missing' % title)
            continue
        x = to16k(np.fromfile(p, dtype='<f4'))
        s = []
        for i in range(0, len(x) - FRAME + 1, FRAME):
            it.set_tensor(inp, x[i:i + FRAME])
            it.invoke()
            s.append(it.get_tensor(out)[0][music])
        scored.append((title, wanted, np.array(s)))

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
