# -*- coding: utf-8 -*-
"""Can DeepFilterNet keep up with a 10 ms block, one block at a time?

`dfn_enhance.py` shows what it does to the audio, processing a whole file at
once. That is the wrong shape for this app twice over: the capture worker gets
480 samples every 10 ms and must return before the next ones arrive, and it may
not allocate while it does. "Ten times real time over two minutes" says nothing
about either.

So this runs the model the way the chain would: one hop at a time, statefully,
timing every call. What matters is not the mean but the **tail** -- one block
over budget is a click in somebody's helmet, and the mean hides it.

    python dfn_realtime.py [clip.raw] [seconds]

Reports the model's own frame geometry first, because if its hop is not our
10 ms then everything after it is a different question.
"""
import os
import sys
import time

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
DEFAULT = r'C:\ml_data\rides\20260809-1201-000.raw'

# What the capture chain gives the worker, from core/src/audio/denoise.rs.
OUR_BLOCK = 480
OUR_RATE = 48000


def _shim_torchaudio():
    """See dfn_enhance.py -- the same removed symbol."""
    import types
    import torchaudio

    if getattr(torchaudio, 'backend', None) is not None:
        return
    from dataclasses import dataclass

    @dataclass
    class AudioMetaData:
        sample_rate: int
        num_frames: int
        num_channels: int
        bits_per_sample: int
        encoding: str

    common = types.ModuleType('torchaudio.backend.common')
    common.AudioMetaData = AudioMetaData
    backend = types.ModuleType('torchaudio.backend')
    backend.common = common
    torchaudio.backend = backend
    sys.modules['torchaudio.backend'] = backend
    sys.modules['torchaudio.backend.common'] = common


def main():
    import torch
    torch.set_num_threads(1)  # A phone's audio thread is one thread.
    _shim_torchaudio()
    from df.enhance import enhance, init_df

    model, state, _ = init_df()
    model.eval()
    sr = state.sr()
    hop = state.hop_size()
    fft = state.fft_size()

    print('DeepFilterNet: %d Hz, hop %d samples (%.1f ms), FFT %d'
          % (sr, hop, 1000.0 * hop / sr, fft))
    print('this app:      %d Hz, block %d samples (%.1f ms)'
          % (OUR_RATE, OUR_BLOCK, 1000.0 * OUR_BLOCK / OUR_RATE))
    if sr != OUR_RATE or hop != OUR_BLOCK:
        print('\n** The geometry does not match, so a block would have to be '
              'buffered or\n** resampled before the model saw it. Everything '
              'below measures the model,\n** not the integration.')
    else:
        print('\nSame rate and the same hop: a capture block is exactly one '
              'model frame.')

    path = sys.argv[1] if len(sys.argv) > 1 else DEFAULT
    seconds = float(sys.argv[2]) if len(sys.argv) > 2 else 30.0
    x = np.fromfile(path, dtype='<f4')[:int(seconds * OUR_RATE)]
    blocks = len(x) // hop
    print('\n%s, %d blocks of %d samples\n' % (os.path.basename(path), blocks, hop))

    # One hop at a time, through the model, timing each. `enhance` is the only
    # entry point this package exposes, and on a single hop it is what the
    # worker would be calling -- allocations, state handling and all.
    times = []
    with torch.no_grad():
        for i in range(blocks):
            frame = torch.from_numpy(x[i * hop:(i + 1) * hop].copy()).unsqueeze(0)
            t0 = time.perf_counter()
            enhance(model, state, frame, pad=False)
            times.append((time.perf_counter() - t0) * 1000.0)

    t = np.array(times[2:])  # the first two carry one-off setup
    budget = 1000.0 * hop / sr
    print('%-22s %8s' % ('per block, ms', ''))
    for name, v in (('mean', t.mean()), ('median', np.median(t)),
                    ('p95', np.percentile(t, 95)),
                    ('p99', np.percentile(t, 99)), ('worst', t.max())):
        over = ' OVER BUDGET' if v > budget else ''
        print('  %-20s %8.2f%s' % (name, v, over))
    print('\nbudget is %.1f ms. %.1f%% of blocks were over it.'
          % (budget, 100.0 * (t > budget).mean()))
    print('Single-threaded on a desktop CPU, in Python. A Rust build on an ARM'
          '\ncore is a different number -- this bounds the shape, not the value.')


if __name__ == '__main__':
    main()
