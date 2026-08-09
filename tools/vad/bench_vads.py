# -*- coding: utf-8 -*-
"""Score every VAD we can get hold of on the same labelled audio.

Two clips, and they answer different questions:

  * `voice over music`, with hand-marked speech spans, gives recall and
    precision. It is the only labelled speech-in-music this project has.
  * `music only`, where the correct output is silence everywhere, gives a false
    positive rate with no labelling judgement in it at all. Every frame a
    detector fires on is wrong by construction.

The second is the one that matters here, and it is why this is worth running:
a detector can look excellent on the first by simply being generous, and the
music-only clip is where that shows up.

Reported threshold-free where possible. A model judged at one cut point can be
made to look like anything, and the chain's own features are already scored by
AUC in docs/MUSIC_GATE.md, so the numbers line up.

    python bench_vads.py

Needs: ten-vad, silero-vad, webrtcvad(-wheels). Any that are missing are
reported as missing rather than silently skipped.
"""
import io
import os
import sys

import numpy as np

RATE = 48_000
BLOCK = 480  # 10 ms, the chain's own block, so results are comparable

RIDES = r'C:\ml_data\rides'
CLIPS = [
    ('voice over music', '20260809-1201-000', '20260809-1201-000.speech'),
    ('music only', '20260809-0142-000', None),
]


def load(stem):
    x = np.fromfile(os.path.join(RIDES, stem + '.raw'), dtype='<f4')
    return x


def labels(path, n_blocks):
    """Per-block truth from the hand-marked spans; all False when unlabelled."""
    y = np.zeros(n_blocks, dtype=bool)
    if path is None:
        return y
    with io.open(os.path.join(RIDES, path), encoding='utf-8') as f:
        for line in f:
            if line.startswith('#'):
                continue
            parts = line.split()
            if len(parts) < 2:
                continue
            try:
                a, b = float(parts[0]), float(parts[1])
            except ValueError:
                continue
            y[int(a * 100):int(b * 100)] = True
    return y


def to_16k(x):
    """48 kHz to 16 kHz by averaging threes.

    Not a resampler. It is what `ten_eval.py` already does, and using the same
    one keeps these numbers comparable with the runs recorded in README.md.
    """
    n = (len(x) // 3) * 3
    return x[:n].reshape(-1, 3).mean(axis=1)


def auc(score, truth):
    """P(score higher on speech than on not), ties half. 0.5 is a coin."""
    a = score[truth]
    b = score[~truth]
    if len(a) == 0 or len(b) == 0:
        return float('nan')
    order = np.argsort(np.concatenate([a, b]), kind='mergesort')
    ranks = np.empty(len(order), dtype=np.float64)
    ranks[order] = np.arange(1, len(order) + 1)
    v = np.concatenate([a, b])
    # Average ranks within ties, so a detector that outputs 0/1 is not flattered.
    sv = v[order]
    i = 0
    while i < len(sv):
        j = i
        while j + 1 < len(sv) and sv[j + 1] == sv[i]:
            j += 1
        if j > i:
            ranks[order[i:j + 1]] = (ranks[order[i]] + ranks[order[j]]) / 2.0
        i = j + 1
    return (ranks[:len(a)].sum() - len(a) * (len(a) + 1) / 2.0) / (len(a) * len(b))


def per_block(prob, hop_s, n_blocks):
    """A detector's own frame rate, resampled onto the chain's 10 ms blocks."""
    if len(prob) == 0:
        return np.zeros(n_blocks)
    idx = np.minimum((np.arange(n_blocks) * 0.01 / hop_s).astype(int),
                     len(prob) - 1)
    return np.asarray(prob)[idx]


# ---- the detectors -------------------------------------------------------

def run_ten(x):
    from ten_vad import TenVad
    hop = 256
    a = np.clip(to_16k(x) * 32768.0, -32768, 32767).astype(np.int16)
    vad = TenVad(hop, 0.5)
    out = []
    for i in range(0, len(a) - hop + 1, hop):
        p, _ = vad.process(a[i:i + hop])
        out.append(p)
    return np.array(out), hop / 16_000.0


def run_silero(x):
    import torch
    from silero_vad import load_silero_vad
    model = load_silero_vad()
    a = torch.from_numpy(to_16k(x).astype(np.float32))
    win = 512  # what the 16 kHz model is built for; anything else it rejects
    out = []
    model.reset_states()
    with torch.no_grad():
        for i in range(0, len(a) - win + 1, win):
            out.append(float(model(a[i:i + win], 16_000).item()))
    return np.array(out), win / 16_000.0


def run_webrtc(x, aggressiveness=2):
    import webrtcvad
    vad = webrtcvad.Vad(aggressiveness)
    a = np.clip(to_16k(x) * 32768.0, -32768, 32767).astype(np.int16)
    win = 320  # 20 ms, one of the three lengths it accepts
    out = []
    for i in range(0, len(a) - win + 1, win):
        out.append(1.0 if vad.is_speech(a[i:i + win].tobytes(), 16_000) else 0.0)
    return np.array(out), win / 16_000.0


DETECTORS = [
    ('TEN VAD', run_ten),
    ('Silero', run_silero),
    ('WebRTC agg=1', lambda x: run_webrtc(x, 1)),
    ('WebRTC agg=3', lambda x: run_webrtc(x, 3)),
]


def main():
    results = {}
    for title, stem, label_file in CLIPS:
        x = load(stem)
        n = len(x) // BLOCK
        truth = labels(label_file, n)
        print('\n=== %s  (%s)  %.1f s, %d blocks, speech %.1f%% ==='
              % (title, stem, len(x) / RATE, n, 100.0 * truth.mean()))
        for name, fn in DETECTORS:
            try:
                prob, hop = fn(x)
            except Exception as e:  # noqa: BLE001 - a missing model is a result
                print('  %-14s unavailable: %s' % (name, e))
                continue
            p = per_block(prob, hop, n)
            fired = p >= 0.5
            row = {'fired': float(fired.mean())}
            if truth.any():
                row['auc'] = auc(p, truth)
                row['recall'] = float(fired[truth].mean())
                row['precision'] = float(truth[fired].mean()) if fired.any() else float('nan')
                print('  %-14s AUC %.3f   fires on %5.1f%%   keeps %5.1f%% of speech'
                      '   %5.1f%% of what it fired on was speech'
                      % (name, row['auc'], 100 * row['fired'],
                         100 * row['recall'], 100 * row['precision']))
            else:
                print('  %-14s fires on %5.1f%% of music-only audio '
                      '(every one is a false positive)'
                      % (name, 100 * row['fired']))
            results.setdefault(name, {})[title] = row
    return results


if __name__ == '__main__':
    main()
