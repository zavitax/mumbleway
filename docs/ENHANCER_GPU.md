# Moving DeepFilterNet off the CPU

Queued 2026-08-10. Nothing here is built. It is written down with the
measurements that justify it and the two that argue against it, so whoever
picks it up starts from evidence rather than from the idea.

## Why

`core/src/audio/deepfilter.rs` runs DeepFilterNet 3 (low-latency) through
`tract`, which is **pure Rust and CPU-only — it has no GPU backend, and no flag
adds one.** Moving to the GPU means changing inference engine, not configuring
this one.

The cost, measured in release on a desktop x86 (`frame_cost`, `MW_CLIP` per
clip):

| Clip | mean | p99 | worst |
|---|---|---|---|
| ride, engine and wind | 0.54 ms | 2.75 | 4.05 |
| voice over music | 2.28 ms | 6.94 | **14.38** |
| ride, quiet, talking | 2.32 ms | 7.13 | **12.94** |

The budget is 10 ms a frame. **The worst frame already exceeds it on a
desktop.** What keeps it viable is the mean and the worker's buffering, so the
real requirement is a mean well under 10 ms — and on a low-end phone (OPPO A3s,
Snapdragon 450) the mean goes over and the enhancer switches itself off. That
was reported from the device and confirmed by the red **Enhancer** dot.

Stage skipping has already been tried and has nothing left to give: on a ride
with no speech, **97.3% of frames already run no decoder at all**.

## The two findings that argue against a GPU

Both are from this codebase, not from the literature.

1. **The Android GPU delegate killed the app on the phone that needs it.**
   TFLite's GPU delegate, used for the YAMNet classifier, segfaulted on an
   OPPO A3s (Adreno 506, Android 12):

   ```
   Fatal signal 11 (SIGSEGV), Cause: null pointer dereference
   #02 TfLiteInterpreterAllocateTensors+8  libtensorflowlite_jni.so
   ```

   It is a **native** crash, so the Dart `try`/`catch` around it never ran and
   the process simply died. That delegate has been removed.

2. **The offload would be partial.** TFLite's own log for YAMNet:
   `31 operations will run on the GPU, and the remaining 16 will run on the
   CPU`, because YAMNet computes its own mel spectrogram and no GPU delegate
   supports `RFFT2D` or `COMPLEX_ABS`. DeepFilterNet is GRU-based with complex
   spectral ops and is likely to split worse — and it is called **100 times a
   second**, so a CPU↔GPU round trip lands inside every 10 ms frame. Dispatch
   and transfer overhead of 1–3 ms per call would eat most of the win.

None of this makes it impossible. It makes "try it and measure" the only
honest plan, with the fallbacks built before the attempt.

## The design, as asked for

**Three rungs, in order, on every platform.**

1. **GPU.** Android: ONNX Runtime with the NNAPI execution provider, or a
   TFLite conversion with the GPU delegate. iOS: Core ML, which is the
   accelerator actually suited to small low-latency models.
2. **CPU.** The current `tract` path, unchanged. This is the fallback, not the
   thing being replaced — it works on five platforms and must keep working.
3. **Bypass.** The enhancer becomes a pass-through. **This rung already exists
   and is proven**: a hundred consecutive frames over 10 ms and `Enhancer`
   switches itself off for the session, which is what fired on the OPPO.

### Falling back from a rung that crashes

The ordinary `try`/`catch` shape does not work here, and finding 1 is why: the
first attempt took the whole process down, so there was no `catch` to run and
no second attempt to make.

So the GPU rung needs **a flag written to storage before the attempt and
cleared after it succeeds**. On start-up, a flag still set means the last
attempt did not return — do not try the GPU again on this device. Nothing else
survives a SIGSEGV.

That is the same shape as a browser's "safe mode after a crash", and it is the
only design that degrades rather than loops.

### Say when acceleration is not being used — on every platform

**Required, and not only for the GPU rung.** Today the panel says nothing at
all on macOS, where the shipped `libtensorflowlite_c` exports no Core ML or GPU
delegate symbols and everything runs on the CPU by construction. A rider
comparing two devices has no way to know why one is warmer than the other.

So: wherever the accelerated path was not built — refused, unavailable,
disabled by the crash flag, or simply absent on this platform — the diagnostics
panel says so in one line, with the measured per-frame cost beside it. The
classifier already does exactly this (`diagClassifierOnCpu`, which reports
milliseconds rather than warning about battery); the enhancer needs the same,
and the message has to be reachable on Windows and macOS too, not only where a
GPU was attempted and refused.

**Claim only what can be checked.** Core ML decides per operation whether to
use the Neural Engine, the GPU or the CPU and reports none of it. So the honest
statement is *the accelerated path was or was not built* — never "an NPU is
doing this".

## What to measure before believing any of it

- **Per-frame mean and p99 on a real phone**, both rungs, using `frame_cost`
  with `MW_CLIP`. The mean is the number that decides it; the tail is what the
  guard reacts to.
- **How many operations actually offload.** If it is another 31-of-47, the
  round trips will cost more than the compute saves at 100 calls a second.
- **That the output still matches.** A GPU path that is fast and different is
  not a win: run `dfn_enhance.py`'s comparison and check the speech-to-gap
  separation still improves by roughly the 14.5 dB measured on the CPU path.

## What is deliberately not in the plan

Replacing `tract`. Whatever happens with acceleration, the CPU rung stays: it
is the only path that cross-compiles to Android, iOS, macOS and Windows without
a per-platform native binary, and that property is why the enhancer lives in
`core` beside the chain instead of in Dart like the classifier.
