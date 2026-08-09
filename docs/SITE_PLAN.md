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

## Shipped 2026-08-09

All live, in publishes 71 to 74 (every one green on all four stores).

- **The app links to this site.** The wordmark opens it, a *Website* item is in
  the overflow menu, and the settings bar has a help button landing on
  `/settings.html`. The language follows the app's own locale rather than the
  device's, because the site is bilingual by whole copies of each page and
  getting it wrong is a working link to documentation the reader cannot read.
- **Server refusals reach the user.** A refused mute used to arrive as a chat
  line from "server", where it read as somebody talking and scrolled away. It
  now lands in a snackbar over whatever screen is on top.
- **`Auto` may only choose a lighter profile after 15 s of quiet.** Going
  heavier is unchanged. It removes an inversion that was measured: quieter music
  was landing in lighter profiles and leaking *more*.
- **The playback panel colours what was transmitted green**, zooms to 64x by
  pinch or by ctrl/cmd-wheel, keeps the playhead on screen, and shows the
  playhead time to the millisecond between the buttons.
- **Six settings screenshots recropped** so each holds exactly its own option,
  and *Choosing the devices* now comes from Windows, where there is actually a
  choice to show.

## Queued, with everything needed to start

Written 2026-08-09 at the end of a session that ran out of room. Each of these
was asked for and none was started badly.

### The music recording - **analysed 2026-08-09**

`C:/ml_data/rides/20260809-0142-000.{s16,csv,raw,wav}` (forward slashes on
purpose; the previous version of this line was written through something that
ate the backslashes and left a carriage return in the middle of the path).

The findings are in `docs/MUSIC_GATE.md`. The short version: the leak is an
**Auto-profile convergence transient**, not a per-block misclassification; the
reporter's "fooled it once and not the second time" is confirmed on 87% of
transmitted blocks; and two of the four candidate features are disproved.

Three traps, each of which cost time here:

- **Read the CSV skipping `#` lines.** The first line is a comment and a plain
  `DictReader` takes it as the header - it silently reports 0% transmitting on
  a file that is 36% transmitting.
- **`level_db` in the log is measured *after* suppression, and the `.s16` beside
  it is the *raw* capture** (`engine.rs:2235`). Read the two as the same signal
  and 64 s of loud music looks like silence.
- **`core/tests/road.rs` already does most of this.** It takes f32 mono 48 kHz
  from `MUMBLEWAY_ROAD_AUDIO`, and the `.raw` is written beside the `.s16`.
  Reach for it before writing another analysis script.

Still wanted: **speech over the same music, at the same level, in the same
helmet.** This clip has none, so it bounds false positives and says nothing at
all about recall - which is the assertion `MUSIC_GATE.md` says will fail first.

### Telegram bot: take a caption — **done 2026-08-09**

Shipped. Anything in the caption beyond the mode is kept verbatim in a `.note`
beside the audio and echoed back; an archive's note goes against every ride in
it; and the inbox path reads a `NAME.txt` beside `NAME.zip`, because the long
rides that can only arrive that way are otherwise guaranteed to arrive with no
explanation. The mode now comes from the first word or a `#noise` / `#speech`
tag, so a sentence mentioning noise in passing is not mistaken for one.

### How a section screenshot has to be cropped

Two standing rules, both asked for after a batch came back wrong. They apply to
every section shot, not only the ones outstanding below.

- **Crop to the documented section and nothing else.** No part of the section
  above or below, however tidy it looks. A reader following a heading on the
  page expects the picture under it to show that heading's settings and only
  those; anything else is a second subject in the frame.
- **The whole section, including its heading, with a margin.** An option cut off
  the bottom is invisible to a reader — nothing on the page says it is missing —
  which is why four crops that had lost their last option were reverted rather
  than shipped.

Those two pull against each other, and the way to satisfy both is not to eyeball
it:

1. Scroll so the section's **heading sits near the top** of the screen, which is
   what gives the section a whole screen to fit in. Cropping out of whatever
   frame happened to contain it is what lost the options the first time.
2. Require the **next section's heading to be in the same frame**. That is the
   only evidence nothing ran off the bottom — the crop then ends in the gap
   before it, so the neighbouring section is used as a boundary and never
   appears in the picture.
3. Snap both edges to rows with no ink, as `crop.py` does, so an edge never
   falls through a line of text.

`settings-noise-phone.webp` was made this way and is the reference: heading,
intro, all five options with Automatic complete, and nothing from Levels or
Feedback suppression.

**Stitching the sweep into one tall screen does not work** and the note that
said to do it was wrong. `scratchpad/stitch.py` assembles the frames by matching
row-ink profiles, and the result has smeared bands over several headings —
crops taken from it are plausible and wrong, which is worse than the fault being
fixed.

### Driving the Android emulator for these

Recorded because three separate approaches failed, each differently, and the
failures are silent:

- **Never swipe down the middle of the settings list.** It drags whichever
  slider is under the finger rather than scrolling, and it moved the incoming
  audio buffer to 280 ms in the middle of a sweep. See CLAUDE.md.
- **x = 1060 does not scroll either** — it is inside Android's edge-gesture
  zone, so the system takes the swipe and the list never moves. The gap between
  the toggles and that zone is too narrow to aim at.
- **Key events do nothing.** `PAGE_DOWN` and `DPAD_DOWN` leave a Flutter list
  where it is.
- Only the **touch-down point** decides which widget claims a gesture, so a safe
  scroll is a question of where the swipe starts. A heading's description
  paragraph is text and is never a control.
- **A burst of input wedges the app** into an ANR dialog, after which every
  scroll silently does nothing and the screenshots look merely unchanged. Pace
  the gestures, and check the app still responds before believing a frame.
- **Read settings back from preferences, not from a screenshot**, when checking
  whether the UI was disturbed:

  ```bash
  adb shell run-as com.mumbleway.mumbleway \
    cat /data/data/com.mumbleway.mumbleway/shared_prefs/FlutterSharedPreferences.xml
  ```

  A capture of a control at the wrong value is indistinguishable from one at the
  right value, which is the same reason the app records its own capture input.

### Screenshots still wanted

Three, and each is blocked on something specific rather than on effort.

**Playback panel, showing the green transmitted blocks.** The APK with the new
panel is built and installed on the emulator, mic and overlay permissions are
granted, and `scratchpad/speech_loop.wav` is 66.7 s of real helmet speech with
0.35 s gaps in it — the gaps matter, because they are what makes the green
alternate with grey instead of filling the whole waveform.

*The blocker:* **recording writes nothing without an audio hold.** The capture
worker does not run until the engine has opened the devices, so with no server
connected and no microphone test running there is nothing to record — the toggle
goes on and off and no file appears. Open the devices first, either by turning
on *Test microphone (hear yourself)* in Settings, which holds audio for as long
as that screen is up, or by connecting to a server. Then play the loop on the
host, record for ~25 s, and open the listen sheet.

`transmitting` does *not* depend on being connected — it is decided by the
transmit mode and the analysis (`engine.rs:2303`) — so green appears on a
voice-activated recording with no server, provided the devices are open.

**Sync.** Not on Android at all; the section only exists where a cloud can carry
the data, so it has to come from the iOS simulator.

**Diagnostics, one per subsection**, including the analyser live with speech
detected *and* transmitting. Note this is more than cropping: `settings.md`
documents Diagnostics as a bulleted list with **no screenshots at all**, so the
markup to hold six or seven figures has to be written as well as the captures
taken.

### A close button on the playback panel

Asked for 2026-08-09. The listen sheet closes by swiping it down or by the back
gesture, and neither is discoverable — the diagnostics panel beside it has an
explicit `✕` in its header and the playback panel should match it.

Where it goes: `_PreviewSheet` in `app/lib/widgets/recording_preview.dart`, in
the header row beside the title, using the same icon and placement as the
diagnostics panel's own close so the two read as the same control. It needs a
tooltip and a label from `_data`/the ARB files in both languages — `close`
already exists as a string on the site side, so check whether the app has one
before adding another.

Worth doing at the same time as the playback screenshots below, since both want
the panel open and the shot should show the button that will be there.

### The music gate: a decision, not a measurement

`docs/MUSIC_GATE.md` is now measured end to end and the input side is closed —
both clips, hand labels, and every candidate scored. What is left is a choice:

- **Accept the trade Helmet already makes** (80.0% of speech kept, a quarter of
  what it sends still music), or
- **Adopt a neural VAD**, which is the only thing measured here that beats it:
  TEN VAD matches Helmet's recall at 80.2% with 89.5% precision and a third of
  the music leak. The ordered plan is in that file.

Six hand-built features have now failed. The properties that separate music from
speech in a quiet room are properties a motorcycle also has, and every feature
the chain currently computes is measured downstream of the one RNNoise decision
it would have to disagree with.

### TFLite in the app — designed, not built

Agreed and written up in `MUSIC_GATE.md`: YAMNet as a *supporting* vote for
Helmet, never a veto and never near the transmit gate, because being wrong about
a profile costs some naturalness where being wrong about transmitting cuts a
rider off. Model in Dart (`tflite_flutter` is mature where the Rust bindings are
not), inference every few seconds rather than per block, and paired with the
15 s calm ratchet that already shipped so it can push towards Helmet promptly
and only release on real quiet. A dot in the diagnostics array so the decision
is visible rather than inferred.

**What it needs first, and does not have:** nothing hands Dart a *waveform*.
`audio_spectrum()` returns 24 bands and YAMNet wants 15 600 raw samples at
16 kHz, so this needs a new self-expiring `#[frb(sync)]` tap built like the
spectrum one — self-expiring because a model polled forever in a rider's pocket
is the failure `DiagnosticsPanel` already guards against.

**Then measure**, because neither number is known: per-inference CPU on a real
phone, and the battery cost of the tap.

`idb` is installed and connected, so the iOS work is reachable — see CLAUDE.md
for how to drive it and CLAUDE.local.md for where it lives. There is also a CUDA
GPU on this machine, unused and not needed: the installed torch is a `+cpu`
build, which is why `cuda.is_available()` is False.
