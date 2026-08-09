---
layout: default
ref: licences
title: Licences
description: MumbleWay's own licence, and every third-party component it is built from.
---

## MumbleWay

**GNU General Public License, version 3.** The full text is
[in the repository]({{ site.repo }}/blob/main/LICENSE).

You may use, study, modify and redistribute it. If you distribute a modified
version, it has to carry the same licence and its source has to be available.

<div class="panel">
<p><strong>Not affiliated with the Mumble project.</strong> MumbleWay is an
independent client that speaks the Mumble protocol. The Mumble name and its
trademarks belong to the Mumble project, and this app is neither endorsed by
nor supported by them. Please report problems with this app
<a href="{{ site.repo }}/issues">here</a> and not to them.</p>
</div>

## The protocol

The Mumble protocol is documented and implemented independently here from those
descriptions. Mumble itself is distributed under a **BSD 3-Clause** licence.
See [mumble.info]({{ site.mumble }}).

## Audio

The parts that do the actual work.

<div class="table-wrap" markdown="1">

| Component | Role | Licence |
|---|---|---|
| [Opus](https://opus-codec.org/) | Voice codec, via the `opus` and `audiopus_sys` crates | BSD 3-Clause |
| [RNNoise](https://jmvalin.ca/demo/rnnoise/), as [`nnnoiseless`](https://crates.io/crates/nnnoiseless) | Neural noise suppression and voice activity detection | BSD 3-Clause |
| [`cpal`](https://crates.io/crates/cpal) | Cross-platform audio device access | Apache-2.0 |
| [`dasp_sample`](https://crates.io/crates/dasp_sample) | Sample format conversion | MIT / Apache-2.0 |
| [YAMNet](https://github.com/tensorflow/models/tree/master/research/audioset/yamnet) | The sound classifier that lets *Automatic* hear an engine and choose the helmet profile. Shipped as `assets/models/yamnet.tflite` | Apache-2.0 |
| [LiteRT / TensorFlow Lite](https://ai.google.dev/edge/litert) | Runs that model. From Google's own Maven and CocoaPods on Android and iOS; the universal `libtensorflowlite_c` inside `tflite_flutter` on macOS | Apache-2.0 |
| [`tflite_flutter`](https://github.com/tensorflow/flutter-tflite) | The Dart binding, vendored under `app/third_party` and patched in one line so its macOS library loads from `Contents/Frameworks`, where Apple requires it | Apache-2.0 |

</div>

Everything else in the capture chain — the echo canceller, gate, expander,
spectral subtractor, limiter, AGC, feedback guard, pitch tracker and jitter
buffer — is written for this project and covered by its own GPL v3.

## Rust

<div class="table-wrap" markdown="1">

| Component | Role | Licence |
|---|---|---|
| [`tokio`](https://tokio.rs/), `tokio-rustls` | Async runtime and TLS transport | MIT |
| [`rustls`](https://github.com/rustls/rustls), `rustls-pemfile`, `webpki-roots` | TLS, without OpenSSL | Apache-2.0 / MIT / ISC |
| [`rcgen`](https://crates.io/crates/rcgen) | Generates the client identity certificate | MIT / Apache-2.0 |
| [`prost`](https://crates.io/crates/prost) | Protocol Buffers, for Mumble's control channel | Apache-2.0 |
| [`aes`](https://crates.io/crates/aes), `aes-gcm`, `cipher` | OCB/AES for Mumble's UDP voice encryption | MIT / Apache-2.0 |
| [`sha2`](https://crates.io/crates/sha2), `hex` | Certificate fingerprints | MIT / Apache-2.0 |
| [`serde`](https://serde.rs/), `serde_json` | Settings and server list serialisation | MIT / Apache-2.0 |
| [`tracing`](https://crates.io/crates/tracing) | Structured logging | MIT |
| [`parking_lot`](https://crates.io/crates/parking_lot) | Locks used on the audio path | MIT / Apache-2.0 |
| [`rand`](https://crates.io/crates/rand) | Nonces and jitter | MIT / Apache-2.0 |
| [`anyhow`](https://crates.io/crates/anyhow), [`thiserror`](https://crates.io/crates/thiserror) | Error handling | MIT / Apache-2.0 |
| [`bytes`](https://crates.io/crates/bytes), [`url`](https://crates.io/crates/url) | Buffers and URL parsing | MIT / Apache-2.0 |

</div>

## Flutter and Dart

<div class="table-wrap" markdown="1">

| Component | Role | Licence |
|---|---|---|
| [Flutter](https://flutter.dev/) and the Dart SDK | Application framework | BSD 3-Clause |
| [`flutter_rust_bridge`](https://cjycode.com/flutter_rust_bridge/) | The bridge between the Dart UI and the Rust engine | MIT |
| [`shared_preferences`](https://pub.dev/packages/shared_preferences), [`path_provider`](https://pub.dev/packages/path_provider), [`share_plus`](https://pub.dev/packages/share_plus), [`package_info_plus`](https://pub.dev/packages/package_info_plus), [`file_selector`](https://pub.dev/packages/file_selector) | Platform plumbing | BSD 3-Clause |
| [`http`](https://pub.dev/packages/http), [`intl`](https://pub.dev/packages/intl) | Networking and localisation | BSD 3-Clause |
| [`qr_flutter`](https://pub.dev/packages/qr_flutter), [`qr`](https://pub.dev/packages/qr) | Renders server invitations | BSD 3-Clause |
| [`mobile_scanner`](https://pub.dev/packages/mobile_scanner) | Scans them back | BSD 3-Clause |
| [`image`](https://pub.dev/packages/image), [`archive`](https://pub.dev/packages/archive) | Image handling and the diagnostic archive | MIT |
| [`flutter_svg`](https://pub.dev/packages/flutter_svg) | Vector artwork | MIT |
| [`freezed`](https://pub.dev/packages/freezed) | Code generation | MIT |

</div>

## Typeface

**[Exo 2](https://fonts.google.com/specimen/Exo+2)** by Natanael Gama, under the
**SIL Open Font License 1.1**. Used in the app and on this site.

This site also sets **[Atkinson
Hyperlegible](https://www.brailleinstitute.org/freefont/)** (Braille Institute,
SIL OFL 1.1) and **[IBM Plex
Mono](https://www.ibm.com/plex/)** (IBM, SIL OFL 1.1).

## TEN VAD — used in research, not shipped

<div class="panel warn">
<p><strong>TEN VAD is not part of the app.</strong> It is not linked into any
build and no release contains it. It lives in
<a href="{{ site.repo }}/tree/main/tools/vad"><code>tools/vad/</code></a>, the
offline tooling used to evaluate voice activity detection against recorded
helmet audio, and it is credited here because it was used in developing the
app even though it is not distributed with it.</p>
</div>

Two things worth knowing if you pick it up from that tooling, both recorded in
[`tools/vad/README.md`]({{ site.repo }}/blob/main/tools/vad/README.md):

- It is **"Apache 2.0 with additional conditions"**, not plain Apache 2.0. The
  additional conditions are real and you should read the upstream `LICENSE`
  rather than assume Apache terms.
- Its `pitch_est.cc` carries **BSD-2 and BSD-3** code from
  [LPCNet](https://github.com/xiph/LPCNet).

Upstream: [TEN-framework/ten-vad](https://github.com/TEN-framework/ten-vad).

The evaluation itself carries a **retraction** at the top of that README — the
measurements behind it were made on audio from the phone's own microphone
rather than a headset boom microphone, and two conclusions were withdrawn as a
result. It is left in place, and marked, rather than deleted.

## Accuracy of this page

Licences are stated from each project's own published terms and are believed
correct, but this page is a summary and not a legal document. **The
authoritative text is the one distributed with each package.** Several Rust
crates are dual-licensed MIT *or* Apache-2.0, at your option, and are marked
"MIT / Apache-2.0" above.

Found something wrong or missing? [Open an issue]({{ site.repo }}/issues) — a
licence attribution error is a bug and will be fixed.
