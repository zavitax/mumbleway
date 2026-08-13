# Handover — echo cancellation and the performance ladder

Written 13 August 2026, at the end of a session that ran out of room. This is
in-flight state, not a convention: **delete it once the work below is done**,
and do not let it become a second CLAUDE.md.

> **Corrected 13 August, later the same evening.** The first version of this
> file led with a divergence bug on `main` and a stash holding its fix. Both
> statements were false when they were written: the fix landed in `82c148c`,
> six commits before this file existed, and there is no stash. The file was
> written from a session summary instead of from `git log`, which is the one
> source that cannot be out of date about its own repository. **Check the
> claims below against the tree before acting on them** — that is the lesson
> this paragraph is here to pass on, and it applies to the rest of the file as
> much as it applied to the two sections now deleted.

---

## 1. What landed, so nobody goes looking for it

`31e8566` shipped the aligner and a 1024-tap filter, and with it a divergence:
`ref_power` is maintained incrementally, drifts low over millions of `f32`
operations, and normalises the NLMS step — so a total that has drifted low makes
every step too large. Measured at a 120 ms delay it ended up **30.9 dB worse
than doing nothing**.

**`82c148c` fixed it**, along with two others found by the same delay sweep:

- **`audit()`** — recomputes `ref_power` exactly once per block and backtracks
  to the last measurably-working coefficients when output exceeds input by 9 dB.
- **A confident alignment pointing at nothing.** Arrivals 5 ms and 60 ms apart
  made the envelope correlator report their *centroid* with a correlation of
  1.00 — a delay at which nothing was ever emitted. It now detects arrivals
  spread wider than the filter reaches and aims at the earliest real one.
- **Hysteresis** (`MOVE_MARGIN`), because two comparable arrivals otherwise made
  the alignment flap every second and the filter never converged.

**The two-arrival case was resolved by not growing the filter.** Growing it to
4 096 taps measured 2.9 dB for four times the arithmetic, because a time-domain
NLMS normalised by a single total power converges badly over a long span on
coloured input. `_WHY_NO_GROWTH` in `aec.rs` records why, and why both
production cancellers are frequency-domain instead. The test is
`cancels_the_nearer_of_two_arrivals`: 2.15 dB is the whole of what is available
to a filter that can only be in one place, and it takes 1.6 of them.

---

## 2. Measured on the OPPO, 13 August

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
nothing.

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

## 3. Outstanding

**Nothing, on paper.** Every ask this file was written to hand over has landed;
what is left is the one thing a keyboard cannot do, in §5. The rest of this
section is what the work turned out to be, for whoever has to change it next.

### 3.1 `Stage::Echo` — the canceller has its own clock

`CaptureProcessor::process_with_reference` is now `cancel_echo` followed by
`suppress`, split for no reason except that the worker can put a stopwatch
between them. Eight stages tile the block instead of seven.

Everything below needed this: with the canceller's time folded in with six
other stages, "blocks are late because of the AEC" is not a sentence anything
could evaluate.

### 3.2 The AEC is constrained to the block budget

`relief::AecCut`, entered only when the ladder has nothing left **and** the run
of late blocks would have fitted without the canceller. Filter goes 512 → 384 →
256 → 128 and stops.

Three things worth knowing about the shape:

- **It is not a set of rungs, and could not be.** Every `Relief` rung costs the
  same thing on every device; the canceller costs 16 µs or 970 depending on
  whether somebody else is talking. `AecCut`'s doc comment carries the argument.
- **It stops at 128 taps rather than reaching zero.** `Relief::ShortAec` already
  argued this: losing echo cancellation on a speakerphone is a howl, and the
  feedback guard that would cover for it goes eleven rungs earlier. The
  originally sketched "last rung: off" would have created the fault it was
  standing next to. Shortening is a different trade — the aligner has already
  pointed the filter at the direct arrival, so what is given up is the tail.
- **Attribution is on the run, not the block.** A hundred blocks late for other
  reasons and one late because of the canceller is not evidence about the
  canceller.

**A superseded instruction, and why.** The original form was "if AEC measures
4 ms or more during the startup probe, switch to half mode, then 750 ms, then
500, then 250, then off". No AEC configuration reaches 4 ms on this device —
full length is 970 µs — so that cascade would never have been entered. Also,
750 ms cannot mean taps: 36 000 taps is ≈34 ms per 10 ms block.

**Still worth a decision from a human.** The tail only fires at the bottom of
the ladder, where the enhancer is already off and a block costs a fraction of a
millisecond — so on any device that is not extraordinarily slow it will never
run. That is what was asked for and it is the right safety net. The *larger*
fault it does not fix is one line up: while the ladder still has rungs, a phone
that is late only because the far end started talking pays for it by selling
the enhancer, the pitch search and RNNoise — none of which is the cause.
`Stage::Echo` now makes preferring the cut over the rung possible. It would be
a real change in ladder behaviour, so it is not made here.

### 3.3 The model is timed against its own ceiling

`probe::MODEL_CEILING_US`, 4 ms. Over it, the cheap model is loaded before the
ladder is walked at all, through `deepfilter::simple_model_wanted` — the same
mechanism single-core devices already used, which is why the worker needed no
new message to act on it. **It fires on the OPPO** (6.93 ms) and on nothing on
a development machine.

Two decisions inside it that will look arbitrary later:

- **The minimum block, not the mean or the second-worst.** Every other figure in
  `probe.rs` is a worst case, because it is asking about deadlines. This asks
  how expensive the arithmetic is, and every error in a wall-clock measurement
  of arithmetic *adds* time. Written as a mean first, and it fired on a machine
  whose model costs 1.9 ms, during a parallel test run — which is precisely the
  "too slow versus merely busy" confusion that got the per-core CPU condition
  deleted.
- **`measure_ladder` does not touch the static; `probe` does.** The existing
  split for `record_rung`, for the same reason.

### 3.4 Windows ships a build that runs

`publish.yml` zips `build/windows/x64/runner/Release` before `msix:create` runs
— checked, not assumed, since that step writes into the same folder — and the
release job now collects `*.zip` alongside the packages. Neither MSIX could be
opened by a tester: the store one is unsigned because Partner Center re-signs
it, and the sideload one needs its certificate trusted first.

The ARM64 job is untouched. It is `if: vars.ENABLE_WINDOWS_ARM64 == 'true'` and
blocked upstream — there is no Arm64 Flutter SDK for Windows.

---

## 4. Landed earlier, recorded here so it is not looked for twice

- **Diagnostics counters and the echo dot.** `aec_erle_db`, `aec_lag_ms`,
  `aec_confidence`, `aec_spread_ms`, `aec_window_ms` and `aec_shortened` cross
  the bridge and are on the panel. There is deliberately no "bypassed" state:
  §3.2 never bypasses, and `aec_window_ms` shows exactly how short the filter
  has become, which is the more useful of the two things a dot could say.
- **Dot ordering.** `spectrum_view.dart` lists them in the order the chain runs
  them: enhancer, echo, suppressor, voice, gate, level, feedback, hiss,
  background, transmit.
- **Three `.arb` keys were defined twice**, found while adding a fourth row to
  the same table. `diagStageEnhancer`, `diagStageFeedback` and
  `diagStageTransmit` each named a block-cost row *and* a chain dot, with
  deliberately different wording; JSON keeps the last, so the dot labels won and
  the cost table had been showing **To the server** for the row that times the
  onset delay and the decision log. The site's own documentation had copied the
  wrong labels out of a screenshot. The two lists are now `diagCost*` and
  `diagStage*`, and `widget_test.dart` fails on a repeated key — which the
  existing coverage test structurally could not see, because it reads the files
  through `jsonDecode`.

---

## 5. Still true, still unverified

**None of the echo work has run on a device.** The fault came from two real
phones — an iPhone and an Android, each hearing itself through the other — and
everything proving the fix is synthesised: synthetic wind, a four-tap room, and
a delay chosen by hand. The quantity the whole design turns on, the real
tap-to-speaker latency on those two phones, has never been measured.

The panel now carries the numbers that would settle it — and, since §3.1, an
**echo** row in the block-cost table that reads near zero on a quiet call and
lifts when the other end talks. Two phones, one call, the panel open on both:
that is the whole of what is left, and it is worth more than everything above
it put together.

**Then delete this file.** It has done its job.
