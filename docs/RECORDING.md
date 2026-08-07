# What to record, and what each recording lets us claim

Every number produced so far came from four clips captured on the phone's own
microphone rather than the headset's, so none of them describe the signal the
app actually sees. This is the minimal set that replaces them.

Each row buys one specific assertion. Nothing here is a wishlist — if a row is
missing, the claim next to it cannot be made, and if a row is present the claim
can be made without argument.

| # | Recording | Length | Lets us assert |
|---|---|---|---|
| **A** | **Quiet control** — stationary, engine off, read the script | 90 s | What the headset and chain do when the problem is easy. Without it, a bad number in B is ambiguous between "hard condition" and "broken chain". |
| **B** | **Speech over noise** — the script, at each speed | 90 s × 3 | *The headline.* How much of the rider's speech the chain transmits, and whether TEN VAD or a trained model beats it. |
| **C** | **Noise only** — riding, nobody talking, each speed | 5 min × 3 | That noise alone transmits nothing, and the training corpus for mixing. |

Roughly **25 minutes of riding per headset.** A and B are the ones that unlock
measurement; C is the one that unlocks training.

## How to record

**Use the app.** Open the diagnostics panel, and under the analyser turn on
*Record for diagnosis*. Everything below about which microphone is which stops
being something to get right: the app records the audio its own capture chain
received, so the route is correct by construction rather than by care.

It writes two files at a time, rotating every 16 MB:

- `<date>-<time>-NNN.s16` — the capture, 16-bit mono at 48 kHz, headerless.
  Play it with `ffplay -f s16le -ar 48000 -ac 1 <file>`.
- `<date>-<time>-NNN.csv` — one line per 10 ms block, holding what the chain
  decided and what it decided it from. This is the part that cannot be
  recovered afterwards: from the audio alone "the gate was shut here" is an
  inference, and from this file it is a fact.

**Pocket the phone and ride.** The switch opens the microphone itself, the same
way a call does, and holds it open until you turn it off — so recording works
without being connected to anything, and keeps working with the screen locked.
On iOS that is the `audio` background mode; on Android it is the microphone
foreground service, which is why a notification stays up while it runs.

Turn it **off** before sharing. The switch closes the last file; sharing while
it is still being written sends a truncated one. The panel then offers *Share
recordings*, which on Android and iOS opens the normal share sheet — the
Telegram intake bot in `tools/vad` accepts them directly, one file at a time up
to 20 MB, which is what the rotation size is chosen for.

If the switch reports blocks lost, storage could not keep up and the recording
has gaps in it. It is still usable; the gaps are just not silence.

**If you record outside the app instead**, it has to be through the headset and
not the phone — this is the whole reason the previous set was unusable. Record
in a way that captures the Bluetooth hands-free route. If in doubt, play the
file back afterwards: if the voice sounds close and the wind sounds distant, it
is the boom mic. If the voice sounds far away, it is the phone. There is no way
to tell from the file itself, which is why the in-app recorder exists.

**Say the condition aloud at the start of every file.** "Cardo Edge Pro, ninety,
visor closed." It costs two seconds, it labels the file better than a filename,
and it doubles as a clean speech sample at the top of every recording.

## The script

Read the same words in every condition. Same content across conditions is what
makes them comparable, and known content is what makes intelligibility
measurable rather than just detection.

Leave a gap of roughly ten seconds between lines. **The rhythm is the label** —
speak, pause, speak — so nothing has to be timed with a stopwatch and nothing
has to be annotated afterwards.

> One. The quick brown fox jumps over the lazy dog.
>
> Two. She sells sea shells by the sea shore.
>
> Three. Pack my box with five dozen liquor jugs.
>
> Four. How much wood would a woodchuck chuck.
>
> Five. The rain in Spain falls mainly on the plain.

The numbers matter more than the sentences: they are unmistakable in a
transcript, so alignment can be checked automatically, and if a line is missing
we know exactly which. The sentences are chosen to cover the consonants that
carry intelligibility — the "s" and "sh" sounds a gate clips first.

## Priority, if time is short

1. **A and B on one headset.** That alone replaces every recall and precision
   number in this repository and settles whether TEN VAD is worth integrating.
2. **C on the same headset.** Unblocks training.
3. **A and B on the other headsets.** Tells us whether any of it generalises,
   which is the difference between a fix for one rider and a fix for the app.

## What we will do with each

- **A** establishes the ceiling: if the chain loses speech even here, the fault
  is not the wind.
- **B** is scored against the rhythm to give recall and precision, the same way
  `core/tests/road.rs` does now, and Whisper is run over it as an
  oracle — it should transcribe the script, and if it does, its output labels
  every future recording for free.
- **C** is mixed with public clean speech to train, per `VOICE_MODEL.md`, and
  run through the chain to check that noise alone still transmits nothing.
