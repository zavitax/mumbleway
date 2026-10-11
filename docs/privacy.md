---
layout: default
ref: privacy
title: Privacy policy
description: What MumbleWay does with your voice, and what it does not.
---

**MumbleWay** — voice for bikers.
Developer: Ilya Melamed. Last updated: 18 September 2026.

## The short version

MumbleWay has **no servers of its own**. It collects no analytics, shows no
advertising, has no account to create, and sends nothing to the developer.

It is a client for [Mumble](https://www.mumble.info/) servers. Your voice goes
to the server *you* chose, encrypted, and nowhere else. Whoever runs that server
decides what happens to it there — which is why the app never picks one for you.

Everything below is detail on that.

## Your voice

When you talk, the app captures audio from your microphone, processes it on your
device to remove wind and engine noise, and sends it to the Mumble server you
are connected to. Voice is encrypted in transit with AES-128, and the control
connection to the server uses TLS.

**The developer never receives your audio and has no way to.** There is no
intermediate service. The connection is between your device and the server you
entered, in the same way a web browser connects to a site you typed in.

What the operator of that server can see is a matter between you and them.
A Mumble server can be configured to record; MumbleWay cannot tell you whether a
given one does. If that matters to you, ask whoever runs it.

Audio is not stored on your device during normal use. It is held in memory only
for the fraction of a second it takes to process and send.

## What is kept on your device

- **Your server list** — names, addresses, ports and usernames you entered.
- **Server passwords**, if you saved any. These are kept in the platform's own
  protected storage (Keychain on Apple devices, the Android Keystore), separate
  from the rest of the list.
- **A client certificate**, generated on your device the first time the app
  runs. Mumble servers use it to recognise you as the same person returning. It
  never leaves the device except as part of connecting to a server.
- **Server certificate fingerprints**, so the app can warn you if a server's
  identity changes.
- **Your settings** — audio devices, noise profile, language, and so on.

Deleting the app removes all of it.

## Optional cloud sync

If you turn on server-list sync in Settings, your list is stored **in your own
cloud account**, not in ours:

- **iPhone, iPad, Mac** — Apple iCloud, under your Apple Account.
- **Android** — Google's Android Backup Service, under your Google Account.
- **Windows** — no sync is offered. Export to a file if you want a copy.

The developer has no access to any of it. Apple's and Google's own privacy
policies govern what they do with data in your account. Turning the setting off
stops further syncing.

## Network connections the app makes

| To | When | What it sees |
|---|---|---|
| The Mumble server you chose | While connected | Your voice, your username, your IP address, whether your microphone is muted, and the app's name and version, your operating system and its processor type |
| A proxy, if you set one for servers | While connected to a server that goes through it | Your IP address, which server you are reaching and when, and how much data passes. **Not** what is said: that stays inside the encrypted connection to the server |
| `publist.mumble.info` | Only when you open the public server directory | Your IP address, as any website would |
| `zavitax.github.io` | Only when an invitation link is *followed* in a browser | Your IP address. **Not** the server, channel or password in the invitation |
| Your local network | Only if you connect to a server on it | — |

The public directory is run by the Mumble project, not by MumbleWay. If you
never open it, the app never contacts it.

**A proxy is never chosen for you.** The setting starts at Direct and stays
there until you change it, even on a machine where the system already has a
proxy configured for browsing. Until you turn on sending voice through it as
well, voice does not pass through a proxy at all — it goes straight to the
server, and the proxy carries only the connection used to set the call up and
keep it running.

### Other MumbleWay users on the same server

When you connect, MumbleWay tells the other MumbleWay users on that server that
you are using it too, and which version. Their app marks you in the list of
people in the channel, and yours marks them.

That is all it says: the app's name, its version, and which MumbleWay features
it understands. Nothing about your device, your location or anything you have
entered. Only people on the same server at the same time receive it, the
server passes it on like everything else, and on a server older than Mumble
1.4 nobody receives it at all.

People using other Mumble apps are told nothing — their app ignores it.

**Muting.** When you mute your microphone, the server is told, so everyone on it
can see that you are muted rather than just quiet — people using other Mumble
apps included. And another MumbleWay user can ask your app to mute your
microphone, for when it is sending noise, or to unmute it, for when you muted
yourself and forgot. Either way you hear a sound and see who did it, and your
app acts on at most one such request every 30 seconds. Being unmuted by others
can be turned off in Settings, with "Let MumbleWay users unmute me". Being
muted cannot, and your own mute button always turns the microphone back on.

**Silencing somebody for yourself.** Turning another rider down in the list of
people in the channel affects only your own device — the server is not involved,
and nobody else's app changes. If that rider uses MumbleWay, their app is told,
so that they can see they are not coming through to you rather than talk to
somebody who stopped listening; they are told again when you let them back in.
Nobody else is told, and a rider using another Mumble app is not told at all.

### Invitation links

An invitation you share is an ordinary `https://zavitax.github.io/mumbleway/join/`
link, because a `mumble://` link is not something most messaging apps will let
anyone tap. **The invitation itself is in the part of the link after the `#`,
which browsers never send to a server.** So the server address, the channel and
any password stay on the device even when the link is opened over the network,
and nothing about who invited whom appears in a web server's logs.

**On Android** the page is usually not fetched at all: MumbleWay registers
those links with the system, so tapping one opens the app directly. It loads
only when it cannot — the app is not installed, or the system has not verified
the link.

On iPhone, iPad, Mac and Windows the page is always fetched, because MumbleWay
does not register the address on those platforms. Tapping the button on the
page is what opens the app.

The site is hosted by GitHub Pages, which sees the IP address of anyone
fetching a page, as any website does. MumbleWay adds no analytics, no cookies
and no trackers to it.

A proxy shared on its own, or shared along with a server, travels in the same
part of the link — including its login, if you choose to include one — so it does
not reach a web server's logs either.

Android also fetches a small file from that domain when the app is installed or
updated, to confirm the app is allowed to open its own links. It says nothing
about you, and happens whether or not you ever use an invitation.

No other network connections are made.

## Permissions, and why

- **Microphone** — to send your voice. This is what the app is for.
- **Camera** — *only* when you tap the button to scan a server invitation QR
  code. The image is decoded on the device, never stored and never transmitted.
- **Local network** (iOS) — so you can reach a Mumble server running at home or
  at a club, which is where many of them are.
- **Bluetooth** — to use your intercom headset's microphone and speakers.
- **Display over other apps** (Android, optional) — for the floating call
  window over a navigation app.
- **Notifications / foreground service** (Android) — to keep audio running while
  the phone is in your pocket. Android requires an app to show a notification
  while it does this, which is the notification you see.

## Diagnostic recording

The diagnostics panel has a **Record for diagnosis** switch. It is **off unless
you turn it on**, and it stays on until you turn it off.

While it is on, the app writes what your microphone hears to your device's
storage, along with what the noise suppression decided about each moment of it.
It exists because faults like "it cut me off mid-sentence" cannot be diagnosed
from a description.

It also writes, for the same length of time, **how the phone itself was
moving** — the accelerometer, the gravity direction and the gyroscope. That is
what makes it possible to tell a deliberate tap on the phone from a pothole, and
it is recorded whether or not you use the tap gesture, because measuring how
often a detector fires by mistake needs rides in which nobody tapped at all.
It is movement of the device, not location: there is no GPS here, and this app
has never asked for the permission.

- **The app uploads nothing, ever.** Recordings leave only if you choose to
  share them, using your device's normal share sheet, to a destination you
  pick.
- **But your own device may sync the folder they are in.** The app writes them
  to the ordinary documents folder — on Windows, `Documents\mumbleway-recordings`
  — and if you have OneDrive set to back up your Documents folder, Windows will
  upload them to *your* OneDrive account like anything else you put there. That
  is your device's arrangement with your own cloud account and nothing to do
  with this app, but "on your device" is not the whole truth on a PC set up that
  way, so it is worth saying plainly. On Android, iOS and macOS the files live
  inside the app's own storage, which no desktop folder-sync setting covers.
- You can delete them from the same panel, or from your device's file manager.
- If you send a recording to the developer, you are sending a recording of your
  own microphone. Please listen to it first.

## Crash reports

If the app crashes, the technical details are written to your device and shown
to you so you can copy them. **They are not sent anywhere.** If you choose to
paste one into a bug report, that is your decision and your copy.

## Children

MumbleWay is not directed at children and collects no information from anyone,
including children.

## Your rights

The developer holds no personal data about you, so there is nothing to request,
correct or delete from us. Data on your device is under your control: delete the
app, or use the in-app controls described above. Data in your iCloud or Google
account is under the terms of those accounts.

## Changes

If this policy changes, the updated version will be published here and the date
at the top will change. The revision history of this document is public in the
[project repository](https://github.com/zavitax/mumbleway/commits/main/docs/privacy.md).

## Contact

Questions about privacy, or about anything in this document, can be raised at
<https://github.com/zavitax/mumbleway/issues>.
