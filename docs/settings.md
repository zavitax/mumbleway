---
layout: default
ref: settings
title: Settings
description: What each setting does, and which ones are worth changing on a motorcycle.
---

Most of this can be left alone. Three settings matter on a bike — **microphone
mode**, **noise cancellation** and **microphone gain** — and the rest exist for
when something specific is wrong.

**The sections below are in the order the app shows them**, so this page can be
read beside the screen rather than searched. Diagnostics comes last because it
is not in Settings at all — it is the waveform icon in the toolbar.

## Audio devices

How sound gets in and out — including several switches that are not obviously
"devices" but sit here because they belong to the route rather than to the
microphone.

### Choosing the devices

<div class="shots">
  <figure>
    <img src="{{ '/assets/img/shots/set-devices-windows.webp' | relative_url }}"
         alt="The audio devices section on Windows: a microphone picker, a
              speakers picker, and a re-check devices button."
         width="1000" height="210" loading="lazy" decoding="async">
    <figcaption>On Windows you pick them. A phone routes audio itself and
    says so instead.</figcaption>
  </figure>
</div>

On **desktop** you pick input and output explicitly, and **Re-check devices**
re-reads the list after you plug in or pair a headset.

On **a phone there is nothing to choose.** The platform owns the routing and
switches to a headset when one connects, so the pickers are not shown at all —
the app says so, rather than offering a control that could do nothing.

### Test microphone (hear yourself)

<div class="shots">
  <figure>
    <img src="{{ '/assets/img/shots/set-testmic.webp' | relative_url }}"
         alt="The test microphone row with its switch off."
         width="560" height="150" loading="lazy" decoding="async">
    <figcaption>Hear yourself exactly as the far end does.</figcaption>
  </figure>
</div>

Plays your processed voice back exactly as the far end hears it — after
suppression, gate and levelling. The fastest way to judge a profile.

**Use headphones.** Through speakers it is a feedback loop.

### Echo cancellation

<div class="shots">
  <figure>
    <img src="{{ '/assets/img/shots/set-echo.webp' | relative_url }}"
         alt="The echo cancellation row, switched on."
         width="560" height="177" loading="lazy" decoding="async">
    <figcaption>On by default. Worth it with speakers, not with a headset.</figcaption>
  </figure>
</div>

On by default, and worth leaving on **when you are using speakers**. A speaker
a few centimetres from the microphone sends everybody back to themselves with a
delay, which is more distracting than almost any other audio fault.

On a headset there is no echo to cancel and it can only take something away.

### Even out speaker loudness

<div class="shots">
  <figure>
    <img src="{{ '/assets/img/shots/set-evenout.webp' | relative_url }}"
         alt="The even out speaker loudness row, switched on, warning that a hiss between sentences may be this."
         width="560" height="192" loading="lazy" decoding="async">
    <figcaption>The row that explains a hiss you were about to blame on the noise chain.</figcaption>
  </figure>
</div>

Brings incoming voices to a similar level, so a quiet rider and a loud one
arrive at the same volume. It adapts to what it hears, which has one visible
consequence: **if a hiss seems to rise between sentences, turn this off to
check.** It may be levelling the gaps up rather than anything being wrong in
the noise chain.

### Incoming audio buffer

<div class="shots">
  <figure>
    <img src="{{ '/assets/img/shots/set-buffer.webp' | relative_url }}"
         alt="The incoming audio buffer slider at 200 ms, with the paragraph explaining what raising it does."
         width="560" height="212" loading="lazy" decoding="async">
    <figcaption>200 ms by default, and the app raises it by itself on a poor link.</figcaption>
  </figure>
</div>

How much of what others say is held back before it is played, in milliseconds.
More buffer rides out a patchy signal without gaps; less means you hear people
sooner.

The buffer is elastic: when a backlog builds — leaving a tunnel, say — it plays
the excess off at up to double speed by removing pitch periods, rather than
dropping it or letting everybody fall permanently behind. The app also raises
it by itself when a link starts losing packets, and comes back down to your
setting afterwards.

**Raise it if the playback-gaps counter in Diagnostics keeps climbing.**
Otherwise leave it alone.

### Room tone

<div class="shots">
  <figure>
    <img src="{{ '/assets/img/shots/set-roomtone.webp' | relative_url }}"
         alt="The room tone row, switched on."
         width="560" height="148" loading="lazy" decoding="async">
    <figcaption>Cosmetic, and only on what you hear.</figcaption>
  </figure>
</div>

Adds a short tail under incoming voices, so a talker cut off by voice
activation does not stop mid-breath. Cosmetic, and some people dislike it.
It applies to what you hear, never to what you send.

### Test speakers

<div class="shots">
  <figure>
    <img src="{{ '/assets/img/shots/set-testspk.webp' | relative_url }}"
         alt="The test speakers row with a play button."
         width="560" height="95" loading="lazy" decoding="async">
    <figcaption>A short tone on whichever output is selected.</figcaption>
  </figure>
</div>

Plays a short tone on the selected output.

## Levels

Microphone gain and speaker volume, with the live meter between them.

### Microphone gain

<div class="shots">
  <figure>
    <img src="{{ '/assets/img/shots/set-micgain.webp' | relative_url }}"
         alt="The levels section: the meter reading -84 dB above the microphone gain slider at +0 dB."
         width="560" height="195" loading="lazy" decoding="async">
    <figcaption>The meter above the slider is the thing to watch, not the number.</figcaption>
  </figure>
</div>

Aim for the meter to peak around three quarters while you speak normally. Too
much gain lifts the engine noise along with your voice, and the suppression
then has a harder problem to solve than it needed to.

### Speaker volume

<div class="shots">
  <figure>
    <img src="{{ '/assets/img/shots/set-speakervol.webp' | relative_url }}"
         alt="The speaker volume slider at +0 dB."
         width="560" height="82" loading="lazy" decoding="async">
    <figcaption>Incoming voices only.</figcaption>
  </figure>
</div>

For incoming voices only.

## Noise cancellation

Filters wind, engine and road noise out of your microphone. **Changes take
effect the next time the app starts.**

<div class="shots">
  <figure>
    <img src="{{ '/assets/img/shots/settings-noise-phone.webp' | relative_url }}"
         alt="The noise cancellation section: off, light, standard, helmet and
              automatic, each with a sentence saying what it is for, and helmet
              selected."
         width="560" height="895" loading="lazy" decoding="async">
    <figcaption>Listed weakest to strongest, with Automatic at the end.</figcaption>
  </figure>
</div>

<div class="table-wrap" markdown="1">

| Profile | What it is for |
|---|---|
| Off | No suppression, only a gentle rumble filter. A diagnostic setting rather than a condition to ride in. |
| Light | Quiet indoor use. Keeps the most natural sound. |
| Standard | General purpose, for most environments. |
| Helmet / motorcycle | Steep wind-noise filter, full suppression and an assertive gate. Built for a microphone inside a helmet at speed. |
| Automatic | Listens to the background and picks one of the settings above, changing at most every few seconds. Useful when one ride covers a quiet car park and a motorway. Never chooses Off. |

</div>

Start on **Helmet** if you are riding and **Standard** if you are not, or leave
it on **Automatic** and forget about it.

## Feedback suppression

For when the speaker is heard by the microphone. Echo cancellation removes what
it can predict; these handle what is left, and they work in quite different
ways.

<div class="shots">
  <figure>
    <img src="{{ '/assets/img/shots/settings-feedback-phone.webp' | relative_url }}"
         alt="The feedback suppression section: four options, from no suppression through cutting only when a howl builds to ducking the microphone while others talk."
         width="560" height="911" loading="lazy" decoding="async">
    <figcaption>Four ways of handling what echo cancellation could not predict.</figcaption>
  </figure>
</div>

<div class="table-wrap" markdown="1">

| Setting | Behaviour |
|---|---|
| No feedback suppression | Echo cancellation alone. **Start here**, and change it only if you hear yourself coming back or a howl builds. |
| Cut only when a howl builds | Leaves ordinary conversation completely alone and cuts hard the moment a tone starts climbing. Does nothing about mild bleed. |
| Suppress whatever echo cancellation missed | Continuous, for persistent bleed. |
| Turn the microphone down while others talk | Ducking. Blunt, effective, and costs you the ability to interrupt. |

</div>

## Hiss removal

For the steady hiss a microphone adds under everything.

<div class="shots">
  <figure>
    <img src="{{ '/assets/img/shots/settings-hiss-phone.webp' | relative_url }}"
         alt="The hiss removal section: leave the sound alone, learn the hiss and subtract it, or turn quiet passages down further."
         width="560" height="769" loading="lazy" decoding="async">
    <figcaption>Start at the top. The other two both take something away as well.</figcaption>
  </figure>
</div>

<div class="table-wrap" markdown="1">

| Setting | Behaviour |
|---|---|
| No hiss removal | Leaves the sound alone. **Start here** — both of the others take something away as well. |
| Learn the hiss and subtract it | Tracks the steady noise floor and takes it out spectrally. |
| Turn quiet passages down further | An expander. Cheaper, and it can sound gated on quiet speech. |

</div>

## Microphone mode

How the channel opens. This is the setting with the largest effect on how the
app feels to use.

<div class="shots">
  <figure>
    <img src="{{ '/assets/img/shots/settings-mode-phone.webp' | relative_url }}"
         alt="The microphone mode section: voice activated, push to talk and
              always on, with voice activated selected."
         width="560" height="506" loading="lazy" decoding="async">
    <figcaption>Voice activated is the default.</figcaption>
  </figure>
</div>

<div class="table-wrap" markdown="1">

| Mode | What it does | When |
|---|---|---|
| Voice activated | Transmits automatically when you speak. The default. | Riding. You have no free hand, and the look-ahead means the first consonant survives. |
| Push to talk | Transmits only while the talk button is held. | Loud passenger, music playing, or anywhere a false trigger would be worse than a missed word. |
| Always on | Transmits constantly. | A quiet room. Uses the most data and sends every noise you make. |

</div>

<div class="panel warn">
<p><strong>Voice activation and music do not get along.</strong> Sharp, tonal,
plucked notes open the gate. If you ride with music and use the same headset,
push-to-talk is the reliable answer today.</p>
</div>

## Floating call window

Keeps the call visible over whatever else is on screen, with the controls in
reach and without going back to the app.

- **Android** needs the "display over other apps" permission.
- **iOS** uses Picture in Picture, which allows three buttons: play/pause
  talks, skip back mutes, skip forward hangs up (twice to confirm).

<div class="shots">
  <figure>
    <img src="{{ '/assets/img/shots/settings-floating-phone.webp' | relative_url }}"
         alt="The floating call window section, with the switch that turns it
              on and a note that it needs the display-over-other-apps
              permission."
         width="560" height="480" loading="lazy" decoding="async">
    <figcaption>The switch, and what it asks for.</figcaption>
  </figure>
  <figure>
    <img src="{{ '/assets/img/shots/floating-phone.webp' | relative_url }}"
         alt="The call floating over the home screen: a connected light, the
         microphone meter, whether anyone is speaking, the route and mode,
         and mute and deafen buttons."
         width="560" height="612" loading="lazy" decoding="async">
    <figcaption>Over another app on Android: state, meter, route and mode, and
  the controls you need without going back.</figcaption>
  </figure>
</div>

## Buttons

Bind a handlebar Bluetooth remote, a headset button or a keyboard key to
push-to-talk, mute, deafen or hang up. Press **Learn**, then the button on the
remote. On Android these keep working with the app in the background while
riding.

<div class="shots">
  <figure>
    <img src="{{ '/assets/img/shots/settings-buttons-phone.webp' | relative_url }}"
         alt="The buttons section: nothing bound yet, an action picker set to push to talk (hold), and a Learn button beside it."
         width="560" height="435" loading="lazy" decoding="async">
    <figcaption>Pick the action, press Learn, then the button on the remote.</figcaption>
  </figure>
</div>

<div class="panel warn">
<p><strong>On iOS, a remote reports a media button press but not a hold.</strong>
Push-to-talk-with-hold therefore cannot work from one — use the toggle action
instead. While a media button is bound, the remote controls MumbleWay rather
than your music app.</p>
</div>

## Network

<div class="shots">
  <figure>
    <img src="{{ '/assets/img/shots/settings-network-phone.webp' | relative_url }}"
         alt="The network section: use the system proxy, on, with a direct connection, and an override that is detected automatically."
         width="560" height="489" loading="lazy" decoding="async">
    <figcaption>For downloads only. Voice does not go through it.</figcaption>
  </figure>
</div>

A proxy override for the app's downloads — the public server directory and
profile files. It does not tunnel voice.

## Sync

Optionally copies your server list and settings between your own devices, via
your own iCloud or Android Backup account. Passwords are held separately from
the server list. Nothing passes through any server of ours, because there is
no server of ours.

**Shown only where something can actually carry the data.** On Windows there is
nothing behind it, so the section is absent rather than permanently greyed out.

## Identity

<div class="shots">
  <figure>
    <img src="{{ '/assets/img/shots/settings-identity-phone.webp' | relative_url }}"
         alt="The identity section showing the certificate fingerprint, with a button to copy it."
         width="560" height="413" loading="lazy" decoding="async">
    <figcaption>The fingerprint a server admin needs to register you.</figcaption>
  </figure>
</div>

Your client certificate and its fingerprint. Mumble servers recognise you by
this certificate rather than by a password, so it is worth keeping — it is what
lets a server remember your registration.

## Diagnostics

**Not in Settings.** Reached from the waveform icon in the toolbar. Nothing in
here changes how the app sounds. It exists to answer *why did that happen*, and
it is the first thing to open when somebody says they could not be heard.

### The live analyser

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

### A light per stage

<div class="shots">
  <figure>
    <img src="{{ '/assets/img/shots/diag-lights-phone.webp' | relative_url }}"
         alt="Two rows of coloured dots labelled echo, suppressor, voice
              detected, gate, levelling, hiss, feedback and to the server."
         width="560" height="71" loading="lazy" decoding="async">
    <figcaption>Green passes audio on, amber holds something back, red stops it
    here.</figcaption>
  </figure>
</div>

One dot per stage of the capture chain: **echo**, **suppressor**, **voice
detected**, **gate**, **levelling**, **hiss**, **feedback** and **to the
server**. The colours mean the same thing at every stage — green is working and
passing audio on, amber is working but holding something back, red is stopping
audio here, and grey is switched off and therefore has no opinion.

The last one is the one to read first. **To the server** red means your voice
is not leaving the phone, and the dot to its left usually says which stage
stopped it.

### Counters

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
the incoming audio buffer**.

The last three are the voice gate, in numbers: **microphone level** is what is
arriving now, **noise floor** is what the app has settled on as this
environment's quiet, and **opens at** is the level your voice has to beat to be
sent. On a bike at speed the floor climbs and the threshold climbs with it —
which is what the noise profiles are for.

### The last thirty seconds

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

### The engine log

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

### Record for diagnosis

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

### Listen back

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
