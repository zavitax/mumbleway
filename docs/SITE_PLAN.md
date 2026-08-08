# Remaining work on the site and the app

Updated 2026-08-08, second pass. Everything described as done is live.

## Done since the first handover

- **Bilingual.** All seven pages exist under `/ru/`. Chrome and every label
  inside the diagrams come from `_data/strings.yml`, keyed by language; the
  layout reads `site.data.strings[page.lang]` and a `ref:` in each page's front
  matter links it to its counterpart, so the switch lands on the same page.
- **The language switch** matches the app's button: active language, flag, code,
  8px radius, 10/8 padding, the same two-line tooltip built from the same two
  ARB strings. Right of the menu when wide, left of the hamburger when narrow,
  visible at every width.
- **The Russian was proof-read** after the first draft, which contained a
  non-word (`уцелевает`), a misplaced negation that reversed an instruction,
  and several calques of English idioms. Do not assume the first pass of a
  translation is finished because it is complete.
- **Figures**: the passenger loop's arrowheads no longer end inside the helmets;
  two new ones (a group over eight kilometres, and the tunnel); helmet and
  server cloud are shared includes so they cannot drift.
- **The top bar had not been sticky for months.** `position: sticky` was
  overridden by a later `position: relative`, and `body { overflow-x: hidden }`
  would have defeated it independently. Both fixed.
- **Screenshots are real captures** from the emulator and from Windows against
  a real Mumble server, including the analyser in both detection states.
- **The settings page follows the app's own order** and documents the settings
  it used to omit, notably *Even out speaker loudness*.
- **A recording can be shared or deleted on its own** from the listen sheet.

## Not started

### Screenshots on the other platforms

**Android and Windows are done.** iOS and macOS are not.

The recipe that works on Android:

```bash
export MSYS_NO_PATHCONV=1          # or Git Bash rewrites /sdcard/...
adb shell screencap -p /sdcard/s.png
adb pull -a /sdcard/s.png out.png  # never `exec-out > file`; it corrupts PNGs
```

On Windows, `PrintWindow` with `PW_RENDERFULLCONTENT` (flag 2) — without that
flag a Flutter window comes back as a blank rectangle — after `MoveWindow` to a
fixed size so every shot shares proportions. The helper is in the scratchpad as
`shot.ps1`.

Then PIL to WebP: 560 px wide for phone, 1000 px for desktop, quality 84.

Two things that made the Android pass work and will be needed again:

- **Demo mode** for a fixed clock and a clean status bar, so a capture taken
  today and one taken next month differ only where the app differs:
  `adb shell am broadcast -a com.android.systemui.demo -e command enter`, then
  `clock -e hhmm 1030`, `battery`, `network`, `notifications -e visible false`.
- **Real speech into the microphone.** Acoustic loopback works on the
  *emulator*, which shares the host's audio devices: concatenate clips from
  `C:\ml_data\speech_road` (real helmet audio, not synthetic), lift the level,
  and `PlayLooping()` it on the host while capturing. **It does not work on
  Windows** — the PC has no loopback input, so the analyser sits at its floor
  (-119 dBFS) and only the not-speaking state can be captured there.

**iOS: partly done.** The add-server form, filled in from a `mumble://` link,
is on the server page. Getting further needs one thing this machine does not
have — see below.

The transfer problem is solved: **`\macbookpro\ilya` is reachable from
Windows with cached credentials**, so the Mac writes to `~/shots` and Windows
copies from the share. Never base64 through the SSH tool; it floods the
context twice over.

What works headlessly:

```bash
xcrun simctl openurl booted "mumble://name@host:64738/"   # fills the add form
xcrun simctl io booted screenshot ~/shots/x.png
# Seed the server list without touching the UI. Through `defaults` inside the
# simulator, not by writing the plist: cfprefsd caches it and overwrites a
# direct edit. The JSON must arrive as one quoted string inside a plist array.
xcrun simctl spawn booted defaults write com.mumbleway.mumbleway   flutter.mumbleway.servers '("{\"localId\":\"a\",\"name\":\"s\",...}")'
```

**What blocks the rest: synthetic clicks are refused** (`System Events` error
-25204) because whatever process the SSH session runs under has no
Accessibility permission. Keystrokes are allowed; clicks are not. So anything
behind a tap — connecting, the settings screen, the diagnostics panel — cannot
be reached from here. Granting Accessibility to the SSH session's parent in
System Settings → Privacy & Security would unblock all of it.

Two other facts worth keeping:

- **The simulator shows "Not responding"** against a server the Mac can reach
  over TCP, so the UDP ping is being dropped somewhere in between. It is a
  property of that network, not a fault — but it means the iOS home screen
  cannot be screenshotted honestly here, and that shot was deliberately left
  out rather than published showing a server that does not answer.
- **macOS does not build on this machine.** `Runner` has entitlements that
  require signing with a development certificate, and there is none installed.
  CI signs with its own secrets. Not worked around.

## Things that will bite

- **Stale cache.** This cost time twice in one session: the served CSS had the
  new rules and the page was still painting the old ones. Hard-reload with
  cache ignored before believing anything looks wrong.
- **kramdown does not parse markdown inside a block element.** Every `<div>`
  wrapping a table needs `markdown="1"`.
- **`baseurl` is not inferred** for a project site.
- **The menu breakpoint is measured, not a media query.** Adding a nav item or
  a language needs no CSS change. It now reads the bar's real padding and gaps
  rather than a constant, which was wrong twice — the padding is a `clamp()`,
  and the language switch added a third item and so a second gap.
- **Never edit source through a PowerShell pipeline**, and run
  `python tool/check_encoding.py` after touching anything with Cyrillic. Note
  also that the *terminal* mangles Cyrillic on this machine: to read Russian
  out of a file, write it to a file and open it, do not print it.
- **The back button exits the app** from the emulator's home screen, and
  `am start` will not bring it back afterwards — use
  `monkey -p com.mumbleway.mumbleway -c android.intent.category.LAUNCHER 1`.

## Outside the site

- **Apple is healthy again.** Publish 68's Apple half was uploaded by hand, and
  publish 69 then completed all four jobs on its own — Windows MSIX, Mac App
  Store, Google Play and TestFlight. The HTTP 500s from `list-apps` were their
  outage and it is over. Nothing in this repository was changed to fix it.

- **The playback panel shipped in publish 69, and one thing in it did not.**
  Per-recording delete and share went out. The fix that lets playback work at
  all when nothing else holds the audio devices open (`be61042`) landed
  *after* that run was triggered, so build 69 has a listen button that does
  nothing unless a call or a recording is already running. Publish again.
- **`docs/MUSIC_GATE.md`** still waits on a road recording with music.
- **A one-off native abort** was seen once at startup
  (`nativeSetAndroidContext` → `SIGABRT` during `configureFlutterEngine`) after
  the app was backed out of and immediately restarted. It did **not** reproduce
  on a clean `force-stop` + start, and no panic message reached logcat. Recorded
  because it is the kind of thing that is dismissed twice and then reported by a
  user, not because there is anything to fix from this alone.

## Queued, with everything needed to start

Written 2026-08-09 at the end of a session that ran out of room. Each of these
was asked for and none was started badly.

### The music recording arrived — analyse it

`C:\ml_dataides60809-0142-000.{s16,csv,wav}`. **This is the recording
`MUSIC_GATE.md` has been waiting for**, and the fault reproduces plainly:

    138.8 s of music and no speech at all
    transmitting 36.1% of blocks, in 17 runs
    two runs of 14.8 s and 13.0 s, then a dozen of 0.5-3 s

The reporter's most useful observation is that **one passage fooled the gate
once and not the second time**. That rules out a pure per-block property — the
same audio decided differently — and points at state: the noise floor tracker,
the AGC, or the hold envelope. Compare `floor_db` and `snr_db` at the two
instances before proposing anything.

**Read the CSV skipping `#` lines.** The first line is a comment and a plain
`DictReader` takes it as the header, which silently reports 0% transmitting on
a file that is 36% transmitting.

### Telegram bot: take a caption with the file

Asked for so a recording can be processed the moment it arrives. Today the
mode is inferred, and a whole ride is filed without anyone saying what it is.
Telegram puts the caption in `message.caption`, beside `document` —
`tools/vad/telegram_intake.py` already parses the update, so it is a field to
read and store beside the ride, not a new mechanism.

### Screenshots still wanted

- **Noise cancellation, feedback suppression, hiss removal and microphone
  mode**, cropped to their sections. Each is taller than the frame it was
  captured in, so this needs the stitched screen: `scratchpad/stitch.py`
  assembles the sweep into one 1080x7473 image by matching row-ink profiles,
  and prints every accent-blue heading position. Crop heading-to-heading from
  that, then snap with `crop.py`.
- **Sync.** Not on Android at all — the section only exists where a cloud can
  carry the data, so it has to come from the iOS simulator.
- **Diagnostics, one per subsection**, including the analyser live with speech
  detected *and* transmitting. The acoustic-loopback recipe is above.
- **"Your own server" shots are broken** and want re-making on the simulator.

`idb` is installed and connected, so all of these are now reachable — see
CLAUDE.md for how to drive it and CLAUDE.local.md for where it lives.
