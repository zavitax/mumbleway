# Store listing copy

Every store wants the same thing in a different shape and a different length.
This holds one set of words, cut to each field, so the app does not describe
itself differently depending on where somebody found it.

**Nothing here claims anything the app does not do.** That is not only honesty —
a listing that oversells noise cancellation earns one-star reviews from riders
who tried it at 120 km/h, and those are the reviews that stay at the top.

Character limits are the stores' own, and each field below is inside its limit.
Re-check with `tool/check_listing.py` after editing.

---

## Name

| Store | Limit | Value |
|---|---|---|
| All | 30 | `MumbleWay` |

Keep it the bundle name. `MumbleWay: Rider Intercom` also fits if the plain name
turns out to be too quiet in search, but the app is called MumbleWay everywhere
else and a store name that disagrees with the icon label is its own small
confusion.

## Subtitle — App Store, Mac App Store (30)

```
Mumble voice for motorcycles
```

## Short description — Google Play (80)

```
Talk to your group over Mumble. Built for wind and engine noise inside a helmet.
```

## Short description — Microsoft Store (500)

```
A Mumble voice client built for talking from inside a motorcycle helmet at speed. Wind and engine noise defeat the suppression in ordinary voice apps, which was tuned for offices; MumbleWay's is tuned for a bike. Connects to any Mumble server, works over Bluetooth intercom headsets, stays reachable over your navigation app, and reconnects itself when signal comes back. You need a Mumble server to talk to — it is a client, not a service, so there is no account and no subscription.
```

## Promotional text — App Store (170)

Changeable without a review, so use it for what is new.

```
Diagnostic recording, so a rider can capture what their headset actually hears and send it in. Russian now follows your phone's language. Bluetooth audio fixes on iOS.
```

## Keywords — App Store (100, comma-separated, no spaces)

```
motorcycle,helmet,intercom,mumble,voip,rider,ptt,walkie,talkie,group,bike,comms,murmur,headset
```

## Full description (4000 — fits App Store, Play and Microsoft Store)

```
MumbleWay is a voice client for Mumble servers, built for talking from inside a motorcycle helmet at speed.

Wind is the hard part. At speed it is loud, broadband and relentless, and it defeats the noise suppression in ordinary voice apps because that was tuned for offices and cafes. MumbleWay's microphone chain is built for a different problem: a steep high-pass that strips wind and engine rumble before anything else sees it, a neural denoiser for the broadband roar, and a transmit decision that measures your voice against a noise floor which climbs with road speed. A steady engine drone raises that floor with it and never clears the margin, however loud it gets. Speech rises above it.

YOU NEED A MUMBLE SERVER
MumbleWay connects to any of them — a friend's box, one at the clubhouse, a public server from the built-in directory, or one you run yourself. It is a client, not a service: no account to create, no subscription, and nobody's servers in the middle.

BUILT FOR A BIKE
• Noise profiles from Light to Helmet, or Auto, which picks from what it hears and shows you where it landed.
• Push to talk, voice activation, or open microphone.
• Works over Bluetooth intercom headsets on the hands-free profile, which is where the boom microphone lives.
• Pair a handlebar Bluetooth remote and it learns whatever your remote actually sends, rather than offering a list of keys that may not match it. Hold to talk, or tap to toggle for remotes that never send a release.
• Walkie-talkie cues on key and unkey, so you know you are transmitting without looking down.

FOR WHEN YOU ARE NOT LOOKING AT THE SCREEN
• A floating window over your navigation app on Android, Picture in Picture on iPhone and iPad, a floating panel on Mac.
• A falling two-tone when the connection drops and a rising one when it returns, so you learn about it from the headset rather than from silence.
• Automatic reconnection on everything except a disconnect you asked for, at a steady ten seconds with the countdown on screen. When your phone reports signal is back it retries at once instead of waiting.
• Audio keeps running with the screen locked and the phone in a pocket.

TALKING TO PEOPLE
• Channel tree and roster, with per-person mute and volume.
• Two servers connected at once.
• Live ping per server, and the app shows whether voice is going direct over UDP or tunnelled through TCP when a carrier will not pass it.
• Join by QR code or a mumble:// link.

PRIVATE BY DEFAULT
Voice is encrypted with AES-128 and the control channel runs over TLS. Server certificates are pinned the first time you connect and a changed certificate is refused until you say otherwise. MumbleWay has no servers of its own, collects no analytics and shows no advertising.

DIAGNOSTICS
An optional panel shows the microphone chain working — the spectrum before and after suppression, and which stage stopped a sound reaching the far end. There is a diagnostic recorder in there too, off unless you switch it on, for capturing what your headset actually hears on a ride.

Available in English and Russian.

MumbleWay is free and open source. It is an independent client and is not affiliated with the Mumble project.
```

---

## What is deliberately not claimed

- **No brand names of headsets.** It has been ridden with one model. "Bluetooth
  intercom headsets" is what can be supported in general; a named brand reads as
  a compatibility promise, and it would be tested by the first person who owns a
  different one.
- **No numbers about the suppression.** No decibels, no percentages. Every
  measurement this project has of its own noise chain came from synthetic audio
  or from recordings later found to be off the wrong microphone. Nothing here is
  ready to be printed on a shelf.
- **No "crystal clear" or "studio quality".** At 120 km/h it will not be, and
  the rider who tries it will say so in public.
- **The server requirement is stated early and twice.** It is the single
  commonest reason a listing like this earns a one-star review: somebody
  installs a voice app, finds nobody to talk to, and concludes it is broken.

## Still needed before any of this can be submitted

- **A privacy policy URL.** Mandatory on every store here for an app that
  records a microphone. Microsoft Store Policy 10.5.1 and Apple's App Privacy
  both require it, and submission is blocked without it. The text above says the
  app collects nothing, so the policy has to say the same or the listing is
  contradicted by its own fine print.
- **Screenshots**, per store and per device size.
- **An age rating** questionnaire per store.
- **Apple App Privacy answers**: the microphone is used for the app's core
  function and audio is not collected — but that questionnaire is answered in
  App Store Connect, not here.
