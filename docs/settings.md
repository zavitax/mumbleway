---
layout: default
title: Settings
description: What each setting does, and which ones are worth changing on a motorcycle.
---

Most of this can be left alone. Three settings matter on a bike — **microphone
mode**, **noise cancellation** and **microphone gain** — and the rest exist for
when something specific is wrong.

## Microphone

### Microphone mode

How the channel opens. This is the setting with the largest effect on how the
app feels to use.

<div class="table-wrap" markdown="1">

| Mode | What it does | When |
|---|---|---|
| Voice activated | Transmits automatically when you speak. The default. | Riding. You have no free hand, and the look-ahead means the first consonant survives. |
| Push to talk | Transmits only while the talk button is held. | Loud pillion, music playing, or anywhere a false trigger would be worse than a missed word. |
| Always on | Transmits constantly. | A quiet room. Uses the most data and sends every noise you make. |

</div>

<div class="panel warn">
<p><strong>Voice activation and music do not get along.</strong> Sharp, tonal,
plucked notes open the gate. If you ride with music and use the same headset,
push-to-talk is the reliable answer today.</p>
</div>

### Microphone gain

Aim for the meter to peak around three quarters while you speak normally. Too
much gain lifts the engine noise along with your voice, and the suppression
then has a harder problem to solve than it needed to.

### Test microphone

Plays your processed voice back exactly as the far end hears it — after
suppression, gate and levelling. The fastest way to judge a profile.

**Use headphones.** Through speakers it is a feedback loop.

## Noise cancellation

Filters wind, engine and road noise out of your microphone. **Changes take
effect the next time the app starts.**

<div class="table-wrap" markdown="1">

| Profile | What it is for |
|---|---|
| Automatic | Chooses between the profiles below from the measured noise floor and spectral tilt, with hysteresis so it does not flap at a junction. Never chooses Off. |
| Helmet / motorcycle | Steep wind-noise filter, full suppression, assertive gate. Built for a microphone inside a helmet at speed. |
| Standard | General purpose, for most environments. |
| Light | Quiet indoor use. Keeps the most natural sound. |
| Off | No suppression, only a gentle rumble filter. A diagnostic setting rather than a condition to ride in. |

</div>

Start on **Helmet** if you are riding and **Standard** if you are not, or leave
it on **Automatic** and forget about it.

## Echo cancellation

On by default and worth leaving on. A helmet speaker sits a few centimetres
from the microphone, so without cancellation everybody hears themselves back
with a delay, which is more distracting than almost any other audio fault.

## Feedback suppression

Separate from echo cancellation, and for what cancellation could not model.

<div class="table-wrap" markdown="1">

| Setting | Behaviour |
|---|---|
| No feedback suppression | Echo cancellation alone. **Start here**, and change it only if you hear yourself coming back or a howl builds. |
| Cut only when a howl builds | Leaves ordinary conversation completely alone and cuts hard the moment a tone starts climbing. Does nothing about mild bleed. |
| Suppress whatever echo cancellation missed | Continuous, for persistent bleed. |
| Turn the microphone down while others talk | Ducking. Blunt, effective, and costs you the ability to interrupt. |

</div>

## Hiss removal

<div class="table-wrap" markdown="1">

| Setting | Behaviour |
|---|---|
| No hiss removal | Leave the residual alone. |
| Learn the hiss and subtract it | Tracks the steady noise floor and takes it out spectrally. |
| Turn quiet passages down further | An expander. Cheaper, and it can sound gated on quiet speech. |

</div>

## Room tone

Adds a short tail under incoming voices, so a talker cut off by voice
activation does not stop mid-breath. Cosmetic, and some people dislike it.
It applies to what you hear, never to what you send.

## Levels and devices

- **Speaker volume** — for incoming voices only.
- **Audio devices** — on desktop, choose input and output explicitly. On phones
  the platform routes audio automatically and connecting a headset switches to
  it, so there is nothing to choose.
- **Re-check devices** — after plugging in or pairing a headset.
- **Test speakers** — plays a short tone on the selected output.

## Incoming audio buffer

How much incoming audio to hold before playing it, in milliseconds. Larger
absorbs more network jitter at the cost of delay.

The buffer is elastic: when a backlog builds — leaving a tunnel, say — it plays
the excess off at up to double speed by removing pitch periods rather than
dropping it or letting everybody fall permanently behind. You should rarely
need to touch this.

## Buttons

Bind a Bluetooth remote or media button to push-to-talk, mute, deafen or hang
up. Press **Learn**, then the button on the remote.

<div class="panel warn">
<p><strong>On iOS, a remote reports a media button press but not a hold.</strong>
Push-to-talk-with-hold therefore cannot work from one — use the toggle action
instead. While a media button is bound, the remote controls MumbleWay rather
than your music app.</p>
</div>

## Floating call window

Keeps the call visible over whatever else is on screen with the controls in
reach.

- **Android** needs the "display over other apps" permission.
- **iOS** uses Picture in Picture, which allows three buttons: play/pause
  talks, skip back mutes, skip forward hangs up (twice to confirm).

## Identity

Your client certificate and its fingerprint. Mumble servers recognise you by
this certificate rather than by a password, so it is worth keeping — it is what
lets a server remember your registration.

## Sync

Optionally copies your server list and settings between your own devices, via
your own iCloud or Android Backup account. Passwords are held separately from
the server list. Nothing passes through any server of ours, because there is
no server of ours.

## Network

A proxy override for the app's downloads — the public server directory and
profile files. It does not tunnel voice.

## Diagnostics

Reached from the waveform icon in the toolbar rather than from settings.

- **Live spectrum analyser** with three traces: microphone, after suppression,
  and what is being transmitted.
- **A light per stage** — echo, suppressor, voice detected, gate, levelling,
  hiss, feedback, to the server.
- **Record for diagnosis**, which saves the microphone to the device along with
  what the chain decided about every 10 ms of it. Off unless you turn it on.
  Sharing produces a `.zip` per 18 MB so it can be sent anywhere.
