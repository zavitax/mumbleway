# -*- coding: utf-8 -*-
"""Detect music over seconds, so Auto can choose Helmet when there is music.

Every feature tried against this fault so far judged a 10 ms block, and all five
failed. Music is not a property of a block: a single 10 ms slice of a guitar and
of a vowel look alike, which is exactly why RNNoise's VAD votes for both. What
separates them is structure over seconds -- a beat, and partials that hold still
while a voice moves.

This is also a cheaper place to act. The transmit gate is one mistake away from
cutting a rider off; the *profile* is not. Helmet already transmits 13.8% of
music against Light's 73.3%, so a detector that only has to nudge Auto towards
Helmet does not have to be right every block, or even every second -- Auto
already has dwell and hysteresis to sit behind.

Measured on the raw microphone, upstream of RNNoise, which is the one property
docs/MUSIC_GATE.md establishes is needed and nothing in the chain has.

    python music_detect.py
"""
import os
import sys

import numpy as np

RATE = 48_000
BLOCK = 480          # 10 ms, the chain's own
WIN_S = 4.0          # long enough for two bars at any sane tempo
HOP_S = 1.0
ROOT = r'C:\ml_data'

# Positives are the two clips known to have music throughout. Negatives are real
# helmet audio from rides recorded before any music was involved.
POSITIVE = [('rides', '20260809-0142-000.raw'), ('rides', '20260809-1201-000.raw')]
NEGATIVE_DIRS = ['speech_road', 'noise_road']
NEGATIVE_RIDES = ['20260808-0512-000.raw', '20260808-0524-000.raw']


def envelope(x):
    """Per-block loudness in dB, which is what a beat lives in."""
    n = len(x) // BLOCK
    e = (x[:n * BLOCK].reshape(n, BLOCK) ** 2).mean(axis=1)
    return 10 * np.log10(np.maximum(e, 1e-12))


def beat(env):
    """Strength of the strongest periodicity between 0.3 s and 2 s.

    A bar of music repeats; wind, an engine at steady throttle and a talker do
    not. Computed on the envelope rather than the waveform, so it is deaf to
    pitch and to level -- a quiet stereo across a car park has the same beat as
    a loud one, which is the case the level-driven Auto gets wrong today.
    """
    e = env - env.mean()
    if not np.any(e):
        return 0.0
    ac = np.correlate(e, e, mode='full')[len(e) - 1:]
    if ac[0] <= 0:
        return 0.0
    ac = ac / ac[0]
    lo, hi = int(0.3 / 0.01), min(int(2.0 / 0.01), len(ac) - 1)
    return float(ac[lo:hi].max()) if hi > lo else 0.0


def tonal_persistence(x, win=2048, hop=1024):
    """How much of the spectrum holds still.

    A held note keeps its partials in the same bins for hundreds of
    milliseconds. A voice moves: formants shift within a syllable and f0 drifts
    across a phrase, which is the observation candidate 1 was built on -- used
    here over a window instead of per block.
    """
    n = (len(x) - win) // hop
    if n < 4:
        return 0.0
    w = np.hanning(win).astype(np.float32)
    frames = np.stack([np.abs(np.fft.rfft(x[i * hop:i * hop + win] * w))
                       for i in range(n)])
    frames = 20 * np.log10(np.maximum(frames, 1e-9))
    # The loudest bins of each frame, and how often the same ones come back.
    top = np.argsort(frames, axis=1)[:, -12:]
    counts = np.zeros(frames.shape[1])
    for row in top:
        counts[row] += 1
    counts /= n
    # Bins that are in the top set most of the time. Noise spreads its peaks
    # around and scores low however loud it is.
    return float((counts > 0.6).sum() / 12.0)


def windows(x):
    w, h = int(WIN_S * RATE), int(HOP_S * RATE)
    for i in range(0, max(0, len(x) - w) + 1, h):
        yield x[i:i + w]


def score_file(path):
    x = np.fromfile(path, dtype='<f4')
    out = []
    for seg in windows(x):
        env = envelope(seg)
        out.append((beat(env), tonal_persistence(seg)))
    return out


def collect():
    pos, neg = [], []
    for d, f in POSITIVE:
        pos += score_file(os.path.join(ROOT, d, f))
    for f in NEGATIVE_RIDES:
        p = os.path.join(ROOT, 'rides', f)
        if os.path.exists(p):
            neg += score_file(p)
    for d in NEGATIVE_DIRS:
        p = os.path.join(ROOT, d)
        if not os.path.isdir(p):
            continue
        for f in sorted(os.listdir(p)):
            if f.endswith('.raw'):
                neg += score_file(os.path.join(p, f))
    return np.array(pos), np.array(neg)


def auc(a, b):
    if len(a) == 0 or len(b) == 0:
        return float('nan')
    v = np.concatenate([a, b])
    order = np.argsort(v, kind='mergesort')
    r = np.empty(len(v))
    r[order] = np.arange(1, len(v) + 1)
    sv = v[order]
    i = 0
    while i < len(sv):
        j = i
        while j + 1 < len(sv) and sv[j + 1] == sv[i]:
            j += 1
        if j > i:
            r[order[i:j + 1]] = (r[order[i]] + r[order[j]]) / 2.0
        i = j + 1
    return (r[:len(a)].sum() - len(a) * (len(a) + 1) / 2.0) / (len(a) * len(b))


def main():
    pos, neg = collect()
    print('%d windows of music, %d of real helmet audio without it '
          '(%.0f s / %.0f s)'
          % (len(pos), len(neg), len(pos) * HOP_S, len(neg) * HOP_S))
    names = ['beat', 'tonal']
    for k, name in enumerate(names):
        a, b = pos[:, k], neg[:, k]
        print('\n%-6s AUC %.3f   music p10/p50/p90 %.2f/%.2f/%.2f'
              '   rest %.2f/%.2f/%.2f'
              % (name, auc(a, b),
                 np.percentile(a, 10), np.percentile(a, 50), np.percentile(a, 90),
                 np.percentile(b, 10), np.percentile(b, 50), np.percentile(b, 90)))
        for bar in np.percentile(b, [90, 95, 99]):
            print('        bar %.2f: catches %5.1f%% of music, %4.1f%% of the rest'
                  % (bar, 100 * (a >= bar).mean(), 100 * (b >= bar).mean()))
    both = pos[:, 0] * pos[:, 1], neg[:, 0] * neg[:, 1]
    print('\n%-6s AUC %.3f  (the two multiplied)' % ('both', auc(*both)))
    return pos, neg


if __name__ == '__main__':
    main()
