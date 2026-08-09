# Music opens the gate, and nothing in the chain disagrees

> **Nothing here has been built.** This is a plan, and the measurements that
> would justify building any of it have not been made. Written down now so the
> reasoning survives, and so the next person does not re-derive the three
> features that have already been tried and disproved.
>
> **2026-08-09: the recording arrived, and it moved most of this file.** The
> measurement is in *[What the recording actually says](#what-the-recording-actually-says)*
> below, and it should be read before anything under it — two of the four
> candidates are disproved by it, and the mechanism described in the next
> section turns out to be only half of what happens. Everything from
> *[Candidates](#candidates-if-it-is-ducking)* onwards is the reasoning as it
> stood *before* that measurement, kept because most of it survives and because
> the parts that did not are worth not re-deriving.

Reported from the road, 2026-08-08: **music with mid-high plucks triggers voice
activation.** Guitar, harp, pizzicato strings, synth plucks — the transient,
tonal kind. The rider is not talking and the channel opens anyway.

## What the recording actually says

`20260809-0142-000` — 138.8 s, music throughout, **no speech at all**. So it
measures false positives and nothing else: every conclusion below is about what
the chain sends when it should send nothing, and none of it bounds what a
candidate would cost in recall.

### The headline number is a property of the noise profile, not of the gate

Running the clip through `core/tests/road.rs` at each profile, with no echo
canceller in the path:

| Profile | Blocks transmitted | Level after suppression |
|---|---|---|
| Off | **80.2%** | −25.4 dB |
| Light | **73.3%** | −35.2 dB |
| Standard | **65.2%** | −48.7 dB |
| Helmet | 13.8% | −81.9 dB |
| Auto | 28.7% (settles on Helmet) | −75.9 dB |

**Helmet very nearly fixes this already, and Light barely helps at all.** The
same audio, the same gate, the same thresholds: the only thing that changed is
how much of the music reached the decision. Any account of this fault that does
not start there is describing the wrong stage.

The session itself was on **Auto** — it logged 30.59% of blocks `speaking`
against 28.7% measured offline, which is as close as those two things get.

### The two tests are not two opinions

`vad_says_speech && snr_says_speech` reads like a conjunction of independent
evidence. It is not. Both are measured on RNNoise's *output*: the VAD is the
network's own probability, and the SNR margin compares the post-suppression
level against a floor tracked on the post-suppression level. Ranked by how well
each separates the blocks that went out from the blocks that did not (AUC, 0.5
is a coin):

```
level_db     0.971      <- after suppression
floor_db     0.947      <- tracked on the above
snr_db       0.778      <- the difference of the two
harmonicity  0.668      <- measured on the denoised signal
vad          0.602      <- RNNoise's own output
modulation   0.533
raw_db       0.521      <- the microphone. A coin.
```

**The raw microphone level knows nothing about whether the block was
transmitted.** Everything that does know is downstream of one decision made by
one network. That is why three features have now failed: they were all computed
on the same side of it.

### The same sound, decided both ways

Fingerprint each block from the *raw* capture — 20 log-spaced bands,
level-normalised — and look for a transmitted block whose sound also occurs
somewhere it was not transmitted, at the same raw level within 1 dB.

**524 of 600 sampled transmitted blocks (87%) have such a twin**, matching to
under 3 dB rms across the bands. The suppression applied to the pairs differs by
up to 70 dB. This is the reporter's "it fooled the gate once and not the second
time", and it is not a per-block property at all.

### What varies is the profile Auto has landed on

Per 10 s, at a roughly constant input level:

```
   0-30s   tx 96-99%    suppression ~14 dB
  30-60s   tx 63->38%   suppression 13 -> 52 dB
  60-110s  tx 0-5%      suppression 71 -> 83 dB
 120-130s  tx 51%       suppression 30 dB      <- quieter passage, Auto backs off
```

`reconsider()` (`denoise.rs:472`) tracks the **raw** floor and moves
Light → Standard → Helmet as it climbs. The music arrives at −24 dBFS median
per block, −17 dBFS overall, so the floor rises to meet it and Auto ends in
Helmet — but the climb takes the best part of a minute, and everything before it
lands goes out on the wire. A track change or a quiet passage puts it back.

So the fault has a shape nobody had guessed: **it is a convergence transient**,
worst in the first ~40 seconds of any new music and again whenever the music
changes character.

### Two candidates die here

- **"Too periodic" (candidate 3) is disproved.** It assumes music scores high on
  harmonicity. Measured on this clip: p50 **0.45**, p90 0.72, p99 0.75, and
  **0.48% of blocks clear the 0.75 voiced bar**. This chain's own periodicity
  measure does not find the music periodic, so an upper bound on it has almost
  nothing to bite on. `harmonicity`'s AUC of 0.668 is real but modest, and it is
  measured post-suppression like everything else.
- **Spectral novelty is disproved before being built.** "Suppression has not
  converged yet" suggests detecting the novel block directly. Distance from an
  EMA of recent raw spectra scores **AUC 0.517–0.551** across windows from
  100 ms to 4 s. A coin. The convergence is slow and global; it is not visible
  as per-block surprise.

### The recall half arrived too — 2026-08-09, later the same day

`20260809-1201-000`, 121 s, **speech over music from the iPhone microphone**.
With the music-only clip beside it, both halves of the acceptance criteria have
numbers for the first time. Same chain, same settings, the two clips differing
only in whether anyone was talking:

| Profile | Music alone | Voice over music |
|---|---|---|
| Off | 80.2% | 86.3% |
| Light | 73.3% | 73.5% |
| Standard | 65.2% | 69.5% |
| **Helmet** | **13.8%** | **59.6%** |
| Auto (→ Helmet) | 28.7% | 62.4% |

**Helmet already separates them, on real audio, by better than four to one.**
It is also the profile Auto settles on. Light and Standard separate them by
essentially nothing — 73.3 against 73.5 is not a discriminator — which says
again that the profile, not the gate, is what decides this.

The pitch measure separates them too, and by more than anything else here:
blocks over the 0.75 voiced bar go from **0.48% on music alone to 11.53% on
speech over music**. That does not rescue candidate 3 — an *upper* bound on
harmonicity still has nothing to bite on — but it does say the measure knows
which clip has a voice in it.

On the device, with Auto in force, the decision log says:

```
transmitting 74.1%   speaking 61.0%      (offline Auto: 62.4% — the same run)
voiced blocks (harmonicity >= 0.75): 24.8 s, 20.4% of the file
  of voiced blocks,     91.4% transmitted     <- recall
  of non-voiced blocks, 69.6% transmitted
```

**Two caveats, and the second is the reason this is not yet the acceptance
measurement.**

*Non-voiced is not the same as not-speech.* Unvoiced consonants have no pitch,
and the hold and fade keep the channel open between words on purpose. Some of
that 69.6% is the chain working correctly. This clip interleaves speech and
music, so it cannot separate "music that leaked" from "a gap held open" — which
is precisely what the music-only clip is for, and why the pair is worth more
than either.

*Scoring the chain by its own harmonicity is circular.* It is the closest thing
to a label this file carries, and it is a number the chain computes and uses.
A real label means a `NAME.speech` sidecar — one `start end` pair per line, by
ear — which `core/tests/road.rs` already reads (`speech_spans`). **That is now
the cheapest remaining step, and it would turn every figure above into an
acceptance test.**

### What this leaves

Two directions, and they are not the same size.

1. **A veto computed upstream of RNNoise.** Everything the decision currently
   uses is downstream of the suppressor, so it cannot disagree with it. This is
   the structural fix and the expensive one.
2. **Make Auto converge faster, or start it pessimistic.** Much cheaper, and it
   addresses where the leak actually is rather than the class of sound. It costs
   nothing in recall on a quiet route — a rider who is talking raises the raw
   floor too, so a fast-converging Auto lands on Helmet during speech as well,
   which is exactly the case where Helmet is known to hurt. That trade needs
   measuring before anyone builds it.

**Neither is justified by this file alone.** It contains no speech, so it can
show a candidate removing the music and cannot show what the same candidate does
to a rider. The recall half has to come from `C:\ml_data\speech_road`, and the
acceptance criteria below still stand unchanged.

## Why it gets through — the original reading, since corrected

> Written before the recording. The mechanism here is real but it is the second
> half of the story, not the first: it explains what the gate does with music
> that reaches it, and the measurement above shows that how much reaches it is
> the larger term.

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

**Still not ruled out, and the recording raises rather than settles it.** The
music arrives at **−24 dBFS median, −17 dBFS overall**. That is not the level of
a ducked background; it is the level of something playing at full tilt into the
microphone. But this clip carries no record of how it was made — whether the
music was ducking, whether it was meant to, or whether it was simply played into
the mic to reproduce the fault on purpose — so the number is consistent with a
ducking failure without being evidence of one. **Ask how the clip was made, or
capture one where the answer is known.** It remains the cheapest thing here by a
wide margin.

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

### 3. Too periodic — **disproved, 2026-08-09**

> The premise does not hold. On the music recording the chain's own periodicity
> measure reads p50 0.45, p99 0.75, with **0.48% of blocks above the 0.75 voiced
> bar**. The music is not scoring as strongly periodic, so an upper bound on
> harmonicity has essentially nothing to catch. Kept below because the reasoning
> is sound and the *premise* is what failed — a different periodicity measure,
> computed before RNNoise, might yet read the way this one was assumed to.

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

**Half of that arrived on 2026-08-09 and it was worth the wait.** 138.8 s of
music alone gave the baseline, disproved two candidates, and found a mechanism
nobody had proposed. What it could not do is the other half: with no speech in
it, it cannot say what any candidate costs the rider, and *that* is the
assertion this whole file says will fail first.

**And the other half arrived hours later.** `20260809-1201-000` is speech over
music, so the pair now bounds both false positives and recall — see the recall
section above.

What is left is not another recording. It is **a `20260809-1201-000.speech`
sidecar**: one `start end` pair per line marking where the talking is, written
by ear. `core/tests/road.rs` already reads it, and with it every figure in this
file becomes an assertion instead of an estimate. Without it the only available
label is the chain's own harmonicity, which is the thing being judged.
