---
layout: default
ref: sending-a-recording
title: Sending a diagnostic recording
description: How to capture what went wrong and get it to someone who can read it.
---

Faults like *"it cut me off mid-sentence"* cannot be diagnosed from a
description. A recording carries the audio **and** what the noise chain decided
about every 10 ms of it, which is what makes the difference between reading a
fault and guessing at one.

## Record the fault

1. Open **[Diagnostics]({{ '/diagnostics.html' | relative_url }})** — the
   waveform icon in the toolbar.
2. Expand the **Record for diagnosis** card and turn the switch on.
3. **Ride, and provoke the fault.** Talk the way you were talking when it went
   wrong, at the speed it went wrong at, with the same headset.
4. Turn the switch off.

Thirty seconds of the actual problem beats ten minutes of ordinary riding. If
the fault is intermittent, record the whole ride — the log makes it possible to
find the moment afterwards.

<div class="panel warn">
<p><strong>It records your microphone.</strong> Not the conversation, not the
other riders — your own microphone, as the app hears it. Anything you say while
it is on is in the file, so turn it off when you have what you need.</p>
</div>

## Listen to it first

Before you send anything, play it back: the **Listen back** button in the
recording card opens a waveform with a playhead you can drag or tap to scrub.
It is a recording of your own voice and you should know what is in it. If you
would rather not send the audio at all, you do not have to — see below.

### Hear what the others heard

The green stretches of the waveform are the parts that went to the server.
Everything else was recorded but never sent.

The **speaker icon** beside play turns green and plays **only the green
stretches**, skipping everything the noise gate rejected. That is as close as
you can get to sitting at the other end of the channel — without a second
phone, a second account, and trying to judge your own voice coming back at you.

It is the fastest way to answer the question that matters: *was I cut off, or
did it just sound like it?* If words are missing from this playback, they were
missing for everyone.

<div class="shots">
  <figure>
    <img src="{{ '/assets/img/shots/recording-card-phone.webp' | relative_url }}"
         alt="The recording card expanded, showing how many files are held and
              how much space they take, with a small red delete button at the
              far left and listen and share buttons at the far right."
         width="560" height="266" loading="lazy" decoding="async">
    <figcaption>Expanded: what is held, and the three things you can do with it.</figcaption>
  </figure>
  <figure>
    <img src="{{ '/assets/img/shots/listen-phone.webp' | relative_url }}"
         alt="The listen-back sheet playing a recording: one chip per recording,
              a waveform whose green stretches are the parts that went to the
              server, a red playhead part way along, and the elapsed and total
              time in minutes, seconds and milliseconds."
         width="560" height="484" loading="lazy" decoding="async">
    <figcaption>Listening back before sending. Green is what went out; drag,
    tap or pinch the waveform.</figcaption>
  </figure>
</div>

## Send it

The share button produces one or more `.zip` files, each under 18 MB so they
fit through anything.

### If you are already in touch with the developer

Send the archive to the Telegram bot. It unpacks, converts and files it
automatically. Ask for the bot's name.

### Otherwise, the general route

1. **Share** the archive to wherever you already keep files — Drive, iCloud,
   Dropbox, WeTransfer, email to yourself.
2. **Open an issue** at
   [github.com/zavitax/mumbleway/issues]({{ site.repo }}/issues).
3. **Paste the link**, and say what went wrong and roughly when in the
   recording it happened.

<div class="panel">
<p><strong>Please do not attach the audio to the issue itself.</strong> Issue
attachments are public and permanent. A link you control can be taken down
again; a file on a public issue cannot.</p>
</div>

### If you would rather not send audio at all

Send **only the `.csv` file** from the archive. It contains no sound — it is
numbers, one row per 10 ms of the ride: whether the channel was open, whether
speech was detected, the signal-to-noise ratio, the level, the noise floor.

Most questions are answerable from that alone: *did the gate close, when, and
what was the chain looking at when it did.* It is small enough to attach to an
issue directly, and there is no voice in it to think twice about.

## What is in the archive

<div class="table-wrap" markdown="1">

| File | What it is |
|---|---|
| `YYYYMMDD-HHMM-NNN.s16` | The audio, exactly as the chain received it: raw 16-bit PCM, mono, 48 kHz, no header |
| `YYYYMMDD-HHMM-NNN.csv` | One row per 10 ms — `transmitting`, `speaking`, `gate_open`, `vad`, `snr_db`, `level_db`, `floor_db`, `harmonicity`, `modulation` |

</div>

The two share a name and belong together: the audio without the log is a
recording nobody can say anything about, and the log without the audio is still
useful.

`transmitting` is the important column. It is what actually went on the wire —
envelope, mode and mute included — so a run of zeros in the middle of speech is
the fault, visible without listening to anything.

## Playing the audio yourself

The `.s16` has no header, so a media player will refuse it. Tell a tool what it
is:

```bash
ffplay -f s16le -ar 48000 -ac 1 20260808-1139-000.s16
```

Or convert it to something ordinary:

```bash
ffmpeg -f s16le -ar 48000 -ac 1 -i 20260808-1139-000.s16 ride.wav
```

Audacity will import it too: **File → Import → Raw Data**, then signed 16-bit
PCM, little-endian, 1 channel, 48000 Hz.

## What happens to it

Nothing automatic. The app has no servers, uploads nothing on its own, and the
developer receives only what you choose to send, to a destination you chose.
The [privacy policy]({{ '/privacy.html' | relative_url }}) says the same thing
in more detail.
