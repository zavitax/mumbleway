"""Train a VAD for a helmet, on mixtures whose labels are exact by construction.

The idea this rests on is in docs/VOICE_MODEL.md: mix clean speech into
motorcycle noise and you know precisely where the speech is, because you put it
there. The label is not annotation, it is bookkeeping. That removes the
bottleneck which has dominated this whole investigation -- three seconds of
hand-labelled audio, enough to disprove a hypothesis and nowhere near enough to
establish one.

What this trains is small on purpose: log-mel in, two GRU layers, one
probability out, around 100k parameters. It has to run on a phone inside a
helmet alongside everything else, and a model that wins offline and eats a core
is not a win. The measured target it has to beat is RNNoise's VAD, which fires
on 38% of the rider's own labelled speech.

Recall is weighted above precision in the loss, deliberately and by a factor
that is stated rather than implied. A missed word is gone and the rider does
not know it happened; leaked wind is annoying and a listener filters it out.
"""

import os
import sys
import glob
import math
import random
import numpy as np
import torch
import torch.nn as nn

RATE = 16_000
HOP = 160  # 10 ms, matching the chain's block
WIN = 400  # 25 ms
MELS = 40
CLIP_FRAMES = 300  # 3 seconds per training example

# How much more a missed speech frame costs than a false alarm. See the module
# comment; this is the single number that encodes "recall matters more".
RECALL_WEIGHT = 4.0


# ---------------------------------------------------------------- features

def mel_bank(n_fft, n_mels, rate):
    def hz_to_mel(f):
        return 2595.0 * math.log10(1.0 + f / 700.0)

    def mel_to_hz(m):
        return 700.0 * (10 ** (m / 2595.0) - 1.0)

    lo, hi = hz_to_mel(60.0), hz_to_mel(rate / 2)
    points = np.linspace(lo, hi, n_mels + 2)
    freqs = mel_to_hz(points)
    bins = np.floor((n_fft + 1) * freqs / rate).astype(int)
    fb = np.zeros((n_mels, n_fft // 2 + 1), dtype=np.float32)
    for m in range(1, n_mels + 1):
        a, b, c = bins[m - 1], bins[m], bins[m + 1]
        for k in range(a, min(b, fb.shape[1])):
            if b > a:
                fb[m - 1, k] = (k - a) / (b - a)
        for k in range(b, min(c, fb.shape[1])):
            if c > b:
                fb[m - 1, k] = (c - k) / (c - b)
    return torch.from_numpy(fb)


class Features(nn.Module):
    """Log-mel, computed on the GPU so the data loader is not the bottleneck."""

    def __init__(self):
        super().__init__()
        self.register_buffer("window", torch.hann_window(WIN))
        self.register_buffer("fb", mel_bank(512, MELS, RATE))

    def forward(self, x):
        spec = torch.stft(
            x, n_fft=512, hop_length=HOP, win_length=WIN,
            window=self.window, center=True, return_complex=True,
        )
        power = spec.real**2 + spec.imag**2
        mel = torch.matmul(self.fb, power)
        return torch.log(mel + 1e-6).transpose(1, 2)  # (B, T, MELS)


# ---------------------------------------------------------------- model

class HelmetVad(nn.Module):
    def __init__(self, hidden=64):
        super().__init__()
        self.norm = nn.LayerNorm(MELS)
        self.gru = nn.GRU(MELS, hidden, num_layers=2, batch_first=True)
        self.out = nn.Linear(hidden, 1)

    def forward(self, feats):
        h, _ = self.gru(self.norm(feats))
        return self.out(h).squeeze(-1)  # logits, (B, T)


# ---------------------------------------------------------------- data

def load_raw48_to16(path):
    x = np.fromfile(path, dtype=np.float32)
    n = (len(x) // 3) * 3
    return x[:n].reshape(-1, 3).mean(axis=1).astype(np.float32)


def load_flac(path):
    import soundfile as sf

    x, sr = sf.read(path, dtype="float32")
    if x.ndim > 1:
        x = x.mean(axis=1)
    if sr != RATE:
        idx = (np.arange(int(len(x) * RATE / sr)) * sr / RATE).astype(int)
        x = x[np.clip(idx, 0, len(x) - 1)]
    return x


def speech_label(clean, frames):
    """Where the speech is, from the clean signal before anything was added.

    An energy gate on studio-clean audio is not a detector doing a hard job --
    it is reading off what we already know. This is the whole reason synthetic
    mixing is worth it.
    """
    pad = frames * HOP + WIN
    c = np.pad(clean, (0, max(0, pad - len(clean))))[:pad]
    energy = np.array([
        float(np.sqrt((c[i * HOP : i * HOP + WIN] ** 2).mean() + 1e-12))
        for i in range(frames)
    ])
    db = 20.0 * np.log10(energy + 1e-9)
    # Relative to the utterance's own peak, so a quiet speaker is not silence.
    return (db > db.max() - 35.0).astype(np.float32)


class Mixer:
    def __init__(self, speech_files, noise_files):
        self.speech = speech_files
        self.noise = noise_files

    def example(self, rng):
        need = CLIP_FRAMES * HOP + WIN

        clean = np.zeros(0, dtype=np.float32)
        while len(clean) < need:
            more = load_flac(rng.choice(self.speech))
            gap = rng.integers(0, RATE)  # silence between utterances
            clean = np.concatenate([clean, np.zeros(gap, dtype=np.float32), more])
        start = rng.integers(0, max(1, len(clean) - need))
        clean = clean[start : start + need]

        noise = load_raw48_to16(rng.choice(self.noise))
        if len(noise) < need:
            noise = np.tile(noise, need // len(noise) + 1)
        n0 = rng.integers(0, len(noise) - need)
        noise = noise[n0 : n0 + need]

        label = speech_label(clean, CLIP_FRAMES)

        # Randomise SNR, weighted low: -5 to +5 dB is where the chain fails.
        snr = float(rng.normal(4.0, 7.0))
        snr = max(-12.0, min(20.0, snr))
        cr = np.sqrt((clean**2).mean() + 1e-12)
        nr = np.sqrt((noise**2).mean() + 1e-12)
        noise = noise * (cr / nr) / (10 ** (snr / 20.0))
        mix = clean + noise

        # Random overall gain, so the model cannot key on absolute level --
        # which is the mistake the current chain makes.
        mix = mix * float(10 ** (rng.uniform(-18.0, 0.0) / 20.0))
        peak = float(np.abs(mix).max())
        if peak > 1.0:
            mix /= peak
        return mix.astype(np.float32), label


def batch(mixer, rng, size):
    xs, ys = [], []
    for _ in range(size):
        x, y = mixer.example(rng)
        xs.append(x)
        ys.append(y)
    return torch.from_numpy(np.stack(xs)), torch.from_numpy(np.stack(ys))


# ---------------------------------------------------------------- train

def main():
    speech_dir, noise_dir, out_path = sys.argv[1:4]
    steps = int(sys.argv[4]) if len(sys.argv) > 4 else 2000

    speech = glob.glob(os.path.join(speech_dir, "**", "*.flac"), recursive=True)
    noise = glob.glob(os.path.join(noise_dir, "*.raw"))
    if not speech or not noise:
        sys.exit(f"need speech and noise: {len(speech)} flac, {len(noise)} raw")
    print(f"{len(speech)} speech files, {len(noise)} noise clips")

    dev = "cuda" if torch.cuda.is_available() else "cpu"
    print(f"training on {dev}")
    feats = Features().to(dev)
    model = HelmetVad().to(dev)
    params = sum(p.numel() for p in model.parameters())
    print(f"{params:,} parameters")

    opt = torch.optim.AdamW(model.parameters(), lr=3e-3, weight_decay=1e-4)
    sched = torch.optim.lr_scheduler.CosineAnnealingLR(opt, steps)
    rng = np.random.default_rng(1234)
    mixer = Mixer(speech, noise)

    model.train()
    for step in range(1, steps + 1):
        x, y = batch(mixer, rng, 16)
        x, y = x.to(dev), y.to(dev)
        with torch.no_grad():
            f = feats(x)
        logits = model(f)[:, : y.shape[1]]
        y = y[:, : logits.shape[1]]

        # Asymmetric: a missed speech frame costs RECALL_WEIGHT times a false
        # alarm. The number is the policy, stated in one place.
        w = torch.where(y > 0.5, torch.full_like(y, RECALL_WEIGHT), torch.ones_like(y))
        loss = nn.functional.binary_cross_entropy_with_logits(logits, y, weight=w)

        opt.zero_grad()
        loss.backward()
        nn.utils.clip_grad_norm_(model.parameters(), 5.0)
        opt.step()
        sched.step()

        if step % 100 == 0 or step == 1:
            with torch.no_grad():
                p = torch.sigmoid(logits)
                hit = ((p > 0.5) & (y > 0.5)).sum().item()
                miss = ((p <= 0.5) & (y > 0.5)).sum().item()
                fa = ((p > 0.5) & (y <= 0.5)).sum().item()
                rec = hit / max(1, hit + miss)
                pre = hit / max(1, hit + fa)
            print(f"step {step:5d}  loss {loss.item():.4f}  "
                  f"recall {rec * 100:5.1f}%  precision {pre * 100:5.1f}%")

    torch.save({"state": model.state_dict(), "mels": MELS, "hop": HOP}, out_path)
    print(f"saved {out_path}")


if __name__ == "__main__":
    main()
