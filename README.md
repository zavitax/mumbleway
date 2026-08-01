# MumbleWay

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

Two further Windows-specific fixes are already applied; they are recorded here
because both are non-obvious and neither shows up when you build the Rust crate
on its own with `cargo build`.

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
