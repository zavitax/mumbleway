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

**Lives in [STORE_DESCRIPTION.md](STORE_DESCRIPTION.md)**, with the tagline, and
is measured from there by `tool/check_listing.py`.

Deliberately not repeated here. Two copies of three thousand characters drift,
and the copy that ends up pasted into a store is never reliably the one that was
edited.

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
