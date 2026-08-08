# Remaining work on the site and the app

Handover written 2026-08-08, at the end of a long session. Everything described
as done is live; everything under "Not started" is exactly that.

## Look at this first

`docs/_includes/fig-pillion.svg` — the rider/pillion feedback loop — is
committed and pushed as `d63c7b1`, and **has never been rendered in a
browser**. It parses as XML and nothing more is known about it. Open
`/scenarios.html` and check it before trusting it; every other figure on the
site needed a correction once it was actually looked at.

## Not started

### 1–4, 6. Screenshots — one job, not five

These are a single pass driving real builds, not five separate tasks. The
existing store screenshots come from `app/test/store_screenshots_test.dart`,
and **that harness cannot produce most of what is wanted**: a widget test has
no engine, so there is no live spectrum and no audio.

What is wanted:

- **Real emulator captures** so buttons carry real text rather than harness
  rendering.
- **The analyser running**, in two states — speech detected and not — on every
  platform. Needs audio playing into the device while capturing. The acoustic
  loopback used earlier this session works: play a `.wav` on the host through
  `Media.SoundPlayer` while the emulator's microphone is live. It is
  unreliable; expect several attempts, and check `speaking` actually fired.
- **The floating call window**, both platforms. Android needs the "display over
  other apps" permission granted first; iOS is Picture in Picture.
- **Settings sections** on `docs/settings.md`, per section rather than one
  screenshot of the whole screen.
- **A recording walkthrough** for `docs/sending-a-recording.html`: switch on,
  card expanded, listen sheet, share sheet.

Capture recipe that worked all session:

```powershell
$adb = "$env:LOCALAPPDATA\Android\Sdk\platform-tools\adb.exe"
& $adb shell screencap -p /sdcard/x.png
& $adb pull /sdcard/x.png out.png     # NEVER `exec-out ... > file` in
                                      # PowerShell; it corrupts binaries
```

Then downscale to WebP with PIL, as `docs/assets/img/shots/` already is —
560 px wide for phone, 1000 px for desktop, quality 84. Those come out at
15–61 KB each.

### 5. Illustrations for "On the road"

One is drafted (see uncommitted, above). Others worth having: a group strung
out over kilometres on one channel, and the tunnel/no-signal case, which is the
honest limitation and currently only prose.

Reuse the helmet from `docs/_includes/fig-range.svg` — it is the app icon's,
lifted from `app/assets/icon/mumbleway.svg` at 7%, and both figures should keep
using the same one.

### 7. Russian, with a persistent language switch

**The largest item by a wide margin, and the one to decide before writing any
code.** A switch is trivial; seven translated pages and a mechanism are not.

Jekyll has no i18n and **GitHub Pages runs no plugins**, so a gem is not an
option. Proposed structure, not yet agreed:

- `/ru/` copies of every page, front matter carrying `lang: ru`.
- `docs/_data/strings.yml` for chrome — nav labels, footer, buttons — keyed by
  language, so `_layouts/default.html` reads `site.data.strings[page.lang]`.
- The switch in the top bar: **right of the menu on wide screens, left of the
  hamburger on narrow**, and visible at every width. It links to the current
  page's counterpart, which means each page needs to know its opposite number —
  simplest is a `ref:` key in front matter shared by both language versions.

Pages to translate: index, settings, scenarios, server, licences, privacy,
sending-a-recording. **`privacy.md` is the delicate one** — its URL is
registered with three app stores, so `/privacy` must keep resolving exactly as
it does. Do not enable `permalink: pretty`; it would move it to `/privacy/` and
404 the URL a reviewer follows.

### 8. Translated text inside the illustrations

Falls out of 7 if the figures become includes taking a language parameter:

```liquid
{% include fig-range.svg lang=page.lang %}
```

…with every `<text>` reading from `site.data.strings`. Doing 7 without this
means the diagrams stay in English on the Russian pages, and retrofitting is
more work than building it in.

## Things that will bite

- **Stale cache.** Testing the site in a browser repeatedly served old CSS with
  new JS and produced nonsense. Cache-bust the stylesheet or hard-reload before
  believing anything looks wrong.
- **kramdown does not parse markdown inside a block element.** Every `<div>`
  wrapping a table needs `markdown="1"` or it renders as pipes.
- **`baseurl` is not inferred** for a project site. It is set in `_config.yml`;
  without it every `relative_url` resolves to the user-site root and the whole
  navigation 404s while the pages build perfectly.
- **The menu breakpoint is measured, not a media query** (`docs/assets/js/menu.js`).
  Adding a nav item needs no CSS change. Adding Russian labels needs no CSS
  change either — that is why it was made measured.
- **Never edit source through a PowerShell pipeline.** `Get-Content |
  Set-Content -Encoding utf8` corrupts UTF-8. Use the editing tools, and run
  `python tool/check_encoding.py` after touching anything with non-ASCII —
  which every Russian file is.

## Outside the site

- **The Apple half of publish 68 never landed.** Both attempts failed on App
  Store Connect returning HTTP 500 (`list-apps`), with Apple's own trace IDs —
  their outage, not our configuration. Google Play and Windows MSIX took build
  68 from the same run. Retry when their API is healthy.
- **The recording playback panel (`80a565f`) is in no store build.** It is
  merged and verified on the emulator — play, seek, waveform, playhead — but
  has never shipped. One publish catches it and the Apple gap up together.
- **`docs/MUSIC_GATE.md`** is still waiting on a road recording with music, and
  should not be acted on before there is one. Three features have already been
  tried against that fault and failed.
