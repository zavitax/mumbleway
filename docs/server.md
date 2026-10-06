---
layout: default
ref: server
title: Your own server
description: Running a Mumble server on Windows, macOS or Linux, in about ten minutes.
---

MumbleWay talks to any [Mumble]({{ site.mumble }}) server. The server software
is called **Mumble Server** (historically *Murmur*, and the binary is still
often `mumble-server` or `murmurd`).

You need one of these:

- **A machine at home** — a spare PC, a NAS or a Raspberry Pi is plenty. A
  Mumble server for a riding group uses almost no CPU and a few MB of RAM.
- **A cheap VPS** — the smallest tier anywhere is more than enough, and it
  saves you opening a port at home.
- **A hosted Mumble server** — several companies rent them by the month.

<div class="panel">
<p><strong>Port 64738, TCP <em>and</em> UDP.</strong> Mumble uses TCP for
control and UDP for voice. If UDP is blocked it falls back to sending voice
over TCP, which works and adds latency. Forward both.</p>
</div>

## Linux

The usual home for a Mumble server, and the least trouble.

### Debian, Ubuntu, Raspberry Pi OS

```bash
sudo apt update
sudo apt install mumble-server

# Sets the SuperUser password and enables the service at boot.
sudo dpkg-reconfigure mumble-server
```

Configuration lives in `/etc/mumble-server.ini` (older packages:
`/etc/murmur.ini`). After editing:

```bash
sudo systemctl restart mumble-server
sudo systemctl status mumble-server
```

### Fedora, RHEL

```bash
sudo dnf install mumble-server
sudo systemctl enable --now mumble-server
sudo mumble-server -supw YOUR_SUPERUSER_PASSWORD
```

### Docker, anywhere

```bash
docker run -d --name mumble \
  -p 64738:64738 -p 64738:64738/udp \
  -v mumble-data:/data \
  --restart unless-stopped \
  mumblevoip/mumble-server:latest
```

Set the SuperUser password on first run:

```bash
docker exec -it mumble mumble-server -supw YOUR_SUPERUSER_PASSWORD
```

## Windows

1. Download the **server** package from
   [mumble.info/downloads]({{ site.mumble }}downloads/) — it is a separate
   download from the client.
2. Install it. The installer offers to run the server as a Windows service;
   accept if you want it up after a reboot.
3. Configure `murmur.ini` (or `mumble-server.ini`) beside the executable, or in
   `%ProgramFiles%\Mumble\`.
4. Set the SuperUser password from an Administrator prompt:

```powershell
cd "C:\Program Files\Mumble"
.\mumble-server.exe -supw YOUR_SUPERUSER_PASSWORD
```

5. Allow it through the firewall — both protocols:

```powershell
New-NetFirewallRule -DisplayName "Mumble TCP" -Direction Inbound `
  -Protocol TCP -LocalPort 64738 -Action Allow
New-NetFirewallRule -DisplayName "Mumble UDP" -Direction Inbound `
  -Protocol UDP -LocalPort 64738 -Action Allow
```

<div class="panel warn">
<p>The executable has been named <code>murmur.exe</code> in older releases and
<code>mumble-server.exe</code> in newer ones. Use whichever is in the folder.</p>
</div>

## macOS

Homebrew is the least painful route:

```bash
brew install mumble-server
brew services start mumble-server
```

Set the SuperUser password:

```bash
mumble-server -supw YOUR_SUPERUSER_PASSWORD
```

The configuration file is under Homebrew's prefix — `/opt/homebrew/etc/` on
Apple Silicon, `/usr/local/etc/` on Intel. `brew info mumble-server` prints the
exact paths for your install.

A Mac at home makes a fine server for a group, but it has to stay awake:
System Settings → Energy, and disable sleep.

## Settings worth changing

In `mumble-server.ini` / `murmur.ini`:

<div class="table-wrap" markdown="1">

| Setting | Suggested | Why |
|---|---|---|
| `welcometext` | Your group's name | Shown on connect. |
| `serverpassword` | Something, if the server is public-facing | The simplest access control there is. |
| `port` | `64738` | The registered default. Change it only if you must. |
| `users` | `20` | Cap it. There is no reason to leave it open-ended. |
| `bandwidth` | `72000` | Bits per second per user, generous for Opus. Lower it if your uplink is thin. |
| `registerName` | Your group's name | The name of the root channel. |
| `registerUrl`, `registerHostname` | *leave empty* | **Setting these lists your server in the public directory.** Leave them blank to stay unlisted. |
| `allowping` | `false` | Stops strangers probing it for user counts. |
| `sslCert`, `sslKey` | Paths to a real certificate | Optional. Without it, clients see a self-signed certificate and pin it on first connect. |

</div>

## Connect from MumbleWay

1. **Add another server** in the app.
2. **Address** — your public IP, your dynamic-DNS name, or the VPS hostname.
3. **Port** — 64738 unless you changed it.
4. **Username** — anything; it is how you appear in the channel.
5. **Password** — the `serverpassword` if you set one.

Then share it with the group by opening the server's **QR code** in the app and
letting them scan it, which beats reading an IP address through a helmet.

<div class="panel good">
<p><strong>Registering users.</strong> Connect once with the Mumble desktop
client as <code>SuperUser</code> using the password you set, and register the
riders. Mumble identifies people by their client certificate rather than a
password, so a registered rider is recognised automatically from then on —
which is why the app's <em>Identity</em> setting is worth keeping.</p>
</div>

<div class="shots">
  <figure>
    <img src="{{ '/assets/img/shots/addserver-phone.webp' | relative_url }}"
         alt="The add-server form: display name, address, port, username and an
              optional password, with shortcuts to browse public servers, import
              a file or scan a QR code."
         width="560" height="883" loading="lazy" decoding="async">
    <figcaption>Type it once, or scan the QR code the app makes.</figcaption>
  </figure>
  <figure>
    <img src="{{ '/assets/img/shots/addserver-ios.webp' | relative_url }}"
         alt="The same form on iPhone, already filled in from a mumble:// link:
              display name, address, port and username, with the add button
              below."
         width="560" height="1218" loading="lazy" decoding="async">
    <figcaption>Or follow a <code>mumble://</code> link, which fills the form
    in for you.</figcaption>
  </figure>
</div>

## Administering it from the app

**You do not need a desktop Mumble client to run the server day to day.**
Everything below is in MumbleWay, on the phone, with gloves on at a petrol
station if it comes to that.

Each action is a request the server may refuse, so the app greys out what you
may not do and reports a refusal in the server's own words rather than pretending
it worked.

### A rider

The **⋯** beside somebody in the channel:

| Entry | What it does |
|---|---|
| **Mute on server (for everyone)**, **Deafen on server** | Silences them for the whole channel, not just for you. Needs Mute/Deafen on that channel. |
| **Move to channel** | Puts them somewhere else — the usual cure for somebody sitting in a channel that will not carry their voice. |
| **Make priority speaker** | Quietens everybody else while they talk. For a ride leader. |
| **Register on this server** | Gives them an account, so the server knows them by certificate next time and an access list can name them. |
| **Kick from server…** | Disconnects them, with an optional reason they are shown. They can come straight back. |
| **Ban from server** | Removes them and bars the address and the certificate, so a new connection does not get round it. |
| **Information** | What the server will say about them: client, system, address, certificate — and only to somebody allowed to ask. |

### The server

The **⋯** on the server card:

- **Registered users** — everybody with an account, and **Remove** to take one
  away.
- **Banned users** — the ban list, with **Lift** on each. A ban is either
  **Until lifted** or counts down.
- **Access tokens** — the passwords that open token-gated channels. Held per
  server and sent when you connect.

### A channel

The **⋯** on a channel row — and it is now drawn per channel, so a channel you
may write to offers it and one you may not does not, wherever you happen to be
standing:

- **New channel here**, **Rename**, **Remove channel**.
- **Permissions** — the access list, which is the part worth explaining.

### Access lists

**Permissions in …** shows the channel's rules in the order the server applies
them, each one saying plainly that it *grants* or *denies* something, to a group
or to one user, **Here** or **And below**.

Rules inherited from the parent channel are shown greyed and marked *from the
parent channel*: they belong to the channel that defines them and are edited
there. Turn off **Also use the parent channel's rules** and they stop applying —
and stop being shown, since a list of rules that no longer do anything is a list
that misleads.

**Groups** are names a rule can grant rights to. Only riders with an account on
this server can be in one, which is why **Register on this server** comes first
in most of these jobs. A group can **Take members from the channel above** and
can be left usable by the channels under it — **Channels below may use this group**.

<div class="panel warn">
<p><strong>Murmur rate-limits writes to an access list.</strong> Two in quick
succession and the second is refused. The app re-reads on its own health tick
rather than immediately after writing, so give it a moment before expecting the
list to show what you just saved.</p>
</div>

## What riders see of your server's text

Two of the settings above are read by every rider who connects, and MumbleWay
draws both of them.

**`welcometext` may contain HTML, and its links work.** Write it with an
anchor and a rider can tap it:

```html
welcometext=Set push-to-talk — <a href="https://example.test/ptt">here is how, with pictures</a>
```

Only `http`, `https` and `mailto` become links. Anything else — `javascript:`,
`file:`, even `mumble:` — stays ordinary text, because a welcome message is
yours to write and a tap that added a server or a proxy from a sentence is not
something a rider agreed to.

**A channel's description is shown too**, under the channel a rider is standing
in and under each channel they are listening to. Not under every row: a
paragraph per channel would bury the list. Its links work the same way. This is
the place to put what a channel is *for* — Mumble's own client shows it when a
channel is selected, so the text serves both.

<div class="panel">
<p><strong>Qt's editor writes a <code>&lt;style&gt;</code> block into every
description it saves.</strong> MumbleWay drops it rather than drawing it, so a
description written in the official client reads as prose here and not as a
stylesheet.</p>
</div>

**A channel nobody may speak in is marked.** Where your access list denies
Speak, the row carries a struck-through microphone, so a rider sees it before
moving there rather than after. Standing in one, they also get a notice on the
server card saying the channel will not carry their voice and that moving is
the cure — and their microphone button turns amber and stops being a switch,
because it is not their silence to lift.

<div class="shots">
  <figure>
    <img src="{{ '/assets/img/shots/server-text-phone.webp' | relative_url }}"
         alt="A server card on the phone: an amber notice saying this channel will not carry your voice and to move or ask whoever runs the server; below it the Root row with a struck-through microphone beside the user count, and under that the channel's description with one word rendered as a tappable link."
         width="560" height="446" loading="lazy" decoding="async">
    <figcaption>All three at once: the notice for a rider standing there, the
    mark on the row, and the description with its link.</figcaption>
  </figure>
</div>

## Going further

This page covers only enough to get a group talking. Mumble has considerably
more — ACLs and groups, channel permissions, Ice/gRPC administration, bots,
positional audio, LDAP authentication:

<div class="panel">
<p><a href="{{ site.mumble_docs }}"><strong>Mumble documentation →</strong></a><br>
<span class="muted">Server configuration, administration and the protocol
itself, from the Mumble project.</span></p>
<p><a href="{{ site.mumble }}"><strong>mumble.info →</strong></a><br>
<span class="muted">Downloads, community and news.</span></p>
</div>
