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

**Through the headset, not the phone.** This is the whole reason the previous
set was unusable. Record in a way that captures the Bluetooth hands-free route
— a call in MumbleWay, or any recorder that takes the HFP input. If in doubt,
play the file back afterwards: if the voice sounds close and the wind sounds
distant, it is the boom mic. If the voice sounds far away, it is the phone.

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
