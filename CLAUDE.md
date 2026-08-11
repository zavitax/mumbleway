# Working in this repository

Conventions and hard-won facts that are not visible from the code. Everything
here was learned by getting it wrong first.

## "Publish" means trigger `publish.yml`

Not "push to main". Pushing runs `build.yml`, which compiles the matrix and
attaches artifacts and uploads to nothing.

```bash
gh workflow run publish.yml --ref main -f track=internal
```

That really uploads, to real stores:

| Job | Where it goes | Reversible? |
|---|---|---|
| Google Play | internal track, `status: completed` — **live at once** to testers named in the console | roll forward only |
| TestFlight | build uploaded, then **every older build is expired** | no |
| Mac App Store | signed `.pkg` to App Store Connect | no |
| Windows MSIX | artifact only; `MSIX_PUBLISHER` is unset, so it is a sideload package | n/a |

None of it submits for App Store review, and no wider Play track goes live: the
workflow uploads alpha, beta and production as `draft` on purpose, so anything
reaching people who did not opt into a test build stops for a human. Choosing a
wider track is an escalation to ask for explicitly, never one to infer from
"publish".

A `v*` tag triggers the same workflow **and** a GitHub release. Tagging is a
release marker with a permanent name, so it is a bigger statement than
publishing a test build — do not reach for it just to get a build to testers.

### The `msstore-cli` skill is a reference, not a route

`.claude/skills/msstore-cli/` documents the Microsoft Store CLI, including
`msstore publish` and `msstore submission delete`. **Publishing still goes
through `publish.yml`.** The workflow does two things a bare `msstore publish`
does not: it reads the pending submission's status first and refuses to submit
over one that is in flight — a submission in certification blocks the next, and
overwriting one costs days — and it builds the MSIX from a clean checkout with
the version derived from `run_number`. Reach for the skill to *read* state
(`msstore apps list`, `msstore submission status`), which the workflow cannot do
interactively, and for nothing that writes.

### Build numbers come from `github.run_number`

Not from `pubspec.yaml`. The version there (`1.0.0+1`) is the human-facing one
and does not need bumping to publish. Both stores reject a build number they
have seen before, and the rejection arrives *after* the build, so the number has
to rise on its own — `run_number` only ever goes up.

### App Store Connect allows six uploads per app per *hour*

Not per day, whatever it says. The seventh publish in an hour comes back:

```
Error Domain=IrisAPI Code=-19241  code=90382
Upload limit reached. The upload limit for your application has been
reached. Please wait 1 day and try again.
```

**"Please wait 1 day" is wrong and it is Apple's own text.** The window is an
hour, and believing the message costs a day of releases: this was recorded here
as a daily limit, and the observation that should have caught it was already in
hand — the limit was hit at 09:16 and the next publish went green at 11:07,
which no daily quota permits.

**TestFlight and Mac App Store fail; Google Play and Windows carry on.** So a
run goes half red, and the red half is the half that will not retry for a
while. Nothing is broken and nothing needs fixing — but the builds do not exist
on Apple's side, which is exactly the sort of thing that reads as "published"
in a summary and is not. Check the jobs, not the run.

So an hour's patience clears it. Still worth not spending uploads on a change
that ships no app code — a documentation edit, a benchmark harness, a test —
because six in an hour is easy to reach on a busy afternoon.

### Check the secrets exist before claiming a publish happened

Every job is gated and **skips cleanly when its secrets are absent**, which
means a run can go green having uploaded nothing at all. `gh secret list` shows
names and dates, never values.

## The privacy policy, and where it lives

**<https://zavitax.github.io/mumbleway/privacy>** — the URL to paste into
Partner Center, App Store Connect and Play Console.

Source is **`docs/privacy.md`**, and that is the only copy. GitHub Pages serves
it from the `main` branch's `/docs` folder, so editing the file publishes it;
there is nothing to copy anywhere and nothing to keep in step.

Every store here requires the URL from an app that records a microphone — Store
Policy 10.5.1 and Apple's App Privacy both — and **submission is blocked without
it**, at the end of a long form rather than the start.

Two things to hold on to when editing it:

- **It has to keep agreeing with `docs/STORE_DESCRIPTION.md`**, which tells
  readers the app collects nothing and shows no advertising. A policy that
  drifts from the listing contradicts the store page instead of supporting it,
  and the fine print is the half a reviewer reads closely.
- **It describes real data flows, not a template's.** Three are easy to lose and
  each is checkable in the code: the public server directory calls
  `publist.mumble.info` (`app_state.dart`), optional sync writes the server list
  to the user's *own* iCloud or Android Backup account with passwords held
  separately (`services/cloud_sync.dart`), and diagnostic recording writes a
  microphone to device storage (`core/src/audio/record.rs`). Add a network call
  or a stored field, and this file is part of the change.

`docs/index.md` is the Pages landing page and exists so the site root is not a
404. It carries the tagline and links here.

## Secrets and this repository

`zavitax/mumbleway` is **public**. Never paste a token, key, certificate or
account identifier into a commit, an issue, a PR, or a chat message. The same
warning is in `docs/RELEASING.md`, over the signing material it applies to.

Publishing needs no access to anyone's developer accounts, and asking for that
access is the wrong move: the credentials are generated by their owner and
pasted in as encrypted secrets, which is the point of the arrangement.

Two consequences that have already shaped the code:

- The Apple team id is injected into `ExportOptions.plist` and the Xcode project
  by the workflow, not committed. The checked-in plist holds a placeholder.
- `APP_STORE_CONNECT_API_KEY` is written to a file, never to `$GITHUB_ENV` and
  never passed as an argument. An argument is visible in the process list, and
  `$GITHUB_ENV` would put a derived copy of a private key somewhere GitHub's
  secret masking cannot reach.

The Telegram intake bot reads its token only from `MUMBLEWAY_TG_TOKEN`, and
`MUMBLEWAY_TG_CHATS` is a mandatory allow-list — a bot username is guessable, so
without the list the first thing it would accept is whatever a stranger sent.

## Never edit source through a shell pipeline

On Windows, `Get-Content | Set-Content -Encoding utf8` **corrupts UTF-8**: the
read uses the system codepage and the write uses UTF-8. This silently turned 23
em-dashes and quotes into Cyrillic mojibake across `engine.rs` and the FFI
layer, and it was committed and pushed unnoticed because Rust does not care and
the diff looked like the intended change. A second repair pass made it worse.

Use the Read/Edit/Write tools. They do not have this failure mode.

`app/lib/l10n/app_ru.arb` is the file with the most to lose. After touching
anything with non-ASCII text, check it decodes — and note that a garbled
*console* rendering is not evidence of a garbled file, because the terminal
decodes with the system codepage too.

## Configure CMake with Ninja on Windows, not the Visual Studio generator

```
cmake -G Ninja ...        # ninja ships in C:\Android\sdk\cmake\3.22.1\bin
```

**Under `-G "Visual Studio 17 2022"` a CMake dependency can open a File Explorer
dialog on the developer's screen, once per configure**, and it looks like a
broken build while being nothing of the kind.

The path is worth knowing because nothing about it is greppable. Eigen — pulled
in by TensorFlow Lite, and by anything else that wants it — runs
`include(CMakeDetermineFortranCompiler)` **unconditionally** at
`eigen/CMakeLists.txt:584`, purely to pick a default for `EIGEN_BUILD_BLAS` and
`EIGEN_BUILD_LAPACK`. Under the Visual Studio generator CMake's Fortran probe is
not a compile but a *project file*, `CompilerIdFortran.vfproj`, in the Intel
Visual Fortran format. With no Intel Fortran installed and no `.vfproj`
association — `assoc .vfproj` answers "File association not found" — launching it
falls through to ShellExecute and Windows asks the user what to open it with.

Nothing is wrong: the answer is "there is no Fortran compiler", which is correct
and wanted. Under Ninja the same probe is an ordinary source compile that fails
quietly. `-DCMAKE_Fortran_COMPILER=NOTFOUND` short-circuits it too.

It is configure-time only, so a parked build stops producing them by itself.

The Android cross-build already sets `CMAKE_GENERATOR_aarch64_linux_android` to
Ninja for its own reasons; this is the same instruction for every other CMake
build on this platform.

## Verification, before saying something works

```bash
cd core && cargo test          # 296 tests
cd app  && flutter analyze     # must be clean
cd app  && flutter test        # 143 tests
```

After any FFI change: `flutter_rust_bridge_codegen generate`, and commit
`app/lib/src/rust/**`. On this machine neither `cargo` nor `flutter` is on the
default PATH:

```
$env:PATH = "$env:USERPROFILE\.cargo\bin;C:\src\flutter\bin;$env:PATH"
```

### `--release` and debug can disagree about whether a model loads

`cargo test` and `cargo test --release` are not the same check here, and the
difference is not speed. **tract's graph self-checks are `#[cfg(debug_assertions)]`**
— `check_compact`, `check_names`, `check_edges` — so a graph tract considers
malformed loads perfectly well in release and refuses to load in debug.

That is not hypothetical: the plain DFN3 hit exactly this, and the two readings
of it are opposite. In release the rung worked; in debug four tests failed with
`duplicate name /convt3/Conv.bias`, which reads like a corrupt download and was
a name collision inside tract's own optimiser. `third_party/tract-core/PATCH.md`
has the mechanism.

So when something model-shaped behaves differently between the two, suspect this
before suspecting the file. And **run the debug suite** — it is the one that can
see this class of fault, and it is what CI runs.

New user-facing strings need keys in **both** `app_en.arb` and `app_ru.arb`,
with genuine Russian: a test fails on a key that is missing from one file or
identical in both. Run `flutter gen-l10n` after editing them.

CI is the only real check for the Swift, Kotlin and plist changes — none of it
compiles locally on Windows. It is also not a device test, and several classes
of bug here only appear on one.

### Drive the iPhone simulator with `idb`, not with clicks

`simctl` can install, launch, open a URL and take a screenshot, but it cannot
tap. The obvious substitute — `osascript` telling System Events to click at a
screen position — **does not work over SSH** and fails with error `-25204`,
because the process behind the session has no Accessibility permission. It is
also the wrong shape: it aims at where the Simulator window happens to be on
screen rather than at the app.

[`idb`](https://fbidb.io/) talks to the simulator directly. No Accessibility,
no window geometry, and coordinates are **device points** — 393 × 852 on an
iPhone 16 — so a tap is expressed in the same units the layout is:

```bash
idb connect <udid>
idb ui tap --udid <udid> 165 265
idb ui text --udid <udid> "hello"
idb ui swipe --udid <udid> 200 700 200 200
```

Things that are easy to reach for and should not be:

- **Writing the preferences plist directly does not stick.** `cfprefsd` has it
  cached and writes back over the edit. Go through `defaults` *inside* the
  simulator instead: `xcrun simctl spawn booted defaults write …`, with any
  JSON value quoted as one string inside a plist array.
- **And that write only lands if the app is not already running.** Setting
  `flutter.mumbleway.locale` under a live app changes nothing — SharedPreferences
  read it at startup. Set it before the first launch, or just tap the flag
  button, which is a one-tap toggle because there are only two languages.
- **`xcrun simctl openurl booted "mumble://user@host:64738/"`** fills in the
  add-server form without any tapping at all, which is often enough on its own.
- **`idb ui swipe` takes four coordinates, and three is not an error.** `swipe
  380 700 600` is accepted and does nothing. A scroll loop built on it makes no
  progress while every call reports success, which reads as a list that will not
  scroll rather than as a malformed command.

**Tap by label, not by measuring a screenshot.** `idb ui describe-all` returns
every element with its frame already in device points — the same unit `idb ui
tap` wants — so there is no pixel-to-point arithmetic to get wrong:

```bash
idb ui describe-all --udid <udid>   # JSON array; frame is {x,y,width,height}
```

Two things that pass while being wrong:

- **Rank the matches before tapping one.** "Add server" is both the screen's
  heading and its button. Tapping the heading does nothing and looks exactly
  like a tap that worked. Prefer an element that is operable, then an exact
  label match.
- **An absent label does not mean what you think.** The listen sheet's toggles
  advertise what tapping them will *do*, so "Play only what was transmitted"
  disappearing means the toggle went on — or means the sheet closed. Deriving
  state from absence alone reported four captures of the same screen as four
  different states, and reported success. Check the sheet is open first.

### Photograph a Mac window with `screencapture -l`, and drive it with CGEvent

```bash
swift tool/winlist.swift                    # windowid, owner, bounds, title
screencapture -x -o -l <windowid> out.png    # exactly one window, no desktop
```

`-x` drops the shutter sound, `-o` the window shadow. The result is the window
at native 2x — a 1000x720-point window comes out 2000x1440 — and the desktop
behind it is never in the file, which is the thing that must not happen: an
earlier session took a full-screen grab and caught another project's work in it.

The window id comes from `CGWindowListCopyWindowInfo`, which reads geometry
rather than pixels and so needs no permission. `screencapture` itself needs
Screen Recording, granted to the terminal.

**This replaces the route recorded in 63db3ee**, which drove the same Mac
*remotely* over Screen Sharing with `vncdotool` and cropped the window out of a
whole-framebuffer grab. All three of the difficulties that commit describes —
ARD security type 30, no per-window capture in the VNC protocol, doubling every
crop because the framebuffer is Retina and bounds are in points — were artifacts
of being remote. Run locally they disappear. That procedure was never written
into the tree; grep for "vncdotool" and there is nothing.

**`System Events -> click at {x, y}` does not work on this app, and says it
did.** It resolves the element under the point and sends it an AXPress. Flutter
draws the whole interface into one NSView, so the only thing there is a generic
group: the call succeeds, returns `group 1 of window mumbleway`, and nothing
happens. Post a real event instead — a `CGEvent` mouse down/up lands wherever
the pointer is put, whatever the accessibility tree exposes. Move the pointer
first, and park it off the window before capturing or a tooltip ends up in the
shot. Resizing and positioning *do* work through System Events, so accessibility
permission is still needed for both.

**Every local rebuild re-asks for the microphone.** The app signs ad-hoc, so a
rebuild changes its signature and macOS treats it as a new app. The prompt is
easy to miss when driving headlessly, and while it sits unanswered the engine
fails with `the audio device did not open in time` — which reads as a broken
audio stack rather than an unanswered dialog. `swift tool/winlist.swift` shows
it as a `UserNotificationCenter` window.

### Never scroll the settings screen down its middle

`adb shell input swipe 540 1750 540 1110` looks like a scroll and is not. The
settings list is mostly full-width sliders, and a swipe that starts on one drags
*it* rather than the list. This silently moved the incoming audio buffer from
200 ms to 280 ms during a screenshot sweep, and the screenshots taken afterwards
documented the wrong value in a way nothing on the page would have contradicted.

Swipe at **x = 1060** instead. Slider tracks end around x = 975 and the toggles
around x = 1010, so the right margin scrolls the list and touches no control.

The general form of it: after driving the UI to take screenshots, **read the
settings back and compare them to what the page claims they are.** A capture of
a control at the wrong value is indistinguishable from a capture at the right
one, which is the same reason the app records its own capture input.

## Audio: what is load-bearing and not obvious

- **Recording must take an audio hold.** The capture worker feeds the recorder
  and does not run until the engine has opened the devices. Starting the
  recorder without a hold writes a file that is the right length and empty. On
  Android it is worse: without `MODE_IN_COMMUNICATION` there is no hands-free
  link, so the audio comes from the *phone's* microphone.
- **That confusion has already cost the project everything it measured once.**
  Audio carries no record of what captured it, so a directory of recordings from
  the wrong microphone looks exactly like one from the right microphone. This is
  why the app records its own capture input rather than trusting care.
- **Android hands a backgrounded app silence, not an error**, from Android 11.
  The microphone-typed foreground service in `OverlayService` runs for the whole
  session — not only during a call — and shortening its life would break
  recording with no visible symptom.
- **Leaving the app had two exits and only one of them left.** Swiping the task
  away reaches `OverlayService.onTaskRemoved`, which stops the service *and*
  ends the process. Backing out reaches `MainActivity.onDestroy`, which stopped
  the service and returned — so the Rust capture worker, the recorder, the
  encoder and the sockets carried on in a cached process with no interface and,
  now, no microphone-typed service. Android then did exactly what the entry
  above says: it stopped handing that process microphone data and handed it
  **digital zero** instead. The engine recorded 8.5 minutes of bit-exact silence
  across three files before the system killed it for excessive CPU, and it
  arrived as *"the gate was closed when it should have been open"*. Every stage
  in the chain was working perfectly on the silence it was given.
  **A stage that is behaving correctly on bad input looks identical to a stage
  that is broken**, which is why `run_worker` now warns when the microphone
  returns bit-exact zero for two seconds. Zero, not "quiet": a real microphone
  in a silent room sits tens of dB above it, so this cannot fire on a quiet
  room.
- **iOS must not be offered A2DP** on a `playAndRecord` session. A2DP is
  output-only; offering it lets iOS take it when music starts and tear the input
  down silently. This was a real bug, reported as "recording only works when
  music is not playing".
- **A file iOS has no type for cannot be shared.** iOS types a shared item by
  its extension. `.s16` is not a type it knows, so it invents a *dynamic*
  identifier (`dyn.…`), and share targets that accept only declared types
  accept none of it: the sheet opens, a target is picked, nothing arrives, and
  nothing logs. Android matches on MIME instead, where
  `application/octet-stream` is ordinary — so this fails only on a phone, and
  looks like a broken share sheet. The recordings go out as one `.zip`
  (`public.zip-archive`) for that reason. Check `UTType(filenameExtension:)`
  before sharing any new file type.
- **`CFBundleLocalizations` must list every `.arb` language.** iOS and macOS
  report an English locale for an undeclared language whatever the phone is set
  to, so a complete translation is simply never asked for. A test asserts this.

### A vendored dylib must be called what its install name says

`vendored_libraries` in a podspec **links** the library as well as copying it.
So the app binary records the dylib's own `LC_ID_DYLIB` as the name to load at
launch, while CocoaPods copies the file into `Contents/Frameworks` under
whatever the podspec calls it. If those disagree, every Mac dies in dyld before
`main`:

```text
Library not loaded: @rpath/libtensorflowlite_c.dylib
tried: '/Applications/mumbleway.app/Contents/Frameworks/
       libtensorflowlite_c.dylib' (no such file)
```

That shipped: the classifier dylib is distributed as
`libtensorflowlite_c-mac.dylib` and calls itself
`@rpath/libtensorflowlite_c.dylib`. Rename the file rather than reach for
`install_name_tool`, so one name is true in the podspec, in the loader and in
`bindings.dart` alike.

**CI cannot see this class of fault.** The macOS job compiles, signs and
uploads without ever launching the result, so the first thing that noticed was a
crash report from a real Mac. `app/test/macos_tflite_dylib_test.dart` reads the
Mach-O header and asserts the three names agree, which is checkable from any
host.

## Say when a device is not accelerating

Anything that runs a model has to report, on every platform, whether the
accelerated path was actually built — and say so where a rider can see it, with
the measured per-frame cost beside it. Two of these are shipped and one is
queued (`docs/ENHANCER_GPU.md`).

**Claim only what is checkable.** Core ML decides per operation whether to use
the Neural Engine, the GPU or the CPU and reports none of it, so the honest
statement is *the accelerated path was or was not built* — never "an NPU is
doing this". The classifier's note reports milliseconds from the device rather
than warning about battery, after an earlier draft asserted a cost nobody had
measured and the real figure turned out to be 2.4 ms.

**A delegate can take the process down, not throw.** TFLite's GPU delegate
segfaulted inside `TfLiteInterpreterAllocateTensors` on an Adreno 506; a Dart
`try`/`catch` around it never ran. Anything that opts into hardware needs a flag
written before the attempt and cleared after, because nothing else survives a
SIGSEGV.

## Instrument the input before debugging the chain

The diagnostics panel had eleven numbers in it and **not one was measured before
the capture chain ran**. The meter beside the microphone gain slider read
`analysis.level_db`, taken after RNNoise and the profile filters — so the one
control that sets the input level was showing the output of the thing that level
feeds, and it sat comfortably mid-scale while the microphone was clipping 35% of
its samples.

That single blind spot presented as four separate faults: distorted speech,
`Helmet` sounding worse than `Standard`, music surviving the gate, and a long
argument between a meter and a measurement that were both correct about
different signals. `record.rs` compounded it by clamping to ±1.0 on the way to
i16, so the recordings showed clipping with nothing to say where it happened.

**A measurement taken after the thing you are debugging cannot exonerate it.**
The panel now carries a microphone peak and a clipped-sample count taken on the
raw block, and the meter reads the microphone.

## Measurement discipline

Several plans in this repository were disproved by their own acceptance tests
passing on the first run, and one trained model collapsed on real audio after
looking excellent on synthetic. The pattern is consistent enough to state:

**Synthetic signals agree with whoever wrote them.** A generator written after a
hypothesis, by the same hand, can only show that a fault does not reproduce
offline — never that it does not happen on a bike.

When a measurement contradicts the plan, the measurement wins, and the
retraction goes in the file where the claim was made. `tools/vad/README.md`
leads with one; `docs/RECORDING.md` and `core/src/audio/pitch.rs` carry others.
Do not quietly drop a disproved claim — someone will re-derive it.

`docs/MUSIC_GATE.md` is the open one: music opens the voice gate, and five
features have now been tried against it and failed. The music recording arrived
on 2026-08-09 and disproved two of them plus the file's own account of the
mechanism — the leak turns out to be an **Auto-profile convergence transient**,
not a per-block misclassification, and every number the transmit decision uses
is measured downstream of the suppressor whose error it is supposed to catch.
What is still missing is speech over the same music: that clip bounds false
positives and can say nothing about recall.
