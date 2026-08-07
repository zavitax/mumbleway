# Evaluating a neural VAD on real helmet audio

RNNoise is currently both the suppressor and the speech detector in the capture
chain, and on recordings made inside a helmet at speed it is failing at the
second job: of blocks a rider hand-labelled as their own speech, its VAD fires
on 38%. That single failure explains all three complaints reported from the
road — cut off, mangled, and wind getting through — because the same component
removes 25 dB of signal it may have misjudged and decides whether to transmit
what is left.

These scripts test whether an off-the-shelf model does better, on the audio that
matters rather than on a benchmark.

## Running

```bash
pip install numpy onnxruntime ten-vad
ffmpeg -i clip.m4a -ac 1 -ar 48000 -f f32le clip.raw     # per recording

# what the chain hands the encoder, for testing a VAD where it would really sit
MUMBLEWAY_ROAD_AUDIO=road MUMBLEWAY_ROAD_DUMP=denoised \
  cargo test --test road dump_the_suppressed -- --ignored --nocapture

python evaluate.py   road silero_vad.onnx        # Silero, raw
python evaluate.py   denoised silero_vad.onnx    # Silero, suppressed
python ten_eval.py   road denoised               # TEN VAD, both
python ten_extract.py road denoised originals out # clips to judge by ear
```

`silero_vad.onnx` comes from `snakers4/silero-vad`, `src/silero_vad/data/`.

## Always run the control first

`evaluate.py` and `ten_eval.py` both take a directory, so point them at a clip
of obviously clean speech before pointing them at anything hard. Any Windows or
macOS box can make one:

```powershell
Add-Type -AssemblyName System.Speech
$s = New-Object System.Speech.Synthesis.SpeechSynthesizer
$s.SetOutputToWaveFile("tts.wav"); $s.Speak("Testing voice activity detection."); $s.Dispose()
```

This is not ceremony. The first Silero run here reported **zero speech in every
recording, including the clean control** — and the cause was that Silero v5
needs 64 samples of the previous chunk prepended to each 512-sample window.
Feed it a bare 512 and it runs without error, returns plausible probabilities,
and detects nothing. It looked exactly like a model that could not cope with
helmet noise. Without the control that would have been reported as a finding
about the audio.

## What was measured

Four recordings, 450 seconds, from a helmet at road speed. Speech in the first
was hand-labelled by the rider.

Seconds of speech found:

| | Silero raw | TEN raw | Silero suppressed | TEN suppressed |
|---|---|---|---|---|
| rec1 (12.7s) | 0.8 | **3.0** | 2.3 | 2.5 |
| rec2 (13.8s) | 0.0 | **3.4** | 1.5 | **4.4** |
| rec3 (293s) | 0.0 | **3.6** | 4.9 | **13.0** |
| rec4 (130s) | 0.0 | **2.1** | 0.0 | 1.2 |

Three things came out of this that were not expected.

**The chain's own suppression helps a neural VAD, and helps it a lot.** Silero
finds nothing at all in three recordings on the raw microphone and finds speech
in all but one after the Helmet profile has run. So the place for such a model
in this app is after the suppression, not in front of it.

**TEN VAD works on the raw microphone where Silero does not**, and finds more
in every configuration. It is also the smaller and faster of the two by its own
published figures, and ships prebuilt for Android and iOS, which matters more
here than a benchmark point.

**Both models independently recovered the rider's hand-labelled spans.** TEN
found all three in the first recording from the raw audio — 4.3-5.2, 6.6-7.7,
10.3-11.4 against labels of 4.2-5.2, 7.5-8.5, 10.1-11.1 — without being shown
them. That is the strongest evidence available that the labels are sound and
that the models are detecting a voice rather than a coincidence.

Across everything, TEN VAD marks **82 seconds** of candidate speech where the
rider labelled 3 and where the shipped chain would transmit a fraction. The
clips are for judging by ear; nothing here is scored against them, because a
score computed against labels known to be incomplete looks like an answer and
is not one.

## Whisper as an oracle does not work here

`transcribe.py` runs faster-whisper large-v3 on the GPU, on the theory that an
ASR model knows what words sound like and can therefore find speech a VAD
misses. It was worth trying and it fails, in a way worth documenting so nobody
tries it again without knowing.

On these recordings it **hallucinates**. The output is dominated by
`Продолжение следует...` ("to be continued") and `Субтитры сделал DimaTorzok`
("subtitles by DimaTorzok") — well-known Whisper artefacts learned from
subtitle training data, which it emits on silence and noise. The timestamps
give it away completely: 57.18, 87.18, 117.90, 147.18, 177.18, 207.18,
237.98 — every thirty seconds, which is Whisper's window length, not anything
in the audio.

It also reports 40 seconds of "speech" in a recording where TEN VAD finds two,
and disagrees with itself between the Light and Helmet versions of the same
audio. None of it is usable as a label, and using it would have poisoned the
training corpus with confident nonsense.

The script is kept because the negative result is worth reproducing, and
because on a recording with genuinely audible speech it may still be useful.
Read `no_speech` and `logprob` in its output before believing any segment.

## The preprocessing is not the problem

`preprocess_sweep.py` exists because three faults in this project so far have
been preprocessing rather than audio. It runs TEN VAD over four variants of the
same signal: the crude mean-of-three decimation the Rust uses, ffmpeg's proper
polyphase resampler, peak normalisation, and a sliding AGC.

They agree within noise, and the control stays at 9.8 s throughout. Level
normalisation actively *hurts* — it lifts the wind by exactly as much as the
voice. So the resampling is fine and the missed speech is not an artefact of
how the audio reaches the model.

What does move the answer is the threshold, and a great deal:

| threshold | rec1 (13s) | rec2 (14s) | rec3 (293s) | rec4 (130s) |
|---|---|---|---|---|
| 0.50 | 3.0s | 3.4s | 3.6s | 2.1s |
| 0.35 | 5.1s | 4.9s | 16.6s | 7.7s |
| 0.25 | 9.7s | 5.5s | 75.1s | 21.0s |
| 0.15 | 12.8s | 13.3s | 250.7s | 112.8s |

0.15 is degenerate — nearly the whole clip is "speech". 0.25 is the useful
recall-favouring point for *finding candidates a human should check*, which is
a different job from deciding what to transmit and deserves its own number.

## Before integrating either

- **TEN VAD is "Apache 2.0 with additional conditions"**, not plain Apache 2.0,
  and `pitch_est.cc` carries BSD-2 and BSD-3 code from LPCNet. Read the LICENSE
  and NOTICES before shipping it in a product.
- Silero is MIT and 2.3 MB of ONNX, which is the simpler licensing story.
- Both want 16 kHz; the chain runs at 48 kHz, so a decimation step joins the
  real-time path.
- Neither has been measured for cost on a phone yet, and neither has been run
  against a recording with speech labelled all the way through — which is the
  next thing needed, and needs a rider with a stopwatch rather than a model.
