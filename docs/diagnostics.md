---
layout: default
ref: diagnostics
title: Diagnostics
description: The panel that says why something happened — the analyser, the stage lights, the counters and the engine log.
---

**Diagnostics is not in Settings.** It is the waveform icon in the toolbar, and
nothing in it changes how the app sounds.

It exists to answer *why did that happen*, which is a different job from
[Settings]({{ '/settings.html' | relative_url }}) and needs a different page. It
is the first thing to open when somebody says they could not be heard, and the
sections below are in the order the panel shows them.

## The live analyser

<div class="shots">
  <figure>
    <img src="{{ '/assets/img/shots/diag-analyser-phone.webp' | relative_url }}"
         alt="The analyser while speech is going out: a grey microphone trace, a
              blue after-suppression trace almost on top of it, and a row of
              green bars along the bottom for what is being transmitted."
         width="560" height="244" loading="lazy" decoding="async">
    <figcaption>Three views of the same 10 ms: what came in, what survived
    suppression, and what went out.</figcaption>
  </figure>
</div>

Grey is the microphone before anything touches it, blue is the same audio after
suppression, and green is what actually went to the server. They are measured
on the **same block**, which is the whole point — you are watching one decision,
not three unrelated meters.

**When the green falls away while the other two do not, the gate closed on
you.** That is the fault riders report as "it cut me off", and this is the one
place it is visible while it happens.

The analyser costs nothing when the panel is shut: it is asked for by being
read, and the request lapses half a second after the last read, so closing the
panel — or just backgrounding the app — stops the transforms without any
switch to remember.

## A light per stage

<div class="shots">
  <figure>
    <img src="{{ '/assets/img/shots/diag-lights-phone.webp' | relative_url }}"
         alt="The profile Automatic has landed on, then two rows of coloured
              dots labelled echo, suppressor, voice detected, gate, levelling,
              hiss, feedback, background and to the server, and a note saying
              background detection is running on the processor at 130
              milliseconds per check."
         width="560" height="170" loading="lazy" decoding="async">
    <figcaption>Where Automatic landed, every stage of the chain, and what the
    classifier costs on this device.</figcaption>
  </figure>
</div>

One dot per stage of the capture chain: **echo**, **suppressor**, **voice
detected**, **gate**, **levelling**, **hiss**, **feedback**, **background** and
**to the server**. The colours mean the same thing at every stage — green is
working and passing audio on, amber is working but holding something back, red
is stopping audio here, and grey is switched off and therefore has no opinion.

The last one is the one to read first. **To the server** red means your voice
is not leaving the phone, and the dot to its left usually says which stage
stopped it.

**Background** is the odd one out: it is not a stage the audio passes through
but the sound classifier
[Automatic]({{ '/settings.html' | relative_url }}) runs. Green means it is
listening and the background is clear, amber that it has heard engine, wind or
music and is holding the helmet setting, and grey that nothing is classifying —
because a profile was chosen by hand, or because this is a desktop, where the
model does not run. When it is grey the line underneath says which.

## Counters

<div class="shots">
  <figure>
    <img src="{{ '/assets/img/shots/diag-counters-phone.webp' | relative_url }}"
         alt="Two lists of counters: incoming audio, with decoded, invented,
              gaps concealed, jitter buffer and speakers tracked; and this
              device, with playback gaps, microphone dropped, microphone level,
              noise floor and the level the gate opens at."
         width="560" height="387" loading="lazy" decoding="async">
    <figcaption>A counter turns amber when it is the one worth reading.</figcaption>
  </figure>
</div>

**Incoming audio** is about what other people sent you: milliseconds decoded,
milliseconds *invented* to cover gaps, how many gaps were concealed, how much
is being held in the jitter buffer, and how many people are being tracked.

**This device** is about you. Playback gaps and microphone drops are both
"this phone could not keep up"; **playback gaps is the one that says to raise
the [incoming audio buffer]({{ '/settings.html' | relative_url }})**.

The last three are the voice gate, in numbers: **microphone level** is what is
arriving now, **noise floor** is what the app has settled on as this
environment's quiet, and **opens at** is the level your voice has to beat to be
sent. On a bike at speed the floor climbs and the threshold climbs with it —
which is what the noise profiles are for.

## The last thirty seconds

<div class="shots">
  <figure>
    <img src="{{ '/assets/img/shots/diag-graphs-phone.webp' | relative_url }}"
         alt="Two small history graphs, CPU and memory, each with its current
              value, a thirty-second trace and a peak."
         width="560" height="253" loading="lazy" decoding="async">
    <figcaption>Half a minute of history, with the peak, under each
    figure.</figcaption>
  </figure>
</div>

Network in and out, voice packets in and out, CPU and memory, each as a current
value and a thirty-second trace with its peak. A number that is fine *now* and
was not a moment ago is exactly the shape of an intermittent fault, and a
single reading cannot show it.

## The engine log

<div class="shots">
  <figure>
    <img src="{{ '/assets/img/shots/diag-log-phone.webp' | relative_url }}"
         alt="The engine log: timestamped lines about audio streams opening,
              connecting to a server, voice going direct over UDP and
              disconnecting, with a problems-only filter, a copy button and a
              clear button."
         width="560" height="335" loading="lazy" decoding="async">
    <figcaption>What the engine and the session actually did, with the
    times.</figcaption>
  </figure>
</div>

Timestamped lines from the audio engine and the session: devices opening and
closing at which rate and channel count, connecting, voice going direct over
UDP or falling back to TCP, disconnecting and why.

**Problems only** hides everything that went right, which is usually what you
want. The two buttons beside it copy the log to the clipboard and clear it.
Copying it into a message is more use to anyone helping you than a description
of the symptom.

## Record for diagnosis

<div class="shots">
  <figure>
    <img src="{{ '/assets/img/shots/recording-card-phone.webp' | relative_url }}"
         alt="The recording card expanded, showing how many files are held and
              how much space they take, with a small red delete button at the
              far left and listen and share buttons at the far right."
         width="560" height="266" loading="lazy" decoding="async">
    <figcaption>Off unless you turn it on. Expanded, it says what is held and
    what you can do with it.</figcaption>
  </figure>
</div>

Saves your microphone to this device, along with what the chain decided about
every 10 ms of it. **Off unless you turn it on**, and it says so on screen the
whole time it is running.

**Share** produces a `.zip` per 18 MB, so a whole ride fits through anything
that carries files. See [sending a diagnostic
recording]({{ '/sending-a-recording.html' | relative_url }}).

## Listen back

<div class="shots">
  <figure>
    <img src="{{ '/assets/img/shots/listen-phone.webp' | relative_url }}"
         alt="The listen-back sheet playing a recording: one chip per recording,
              a waveform whose green stretches are the parts that went to the
              server, a red playhead part way along, and the elapsed and total
              time in minutes, seconds and milliseconds."
         width="560" height="484" loading="lazy" decoding="async">
    <figcaption>Green is what went out. Drag, tap or pinch the
    waveform.</figcaption>
  </figure>
</div>

Plays a recording back with its waveform and a playhead you can drag or tap to
scrub. **The green stretches are the parts that actually went to the server**,
read from the decision log beside the audio — so the question a rider is really
asking, *was I heard here*, is answered by the picture rather than by listening
for a gap.

Pinch to zoom on a phone, or hold ctrl and use the wheel on a desktop; the
playhead stays in view as it moves. The clock beneath counts milliseconds,
because a gate that shuts mid-word does it well inside a second.

A single recording can be shared or deleted from here, which is where that
decision actually gets made: you have just heard what is in it.

On the card and on the listen sheet alike, delete and share sit at opposite
ends of their row, deliberately: one sends a file and the other destroys the
only copy of a ride that cannot be recorded again. Only the destructive one
asks first.
