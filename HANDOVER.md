# Handover — echo cancellation and the performance ladder

Written 13 August 2026, at the end of a session that ran out of room. This is
in-flight state, not a convention: **delete it once the work below is done**,
and do not let it become a second CLAUDE.md.

Read this order: the bug on `main` first, then the stash, then the asks.

---

## 1. There is a correctness bug on `main` right now

`31e8566` shipped the aligner and a 1024-tap filter. It also shipped a
divergence bug, found later in the same session and **fixed only in the stash**:

`EchoCanceller::ref_power` is maintained incrementally — one add and one
subtract per sample — because recomputing 1 024 squares per sample would cost
more than the filter does. Over a call that is millions of `f32` operations
against a running total, and the error accumulates. `ref_power` normalises the
NLMS step, so a total that has drifted *low* makes every step too large.

Measured: at a 120 ms echo delay the canceller ended up **30.9 dB worse than
doing nothing** — it stopped subtracting and started adding. Audibly that is a
rising roar, worse than the echo it replaced. It arrives quietly, after minutes
of working correctly, which is why no earlier test saw it.

The fix in the stash is `audit()`, run once per block: recompute `ref_power`
from the ring exactly, and forget the learned path if output power exceeds
input by 9 dB. 1 024 multiply-adds against the ~1 000 000 the filter already
does in that time.

**Do not leave this on `main` longer than necessary.**

---

## 2. What is in `stash@{0}`

```
git stash list
git stash pop
```

Contains, against `core/src/audio/aec.rs` and `core/tests/echo_alignment.rs`:

- **`audit()`** — the `ref_power` recompute and divergence guard above.
- **Earliest-near-best lag selection.** Prefer the earliest lag scoring within
  95% of the best, because sound cannot arrive before it was made. Without it a
  25 ms delay (which falls between the 10 ms search bins) lost to a spurious
  self-similarity match at 300 ms.
- **Strongest-peak fallback.** When the accepted arrivals spread further than
  `MAX_TAPS`, take the strongest alone. Without it, 250 ms and 400 ms delays
  aimed the filter at the first 85 ms of a path whose echo is a quarter-second
  later — reporting an alignment and cancelling nothing.
- **`finds_the_echo_at_every_plausible_delay`** — the sweep that found all
  three, across 0, 10, 25, 60, 120, 250, 400 and 600 ms.

With these, the delay sweep goes from 6/8 to 8/8 on cancellation, and the voice
sweep tightens (120 ms echo found at lag 110 + span 21, against 80 + 85 before).

### The one thing blocking that stash

**`cancels_an_internal_copy_and_its_acoustic_twin` regresses to 2.9 dB.** The
filter does grow — the first assertion passes — but cancellation collapses.
2.5× more convergence time changed nothing, so it is not settling time. The
earliest-preference rule and the multi-arrival span are fighting each other and
it needs diagnosing rather than tuning.

That is the first task for whoever picks this up.

---

## 3. Measured on the OPPO, 13 August

Both harnesses cross-compile and run on the device. Numbers are means over
20 s; **ignore the worst-case columns**, they are a phone doing other things.

### Whole chain — `cargo test --test chain_cost`

```
enhancer: Full          1316 blocks, Helmet profile
enhancer             6.929 ms    88.8%
suppression          0.405 ms     5.2%
feedback             0.109 ms     1.4%
encode               0.361 ms     4.6%
whole block          7.808 ms   (78.1% of the 10 ms budget)
```

**Caveat that matters:** `chain_cost.rs:116` passes an *empty* echo reference,
so the canceller takes its idle shortcut and this run measures it doing
nothing. It also folds the AEC into `suppression` with six other stages.

### The canceller alone — `cargo test --test aec_cost`

```
idle (nothing playing)          16 µs
far end talking, 1024 taps     970 µs

taps   covers     mean µs
1024   21.3 ms      970
 768   16.0 ms      723
 512   10.7 ms      447
 384    8.0 ms      343
 256    5.3 ms      235
 128    2.7 ms      128
```

Linear at **≈0.95 µs per tap per block**, no cliff. Every rung can be read off
this line.

**The consequence:** the enhancer leaves ~900 µs of the block, and 1024 taps
wants 970 µs of it. On this phone the chain does not fit while the far end is
speaking. The idle shortcut means a headset call still pays nothing.

### Running them again

```powershell
$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"
$shim = "<scratchpad>"   # cmake-shim.cmd + android-arm64.cmake live here
$ndk  = "C:\Android\sdk\ndk\27.0.12077973\toolchains\llvm\prebuilt\windows-x86_64\bin"
$env:PATH = "$ndk;C:\Android\sdk\cmake\3.22.1\bin;$env:PATH"
$env:CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER = "$ndk\aarch64-linux-android26-clang.cmd"
$env:CC_aarch64_linux_android = "$ndk\aarch64-linux-android26-clang.cmd"   # cc-rs needs this
$env:AR_aarch64_linux_android = "$ndk\llvm-ar.exe"
$env:CMAKE_aarch64_linux_android = "$shim\cmake-shim.cmd"
$env:CMAKE_TOOLCHAIN_FILE_aarch64_linux_android = "$shim\android-arm64.cmake"
$env:CMAKE_GENERATOR_aarch64_linux_android = "Ninja"
cargo test --release --target aarch64-linux-android --test aec_cost --no-run
adb push target\aarch64-linux-android\release\deps\aec_cost-<hash> /data/local/tmp/aec_cost
adb shell chmod 755 /data/local/tmp/aec_cost
adb shell "/data/local/tmp/aec_cost --ignored --nocapture"
```

`CC_aarch64_linux_android` is the addition to what CLAUDE.local.md records —
without it `tract-linalg`'s build script fails looking for
`aarch64-linux-android-clang`, which is not a filename the NDK ships.

---

## 4. Outstanding, in the order they unblock each other

### 4.1 Split the AEC out of `Stage::Suppression` — do this first

Three other asks need it and none can be done without it. The canceller is
step 0 inside `process_with_reference`, so its time is currently indivisible
from the rumble filter, RNNoise, the pitch search, the gate, AGC and limiter.

Needed for: attributing lateness to the AEC, the counters, and the dot.

### 4.2 Constrain the AEC to the block budget

The rule as given: **once the ladder is walked, if blocks are still late and
the AEC is the cause, start reducing taps.** From the measured line each step
returns a known amount:

| step | taps | covers | returns |
|---|---|---|---|
| 1 | 768 | 16.0 ms | ~250 µs |
| 2 | 512 | 10.7 ms | ~520 µs |
| 3 | 256 | 5.3 ms | ~735 µs |
| 4 | 128 | 2.7 ms | ~840 µs |
| last | off | — | ~970 µs |

Stepping down costs echo *tail*, not the echo itself: the aligner already
points the filter at the direct arrival, so 128 taps still cancels the loud
part. That is what makes this a defensible last resort.

**A superseded instruction, and why.** The original form was "if AEC measures
4 ms or more during the startup probe, switch to half mode, then 750 ms, then
500, then 250, then off". The measurement says no AEC configuration reaches
4 ms on this device — full length is 970 µs — so that cascade would never be
entered. The budget-driven form above replaced it. Also note that 750 ms cannot
mean taps (36 000 taps ≈ 34 ms per 10 ms block); the history and search windows
that *could* be meant cost nothing measurable.

### 4.3 DF3 to the cheap model at ≥4 ms on the startup probe

Straightforward and unblocked. **It fires on the OPPO every time** — DF3 at
Full measures 6.93 ms — so expect this to change behaviour on that device
immediately, and to be the change that actually buys the budget back.

### 4.4 Diagnostics: counters and a dot for the AEC

Counters wanted: ERLE, alignment lag, confidence, filter span. `alignment()`
and `filter_span_ms()` exist in Rust and reach nothing — no FFI, no UI. Needs
`flutter_rust_bridge_codegen generate` and l10n keys in **both** arb files.

The dot should also show *bypassed by the ladder*. **There is no such state
today**: `relief.rs` has `skip_pitch`, `skip_feedback`, `skip_rnnoise` and the
rest, and no `skip_aec`. 4.2 is what creates the state for the dot to show.

### 4.5 Dot ordering is wrong — confirmed

`guard.process` is `engine.rs:2990`, de-hiss is `2999`. **Feedback runs before
de-hiss and the UI lists hiss first.** Correct order:

```
enhancer, echo, suppressor, voice, gate, level, feedback, hiss, background, transmit
```

`spectrum_view.dart:1173-1182`.

### 4.6 Windows: ship a runnable EXE zip beside the MSIX

Not started. Published Windows builds should attach a zip containing a portable
EXE, in addition to the MSIX, so a build can be run locally without installing.
`.github/workflows/publish.yml`.

---

## 5. Still true, still unverified

**None of the echo work has run on a device.** The fault came from two real
phones — an iPhone and an Android, each hearing itself through the other — and
everything proving the fix is synthesised: synthetic wind, a four-tap room, and
a delay chosen by hand. The quantity the whole design turns on, the real
tap-to-speaker latency on those two phones, has never been measured.

4.4 is what makes the next two-phone call produce evidence instead of an
impression. Doing it before the next publish is worth more than it looks.
