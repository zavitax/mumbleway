# cpal 0.18.1, with one hunk changed

Upstream: <https://github.com/RustAudio/cpal>, Apache-2.0 / MIT. The licence
file beside this one is theirs and unmodified.

A verbatim copy of the published `cpal` 0.18.1 with a single change, in
`src/host/aaudio/mod.rs` and the feature list in `Cargo.toml`. To see exactly
what differs:

```bash
cargo download cpal==0.18.1     # or unpack it from ~/.cargo/registry
diff -ru <pristine> third_party/cpal
```

## What it adds

**Which microphone Android gives us when another app wants it too.**

cpal never calls `AAudioStreamBuilder_setInputPreset`, so every capture stream
opens with AAudio's default of `VOICE_RECOGNITION`. From Android 10 two apps
may capture at once: the loser is handed digital silence rather than an error,
and only `VOICE_COMMUNICATION` and `CAMCORDER` are treated as privacy
sensitive — the presets that stop a second app capturing alongside. On the
default preset a navigation app listening for voice commands can take the
microphone, and nothing in the stream distinguishes that from a quiet room.

The patch adds `ANDROID_INPUT_PRESET`, a global the app sets before opening the
device. Zero means "leave it alone", which is exactly upstream's behaviour, so
the copy is inert until something asks.

## What it costs, and why it is behind a feature

`AAudioStreamBuilder_setInputPreset` and `AAudioStream_getInputPreset` arrived in
**API 28** and `ndk` gates both behind `api-level-28`, where cpal asks for
`api-level-26`. Checked against the NDK's own stubs rather than assumed:

| stub | `AAudioStreamBuilder_setInputPreset` |
|---|---|
| API 26 | absent |
| API 27 | absent |
| API 28 | present |

So a build at a lower `minSdk` fails to link, and one that links against 28
produces a library Android 8 (API 26–27) refuses to load (`UnsatisfiedLinkError`).
`dlsym`-ing around it is not available either: in `ndk` 0.9, both
`ndk::audio::AudioStreamBuilder` and `ndk::audio::AudioStream` keep their raw
pointers private (neither exposes a public `as_ptr()`), so there is no way to
reach the underlying AAudio pointers from outside the `ndk` crate without
vendoring `ndk` or replacing `ndk::audio` with raw `ndk-sys`.

**`android-input-preset` is therefore off by default in `cpal`**, and turning it
on is a statement that the app requires API 28. MumbleWay's `core/Cargo.toml`
enables `android-input-preset`, and `app/android/app/build.gradle.kts` raises
`minSdk` to 28 (`maxOf(flutter.minSdkVersion, 28)`) to match.

## This is already proposed upstream

**Do not write a competing pull request.** `RustAudio/cpal#995` — "feat(android):
add `input_preset` to `StreamConfig`" — has been open since 2025-07-31 and adds
the same capability, behind an `android-input-preset` feature pulling
`ndk/api-level-28`, which is the same answer to the same API-level problem. A
maintainer has since said it is "a candidate for opt-in with #1010 together with
the other audio input presets", so the final shape belongs to that larger
`StreamConfigBuilder` change.

This copy is deliberately the smallest diff that works rather than a guess at
that API, so it can be deleted outright when either lands. When it does, replace
`ANDROID_INPUT_PRESET` with whatever upstream settled on and drop the
`[patch.crates-io]` entry in `core/Cargo.toml`.

## Not verified on a device

It compiles for `aarch64-linux-android` with and without the feature. Nothing
here has been run on a phone, and the thing it is for — whether
`VOICE_COMMUNICATION` actually stops another app taking the microphone — can
only be answered by one. Note also that the preset switches on the platform's
own AEC, noise suppression and AGC on most devices, which this project already
does for itself; `CLAUDE.md` records the hardware canceller fighting ours as a
known hazard.
