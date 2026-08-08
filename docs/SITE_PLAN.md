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

**iOS is buildable and runnable** — `flutter build ios --simulator --debug`
takes about a minute on the Mac, `xcrun simctl install/launch` works, and a
server can be added without touching the UI:

```bash
xcrun simctl openurl booted "mumble://name@host:64738/"
xcrun simctl io booted screenshot /tmp/shots/x.png
```

What stopped it was **getting the file back**. There is no scp from this
machine to the Mac (`Permission denied (publickey)`), and base64 through the
SSH tool floods the context. Solve the transfer first — an HTTP one-liner from
the Mac, a shared folder, or an SSH key — and the captures themselves are ten
minutes.

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
