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
- **Screenshots are real captures** from the emulator against a real Mumble
  server, including the analyser in both detection states.

## Not started

### Screenshots on the other platforms

Android is done. **iOS, macOS and Windows are not.** What exists is a working
recipe rather than a plan:

```bash
export MSYS_NO_PATHCONV=1          # or Git Bash rewrites /sdcard/...
adb shell screencap -p /sdcard/s.png
adb pull -a /sdcard/s.png out.png  # never `exec-out > file`; it corrupts PNGs
```

Then PIL to WebP at 560 px wide, quality 84 — 12–62 kB each.

Two things that made the Android pass work and will be needed again:

- **Demo mode** for a fixed clock and a clean status bar, so a capture taken
  today and one taken next month differ only where the app differs:
  `adb shell am broadcast -a com.android.systemui.demo -e command enter`, then
  `clock -e hhmm 1030`, `battery`, `network`, `notifications -e visible false`.
- **Real speech into the microphone.** Acoustic loopback works: concatenate
  clips from `C:\ml_data\speech_road` (real helmet audio, not synthetic), lift
  the level, and `PlayLooping()` it on the host while capturing. That is what
  produced the speech-detected analyser shot. Check the gate actually fired —
  the legend flips to "Sending" and three lights go green — rather than
  assuming it did.

Still missing: the share sheet, most of the settings sections (three of about
fourteen are shown), and iOS Picture-in-Picture for the floating window.

### The settings page does not match the app's own order

Noticed while capturing and **not acted on**, because it is a content decision
rather than a bug. The app's settings screen opens on *Audio devices* and
groups things differently from `settings.md`, which is organised by topic. The
app also has **Even out speaker loudness**, which the page does not mention at
all, and lists the noise profiles Off→Automatic where the page lists them
Automatic→Off.

The page is not wrong, but a reader with the screen open has to translate
between two orders. Worth reconciling deliberately, in one pass, rather than
patching.

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

- **The Apple half of publish 68 never landed.** Both attempts failed on App
  Store Connect returning HTTP 500 (`list-apps`) with Apple's own trace IDs.
  Google Play and Windows took build 68 from the same run. Retry when their API
  is healthy.
- **The recording playback panel (`80a565f`) is in no store build.** It is
  merged and verified — and now screenshotted, playing, on the site — but has
  never shipped. One publish catches it and the Apple gap up together.
- **`docs/MUSIC_GATE.md`** still waits on a road recording with music.
- **A one-off native abort** was seen once at startup
  (`nativeSetAndroidContext` → `SIGABRT` during `configureFlutterEngine`) after
  the app was backed out of and immediately restarted. It did **not** reproduce
  on a clean `force-stop` + start, and no panic message reached logcat. Recorded
  because it is the kind of thing that is dismissed twice and then reported by a
  user, not because there is anything to fix from this alone.
