# -*- coding: utf-8 -*-
"""DeepFilterNet over a corpus clip, for comparison against our own chain.

DeepFilterNet is a real-time *speech* enhancer: it keeps voice and removes
everything else. That is the same job `Helmet` does and a different method --
ours is RNNoise plus a gate plus filters chosen by level, and this is one
network trained end to end. `docs/MUSIC_GATE.md` is the record of our chain
leaving music on the wire, so the interesting question is what this does with
the same clip.

It runs at 48 kHz, which is what the corpus already is, so nothing is resampled
on the way in and the comparison is not confounded by a rate conversion.

    python dfn_enhance.py                       # the four corpus clips
    python dfn_enhance.py path/to/clip.raw ...  # anything else

Writes `<name>-dfn.wav` beside the source and, when YAMNet is available, says
what the enhancement did to the "loud structured background" score -- which is
this project's one measured proxy for "is there still music in here".
"""
import os
import sys
import time

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
RIDES = r'C:\ml_data\rides'
CLIPS = [
    '20260809-0142-000.raw',   # music only
    '20260809-1201-000.raw',   # voice over music
    '20260808-0512-000.raw',   # ride, engine and wind
    '20260808-0524-000.raw',   # ride, quiet, talking
]
FRAME = 15600


def _shim_torchaudio():
    """Puts back the one symbol DeepFilterNet imports and torchaudio removed.

    `df.io` does `from torchaudio.backend.common import AudioMetaData`, which
    was true in 2023 and is not now — the whole `torchaudio.backend` package is
    gone in 2.11, and `AudioMetaData` with it. DeepFilterNet 0.5.6 only uses it
    as a type, so a stand-in with the same fields is enough.

    The alternative is pinning torchaudio back three years, which would drag
    torch with it and break everything else in `tools/vad`. A dozen lines here
    is the cheaper side of that trade.
    """
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


def write_wav(path, x, sr):
    """16-bit mono WAV, with the standard library.

    Not `torchaudio.save`: 2.11 hands saving to `torchcodec`, which is another
    dependency to install for something `wave` has always done. Everything in
    this corpus is mono at one rate, so there is nothing here worth a library.
    """
    import wave
    pcm = np.clip(x, -1.0, 1.0)
    pcm = (pcm * 32767.0).astype('<i2')
    with wave.open(path, 'wb') as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(sr)
        w.writeframes(pcm.tobytes())


def music_score(x48):
    """Median YAMNet `Music` over a clip, or None if the model is not here."""
    try:
        from ai_edge_litert.interpreter import Interpreter
    except ImportError:
        return None
    model = os.path.join(HERE, 'yamnet.tflite')
    if not os.path.exists(model):
        return None
    n = (len(x48) // 3) * 3
    x = x48[:n].reshape(-1, 3).mean(axis=1).astype(np.float32)
    it = Interpreter(model_path=model)
    it.allocate_tensors()
    inp = it.get_input_details()[0]['index']
    out = it.get_output_details()[0]['index']
    scores = []
    for i in range(0, len(x) - FRAME + 1, FRAME):
        it.set_tensor(inp, x[i:i + FRAME])
        it.invoke()
        scores.append(it.get_tensor(out)[0][132])
    return float(np.median(scores)) if scores else None


def main():
    import torch
    _shim_torchaudio()
    from df.enhance import enhance, init_df

    model, state, _ = init_df()
    sr = state.sr()
    print('DeepFilterNet at %d Hz\n' % sr)

    args = sys.argv[1:] or [os.path.join(RIDES, c) for c in CLIPS]
    print('%-26s %7s  %6s  %7s -> %-7s' % ('clip', 'seconds', 'x real', 'music', 'music'))
    for path in args:
        if not os.path.exists(path):
            print('%-26s missing' % os.path.basename(path))
            continue
        if path.endswith('.raw'):
            x = np.fromfile(path, dtype='<f4')
        else:
            import torchaudio
            wav, got = torchaudio.load(path)
            assert got == sr, 'expected %d Hz, got %d' % (sr, got)
            x = wav.mean(dim=0).numpy()

        before = music_score(x)
        t0 = time.perf_counter()
        # One channel, as a tensor of shape (1, n) -- the API's own layout.
        clean = enhance(model, state, torch.from_numpy(x.copy()).unsqueeze(0))
        took = time.perf_counter() - t0
        y = clean.squeeze(0).numpy()
        after = music_score(y)

        out = os.path.splitext(path)[0] + '-dfn.wav'
        write_wav(out, y, sr)
        seconds = len(x) / sr
        print('%-26s %7.1f  %6.1f  %7s -> %-7s' % (
            os.path.basename(path), seconds, seconds / took,
            '%.3f' % before if before is not None else '-',
            '%.3f' % after if after is not None else '-'))

    print('\n"x real" over 1 means faster than real time on this CPU, which is'
          '\nthe first question for anything that would run on a phone.')


if __name__ == '__main__':
    main()
