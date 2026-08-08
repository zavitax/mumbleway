# Music opens the gate, and nothing in the chain disagrees

> **Nothing here has been built.** This is a plan, and the measurements that
> would justify building any of it have not been made. Written down now so the
> reasoning survives, and so the next person does not re-derive the three
> features that have already been tried and disproved.

Reported from the road, 2026-08-08: **music with mid-high plucks triggers voice
activation.** Guitar, harp, pizzicato strings, synth plucks — the transient,
tonal kind. The rider is not talking and the channel opens anyway.

## Why it gets through

The transmit decision is two tests and nothing else (`core/src/audio/denoise.rs:779`):

```rust
vad_says_speech && snr_says_speech
```

A plucked note passes both, and not marginally.

- **RNNoise's VAD votes for it.** It is trained on speech against *noise*, and
  music is neither. A tonal, harmonically rich event with a sharp onset
  resembles a voice far more than it resembles wind, which is what the detector
  was taught to reject.
- **The SNR margin votes for it.** A pluck is loud against the tracked noise
  floor, which is exactly the condition the margin exists to detect.

There is no third test. `pitch_says_speech` (`denoise.rs:613`) and `modulation`
are computed every block and published to the diagnostics panel, but neither
gates anything.

**And the pitch test would vote the wrong way if it did.** It searches 75–350 Hz
(`core/src/audio/pitch.rs:101-103`), chosen to reject a motorcycle's firing
fundamental at 30–60 Hz by construction. Guitar open strings are E2 82 Hz, A2
110, D3 147, G3 196, B3 247, E4 330 — every one of them *inside* the human
range. The feature that rejects an engine by construction endorses a guitar by
construction.

This is not a surprise. `core/tests/suppression.rs` already found it: wind,
engine, traffic and randomly-shaped unknown noise were rejected outright at
every level tested, and **music was the only leak, at 2.2% of blocks**.

## What has already been tried, and failed

Recorded at `denoise.rs:764-778` and worth reading before proposing anything:

> "three features have now been tried: periodicity, level, and syllabic
> modulation"

The syllabic-modulation attempt is the instructive one. Speech is syllables at
three to eight a second; an engine is not, whatever its level does. It recovered
**nothing** — recall stayed at 59.1% — while still leaking 12–41% of synthetic
engine and traffic. Worst of both.

One caveat that changes how much those results bind here: **they were tried as
overrides to raise recall**, letting more audio through. Rejecting music is the
opposite direction — an additional veto, which can only lower recall. A feature
that failed to safely admit more speech has not thereby been shown useless at
excluding music. It has, however, been shown not to separate the classes as
cleanly as its description suggests.

## Check the cheap thing first

**Is the music actually ducking?**

The app asks for it — `.duckOthers` on iOS, `AUDIOFOCUS_GAIN_TRANSIENT_MAY_DUCK`
on Android. What has been verified is only that the *request is granted*, on an
Android emulator, from `dumpsys audio`. Whether a real player over a real
Bluetooth route to a helmet intercom actually turns down has never been
measured.

If it is not ducking, music arrives at the microphone 15–20 dB hotter than it
should and no gate would survive it. That is a routing fault wearing a DSP
fault's clothes, and it is far cheaper to fix. **Rule it out before writing any
of the below.**

## Candidates, if it is ducking

Ranked by cost, and by risk to speech — which is the constraint that matters,
because every one of these can only ever close the gate more often.

### 1. f0 *stability*, not f0 presence

A plucked string holds its pitch to within a few cents for the whole decay.
Speech f0 never sits still: declination across a phrase, intonation within it,
microprosody within that. Over 200–300 ms a talker moves several percent; a
guitar moves almost nothing.

`f0_hz` is already computed every block and carried in `BlockAnalysis`, so the
cost is a short ring and a variance. **It is also orthogonal to what failed**:
the previous attempt asked *is there a pitch*, which music answers yes to. This
asks *is it too steady to have come from a throat*.

Risk: a sustained monotone hum would be rejected. Mitigate by requiring both
unusual steadiness and a long window, and note that the hold and fade keep the
channel open through short steady passages inside real speech.

### 2. Monotonic decay after an onset

A pluck is a sharp attack followed by monotonic exponential decay with no
re-excitation. Speech re-excites every glottal cycle and every syllable. A short
state machine over `level_db`: if level has fallen monotonically, within
tolerance, for more than ~250 ms after a transient, it is a struck or plucked
note.

Risk: a single word trailing off resembles this. The window has to be longer
than a syllable.

### 3. Too periodic

Voiced speech carries breath noise — harmonic-to-noise ratio around 10–20 dB. A
plucked string is 30 dB or better. This is an *upper* bound on harmonicity,
which inverts how the existing feature is used: today high harmonicity argues
for speech, and past some point it should argue against.

Cheapest of all to try, since the value is already computed. Most likely to
misfire on a clear, close-mic voice in a quiet helmet.

### 4. Formant motion

The strongest discriminator and the most expensive. Speech has formants that
*move* — the spectral envelope changes shape within a syllable. A plucked note
has a fixed envelope whose level decays. Measure frame-to-frame change of the
level-normalised band energies.

The 24-band envelope already exists, but only while the diagnostics panel is
open — the analyser is deliberately disarmed otherwise, and arming it always
would put three transforms per block in a rider's pocket. This needs a cheaper
always-on envelope before it is even testable.

## The method, which is the part that is actually new

Three features have been disproved here by their own acceptance tests, and one
trained model collapsed on real audio after looking excellent on synthetic.
**Synthesised plucks would agree with whoever wrote them.** The generator would
be written after the hypothesis, by the same hand, and would show only that the
fault does not reproduce offline.

What is different now is that the fault can be captured directly.

Since 2026-08-08 the diagnostic recorder writes a **`transmitting`** column
(`core/src/audio/record.rs`) — what actually went on the wire, envelope and mode
and mute included, rather than the instantaneous detector. So:

1. **Ride, or sit still, with the music playing and say nothing.** Every block
   where `transmitting = 1` is a labelled false positive, in real audio, at the
   real level, over the real route.
2. **Say something with the same music playing.** Those blocks are the recall
   that must not be lost.
3. The bot already splits a ride on that column
   (`tools/vad/telegram_intake.py`), so both sets arrive cut and playable.

That gives a labelled set on which every candidate above can be scored offline,
against real audio, before a line of it ships.

## Acceptance, when there is a recording

A new case in `core/tests/suppression.rs`, driven by the recording rather than
by a generator:

- **False positives fall.** Transmitted share over the music-only passage drops
  from its measured baseline to near zero.
- **Recall does not.** Transmitted share over the speech-with-music passage does
  not fall — this is the assertion that fails first, and the one worth watching.
- **Nothing else regresses.** The existing wind, engine, traffic and
  unknown-noise cases stay at zero, and the clean-speech case stays where it is.

Two of the three must hold simultaneously or the candidate is not an
improvement, only a different trade. A gate that rejects music by rejecting
quiet speech has moved the fault rather than fixed it — and moved it somewhere
worse, because a rider notices being unheard and does not notice music that was
never sent.

## What would settle it fastest

One recording, two minutes long: thirty seconds of the offending music alone,
thirty seconds of speech over it, and the same again with the music at a level
the rider considers normal. That is enough to measure a baseline, score all four
candidates, and find out whether the ducking works — which may turn out to be
the whole of it.
