# MumbleWay — tagline and description

Paste-ready copy. Field-by-field variants, character limits and the reasoning
behind what is *not* claimed are in [STORE_LISTING.md](STORE_LISTING.md); this
file is the two pieces of writing themselves, kept clean so they can be copied
without editing.

---

## Tagline

> **Talk through the wind.**

Four words, and they name the problem rather than the product category. Every
rider who has tried to speak into a phone at 100 km/h knows immediately what it
is about; nobody else needs to.

Alternates, if a store wants a different length or the primary is taken:

| | Tagline | Use |
|---|---|---|
| Short | **Talk through the wind.** | primary |
| Descriptive | **Group voice, built for a helmet at speed.** | where the category is not obvious |
| Plainest | **Mumble voice for motorcycles.** | app subtitle field, 28 characters |

---

## Description

```
MumbleWay is a voice client for Mumble servers, built for talking from inside a motorcycle helmet at speed.

Wind is the hard part. At speed it is loud, broadband and relentless, and it defeats the noise suppression in ordinary voice apps because that was tuned for offices and cafes. MumbleWay's microphone chain is built for a different problem: a steep high-pass that strips wind and engine rumble before anything else sees it, a neural denoiser for the broadband roar, and a transmit decision that measures your voice against a noise floor which climbs with road speed. A steady engine drone raises that floor with it and never clears the margin, however loud it gets. Speech rises above it.

YOU NEED A MUMBLE SERVER
MumbleWay connects to any of them — a friend's box, one at the clubhouse, a public server from the built-in directory, or one you run yourself. It is a client, not a service: no account to create, no subscription, and nobody's servers in the middle.

BUILT FOR A BIKE
• Noise profiles from Light to Helmet, or Auto, which picks from what it hears and shows you where it landed.
• Voice activation opens 240 ms ahead of its own decision, so a word keeps its first consonant. The delay is then paid back to 60 ms rather than carried all call.
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
• A jitter buffer that plays a backlog off at up to double speed after a tunnel, rather than leaving everybody a second behind.
• Join by QR code or a mumble:// link.

PRIVATE BY DEFAULT
Voice is encrypted with AES-128 and the control channel runs over TLS. Server certificates are pinned the first time you connect and a changed certificate is refused until you say otherwise. MumbleWay has no servers of its own, collects no analytics and shows no advertising.

ON AN OLDER PHONE
A block of audio arrives every 10 milliseconds and the whole chain has to finish before the next one. If your phone cannot manage that, MumbleWay gives stages up one at a time, cheapest first, in an order that was measured rather than guessed — and says which ones, rather than quietly sounding worse.

DIAGNOSTICS
An optional panel shows the microphone chain working — the spectrum before and after suppression, and which stage stopped a sound reaching the far end. There is a diagnostic recorder in there too, off unless you switch it on, for capturing what your headset actually hears on a ride. You can play a recording back hearing only what would have been transmitted, or through the processing chain, so you can judge for yourself how you sound at the far end without a second phone.

Available in English and Russian.

MumbleWay is free and open source. It is an independent client and is not affiliated with the Mumble project.
```

3191 characters, inside the 4000 that App Store, Google Play and Microsoft Store
each allow. Run `python tool/check_listing.py` after any edit.

### A note on the bullet character

`•` rather than `-`, because Google Play and the Microsoft Store render the
description as near-plain text and a hyphen at the start of a line reads as a
dash mid-sentence once the text reflows. Apple wraps it the same way. None of
the three supports Markdown here, so the formatting has to survive being
treated as prose.
