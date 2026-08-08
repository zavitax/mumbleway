---
layout: default
ref: scenarios
title: On the road
description: Setups that work, what to expect from each, and where it stops working.
---

## Before the first ride

Do this at home, not in a car park with gloves on.

1. **Pair the headset** and check the phone is routing to it — play something
   and hear it in the helmet.
2. **Add the server** by hand, by scanning a QR code from whoever runs it, or
   by following a `mumble://` link.
3. **Set the profile** to Helmet and the mode to Voice activated.
4. **Set the gain** with *Test microphone*, wearing headphones, with the helmet
   on. Peak around three quarters when you speak normally.
5. **Connect, and leave it connected** while you get ready. The app takes the
   audio session only while a call is up, so nothing is recorded and your
   headset keeps its sound quality for other apps until you connect.

## Two riders

The simplest case, and the one an intercom already handles well. Use this
instead when you keep losing each other — a city, heavy traffic, or a ride
where one of you stops for fuel and the other does not.

<div class="panel">
<p><strong>Expect:</strong> a couple of hundred milliseconds of delay. You will
talk over each other for the first few minutes and then stop doing it. Neither
of you needs to be within any particular distance of the other.</p>
</div>

## A group

Where it earns its place. Everyone joins one channel, and the group can be
strung out over several kilometres without anyone dropping out of the
conversation.

{% include fig-group.svg lang=page.lang %}

- **Voice activation** for everyone, or the person at the back spends the ride
  holding a button.
- **Agree a channel per group** if two groups share a server, rather than
  everyone talking in one.
- **A rider with a loud bike** may need Helmet even when everyone else is on
  Standard. It is a per-rider setting for a reason.

## Rider and passenger

Both wear headsets and both join the channel. It works, with one caveat: two
microphones inside two helmets a foot apart, both on the same channel, is the
easiest way to build a feedback loop.

{% include fig-passenger.svg lang=page.lang %}

If you hear a howl:

1. Turn one of the two speaker volumes down.
2. Set **Feedback suppression** to *Cut only when a howl builds* on both.
3. Failing that, put the passenger on push-to-talk.

## Riding with music

Music and voice activation are an unhappy pair today: **sharp, tonal, plucked
notes open the gate**, and the far end hears your music instead of you.

Until that is fixed, either:

- Use **push to talk** while music is playing, or
- Play music from a device that is not routed through the same headset.

Music from your phone ducks under a call rather than stopping — that part works
— but ducked music is still music, and the detector still hears it.

## A rally, or riders who do not know each other

- **Share the server with a QR code** from the app rather than dictating an
  address through a helmet.
- **Register on the server** so people appear under names rather than as
  "user1". Registration is by client certificate, so keep your identity.
- **Set a sensible channel structure** on the server — one per group, one for
  everyone — rather than fifty people in Root.

## Where it stops working

<div class="panel warn">
<p><strong>No signal, no conversation.</strong> A tunnel, a valley, a mountain
pass, a border where your data plan quietly stops. The app reconnects on its
own when the link comes back, and the elastic buffer catches you up rather than
leaving everybody a second behind — but while there is no data there is no
voice, and an intercom would still have been working.</p>
</div>

{% include fig-tunnel.svg lang=page.lang %}

Two practical consequences:

- **Do not use it as the only way to say something urgent.** Agree hand signals
  for the things that matter, exactly as you would without it.
- **Take an intercom too, if your riding is remote.** These are complementary
  tools. This one has unlimited range where there is coverage and none where
  there is not.

## Data and battery

Roughly **3–6 MB per hour of actual talking**, more when the link is poor and
error correction rises. Silence costs almost nothing — voice activation means
nothing is sent between sentences.

Battery is the more noticeable cost. A phone holding a Bluetooth call and a
mobile data connection for several hours will want charging. A hardwired mount
solves it and is worth having anyway.
