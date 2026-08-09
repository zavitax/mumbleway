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

### Provenance: the music came from the room, not from the phone

Both clips were made with music playing on **2.1 computer speakers**, picked up
acoustically by the microphone. Two consequences, and the second is the larger.

**Ducking never applied, so it is not what these clips measure.** `.duckOthers`
and `AUDIOFOCUS_GAIN_TRANSIENT_MAY_DUCK` govern audio played by other apps on
the same device. Music in the room is not that. The question at the top of this
file is therefore not answered by either recording — it is out of scope for
them, which is different from being ruled out.

**And that is the fault, not an approximation of it.** Confirmed by the reporter:
music through the intercom causes no trouble, and what is being chased is an
*external* source heard by the microphone. So these clips are on target, and the
usual provenance worry — that a recording describes a different signal from the
one the complaint is about, as `core/tests/road.rs` records in its header — does
not apply here. It is worth saying plainly, because an earlier draft of this
section said the opposite.

It also removes a whole class of fix. There is no routing, focus or session
setting that can turn down a stereo in the room. Whatever is done about this has
to be done in the chain.

### Quieter music leaks more, because Auto reads level

Attenuating the music-only clip and re-running the chain tests that directly,
since Auto chooses on the tracked raw floor and Helmet is what does the
rejecting:

| Music level | Auto picks | Transmitted |
|---|---|---|
| −17 dBFS (as recorded) | **Helmet** | 28.7% |
| −27 dBFS | Standard | **47.2%** |
| −37 dBFS | Standard | 21.9% |
| −47 dBFS | **Light** | **36.4%** |

**It is not monotonic, and the loudest case is the best handled.** Ten dB
quieter is nearly twice as bad, because Auto drops out of Helmet into Standard;
thirty dB quieter is worse again, because it drops into Light. `Off` sits at
80.2% at every level, unchanged — the level-only path compares against a floor
that scales with the signal, so it cannot see a change of gain at all.

So the recording that prompted all of this **caught the chain at its best**, and
that matters precisely because the source is external: how loud it arrives is a
question of how far away it is. A stereo in the same room at −17 dBFS lands in
Helmet and is mostly rejected. The same stereo across a car park, or a vehicle
alongside at the lights, arrives ten or twenty dB down — and lands in Standard
or Light, where music is barely suppressed at all.

**The chain is therefore best at the case a rider would most expect it to
struggle with, and worst at the moderate one they would not think to mention.**
That is also a testable prediction to put to the reporter: the fault should be
worse with the source further away, not nearer.

*Caveat on the method:* scaling a recording is not the same as recording quieter
music. The gain is applied to the microphone's own noise as well, so the SNR is
preserved where a genuinely quieter source would have had a worse one. What the
sweep shows exactly is the **profile selection**, which depends on absolute
level; the transmitted shares within each profile are indicative rather than
measured-from-life.

### Scored against hand labels — 2026-08-09

The sidecar arrived: 23 ranges marked by ear, **66.9 s of speech in a 121.0 s
clip, 55.3%**. So "transmit everything" scores 55.3% precision, and that is the
number every figure below has to beat to mean anything.

| Profile | Sent | Kept of speech | Of what it sent, was speech |
|---|---|---|---|
| Off | 86.3% | 94.2% | 60.6% |
| Light | 73.5% | 90.1% | 68.1% |
| Standard | 69.5% | 88.5% | 70.8% |
| **Helmet** | 59.6% | **80.0%** | **74.6%** |
| Auto (→ Helmet) | 62.4% | 81.7% | 72.7% |

**This is a trade, not a win.** Helmet buys 14 points of precision over Off and
pays 14 points of recall for them. A quarter of what it sends is still music,
and it drops a fifth of the rider's speech. Nothing in the chain is separating
the two classes well; the profiles are choosing a point on one curve.

And the curve is not steep, because **no feature here is strong**. Against the
labels, threshold-free:

```
level_db     0.827      <- best, and it is post-suppression level
snr_db       0.767
vad          0.718
harmonicity  0.603      <- the pitch measure, on real mixed audio
modulation   0.358      <- WORSE than a coin
```

Three things in that list are worth stating plainly.

**Modulation is inverted.** At 0.358 it is not weak, it is backwards: this music
has *more* syllabic-rate envelope movement than the speech does. The attempt
recorded above that "recovered nothing at all" was not unlucky — the feature
points the wrong way on real material, and any future use of it has to account
for that rather than re-tune a threshold.

**Harmonicity is much weaker than the earlier read suggested.** The 0.48% versus
11.53% figures compared two whole clips; against per-block labels it scores
0.603, and the 0.75 bar catches only **18.7% of speech** while admitting 2.6% of
the rest. It is precise and nearly deaf. That is consistent with candidate 3
being dead and it also caps what candidate 1 can be worth, since both read the
same signal.

**RNNoise's VAD fires on 57% of labelled speech** in Helmet, against the SNR
margin's 84%. That is the same ratio `core/tests/road.rs` found on helmet audio
with no music in it at all, so it is a property of the detector on real speech
rather than anything music does.

### A neural VAD solves this, and the chain's own features do not

`tools/vad/bench_vads.py` runs every detector we can get hold of over both
clips, scored the same way. The music-only clip needs no labelling judgement at
all: the correct output is silence everywhere, so **every frame a detector fires
on is wrong by construction.**

| Detector | AUC | Keeps of speech | Was speech | Fires on music-only |
|---|---|---|---|---|
| **Silero** | **0.917** | 71.0% | **97.9%** | **0.000%** (0 of 13 877) |
| **TEN VAD** | 0.903 | **80.2%** | 89.5% | 4.9% |
| WebRTC agg=3 | 0.578 | 93.6% | 59.8% | 45.8% |
| WebRTC agg=1 | 0.525 | 99.9% | 56.6% | 84.0% |
| *the chain (Helmet)* | — | *80.0%* | *74.6%* | *13.8%* |
| *RNNoise's own VAD* | *0.718* | — | — | — |

**Silero did not merely stay under the threshold on the music — it never came
near it.** Peak probability across the whole 138.8 s clip is **0.105**. TEN VAD
peaks at 0.756 and fires on 4.9%.

And **TEN VAD matches the chain's recall exactly — 80.2% against 80.0% — while
being right about what it sends far more often (89.5% against 74.6%) and leaking
a third as much music (4.9% against 13.8%).** That is not a trade. It is the
same recall for better precision on both counts.

Two things this settles:

- **The missing feature was never going to be found by hand.** Every candidate
  in this file scores between 0.36 and 0.83; both neural detectors clear 0.90.
  Five hand-built features have now been tried against this fault.
- **They are upstream of RNNoise**, which is the property
  [the analysis above](#the-two-tests-are-not-two-opinions) said was needed and
  which nothing in the chain has. They are fed the raw microphone.

WebRTC is included because it is the obvious cheap answer and it is worth
recording that it is useless here: at its most aggressive setting it still calls
**45.8% of pure music speech**, and its AUC of 0.578 is close enough to a coin
that no threshold rescues it.

#### Whisper, which should have been the ceiling and is not

An ASR model asks a stronger question than a VAD, so it ought to win. Run with
`vad_filter` off — leaving it on would have scored Silero again under another
name — `whisper base` gives:

| | Marked as speech | Keeps of speech | Was speech |
|---|---|---|---|
| voice over music | 90.0% | **98.5%** | 60.5% |
| music only | **14.4%** | — | 0% by construction |

**Excellent recall, useless precision.** 60.5% barely clears the 55.3% a client
that transmitted everything would score, because Whisper's segments run straight
through the pauses — its timestamps are a transcript's, not a detector's. And it
marks 14.4% of pure music as speech, three times TEN VAD and infinitely more
than Silero.

Two things worth taking from it anyway:

- **It found 98.5% of the hand-marked speech**, which is independent support for
  the labels themselves. They were marked by ear, and nothing until now had
  checked them.
- **What it transcribed from 138.8 s of music was `"Music Music Music"`.** The
  model knows perfectly well what it is listening to; that knowledge is in the
  *text*, not in the timestamps. Any use of Whisper here would have to read what
  it said rather than when it said it — which is an oracle's job, offline, and
  it is already what `tools/vad/transcribe.py` uses it for.

So the ranking is unchanged: Silero for precision, TEN VAD for recall, and
Whisper stays the labelling oracle it already was.

### Detecting music to drive the profile — tried, and it fails on a bike

A better idea than gating, and worth recording why it did not work rather than
leaving the next person to have it again.

The reasoning was sound on every count. The profile *is* what decides this
(Helmet 13.8% against Light 73.3%), Auto picks by level and therefore gets
quiet music wrong, and a profile hint is a far safer place to be wrong than the
transmit gate — Auto already has dwell and hysteresis to absorb it. It also
opens the one axis none of the five failed features touched: **they all judged a
10 ms block, and music is structure over seconds.** A single slice of a guitar
and of a vowel look alike, which is why RNNoise votes for both.

So `tools/vad/music_detect.py` scores 4-second windows on the raw microphone:
**beat**, the strongest envelope periodicity between 0.3 s and 2 s, and **tonal
persistence**, how much of the spectrum holds still. Against 253 s of music and
292 s of real helmet audio:

```
beat    AUC 0.579
tonal   AUC 0.537
both    AUC 0.610
```

Coins. And the breakdown says why, which is the part worth keeping:

```
beat, music vs speech_road          0.869   <- separates music from a talker
beat, music vs noise_road           0.390   <- inverted
beat, music vs rides without music  0.225   <- strongly inverted
```

**On a motorcycle the background is a machine, and a machine is more periodic
than music.** Engine firing and tyre roar give an envelope periodicity of 0.62
median where the music scores 0.36. Tonal persistence goes the same way: steady
engine noise holds its partials *perfectly still* (median 1.00) where music,
which changes chord, scores 0.83.

Both features do separate music from speech. Neither separates music from the
noise a rider is actually sitting in, and that is the discrimination the profile
decision needs. It is the same trap `core/src/audio/pitch.rs` already documents
from the other side — the pitch search had to exclude 30–60 Hz precisely because
an engine is periodic — arrived at again by a different route.

**Six hand-built features have now failed against this fault.** Every one of
them was reasonable, and the pattern is consistent enough to state: the signal
properties that distinguish music from speech in a quiet room are properties a
motorcycle also has.

**What survives is the placement, not the detector.** Driving the *profile*
rather than the gate is still the lower-risk move, and a neural VAD can drive
it: sustained energy with the VAD saying no speech is a much better "pick
Helmet" signal than anything measured here, and it reuses the model from the
step below instead of adding a second one. Worth folding into step 4.

### YAMNet, asked what the sound *is*

The right question and the right size: 4 MB of TFLite, one 0.975 s frame at a
time, and 521 classes of which two are `Speech` and `Music`. It can answer the
thing the profile decision wants to know rather than the thing the gate wants.

| Clip | Music p50 | Speech p50 | Loudest class |
|---|---|---|---|
| music only | **0.980** | 0.004 | Music |
| voice over music | 0.500 | 0.148 | Music |
| ride, no music A | **0.969** | 0.006 | Music |
| ride, no music B | 0.002 | 0.084 | Speech |

Music score, clips with music against clips without: **AUC 0.773**. Better than
anything hand-built here (0.579) and well short of the VADs on their own task
(0.90+).

**Ride B is the encouraging result and is worth stating.** It is a rider talking
*about* music — "let's try to turn on the music, the music on the phone does not
play" — and YAMNet scores Music at 0.002. The words are about music and the
sound is not, and it is not fooled. No lexical method would manage that.

**Ride A is not a problem, and the question it raised is withdrawn.** It scores
Music 0.969, as high as the actual music clip, and the first reading here called
that a false positive on engine noise. That reading assumed the target was
*music*. It is not.

**What the profile decision needs to know is whether the background is something
Helmet handles better** — and an engine at speed is exactly that. Scoring it
0.969 is the right answer to the right question. Under that target every clip
above is correct:

| Clip | Music score | Wanted |
|---|---|---|
| music only | 0.980 | Helmet |
| voice over music | 0.500 | Helmet |
| ride A, engine and wind | 0.969 | **Helmet** |
| ride B, rider talking, quiet | 0.002 | not Helmet |

So the AUC of 0.773 was measured against the wrong label and understates it. The
class is called `Music`; what it is being used as is *"loud structured
background"*, and the name is the model's, not the meaning.

This also retires the request to listen to `20260808-0512-000` and re-check the
beat/tonal negatives. Those negatives were only contaminated with respect to a
target we no longer have. (The `.wav` did not exist — only the `.raw` — and has
now been written beside it, for anyone who wants to hear it anyway.)


### Built: Auto may only lighten after 15 s of quiet

The one change from all of this that is in the code. `reconsider()` now counts
how long the raw floor has been below −55 dB and refuses to choose a *lighter*
profile until that reaches 15 seconds. Going heavier is unchanged.

The asymmetry is the point: going more aggressive costs some naturalness, going
lighter costs whatever is in the room going onto the wire, and `Light` was
measured to suppress music barely at all. Music playing at any level is not
quiet, so it can no longer talk `Auto` downwards.

Measured on the attenuation sweep, against the same figures before the change:

| Music level | Before | After |
|---|---|---|
| −17 dBFS | 28.7% (Helmet) | 28.7% (Helmet) |
| −27 dBFS | 47.2% (Standard) | **47.2% (Standard)** |
| −37 dBFS | 21.9% | 18.3% |
| −47 dBFS | 36.4% (Light) | **29.2% (Standard)** |

**And it costs nothing on speech**: the labelled clip is unchanged at 81.7%
recall and 72.7% precision, because a rider talking keeps the floor up and so
never qualified to lighten anyway.

**It does not fix the −27 dBFS case, and that is a different fault.** The
ratchet only blocks going lighter; at that level `Auto` never escalates to
Helmet in the first place, because the tracked floor sits near −50 dB while the
music is at −27. The floor tracker under-reads music, which is its own problem
and is not addressed here.

So this is a guard, not the fix. It removes the inversion at the quiet end and
leaves the middle alone.

### Built 2026-08-09: the classifier, the tap, and the 15 s hold

Shipped. The rule, as asked for: **when the background is loud and structured,
`Auto` takes Helmet; when it stops being so, Helmet is held for fifteen more
seconds.**

- **`core/src/audio/waveform.rs`** is the tap the design said was missing:
  15 600 samples at 16 kHz, decimated from 48 kHz by averaging threes — the
  same three-tap box average `yamnet_bench.py` used, deliberately, so the app's
  input matches the input every number here was measured on.
- **It arms by being read** and lapses five seconds later, exactly like the
  spectrum. Nothing polls, nothing collects, nothing infers — so a rider who
  has chosen a profile by hand pays nothing at all.
- **Escalating skips the dwell.** The five-second dwell exists to stop `Auto`
  flapping on a level threshold; this is not a level. Coming down does not
  skip anything, and composes with the calm ratchet: after the hold expires,
  lightening still needs its own fifteen seconds of real quiet.
- **The verdict is a tri-state**, not a bool. "Nobody is classifying" and "the
  background is clear" lead to opposite behaviour, and on desktop nothing ever
  classifies.
- **`background_classifier` is a setting**, on by default, hidden where the
  model cannot run, and the panel warns when it is running on the CPU.

**The bar is 0.30, and it was measured rather than chosen.**
`tools/vad/yamnet_threshold.py` scores frames rather than medians, because what
the app does with a frame is take Helmet on it:

| Clip | Wanted | ≥0.05 | ≥0.20 | ≥0.30 | ≥0.50 |
|---|---|---|---|---|---|
| music only | Helmet | 97.9% | 95.8% | 95.8% | 95.1% |
| voice over music | Helmet | 84.7% | 69.4% | 62.9% | 57.3% |
| ride A, engine and wind | **Helmet** | 100% | 100% | 100% | 100% |
| ride B, quiet, talking | no | **0.0%** | **0.0%** | **0.0%** | **0.0%** |

The separation is total: the negative clip never fires at any bar down to 0.05.
0.30 was taken because it sits in the flat part of all four curves — a
threshold on a plateau is robust, one on a slope is tuned to the clips it was
picked on.

**The negative side is now confirmed on somebody else's corpus.**
`yamnet_threshold.py --librispeech` scores 40 speakers of `dev-clean`, each a
different voice in a different room recorded on different equipment, none of it
ours:

| | n frames | ≥0.05 | ≥0.10 | ≥0.30 |
|---|---|---|---|---|
| LibriSpeech, 40 speakers | 342 | 0.3% | **0.0%** | **0.0%** |

So "one speaker, one room" is answered for false positives: at the shipped bar,
clean speech fired **zero times in 342 frames**. It says nothing about the
positive side — for that there is no substitute for another genre in another
room, and the tool now takes any file on the command line so that a new clip is
one command rather than an edit.

**Hardware acceleration: asked for, attempted, and mostly refused.**

The app asks for Core ML on iOS and the GPU delegate on Android, and falls back
to a plain CPU interpreter when either is turned down. On the Android emulator
the fallback is what happens, and the log says why in two separate ways:

```
Following operations are not supported by GPU delegate:
  COMPLEX_ABS, RFFT2D, SPLIT, GATHER (1D indices only), PAD, STRIDED_SLICE …
31 operations will run on the GPU, and the remaining 16 will run on the CPU.
Can not open OpenCL library on this device … Falling back to OpenGL
TfLiteGpuDelegate Init: [GL_INVALID_ENUM] … Created 0 GPU delegate kernels.
```

Two findings, and the first is the one that generalises:

1. **YAMNet is a poor candidate for delegation, on any device.** It computes its
   own mel spectrogram inside the graph, so `RFFT2D` and `COMPLEX_ABS` are part
   of the model — and no GPU delegate supports them. Even where the delegate
   initialises cleanly, **31 of 47 operations offload and 16 stay on the CPU**,
   with a transfer at each boundary. So "runs on the NPU" was never available
   here in the whole-graph sense; the best case is a partial offload of the
   convolutional stack.
2. **The emulator has no usable GPU driver**, which is a fact about the emulator
   and says nothing about a phone. What it did prove is that the fallback path
   works: the interpreter is rebuilt without the delegate, the classifier runs,
   and Diagnostics says it is on the processor.

**What the app claims is therefore deliberately narrow**: not "an NPU is doing
this", only that the accelerated path was or was not built. Core ML decides per
operation whether to use the Neural Engine, the GPU or the CPU and reports none
of it, so anything stronger would be a claim the code cannot check.

**An inference costs 2.4 ms, and that retires the worry.** Measured on Apple
Silicon by driving this exact model through this exact runtime with `ctypes` —
CPU only, XNNPACK, no delegate — averaged over 20 invocations. At one inference
every two seconds that is **0.12% of one core**. A sanity check came with it: a
440 Hz tone scores `Sine wave` 0.996, `Musical instrument` 0.012, `Music` 0.000,
which is the whole graph including the FFT front end behaving correctly.

So the first draft of the panel's note — which told riders it "costs battery on
a long ride" — was overstating a cost nobody had measured. It now reports the
milliseconds from the rider's own device instead, which is both honest and more
use: a slow phone shows a big number and a fast one shows the truth.

**macOS can run it, and it would be CPU-only.** The `tflite_flutter` package
already contains `libtensorflowlite_c-mac.dylib`: universal x86_64 + arm64,
`@rpath` install name, adhoc-signed, and depending on nothing but libSystem,
CoreFoundation and libc++. It loads, and the model runs on it. But its symbol
table has **no `TfLiteCoreMlDelegateCreate` and no `TfLiteGpuDelegateV2Create`**
— only XNNPACK. So macOS would be CPU-only by construction, at the 2.4 ms
above, on a TFLite **2.11.0** runtime three years older than the LiteRT the
Android side pulls.

What stops it being switched on is not capability: the podspec ships the dylib
with `vendored_libraries` commented out, so wiring it up means overriding a
third-party podspec and shipping a community-built binary through the Mac App
Store. That is a decision about supply chain and store risk, not about whether
it works — it does.

**What is still unmeasured**: per-inference cost on a *phone*, where the
delegate may attach and where finding 1 says the win will be partial anyway.
And the positive side of the corpus is still one genre, one room.

### DeepFilterNet, 2026-08-09 — the first thing that makes the problem easy

Installed locally to try (`tools/vad/dfn_enhance.py`). It is a *speech
enhancer* rather than a detector, which is the category everything else in this
file has ignored: instead of trying to recognise music and change a profile, it
removes everything that is not a voice. It runs natively at 48 kHz, so the
corpus goes in without a resample.

| Clip | YAMNet `Music`, before → after |
|---|---|
| music only | 0.980 → **0.004** |
| voice over music | 0.500 → **0.000** |
| ride, engine and wind | 0.969 → **0.004** |
| ride, quiet, talking | 0.002 → 0.004 |

**But a score of 0.000 is only good news if the voice survived it**, and that
table cannot say. The hand labels can. Against the 23 marked ranges in
`20260809-1201-000`:

| | speech | gaps | separation |
|---|---|---|---|
| original | −11.5 dB | −12.9 dB | **1.5 dB** |
| DeepFilterNet | −16.0 dB | −32.0 dB | **16.0 dB** |

Read the last column. **1.5 dB is the whole fault this file is about**: with
music playing, the gaps between words are within a decibel and a half of the
speech, so no threshold on level can separate them and every level-derived
feature here was doomed before it was written. DeepFilterNet takes 19.0 dB out
of the gaps and 4.5 dB out of the speech, which turns that into 16 dB — the
quantity the transmit gate keys on, improved by 14.5 dB.

**What this is not yet.** Level separation is not intelligibility: −4.5 dB on
the speech is a level change, and whether the voice is *damaged* needs ESTOI,
which `core/src/audio/quality.rs` already implements and this has not been run
through. It is also one clip, processed offline as a whole file — the streaming
mode a phone would use is a different thing to measure. And 10.4× real time on
a desktop CPU says nothing about an ARM core with a 10 ms budget and no
allocation allowed on the audio thread.

So: the most promising measurement in this file, and three specific reasons not
to believe it yet.

### The platforms were never running the same chain — live experiment

Reported 2026-08-09: **the same music in `Helmet` is suppressed markedly better
on Android than on an iPhone**, and better again on the Android emulator.

Not a bug in the chain. It is upstream of it, and it is an asymmetry nobody
chose:

- **Android** captures through cpal's AAudio backend, which sets **no input
  preset** — so AAudio applies its default, `VOICE_RECOGNITION`, and Google's
  noise suppression runs before our chain sees a sample. Verified by reading
  cpal 0.18: there is no `InputPreset` anywhere in its AAudio host.
- **iOS** used `mode: .default`, chosen deliberately so that Apple's voice
  processing would not sit underneath ours. That gave a genuinely raw
  microphone — and left `Helmet` doing the entire job alone.

So every comparison between the two platforms in this file has been comparing
different inputs, and the iPhone was the harder one by construction.

**The experiment**: iOS moves to `mode: .voiceChat`, one variable, with our own
AEC left running so the result can be attributed. Published for testing. What
decides it is music in `Helmet` against the same clip on Android; what would
call it off is echo pumping (two cancellers fighting) or the system AGC lifting
quiet music, which is precisely the input that produced the level inversion
recorded earlier in this file.

### The design as agreed, and the choices behind it

Written down before any of it was built, and kept because the reasoning is the
part worth having.

**What it decides.** One boolean on `AudioShared` — the background is loud and
structured — consulted by `reconsider()` as a *supporting* vote for Helmet, never
as a veto and never anywhere near the transmit gate. Wrong here costs some
naturalness; wrong at the gate cuts a rider off. That asymmetry is the whole
reason this is worth doing at the profile.

**Where the model runs: Dart, not Rust.** Asked again while it was being built,
so the reasons are written out rather than asserted:

- **The accelerators are only reachable through the platform SDKs.** This is the
  decisive one. Core ML on iOS and the GPU delegate on Android come from
  `TensorFlowLiteSwift/CoreML` and `com.google.ai.edge.litert:litert-gpu`,
  which the Flutter plugin already pulls through CocoaPods and Gradle. From
  Rust, `tflitec` binds a TFLite C library you must supply yourself and gets
  you no delegates; `tract` and `ort` are pure-CPU. "Use the NPU where
  available" is not implementable in the Rust core without rebuilding the
  platform integration the plugin already is.
- **Cross-compilation.** `core` is built for five targets. A C++ TFLite
  dependency would have to be vendored or built for each; Gradle and CocoaPods
  do that already, for the two platforms that matter.
- **Nothing about it is real-time.** One inference every two seconds on a
  0.975 s window, off the audio thread entirely. There is no latency argument
  for the core.
- **`core` stays testable anywhere.** Its 300-odd tests need no model, no GPU
  and no platform SDK, and a neural runtime in there would end that.

The cost, stated: the decision now lives outside the engine, so the chain has
to accept an opinion from something that may not exist. That is why
`background_noisy` is a tri-state rather than a bool — "nobody is classifying"
and "the background is clear" lead to opposite behaviour.

**What is missing to make that possible.** Nothing hands Dart a *waveform*
today. `audio_spectrum()` gives 24 bands, which YAMNet cannot eat: it wants
15 600 raw samples at 16 kHz, 0.975 s. So this needs a new `#[frb(sync)]` tap
returning a decimated mono window, built exactly like the spectrum one —
self-expiring so it stops when nobody is asking, since a model polled forever in
a rider's pocket is the failure `DiagnosticsPanel` already guards against.

**Cadence: once every few seconds, not per block.** The thing being detected
changes over tens of seconds and `AUTO_DWELL_BLOCKS` is already 5 s. Pair it with
the 15 s calm ratchet above: the classifier can push *towards* Helmet promptly
and can only release with the quiet the ratchet already requires.

**A dot in the diagnostics array**, beside the existing per-stage lights, so the
decision is visible rather than inferred — the panel's whole purpose. Green when
the background is clear, amber when it is loud and structured and Helmet is
being suggested, grey when the classifier is not running, which it will not be
whenever the panel is shut. It needs a `UiStage` id, a localised label in both
ARB files, and the same `if (state.diagnosticsOpen)` gating as the analyser.

**What must be measured before it ships**: per-inference CPU on a real phone,
and the battery cost of the tap. Neither is known.

**The 0.5 on speech-over-music is not a concern**, though an earlier draft here
said it was. Nobody talks continuously, so the frames between sentences carry
the same loud background and score like the music-only clip; and a single frame
never decides anything on its own, because the 15 s calm ratchet is what governs
coming back down. The classifier only has to be right *sometimes* to hold
Helmet, and wrong *continuously for fifteen seconds* to release it.

### The plan

Ordered so that each step is worth doing even if the next is never taken.

**1. Confirm it on more than one clip.** One music genre, one speaker, one room.
The result is large enough that it is unlikely to be noise, but every previous
conclusion in this file was overturned by the next measurement, and two of them
were mine from the same day. `bench_vads.py` takes a clip and its labels, so
this is a matter of recording rather than of code.

**2. Decide between them on the axis that matters to a rider.** Silero is the
better *detector*; TEN VAD keeps more speech. Being cut off mid-sentence is the
complaint this project has heard most, so 80.2% against 71.0% recall is worth
more than 97.9% against 89.5% precision — provisionally TEN VAD, but this is a
judgement to make with a rider, not from a table.

**3. Measure the cost before designing around it.** Both are small, but the
capture path has a 10 ms budget and no allocation on the audio thread. Needed:
per-block CPU on a real phone, model size in the bundle, and the added latency
(Silero's window is 512 samples at 16 kHz, so 32 ms, which is not free next to
the existing 80 ms onset delay). If it does not fit the worker, none of the
above matters.

**4. Add it as a third opinion, not a replacement.** The transmit decision is
`vad && snr`, both downstream of RNNoise. A neural VAD is the first genuinely
independent signal available, so the shape is `(vad && snr) || neural`, or
neural as a veto — which of those depends on step 2's recall/precision choice.
Keep the existing arms: they are what make the chain work with the panel closed
and the profile on Off.

**5. Re-run `core/tests/suppression.rs` and the road corpus.** The existing wind,
engine and traffic cases stay at zero or the change is not an improvement, it is
a different trade. This is the assertion most likely to fail.

**What is deliberately not in the plan:** training anything. There is a CUDA GPU
on the dev machine and it is not needed. Two off-the-shelf models already beat
every hand-built feature here, and this project has one trained model in its
history that collapsed on real audio after looking excellent on synthetic.

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

> **Retired, 2026-08-09. Do not spend time on this.** The fault is *external*
> music — a stereo in the room, a car alongside, a PA at the lights — reaching
> the microphone acoustically. Ducking governs audio played by other apps on the
> same device and can do nothing about sound in the air, so it is not a cheaper
> explanation here; it is not an explanation at all. Music played through the
> intercom is reported as no trouble.
>
> Everything above this line stays because the reasoning was sound and the
> conclusion was wrong for a reason worth keeping: it assumed the music reached
> the microphone through the phone. Once the source is outside, the entire
> routing hypothesis goes with it, and what is left is a DSP problem after all.

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

**The sidecar arrived on 2026-08-09 and the scored numbers are above.** Both
halves of the acceptance criteria now have real labels behind them, and the
answer is that no candidate in this file is supported by them: the strongest
feature the chain computes scores 0.827 and the pitch measure scores 0.603.

What is left is a decision, not a measurement. Either accept the trade Helmet
already makes -- 80% of speech kept, a quarter of what is sent still music --
or find a feature that is not measured downstream of RNNoise, since every one
that is has now been scored and none separates the classes.
