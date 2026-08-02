# MumbleWay

[![build](https://github.com/zavitax/mumbleway/actions/workflows/build.yml/badge.svg)](https://github.com/zavitax/mumbleway/actions/workflows/build.yml)

A cross-platform Mumble client built for noisy environments — specifically, for
talking from inside a motorcycle helmet at speed.

Windows · macOS · iOS · iPadOS · Android phones and tablets.

## Layout

```
mumbleway/
├── core/                  Rust: the entire client engine, no UI, fully testable
│   ├── proto/             Mumble.proto + MumbleUDP.proto (upstream schemas)
│   └── src/
│       ├── varint.rs      Mumble's bespoke varint codec
│       ├── crypto/ocb2.rs OCB2-AES128 with the eprint 2019/311 mitigation
│       ├── net/           TLS control channel, UDP voice, TCP tunnel fallback
│       ├── audio/         capture DSP, RNNoise, Opus, jitter buffer, cpal I/O
│       └── session/       handshake, reconnect state machine, multi-server
└── app/                   Flutter UI
    ├── lib/               screens, widgets, state
    └── rust/              thin flutter_rust_bridge shim over `core`
```

The split is deliberate: `core` has no Flutter dependency, so the protocol and
audio logic is unit-testable without a device or a UI.

## How the requirements are met

| # | Requirement | Where |
|---|---|---|
| 1 | Connect to public and saved private servers | `session/`, servers persisted via `shared_preferences`, identity certificate in `net/tls.rs` |
| 2 | Noise cancellation for a helmet | `audio/denoise.rs`, `audio/dsp.rs` |
| 3 | Reliable reconnect except on user disconnect | `session/reconnect.rs`, `session/mod.rs` |
| 4 | Modern, intuitive UI | `app/lib/` — Material 3, oversized controls |
| 5 | Five platforms | one Rust core + one Flutter UI; cpal covers all backends |
| 6 | Status indication | `ConnectionState` → `widgets/status_badge.dart` |
| 7 | Two servers at once (bonus) | `session/manager.rs` |
| 8 | Audio device selection, testing and levels | `audio/engine.rs`, Settings screen |
| 9 | Always-visible input meter | `widgets/ptt_button.dart` |
| 10 | Server profile import and public directory | `session/profile.rs`, Import screen |
| 11 | Live ping per server | `net/ping.rs` |
| 12 | Channel tree, roster and per-user mute | `widgets/channel_panel.dart` |
| 13 | Drop/resume audio cues | `audio/engine.rs` (`AudioCue`) |
| 14 | Floating call window | `android/.../OverlayService.kt`, `ios/Runner/PipController.swift`, `macos/Runner/FloatingPanel.swift` |

## Working in the background

A rider is normally looking at a navigation app, not at this one, so anything
that only appears on screen is something they will miss.

* **Audible connection cues.** A falling two-tone when the link drops, a rising
  one when it comes back. Deliberately not on the first connect — the user is
  looking at the screen then, and a chime on every launch is noise. They also
  bypass the deafen flag, because "the connection dropped" is exactly what a
  deafened user still needs to know.
* **The 15 s rule.** Fifteen seconds of total silence from the server counts as
  a drop and triggers reconnection. With a 5 s ping interval that is three
  missed pings.
## The floating call window

Every platform can keep the call reachable after you leave the app, but no two
of them do it the same way, and the differences are visible to the user rather
than hidden behind one abstraction.

| Platform | Mechanism | Controls |
| --- | --- | --- |
| Android | Overlay window, `SYSTEM_ALERT_WINDOW` | talk, mute, deafen, hang up |
| iOS / iPadOS | Picture in Picture | talk, mute, deafen |
| macOS | Floating `NSPanel` | talk, mute, deafen, hang up |
| Windows | — | the window is already there |

All of them show the same two things as the main screen:

* **The input meter**, with the tracked noise floor and the level voice
  activation opens at marked separately. The gap between those two marks is the
  margin being tuned — on a bike the floor climbs with road speed, and a meter
  showing only the threshold makes that look like a control that has drifted
  rather than wind. The bar turns green once the level would open the gate.
  All four surfaces share one scale (`OverlayBridge.meterFraction`) so the
  marks cannot drift apart between them.

* **A flashing on-air light** while audio is going out. It blinks rather than
  sitting steady because a static colour is easy to stop noticing, and a
  channel left keyed open — talking to a group that can hear everything — is
  the failure worth catching. The blink runs off its own timer on each
  platform, so the rate stays even regardless of how often state is pushed.

* **Android** draws a small draggable island over the navigation app. It runs
  from a foreground service, which is also what keeps the audio engine alive
  while another app is in front. Needs the "display over other apps"
  permission, which Android only grants from its own settings screen.

* **iOS** has no system-wide overlay at any privilege level, so the only way to
  stay visible is Picture in Picture. It is built for video, and the way audio
  apps use it — Google Meet included — is to render their own frames into an
  `AVSampleBufferDisplayLayer` and hand that to `AVPictureInPictureController`.
  MumbleWay draws the call state into those frames: talk indicator, mute and
  deafen badges, and who is speaking.

  The system owns the buttons. There are exactly three programmable ones and no
  API to add a fourth or relabel them, so they go to **play/pause → talk,
  skip back → mute, skip forward → deafen**. Hang-up does not fit; it can be
  bound to the close button, but only behind a setting that is off by default,
  because tidying a window away should not silently drop a call. Returning to
  the app never disconnects.

* **macOS** uses a non-activating floating panel, so clicking talk does not pull
  the app forward and steal focus from whatever is in front.

## Buttons

A handlebar Bluetooth remote is the practical way to key a microphone with
gloves on at speed. Most present as a Bluetooth HID keyboard or media
controller, so *Settings → Buttons* learns whatever the device actually sends
rather than offering a list of keys — the only reliable way to find out what a
given remote emits is to press it.

A button can hold-to-talk, toggle transmit (for remotes that send a click but
never a release), or toggle mute or deafen.

On Android these keep working with the app in the background: the foreground
service owns a media session, which is what puts the app at the front of the
media-button queue ahead of whatever music is playing.

Keying and unkeying play short walkie-talkie cues — a click on press, a roger
beep and squelch tail on release. They are quieter than the status cues because
they fire constantly, and they go to the playback queue only, so they are heard
locally and never reach the far end.

## The noise-cancellation chain

Wind and engine noise is the hard problem, and RNNoise alone does not solve it.
The microphone chain, per 10 ms block:

1. **4th-order high-pass** (180 Hz on the helmet profile) — strips wind and
   engine rumble before anything else sees it.
2. **RNNoise** (`nnnoiseless`) — a recurrent denoiser that handles the
   non-stationary broadband noise a moving bike produces.
3. **SNR gate against a tracked noise floor** — see below.
4. **AGC** — the rider shouts on the motorway and murmurs at the lights.
5. **Limiter** — catches wind gusts before they clip.

### Why the SNR gate exists

RNNoise's voice-activity output cannot be trusted on its own here. Measured on a
steady 55 Hz + 110 Hz engine drone, it reports a speech probability of **0.82**:
the harmonic structure of an engine looks like voiced speech to the network.

Gating on that alone keys the transmitter continuously, and because the AGC then
sees a "speaking" block it winds up to its full +24 dB ceiling and amplifies the
residual noise straight back to the original level — undoing all the filtering.

So the speech decision requires the network **and** a signal-to-noise test
against a noise floor tracked by minimum statistics. Steady noise raises the
floor with it and never clears the margin, however loud it gets, while real
speech rises clearly above it. `helmet_profile_crushes_engine_rumble` and
`speech_over_loud_engine_noise_is_still_transmitted` pin both halves of this.

One wrinkle worth knowing: RNNoise's first frames come out near-silent while its
internal lookahead fills. Feeding those to the floor tracker pins the estimate
tens of dB too low and makes everything afterwards look like speech, so the chain
stays muted for a 150 ms warm-up (`WARMUP_BLOCKS`).

## Reconnection

Everything except a user-initiated disconnect is retried, including ping
timeouts. Backoff starts at 500 ms and is capped at **20 s** — deliberately low,
because a rider coming out of a tunnel should be back in seconds. Jitter avoids
a stampede when a server restarts. A connection that stays healthy for 30 s
resets the backoff.

Ping timeout is detected as 16 s of total silence from the server (Mumble itself
drops clients after 30 s without a ping, and we ping every 5 s).

## Transport

Voice prefers UDP with OCB2 encryption. UDP is probed with encrypted pings; if
it never comes up, or goes quiet mid-ride as a carrier drops the NAT binding,
voice automatically falls back to tunnelling through the TLS control channel.
The UI shows which path is live.

## Security notes

- **Server certificates**: Mumble servers are effectively all self-signed, so
  WebPKI validation would reject nearly every real server. The client uses
  trust-on-first-use — it pins the fingerprint on first connect and refuses a
  changed certificate until the user explicitly accepts it. Handshake signatures
  are still verified cryptographically; only the trust root is relaxed.
- **Client identity**: a self-signed certificate is generated once and reused;
  this is what a server ties registration to, so it must not be regenerated.
- **OCB2**: ported faithfully from upstream, including the counter-cryptanalysis
  fix. Note the mitigation *deliberately* perturbs one bit of an all-zero block —
  that is upstream's designed behaviour and is required for wire compatibility.

## Building

Prerequisites: Rust (stable), Flutter (stable). Both are already installed if you
ran this project's setup.

```bash
# Engine tests — no device or UI needed
cd core && cargo test

# The app
cd app && flutter run -d windows      # or macos / android / ios
```

### Windows: Developer Mode is required

`flutter build`, `flutter run` and `flutter test` all fail on Windows with
*"Building with plugins requires symlink support"* unless Developer Mode is
enabled. Enable it once:

```
start ms-settings:developers
```

This is a Flutter/Windows requirement for any project with plugins, not
something specific to this app.

### Toolchain quirks already handled

Every item below is fixed in the repository. They are recorded because none of
them is obvious from the error message, and several only appear on newer
toolchains than a given machine may have.

**libopus versus CMake 4 (all platforms).** `audiopus_sys` builds libopus from
source, and libopus declares `cmake_minimum_required` below 3.5, which CMake 4
refuses:

```
CMake Error at CMakeLists.txt:1 (cmake_minimum_required):
  Compatibility with CMake < 3.5 has been removed from CMake.
```

`CMAKE_POLICY_VERSION_MINIMUM=3.5` is set in **two** places, and both are
needed. `.cargo/config.toml` covers local builds. The CI workflow also sets it
as an environment variable, because cargo locates `.cargo/config.toml` by
walking up from the *working directory* rather than from `--manifest-path`, and
cargokit builds the iOS pod from an Xcode script phase that runs under
`DerivedData`, outside the repository, where the file is never found.

**Gradle 9 (Android).** cargokit's Gradle plugin calls `Project.exec()`, which
Gradle 9 removed; Flutter 3.44 scaffolds Gradle 9.1 with AGP 9. The task in
`rust_builder/cargokit/gradle/plugin.gradle` now injects `ExecOperations`.

**compileSdk (Android).** cargokit's `rust_builder` module pinned `compileSdk 33`
while the AndroidX libraries Flutter depends on require 34 or later, failing in
`checkReleaseAarMetadata`. Raised to 36.

**Audio frameworks (macOS and iOS).** cpal's CoreAudio backend emits
`cargo:rustc-link-lib=framework=` directives, but those do not survive into a
static library — Xcode links the `.a` and never sees them, so the app failed
with undefined `_AudioUnitRender` and friends. Both podspecs now declare the
frameworks explicitly.

**Deployment targets (macOS and iOS).** Left unset, libopus's C objects follow
the SDK while Rust targets a much older OS, and the linker complains the objects
are "built for newer 'iOS' version". The podspecs and workflow pin iOS 13.0 and
macOS 10.15 to match the Xcode projects.

The remaining two are Windows-specific, and neither shows up when you build the
Rust crate on its own with `cargo build`.

**CMake nested inside MSBuild.** `audiopus_sys` compiles libopus from source with
CMake, and under `flutter build windows` that CMake runs inside an MSBuild
invocation. Its Visual Studio generator then inherits the outer build's MSBuild
state and the compiler-probe project fails with

```
error MSB4018: The item metadata "%(FullPath)" cannot be applied to the path
"VCTargetsPath\x64\Debug\VCTargetsPath.tlog\ParallelCustomBuild.read.1.tlog"
```

`rust_builder/cargokit/cmake/cargokit.cmake` therefore sets
`CMAKE_GENERATOR=Ninja` **for WIN32 only**, which avoids MSBuild nesting
entirely. Ninja ships with the Visual Studio CMake component. The scoping is
deliberate — forcing Ninja on macOS, iOS or Android would break those builds.
Re-running `flutter_rust_bridge_codegen integrate` would overwrite this file.

**Debug/release CRT mismatch.** The `cmake` crate infers `CMAKE_BUILD_TYPE` from
`opt-level`, so a Flutter debug build compiled libopus at opt-level 0 against the
*debug* CRT (`/MDd`). Rust's MSVC target always links the release CRT, so the
link failed with `unresolved external symbol __imp__CrtDbgReportW`.
`app/rust/Cargo.toml` pins `opt-level = 2` for `audiopus_sys`, `opus` and
`nnnoiseless` in the dev profile. That resolves the mismatch and is correct
regardless: those run on every 10 ms block and are too slow unoptimised to hold
realtime.

### iOS and macOS

Those targets are configured (entitlements, `Info.plist`, background audio) but
can only be compiled on a Mac with Xcode.

### Android

`minSdk` is raised to 26 because cpal's Android backend uses AAudio via
`ndk::audio`. Building also needs the Android SDK and NDK.

## Testing

```bash
cd core && cargo test                                              # 109 tests
cd core && cargo test --test audio_hardware -- --ignored           # needs a mic
cd app  && flutter test                                            # 6 tests
```

The engine's test suite covers the parts that are hard to debug on a moving
motorcycle: OCB2 round-trips, replay and reorder handling, varint edge cases,
packet framing, the DSP chain's behaviour on rumble and on speech-over-rumble,
jitter-buffer loss concealment, resampler phase continuity across buffers, and
the reconnect policy's treatment of every disconnect reason.

`tests/audio_hardware.rs` is separate and `#[ignore]`d because it opens real
devices. It asserts the capture path keeps up with real time — roughly 100
encoded frames per 2 seconds — which is the one property no amount of offline
unit testing can establish.

## Signing and publishing

See **[docs/RELEASING.md](docs/RELEASING.md)** for the one-time setup: SignPath
for Windows code signing (free for open source), Apple certificates and App
Store Connect keys, and the Google Play upload key.

You generate every credential yourself and paste it in as an encrypted GitHub
secret — nothing in that process requires sharing account access with anyone.
Until the secrets exist, builds still succeed and simply come out unsigned,
with a notice in the job log saying so.

## Continuous integration

`.github/workflows/build.yml` runs on every push and pull request to `main`:

| Job | Runner | Produces |
|---|---|---|
| Tests | Ubuntu | `cargo fmt --check`, clippy, 109 engine tests, `flutter analyze`, widget tests |
| Windows | windows-latest | `mumbleway-windows-x64.zip` |
| macOS | macos-latest | `mumbleway-macos.zip` |
| iOS | macos-latest | `mumbleway-ios-unsigned.zip` |
| Android | Ubuntu | per-ABI APKs and an AAB |

The platform builds are gated on the test job. Pushing a `v*` tag additionally
publishes a GitHub release containing every artifact.

The hardware audio tests stay excluded in CI — the runners have no audio
devices, so they would fail for reasons unrelated to the code. Run them locally
before shipping a change to the capture path.

The iOS artifact is **unsigned**, because CI has no signing identity. It proves
the target compiles and links; installing it on a device needs re-signing with
your own provisioning profile.
