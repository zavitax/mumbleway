# Building a voice model for a helmet

A methodology for training our own network to do two jobs from one mono
microphone: decide when the rider is talking, and hand the encoder the cleanest
version of it we can produce.

Written after measuring the current chain against real helmet recordings. The
numbers below are from that work and are the reason for every choice here.

---

## 1. What the measurements say the problem is

RNNoise is currently **both** the suppressor and the detector, and on audio
recorded inside a helmet at road speed it is failing at both:

| measurement | value | where |
|---|---|---|
| Speech blocks its VAD fires on | **38%** | `core/tests/road.rs` |
| Blocks that clear the SNR margin | 88% | same |
| Signal it removes, Helmet profile | **25 dB** | stage attribution |
| Of the rider's speech, transmitted | **57%** | labelled recall |
| Of what is transmitted, actually speech | 54% | labelled precision |

One component, three failures, one root: it removes 25 dB of a signal it has
misjudged, then decides whether to transmit what is left of it. This is why the
rider reports being cut off, sounding mangled, **and** leaking wind — all at
once, which no single threshold can produce and no threshold can fix.

Three attempts to buy recall by moving thresholds all failed, and the reason is
recorded in `core/src/audio/denoise.rs`: the VAD is the binding constraint, and
it is also the only thing in the chain that recognises an engine. Weakening it
puts 44–76% of engine noise on the wire. **The ceiling is the model, not its
tuning.**

Two off-the-shelf models were then measured (`tools/vad/README.md`). TEN VAD
finds real speech the chain misses, on the raw microphone, where Silero finds
almost none. That establishes the speech *is* recoverable and that the current
detector is what is losing it.

**So the goal is not a better threshold. It is a better model, and one we can
train on the condition that actually matters — which no public corpus
contains.**

---

## 2. The single most important design decision

**Train on synthetic mixtures, because they come with perfect labels for free.**

The labelling bottleneck has dominated this whole investigation: three seconds
of hand-labelled speech, which was enough to disprove a hypothesis and nowhere
near enough to establish one. Hand-labelling hours of audio is not going to
happen.

It does not have to. If we mix a **clean speech recording** with a **noise-only
motorcycle recording**, then we know exactly where the speech is — it is where
the clean signal was, before we added anything. The label is exact, free, and
available for as many hours as we care to generate.

That also gives the enhancement target for free: the clean signal *is* the
thing the network should output. One mixing pipeline produces the input, the
VAD label and the enhancement target together.

This reframes the recording task completely. **We do not need recordings of
people talking on motorbikes. We need hours of motorbike with nobody talking.**
That is vastly easier to collect: ride, stay quiet, keep the recorder running.

---

## 3. Architecture

### 3.1 One encoder, two heads

```
   mono 16 kHz ─► STFT ─► shared encoder ─┬─► mask head ──► enhanced speech
                                          └─► VAD head ───► speech probability
```

The two tasks want the same features. A network that can separate speech from
wind well enough to mask it has already learned what speech looks like, and the
VAD head is then a few thousand parameters on top. Training them jointly is
also a regulariser: the enhancement target is dense supervision at every
time-frequency bin, where the VAD label is one bit per frame, so the mask head
teaches the encoder far faster than the VAD head could alone.

### 3.2 Start from an existing model, do not start from scratch

With a 6 GB laptop GPU and a few hours of in-domain noise, fine-tuning beats
training from scratch by a wide margin and gets to an answer in days rather
than months.

Candidates, in the order I would try them:

| model | size | why |
|---|---|---|
| **DeepFilterNet3** | ~2 M params | Real-time on one core, and **the inference side is already written in Rust** — which is the language this codebase is in. Best deployment fit by a distance. |
| **GTCRN** | ~24 K params | Astonishingly small for its quality; designed for exactly this constraint. Good if DeepFilterNet is too heavy on a phone. |
| **DTLN** | ~1 M params | Older, very well documented, trivially exportable. A safe fallback. |

All three are mask-based, causal, mono, and 16 kHz — which is what a Bluetooth
hands-free link gives us anyway.

The VAD head is ours regardless: a 2-layer GRU (64 units) or a small TCN over
the encoder output, ~20 K parameters.

### 3.3 Latency budget

The chain runs 10 ms blocks. Total added algorithmic latency must stay under
about 30 ms or it eats into the 80 ms onset look-ahead that already exists.
That rules out any model needing a look-ahead window of its own, and rules in
the causal mask-based family above.

---

## 4. Loss design

This is where the emphasis on recall and on *not over-suppressing* has to be
made explicit, because the default losses produce exactly the failure we
already have.

### 4.1 VAD head — asymmetric, because the errors are not symmetric

A missed word is unrecoverable; the listener does not know it happened. Leaked
wind is annoying and a listener filters it out. So:

```
L_vad = w⁺ · BCE(speech frames) + w⁻ · BCE(non-speech frames),   w⁺/w⁻ ≈ 4
```

Report **recall at a fixed false-alarm rate** as the headline number, not
accuracy and not F1. Accuracy is dominated by the non-speech majority class and
F1 weights the two errors equally, which is precisely the assumption being
rejected.

### 4.2 Mask head — punish removing speech harder than leaving noise

The measured 25 dB of over-suppression is what a symmetric loss produces. An SI-SDR
or MSE loss is perfectly happy to delete a quiet consonant, because the error
from deleting it is small. Add an explicit asymmetry:

```
L_mask = MR-STFT(ŝ, s)  +  λ · mean(relu(|S| − |Ŝ|)²)
                                    └── only penalises attenuation
```

The second term is zero when the network leaves too much in and grows when it
takes too much out. It is the direct expression of "I would rather hear some
wind than lose the word", which is the trade the rider actually wants.

Also include a band-limited intelligibility term over 300–4000 Hz. The existing
`core/src/audio/quality.rs` measure is a reasonable target to correlate
against, and it is already the yardstick the offline suites use.

### 4.3 Joint

```
L = L_mask + α · L_vad,   α tuned so neither head collapses
```

Watch for the mask head winning and the VAD head degenerating to "always
speech" — the asymmetric weighting makes that an attractive local minimum.

---

## 5. The corpus

### 5.1 Clean speech (public)

| corpus | size | licence | note |
|---|---|---|---|
| **LibriSpeech** | 960 h | CC BY 4.0 | The workhorse. Read speech, clean. |
| **Common Voice** | 1000s h | CC0 | Accents, ages, non-native speakers, **and Russian** — the app is localised ru. |
| **VCTK** | 44 h | CC BY 4.0 | 110 speakers, studio quality, good for held-out speaker tests. |
| **Golos** | ~1000 h | public | Russian, crowd-recorded on phones — closer to our channel than studio audio. |

Bias the mix towards **short utterances and single words**, because that is
what intercom traffic is. LibriSpeech is continuous reading and will
over-represent long fluent passages.

### 5.2 Noise (public)

| corpus | licence | note |
|---|---|---|
| **MUSAN** | CC / public domain | 109 h speech/music/noise, redistributable. The standard base. |
| **DEMAND** | CC BY-SA | Real multichannel ambient — includes traffic, car interiors. |
| **FSD50K** | CC | **Has explicit motorcycle and engine classes.** Nearest public thing to our case. |
| **AudioSet** | labels CC BY | Classes for *Motorcycle*, *Wind noise (microphone)*, *Vehicle*. Clips need fetching separately. |
| **WHAM!** | CC BY-NC | Real ambient recordings in noisy public places. |
| **QUT-NOISE** | free for research | Built specifically to *evaluate* VAD across noise types and SNRs — use for test, not train. |

**Keep the music.** The one measured false positive the current chain has is
loud music (`core/tests/suppression.rs`), and it is the case a harmonicity gate
could never fix. Training against MUSAN's music partition is the direct
approach.

### 5.3 Motorcycle noise (ours — the part nobody else has)

This is the gap and it is the deliverable that unblocks everything else.

**Protocol.** Record *noise only* — the rider says nothing:

- **Speeds**: stationary/idle, 50, 90, 130 km/h. Wind noise scales roughly with
  the sixth power of velocity; a model trained at one speed will not transfer.
- **Visor**: closed, cracked, open. Changes the spectrum dramatically.
- **Head position**: straight, turned, tucked behind the screen.
- **Surfaces**: smooth tarmac, coarse chip, wet.
- **Traffic**: alone, beside lorries, in town, at a junction with other bikes
  idling — the case `testsig::traffic` was invented to approximate.
- **Duration**: aim for **5–10 hours**. It is unattended recording; the cost is
  disk, not effort.

**Record through the real channel.** This matters more than any of the above:
capture via the Cardo Edge Pro over Bluetooth HFP through the phone, not with a
studio recorder. The microphone response, the 16 kHz band limit and the codec
artefacts are all part of the domain, and a model trained on studio-clean noise
and deployed behind an HFP link will meet a distribution it has never seen.

**Also record a small paired set** — the same rider, same script, quiet room and
then on the bike. Not for training (alignment is impractical) but as an honest
held-out test with real rather than synthesised mixing.

### 5.4 Synthetic noise (already built)

`core/src/audio/testsig.rs` generates seeded wind, engine, traffic, music and
randomly-shaped unknown noise. Its role is **not** training data — real noise is
better — but as an adversarial test set whose parameters can be swept, and to
check that the model has not overfitted to the specific bikes we recorded.

---

## 6. Training pipeline

**Mix dynamically at training time, never pre-mixed.** Pre-mixing fixes the
SNR/noise pairing and the network memorises it.

Per example:

1. Draw a clean utterance; draw a noise segment.
2. Draw SNR uniformly in **−10 … +20 dB**. Weight the low end: −5 to +5 dB is
   where the current chain fails and where the model must be good.
3. Random gain, so the model does not key on absolute level — the mistake the
   existing chain makes.
4. Simulate the channel: band-limit to 16 kHz, optionally apply mSBC/CVSD codec
   artefacts, occasional packet loss.
5. Augment: spectral tilt (helmet resonance), mild clipping, DC offset.
6. **Labels fall out**: VAD target from an energy threshold on the *clean*
   signal before mixing; enhancement target is the clean signal itself.

Keep a **speaker-disjoint** and **noise-recording-disjoint** validation split.
Splitting randomly over segments leaks — segments from one ride are far more
alike than two rides are.

---

## 7. Validation

The harnesses already exist and should be the acceptance criteria, unchanged,
so the new model is judged by the same yardstick as the old chain:

- **`core/tests/road.rs`** — recall and precision against hand labels on real
  helmet audio. This is the headline. Current chain: 57% / 54%.
- **`core/tests/suppression.rs`** — noise alone must transmit *zero*. The
  current chain achieves this and it is the constraint every recall improvement
  has broken so far.
- **`core/src/audio/quality.rs`** — intelligibility of what is transmitted,
  which catches a model that raises recall by sending mush.
- **Stage attribution** — the enhanced output must not sit 25 dB below the
  input. Over-suppression is a measurable regression now, not a matter of
  opinion.

Add: **cost on the actual phones**, measured before anything ships. A model
that wins offline and eats 30% of a core on an OPPO is not a win.

---

## 8. Staging

Sequenced so each step is useful even if the next never happens.

| stage | work | unblocks |
|---|---|---|
| **0** | Replace the transmit decision's dependence on RNNoise's VAD with TEN VAD, measured. | Immediate recall improvement with no training at all. |
| **1** | Record 5–10 h of motorcycle noise through the real headset. | Everything below. Do this first — it is the long pole and it needs a rider, not a GPU. |
| **2** | Build the dynamic mixing pipeline; validate that the free labels agree with the hand-labelled recordings. | Unlimited labelled training data. |
| **3** | Fine-tune DeepFilterNet3 on the mixture, mask head only. | Fixes over-suppression; keeps existing VAD. |
| **4** | Add and train the VAD head jointly with the asymmetric loss. | Fixes recall at the root. |
| **5** | Quantise, export ONNX, measure on device, ship behind a setting. | Ships. |

**Stage 0 and Stage 1 can start today and do not depend on each other.**
Stage 1 is the one that needs the user.

---

## 9. What could make this not work

Recorded honestly, because every confident hypothesis in this project so far
has had to be withdrawn:

- **Synthetic mixing may not reproduce a helmet.** Wind inside a helmet is
  partly generated *at the microphone* by turbulence, not a distant source
  arriving through air. Additive mixing may simply be the wrong model, in which
  case the paired recordings of §5.3 become essential rather than a nicety.
- **The speech may be genuinely unrecoverable at these SNRs**, and the honest
  outcome would be a better *push-to-talk* experience rather than better
  detection.
- **A model that fixes recall may cost precision** in ways the offline suites
  do not capture, and only riding will show it.
- **Two models have now looked decisive offline and failed on real audio.**
  Nothing here should be believed until it is measured on `road.rs`, and
  nothing should ship until it is heard on a bike.
