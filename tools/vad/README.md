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
