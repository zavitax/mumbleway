---
layout: default
ref: index
title: MumbleWay
description: Voice for bikers. A Mumble client built for wind and engine noise inside a helmet.
---

## Intercoms stop at the end of the group

A Bluetooth intercom pairs riders together and holds them within a few hundred
metres of each other. Lose the group at a junction and you lose the
conversation. MumbleWay puts the conversation on the internet instead: everyone
joins the same [Mumble]({{ site.mumble }}) channel over mobile data, and it
makes no difference whether the next rider is a hundred metres ahead or in
another country.

What it is not is a general-purpose voice app with a motorcycle icon. Every
decision in the capture chain assumes a microphone a centimetre from your
mouth, inside a helmet, at speed, with wind and an engine underneath.

{% include fig-range.svg lang=page.lang %}

<div class="grid">
  <div class="panel spec">
    <span class="k">Range</span>
    <span class="v">Anywhere</span>
    <p class="muted">Mobile data, not a radio link between helmets.</p>
  </div>
  <div class="panel spec">
    <span class="k">Codec</span>
    <span class="v">Opus</span>
    <p class="muted">48 kHz, with forward error correction that rises as the link degrades.</p>
  </div>
  <div class="panel spec">
    <span class="k">Servers</span>
    <span class="v">Yours</span>
    <p class="muted">Any Mumble server. No account, no directory, no company in the middle.</p>
  </div>
</div>

## What it does about noise

Speech from inside a helmet at speed is a hard signal. The chain is built for
it, and every stage is visible while it runs.

- **A neural speech enhancer at the head of the chain.** DeepFilterNet 3, on
  the phone, in the 10 ms a block gets. It is the single largest lever here —
  on the road it separates speech from the gaps between it by around 16 dB
  where the rest of the chain manages 1.5 — which is why it is the first thing
  softened and the last thing given up when a device runs short.
- **Wind and engine suppression** tuned for a helmet, with lighter profiles for
  standing still and for indoors, and an automatic setting that picks between
  them from what it hears.
- **Echo cancellation**, so a helmet speaker a few centimetres from the
  microphone does not send everybody back to themselves.
- **Voice activation that is not causal.** A threshold decides mid-syllable, so
  by the time the gate opens the sound that opened it is already gone. The audio
  is delayed and the decision is not: the channel opens on the sound that *led
  into* it, and a word starting on "s", "f" or "sh" keeps its first consonant
  instead of arriving as "ixty". Measured on three real rides, 160 ms of
  look-ahead covered 94% of openings against 90% at 80 ms.
- **The delay is then paid back.** Carrying a fixed look-ahead for ever is
  latency on every transmission to protect the first tenth of a second of one.
  So a phrase opens **240 ms** ahead and the debt is repaid at 1.1× — pitch
  periods removed whole, so duration changes and pitch does not — until the
  delay settles at **60 ms**. Not zero: a little slack absorbs a late block
  instead of turning it into a dropout.
- **The channel then holds for a full second** after you stop, fading over the
  last 30 ms so a trailing "t" or "s" survives and the cut is not a click. A
  second bridges the gaps *inside* a sentence, which is what stops a phrase
  arriving as four fragments.
- **Feedback suppression**, a de-hisser, automatic levelling and a limiter.
- **An elastic jitter buffer** that plays a backlog off at up to double speed by
  removing pitch periods, rather than letting a tunnel put everybody a second
  behind.

{% include fig-timeline.svg lang=page.lang %}

## When a phone cannot keep up
{: #keeping-up}

A block of audio arrives every 10 milliseconds, and the whole chain has to be
finished before the next one turns up. On a current phone that is comfortable.
On an outdated or entry-level one it is not, and the useful thing to do about
that is not to pretend otherwise.

So the device is **measured against the deadline when the app starts, before
your first call**, and watched while you talk. If it does not fit, stages are
given up one at a time — in an order that was measured rather than guessed, and
never quietly.

**The cheapest quality goes first.** The enhancer softens by two steps before
anything else is touched: they are the two largest savings on the chain and
cost almost nothing, and on voice over music the first of them measured
*better* than the full setting. Then the look-ahead stops being paid down,
which changes no sample a listener hears — only the delay goes back up to what
it was before that existed. Then three detectors whose work the level meter
largely duplicates. Then the diagnostics panel's own drawing, which is free to
give up because nobody's voice passes through it. Then the background
classifier, so *Automatic* stops noticing that the road has become a car park.
Then a cheaper, more aggressive noise model. **Switching the enhancer off is
the fourteenth and last thing tried**, because it is the one that turns 16 dB
of separation between speech and gaps back into 1.5.

**It does not climb back during a session.** A device that was late once will
be late again, and a chain that switched stages on and off as the load moved
would sound worse than either state. Restarting the app tries the whole chain
again.

**And it tells you.** Stages that have been given up are struck through in the
diagnostics panel, the toolbar icon becomes an amber warning so you find out
without going looking, and the panel says in plain words what was dropped and
what it costs.

<div class="shots">
  <figure>
    <img src="{{ '/assets/img/shots/degraded-windows.webp' | relative_url }}"
         alt="The diagnostics panel after the chain has been cut back: the
              analyser replaced by a note saying it was switched off, Suppressor
              struck through in the stage list, the enhancer reading Off, and a
              warning explaining that parts of the noise chain were switched off
              and that a more powerful device would run the whole chain."
         width="1000" height="453" loading="lazy" decoding="async">
    <figcaption>The bottom of the ladder, and the panel saying so.</figcaption>
  </figure>
</div>

One thing worth knowing: this measures the device **as it finds it**, not as it
could be. A computer that is busy with something else when the app opens can
start a step lower than the same computer idle.

## Watch how it works

Most voice apps tell you nothing. When somebody says "it cut me off", there is
no way to find out which stage cut them.

MumbleWay has a diagnostics panel with a live spectrum analyser showing three
traces at once — microphone, after suppression, and what is actually being
sent — with a light per stage of the chain. The sent trace going flat while the
other two do not is the most useful thing it shows.

It can also **record what the microphone heard along with what the chain decided
about it**, block by block, so a recording that cuts out can be examined rather
than guessed at. That exists because a whole round of measurements was once
invalidated by discovering the recordings behind them had come from the phone's
own microphone rather than the headset's. Audio carries no record of what
captured it; recording from inside the app makes it the chain's own input by
construction.

<div class="shots">
  <figure>
    <img src="{{ '/assets/img/shots/analyser-speaking-phone.webp' | relative_url }}"
         alt="The diagnostics panel while speech is being detected: three traces,
              the transmitted one filled in, and the voice, gate and to-the-server
              lights all green."
         width="560" height="1244" loading="lazy" decoding="async">
    <figcaption>Speech detected, and going out.</figcaption>
  </figure>
  <figure>
    <img src="{{ '/assets/img/shots/analyser-silent-phone.webp' | relative_url }}"
         alt="The same panel with no speech: the transmitted trace is flat, the
              legend reads Not sending, and the voice, gate and to-the-server
              lights are red."
         width="560" height="1244" loading="lazy" decoding="async">
    <figcaption>No speech: the sent trace goes flat while the others do not.</figcaption>
  </figure>
  <figure>
    <img src="{{ '/assets/img/shots/home-phone.webp' | relative_url }}"
         alt="The main screen connected to a server, showing latency, the UDP
              round trip, the channel and the microphone meter."
         width="560" height="1244" loading="lazy" decoding="async">
    <figcaption>Connected: latency, channel, and the meter.</figcaption>
  </figure>
</div>

On a desktop the same thing is a two-pane window: your servers down one side,
the channel and who is in it down the other.

<div class="shots">
  <figure>
    <img src="{{ '/assets/img/shots/home-desktop.webp' | relative_url }}"
         alt="MumbleWay on Windows: saved servers on the left with latency and
         the UDP round trip, the channel and its members on the right, and
         the microphone meter along the bottom."
         width="1000" height="691" loading="lazy" decoding="async">
    <figcaption>Windows, connected. The same app, given room.</figcaption>
  </figure>
  <figure>
    <img src="{{ '/assets/img/shots/diagnostics-desktop.webp' | relative_url }}"
         alt="The diagnostics panel on Windows: the analyser and stage lights
         above three columns of counters for incoming audio, this device and
         the server."
         width="1000" height="691" loading="lazy" decoding="async">
    <figcaption>The diagnostics panel, where a wide window shows every counter
  at once instead of stacking them.</figcaption>
  </figure>
  <figure>
    <img src="{{ '/assets/img/shots/home-macos.webp' | relative_url }}"
         alt="MumbleWay on macOS, connected to a public server: the server card
              down the left showing its voice path as TCP tunnelled in amber,
              two people in the channel and the channel tree on the right, and a
              live microphone meter along the bottom."
         width="1000" height="720" loading="lazy" decoding="async">
    <figcaption>macOS. This one says <strong>TCP</strong> rather than UDP — the
    network was not passing the voice port, and the app says so instead of
    quietly sounding worse.</figcaption>
  </figure>
</div>

## Advantages

<div class="grid">
  <div class="panel good">
    <h3>Range is not a constraint</h3>
    <p>Riders separated by traffic, a junction or a border stay in the same
    conversation. An intercom cannot do this at any price.</p>
  </div>
  <div class="panel good">
    <h3>Your server, your rules</h3>
    <p>Any Mumble server will do, including one on a Raspberry Pi at home.
    There is no account to make and no directory your channel appears in.</p>
  </div>
  <div class="panel good">
    <h3>More than two riders</h3>
    <p>A Mumble channel holds as many people as the server allows, without the
    chain-of-hops fragility that mesh intercoms have.</p>
  </div>
  <div class="panel good">
    <h3>It shows its working</h3>
    <p>A live analyser, per-stage status, and recordings that carry the
    decisions alongside the audio.</p>
  </div>
  <div class="panel good">
    <h3>Nothing is collected</h3>
    <p>No account, no analytics, no advertising, no telemetry. Audio is never
    stored unless you turn recording on yourself. See the
    <a href="{{ '/privacy.html' | relative_url }}">privacy policy</a>.</p>
  </div>
  <div class="panel good">
    <h3>Free software</h3>
    <p>GPL v3, source in the open, and it speaks a documented protocol with
    other clients rather than one vendor's.</p>
  </div>
</div>

## Disadvantages

Worth reading before you rely on it. These are real and none of them is going
to be fixed by a setting.

<div class="grid">
  <div class="panel warn">
    <h3>No signal, no conversation</h3>
    <p>This is its defining weakness. An intercom keeps working in a valley,
    a tunnel or a dead spot; this does not. If your riding is mostly remote,
    an intercom is the better tool and this is a supplement to it.</p>
  </div>
  <div class="panel warn">
    <h3>It costs mobile data and battery</h3>
    <p>Roughly 3–6 MB per hour of talking on the wire, more with error
    correction on a poor link. A phone doing continuous voice over mobile data
    with the screen off still gets noticeably warmer than one that is not.</p>
  </div>
  <div class="panel warn">
    <h3>Latency is a network's, not a radio's</h3>
    <p>The capture chain contributes little — the look-ahead pays itself down
    to 60 ms — but the network does not, so expect a couple of hundred
    milliseconds on a good mobile link and more on a bad one, against near-zero
    for an intercom between two adjacent helmets. Conversation works;
    interrupting somebody mid-sentence does not land the way it does face to
    face.</p>
  </div>
  <div class="panel warn">
    <h3>Bluetooth costs you audio quality</h3>
    <p>A headset microphone is only reachable over the hands-free profile,
    which is mono and narrowband. While a call is up, music through the same
    headset drops to telephone bandwidth. That is a property of Bluetooth, not
    of this app, and every voice app on your phone has it.</p>
  </div>
  <div class="panel warn">
    <h3>You need a server</h3>
    <p>There is no MumbleWay service to sign up to. Someone in the group has to
    run a Mumble server or rent one — a deliberate choice, and still a step
    that an intercom does not ask of anybody.
    <a href="{{ '/server.html' | relative_url }}">It takes about ten minutes.</a></p>
  </div>
  <div class="panel warn">
    <h3>Music rides out with your voice</h3>
    <p><strong>Being heard over your own music is solved</strong> — anchoring
    the gate to the tracked background took speech through from 63% to 98% on a
    voice-over-music ride. What is not solved is the other direction: on that
    same ride, 44% precision means a good deal of what goes out during a phrase
    is music rather than speech. The detector was trained to tell speech from
    noise, and music is neither. Push-to-talk avoids it entirely, and the work
    is <a href="{{ site.repo }}/blob/main/docs/MUSIC_GATE.md">documented in the
    open</a>, including every attempt that has already failed.</p>
  </div>
</div>

## Where to go next

<div class="grid">
  <div class="panel">
    <h3><a href="{{ '/settings.html' | relative_url }}">Settings</a></h3>
    <p>What every setting does, and which ones actually matter on a bike.</p>
  </div>
  <div class="panel">
    <h3><a href="{{ '/scenarios.html' | relative_url }}">On the road</a></h3>
    <p>Setups that work: a pair, a group, a passenger, a rally.</p>
  </div>
  <div class="panel">
    <h3><a href="{{ '/server.html' | relative_url }}">Your own server</a></h3>
    <p>Mumble server on Windows, macOS or Linux, in about ten minutes.</p>
  </div>
</div>
