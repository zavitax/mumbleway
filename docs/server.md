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
         width="560" height="1217" loading="lazy" decoding="async">
    <figcaption>Type it once, or scan the QR code the app makes.</figcaption>
  </figure>
  <figure>
    <img src="{{ '/assets/img/shots/addserver-ios.webp' | relative_url }}"
         alt="The same form on iPhone, already filled in from a mumble:// link:
              display name, address, port and username, with the add button
              below."
         width="560" height="1214" loading="lazy" decoding="async">
    <figcaption>Or follow a <code>mumble://</code> link, which fills the form
    in for you.</figcaption>
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
