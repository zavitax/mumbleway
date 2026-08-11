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
read beside the screen rather than searched. Diagnostics has a
[page of its own]({{ '/diagnostics.html' | relative_url }}): it is not in
Settings at all, and it answers a different question — not *what should this be
set to* but *why did that happen*.

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

### Light noise model

<div class="shots">
  <figure>
    <img src="{{ '/assets/img/shots/set-simplemodel-windows.webp' | relative_url }}"
         alt="The light noise model row, switched off, explaining that it runs a
              smaller speech cleaner costing a third as much, keeps the rest of
              the noise chain working on a slow phone, is harsher on quiet
              speech and adds 20 ms of delay."
         width="1000" height="83" loading="lazy" decoding="async">
    <figcaption>Off by default. A trade to make on purpose, not an
    improvement.</figcaption>
  </figure>
</div>

Swaps the speech cleaner at the head of the chain for a smaller one that costs
**about a third as much to run**. It is off by default, and it is a trade
rather than an upgrade.

Turn it on if your phone is
[giving stages up]({{ '/index.html#keeping-up' | relative_url }}):
paying for the cleaner model up front can keep the rest of the noise chain
running instead of it being switched off piece by piece, which is the worse
outcome of the two.

What it costs is real. The smaller model is **more aggressive, not worse** — it
takes 4 to 6 dB more out of the speech along with the noise, which wins where
the background is loud enough to be worth removing and loses where it is not.
So it is harsher on a quiet voice in a quiet room, and it adds **20 ms of
delay**, because it looks two frames ahead where the standard model looks none.

**Some devices take it whether or not you choose it.** A single-core phone gets
it from the start, and any device that runs out of everything else to give up
ends on it before losing the cleaner altogether.

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

**Raise it if the playback-gaps counter in
[Diagnostics]({{ '/diagnostics.html' | relative_url }}) keeps climbing.**
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
              automatic, each with a sentence saying what it is for, with
              automatic selected and its paragraph describing the sound
              classifier and the two cooldowns."
         width="560" height="933" loading="lazy" decoding="async">
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
| Automatic | Listens to the background and picks one of the settings above. Useful when one ride covers a quiet car park and a motorway. Never chooses Off. |

</div>

**Automatic is the default**, and for most people it is the answer: it reaches
the helmet setting within a second of hearing an engine and takes its time
coming back down. Choose **Helmet** by hand if you would rather it never
changed, or **Standard** if you are not riding.

### How Automatic decides

Two things, and they pull in the same direction.

**A sound classifier, on phones.** Automatic runs a small neural model on
what the microphone hears — about one look per two seconds, on the phone's
accelerator where there is one. When it hears **engine, wind or music** it takes
the helmet setting *immediately*, without the few seconds the level-based part
waits, and holds it for **fifteen seconds** after they stop.

It is a vote for the helmet setting and nothing else. It can never choose a
lighter one, it never touches the decision about whether to transmit, and it
does nothing at all unless Automatic is chosen — a profile you picked by hand is
an instruction. It runs on Android, iOS and macOS; on Windows there is no
classifier yet and Automatic uses levels alone.

**Dialling down is slow, and slower the further down it goes.** Leaving the
helmet setting for Standard needs **fifteen seconds** of the background actually
asking for it; going on from Standard to Light needs **a minute more**. It walks
down one step at a time rather than jumping, because Light barely suppresses
anything and arriving there wrongly is the expensive mistake.

Going *up* has no such wait. Being under-suppressed at speed loses you; being
over-suppressed at a coffee stop sounds slightly processed.

**Diagnostics shows where it landed** — the profile in force, and a
**Background** light for what the classifier is saying. See
[Diagnostics]({{ '/diagnostics.html' | relative_url }}).

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

**The window has a close button, and it means "not right now".** It is not the
same as turning the feature off, which is this setting: closing it lasts until
you go back into the app, and the next time you leave the app it returns. It is
deliberately not remembered between rides, so tapping it once cannot leave you
hunting for the talk button a week later. Picture in Picture on iOS behaves the
same way, so the gesture has one answer on both phones.

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
         alt="The call floating over the home screen: a close cross in the top
         left corner, a connected light, the microphone meter, whether anyone
         is speaking, the route and mode, and mute and deafen buttons."
         width="560" height="506" loading="lazy" decoding="async">
    <figcaption>Over another app on Android: state, meter, route and mode, and
  the controls you need without going back. The cross that puts it away is in
  the corner, where Picture in Picture puts it on an iPhone.</figcaption>
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

<div class="shots">
  <figure>
    <img src="{{ '/assets/img/shots/settings-sync-ios.webp' | relative_url }}"
         alt="The sync section: one switch, sync servers and settings across
              devices, with a line underneath saying to sign in to iCloud on
              this device to use it."
         width="560" height="253" loading="lazy" decoding="async">
    <figcaption>On iPhone, where iCloud can carry it. On Windows the section is
    not there at all.</figcaption>
  </figure>
</div>

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

**Not a setting, and no longer on this page.** It is the waveform icon in the
toolbar, and it has grown enough to need its own: the analyser, a light per
stage of the capture chain, the counters, thirty seconds of history, the engine
log, and recording a ride to listen back to.

See [Diagnostics]({{ '/diagnostics.html' | relative_url }}).
