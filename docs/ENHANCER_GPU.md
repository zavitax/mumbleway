# The enhancer on a low-end phone

Written 2026-08-10 as a plan to move DeepFilterNet onto the GPU. **The plan was
wrong and the measurements are why.** Both are kept: the reasoning that led to
the GPU is the same reasoning anybody else will arrive at, and the numbers that
killed it are the useful part.

**What shipped instead is an effort ladder** — `Effort` in
`core/src/audio/deepfilter.rs`.

---

## The measurement that changed the answer

The enhancer is pure Rust, so it can be cross-compiled for
`aarch64-linux-android` and run on a phone **with no APK at all**. That matters
here more than usual: Play Protect refuses a locally built APK on this machine,
so reaching the physical device otherwise means a Play release — half an hour
per number.

`tools/dfbench` is that binary.

```powershell
$ndk = "C:\Android\sdk\ndk\27.0.12077973\toolchains\llvm\prebuilt\windows-x86_64\bin"
$env:CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER = "$ndk\aarch64-linux-android26-clang.cmd"
cd tools\dfbench; cargo build --release --target aarch64-linux-android
adb push target\aarch64-linux-android\release\dfbench /data/local/tmp/
adb shell chmod 755 /data/local/tmp/dfbench
adb push C:\ml_data\rides\20260810-1040-000.raw /data/local/tmp/clip.raw
adb shell /data/local/tmp/dfbench /data/local/tmp/clip.raw
```

On the OPPO A3s (Snapdragon 450, eight Cortex-A53 at 1.8 GHz) — the device that
reported the fault:

```
1311 frames  mean 6.23 ms  p50 7.87  p95 8.10  p99 8.32  worst 9.27
stage            share    mean ms
  zero-mask       30.3%     2.32
  both decoders   69.7%     7.92
over the 10 ms block budget: 0 frames (0.0%)
```

**Not one frame over budget.** The model fits on the phone that could not run
it. What does not fit is the model *plus* RNNoise plus the profile filters plus
the encoder, on one A53 core, inside the same 10 ms.

The old guard could not see that. The enhancer carried the only stopwatch in
the chain, so when blocks ran late it was the only stage that could be blamed —
and it switched itself off for the session on that evidence. Every stage
carries a clock now (`core/src/audio/timing.rs`), and the panel shows where a
block's time goes, with an `unattributed` row for the part no stage was timing.

---

## Why the GPU was the wrong lever anyway

Independent of the above, and worth keeping because it is not obvious.

**The graph will not offload.** The three ONNX models are 483 nodes:

| | enc | erb_dec | df_dec |
|---|---|---|---|
| nodes | 223 | 141 | 119 |
| weights | 0.6 MB | 0.2 MB | **19.2 MB** |
| GRU | 1 | 2 | 3 |

The compute is **6 GRUs, 22 Convs, 8 Einsums**; roughly 380 of the 483 nodes are
shape glue — `Constant`, `Reshape`, `Shape`, `Gather`, `Slice`, `Concat`,
`Unsqueeze`, `Cast`, `Pad`. Neither ONNX Runtime's NNAPI provider nor TFLite's
GPU delegate implements `GRU` or `Einsum`, so the graph would partition into
islands around those six GRUs with a copy at every boundary — **a hundred times
a second**. The same partitioning gave 31-of-47 on YAMNet, a model called once
every two seconds.

**And there is no bandwidth to win.** `df_dec` is 19.2 MB of the 20 MB total:
three GRUs of hidden size 512. At one frame per call a GRU step is
matrix-by-*vector*, about two flops per byte loaded, so it is memory-bound. A
mobile GPU shares the same memory bus as the CPU. The bottleneck is bytes, and
the GPU moves the same bytes.

**And it crashed last time.** TFLite's GPU delegate segfaulted inside
`TfLiteInterpreterAllocateTensors` on this exact device — native, so the Dart
`catch` never ran and the process simply died.

---

## What shipped: an effort ladder

Four rungs, one step per hundred consecutive missed deadlines, measured rather
than guessed. The rung sets `max_db_df_thresh` — a public field on `DfTract`, so
a step costs one assignment with no rebuild and no allocation.

| Rung | `max_df` | Cost on the A3s | Worst clip | Voice over music |
|---|---|---|---|---|
| Full | 20 | 6.29 ms | 27.0 dB | 14.1 dB |
| Reduced | 0 | 4.62 ms | 24.2 dB | **15.7 dB** |
| ERB only | −15 | 4.34 ms | 22.1 dB | 15.9 dB |
| Bypassed | — | 0 | — | — |

Separation is speech-to-gap in dB across the ride corpus, measured by
`dfbench --log <the .csv>`.

**Stepping down is not purely a loss.** On voice over music — the clip this
model was adopted for — `Reduced` separates *better* than `Full`. The DF decoder
takes **11.5 dB out of the speech** at full effort and 9.8 dB at reduced: it is
the speech being eaten, not the music surviving. That is a measured account of
*"speech gets choppier when Auto switches to Helmet"*.

It is still a loss on quieter material — 27.0 dB to 24.2 on one ride — so this
is a degradation path and **not a new default**.

It only ever falls. Climbing back would be settled by the same measurement that
pushed it down, so a device on the edge would oscillate, and every change of
rung is audible.

### The panel says which rung, on every platform

Required, and the honest form of the original "say when acceleration is not in
use". A rider comparing two phones has no other way to tell why one sounds
different — every other number on the panel reads the same afterwards. Amber
for the rungs that still enhance, red for bypassed, and **nothing at all at
full effort**: a note that says "working normally" on every device teaches
people to stop reading the panel.

---

## What is still open

- **The ladder has not been seen to fire on the device.** Every rung is
  measured, and the stepping is unit-tested, but the phone has not yet run a
  build with it. That needs a Play release and a ride.
- **`MIN_DB`.** The zero-mask branch zeroes rather than attenuates, and 85% of
  a clean ride takes it. Untested, and the leading remaining candidate for
  choppiness.
- **f16 weights** would halve `df_dec`'s 19.2 MB and are the only remaining
  idea that attacks the bottleneck directly. Note that Cortex-A53 is ARMv8.0
  and has **no half-precision arithmetic** — only conversion — so this could
  easily be slower there. Measure with `dfbench` before believing it, and note
  that it needs `libDF` vendored, because the functions that build the models
  are private.
