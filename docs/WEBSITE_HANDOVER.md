# Website, store listings, screenshots and credits — handover

Written 2026-08-11, part way through the work, because the request was four
deliverables and only the first was started. This is what was done, what was
not, and the things that will otherwise be rediscovered the hard way.

**Not a site page.** No front matter, so Jekyll leaves it alone; it is a
working note like [SESSION_2026-08-10.md](SESSION_2026-08-10.md).

## State of each deliverable

| | State |
|---|---|
| Windows screenshots | **started, blocked** — build done, recordings seeded, app will not start locally (see below) |
| Degraded-panel screenshots | **not started** — decided: shoot on the OPPO, no runtime override |
| Website copy | **not started** — `index.md` last touched 2026-08-09 |
| Store texts and screenshots | **not started** — `STORE_DESCRIPTION.md` and `STORE_LISTING.md` last touched 2026-08-07 |
| Licences and credits | **partly current** — three dependencies missing, listed below |

Nothing on this list is committed except this file. The working tree was clean
at `9d28b7f`.

## Two traps, both already paid for

### The app will not start from a warm `flutter build windows`

It aborts before showing a window, with no window ever appearing — so it looks
like a build that silently did nothing:

```
Unhandled Exception: Bad state: Content hash on Dart side (-1471200315) is
different from Rust side (-1235189084), indicating out-of-sync code.
```

**It is a stale Rust DLL in the Release folder, not out-of-sync sources.** The
committed pair agrees — both `frb_generated.dart` and `frb_generated.rs` carry
`-1471200315` and were committed together in `9d28b7f`. CI builds from scratch
and is unaffected, so **the published builds are fine**; this was raised as a
possible shipped fault and it is not one.

```powershell
cd app; flutter clean; flutter build windows --release
```

Budget about six minutes. Check the hash agrees before blaming anything else:

```powershell
Select-String app\lib\src\rust\frb_generated.dart -Pattern "rustContentHash"
Select-String app\rust\src\frb_generated.rs -Pattern "CONTENT_HASH"
```

### Never capture the whole screen

A full-desktop grab was used to find the app window and it captured unrelated
work in another terminal — someone else's repository, issue numbers and branch
names. The file was deleted immediately and nothing reached the repository, but
the screenshot never needed the desktop in it.

**Capture the app window by its rectangle.** `GetWindowRect` on the process's
window handle, then `CopyFromScreen` bounded to that rectangle. If the handle is
`0` the app has not started — see the trap above — and the answer is to fix the
build, not to widen the capture.

## Screenshots

### Convention

`docs/assets/img/shots/<subject>-<platform>.webp`, 33 of them today. Platform
suffixes in use: `-phone`, `-ios`, `-desktop`, `-windows`. WebP, not PNG.

Existing desktop shots are `home-desktop.webp`, `diagnostics-desktop.webp` and
`set-devices-windows.webp`, so a new Windows set should match their window size
and theme rather than being cropped differently.

### Already set up

Two real rides are seeded at `%USERPROFILE%\Documents\mumbleway-recordings`,
which is where `recording_toggle.dart` puts them on Windows
(`getApplicationDocumentsDirectory()` + `mumbleway-recordings`):

| Clip | Blocks | Transmitted |
|---|---|---|
| `20260810-1849-000` | 6456 | **3614** |
| `20260810-1912-000` | — | — |

The first is the one to open: at 56% transmitted the waveform has plenty of
green, which is the whole point of screenshotting the listen sheet with content
in it rather than an empty one.

### What was asked for

- **The diagnostics panel**, which has gained a great deal since the last shot:
  the top-three classifier rows, the enhancer effort rung, the new `Model` row
  (`Low latency` / `Light`), the per-core CPU graph, and — on a device that has
  degraded — struck-through stage names and the yellow warning.
- **The listen sheet with content**: the waveform with green transmitted
  regions, and both toggles visible — transmitted-only (green) and chain
  playback (amber).

### The degraded panel is shot on the OPPO — decided

A degraded panel cannot be produced on a fast desktop by waiting, and
`MW_RELIEF` is honoured only by `core/tests/chain_cost.rs` — there is no
runtime override in the app. **The decision is to shoot those states on the
OPPO A3s rather than add one**, which keeps a debug-only path for taking
marketing pictures out of the shipping app.

It works because that device genuinely gets there: on a real call it walks the
whole ladder and stops the enhancer, and the panel rungs were confirmed
appearing on it. So the struck-through stage names, the yellow toolbar warning
and the "a more powerful device" message are all reachable by making a call and
waiting, with no instrumentation at all.

Two things about capturing it, both already paid for:

- **`adb shell screencap -p` into a file, then `adb pull`.** Piping it through
  PowerShell's `>` corrupts the PNG — the redirect is not binary-safe, and the
  result is a file of the right sort of size that no viewer will open.
- The panel is a **bottom sheet**, so it scrolls. `CLAUDE.md` has the warning
  about swiping down the middle of a settings list dragging a slider instead;
  the same applies here wherever a control is under the thumb, and **x = 1060**
  is the safe column on this device.

The desktop set is still worth taking for the *undegraded* panel, which is what
most people will see and is the better picture for a store listing.

## Website

Pages are `docs/*.md` with front matter, and **every one is mirrored under
`docs/ru/`** — `diagnostics`, `index`, `licences`, `privacy`, `scenarios`,
`sending-a-recording`, `server`, `settings`. A change to an English page that
does not reach its Russian twin leaves the site half updated, and the l10n test
does not cover site copy.

`SITE_PLAN.md` holds the intended structure; read it before adding a page.

### What has changed since 2026-08-09 and is unrepresented

Grouped by where it would belong.

**A device that cannot keep up now says so, and degrades in a defined order.**
The whole-chain relief ladder is the largest single omission — fifteen rungs,
from bending the enhancer through giving up display work to swapping the model
and finally bypassing the enhancer. It never climbs back within a session, it
starts from a startup probe, and it is surfaced as struck-through stage names, a
yellow toolbar warning and a message suggesting a more powerful device. Three
conditions move it: a block missing the 10 ms deadline, the whole device over
90% for five seconds, and any single core over 75% for five seconds.
`core/src/audio/relief.rs` is the authority and its doc comment carries the
measurements.

**Light noise model**, a setting rather than a rung — picks the cheaper
DeepFilterNet and leaves the rest of the chain alone. Single-core devices take
it whether or not it is chosen. Belongs in `settings.md`.

**The diagnostics panel** grew the top-three classifier output, the enhancer
effort rung, the model row, per-core CPU lines under the app's own, and honest
absence text where a platform refuses per-core figures. `diagnostics.md`.

**The listen sheet** gained two toggles — play only what would have been
transmitted, and play through the processing chain — plus a line explaining why
a recording has no green in it. `sending-a-recording.md`, possibly
`diagnostics.md`.

**Word starts survive.** The enhancer was found to be eating leading unvoiced
consonants — "shalom" arriving as "alom" — and the look-ahead pay-down and gate
hold were built against measurements. User-visible as "it no longer swallows the
start of words", which is worth saying plainly somewhere.

**The noise gate is anchored to the tracked floor**, which took Helmet recall
from 63% to 98% on voice over music. `scenarios.md` is where helmet and road
behaviour is described.

**Servers you connect to move to the top of the list**, with a connected server
keeping its place.

**Android**: the floating window has a close button; backing out no longer
leaves a live engine running with the microphone.

**macOS works**, after the vendored classifier dylib was renamed to match its
own install name.

## Store texts and screenshots

`STORE_DESCRIPTION.md` and `STORE_LISTING.md` are both from 2026-08-07 and
predate everything above.

Four stores now, not three — **the Microsoft Store listing is live**, and
`RELEASING.md` §4 is the reference for what Partner Center wants. Screenshot
sizes differ per store and none of that is recorded anywhere yet; that research
is part of the job.

Two constraints that already bind:

- **The listing must keep agreeing with `privacy.md`** — it tells readers the
  app collects nothing and shows no advertising, and a reviewer reads the fine
  print. `CLAUDE.md` records this.
- **A privacy policy URL is mandatory** and is
  <https://zavitax.github.io/mumbleway/privacy>.

## Licences and credits

`docs/licences.md` and `docs/ru/licences.md`. Better shape than the rest: YAMNet,
TensorFlow Lite and the patched `tract` were all attributed in the last two
days.

**Missing, all added on 2026-08-11 by `core/src/usage.rs`:**

| Crate | Licence | Why it is there |
|---|---|---|
| `mach2` | MIT / Apache-2.0 | Mach `task_info` on iOS and macOS |
| `libc` | MIT / Apache-2.0 | `sysconf` for the clock tick and page size |
| `sysinfo` | MIT | process and per-core CPU on Windows |

Confirm each licence from the crate rather than from this table — it was written
from memory and not from the `Cargo.toml` files.

Worth checking at the same time whether the vendored plain DeepFilterNet 3
weights at `core/models/` are attributed. They are 7.6 MB of someone else's
training and `core/models/README.md` explains why they are vendored, but that is
not the same as a licence entry.

## Suggested order

Licences first: smallest, and the only one of the four that is a store
requirement rather than an improvement. Then store texts, which the website copy
can borrow from. Then the website. Screenshots last, because they need the clean
rebuild above.

The two screenshot passes are independent and can be taken in either order: the
Windows set for the ordinary panel and the listen sheet with content, and the
OPPO set for the degraded states.
