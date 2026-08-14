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

## 5. It ran on a device, and the device disagreed

**14 August, build 122** — an iPhone alone in a room with its speaker and
microphone open, a second phone in an acoustically isolated room doing all the
talking. Every sound in that recording is echo. `20260814-0108-000` in the
corpus.

Build 122 is `da2239c`, so the aligner, the divergence fix and everything in
§1–§3 were all in it. The echo came back anyway:

```
588 blocks with the microphone over -35 dBFS   (all of it echo)
520 of them transmitted                         88.4%
median   mic -16.8 dB  ->  sent -45.5 dB
loudest  mic -10.1 dB  ->  sent -19.0 dB
```

**The variance is the finding, not the average.** Three seconds of one
recording, one room, a phone that did not move:

```
10.50 s   mic -12.2  sent -48.4    36.2 dB removed
11.50 s   mic  -8.5  sent -36.4    27.9 dB
15.50 s   mic  -8.5  sent -20.9    12.4 dB
```

An adaptive filter does not lose 24 dB on a path that has not moved. The
reference is moving underneath it.

### Two causes, one fixed here

**The enhancer was in front of the canceller.** Fixed: `cancel_echo` now runs
before `enhancer.process`. NLMS learns a *linear time-invariant* map, which is
the only reason a thousand taps can describe a room; DeepFilterNet is a
per-frame gain mask that returns exact zeros below `MIN_DB`. In front of the
filter there is no fixed set of taps that models what it sees, and on the
zero-mask frames the canceller reads zero error as perfect cancellation and
snapshots a filter that is doing nothing. Every production canceller runs ahead
of noise suppression for this reason. **The comment that used to sit at that
line said the ordering was "worth watching if echo behaviour changes"** — it
changed, and that was the answer.

**The echo reference has no time base. Not fixed, and — measured on build 123 —
not the cause either.** The suspicion was that the FIFO's silences and clears
were moving the alignment. `echo_ref_samples` says otherwise: of the loud echo
blocks, **99.4% had a full 480-sample reference** and 100% had one within the
preceding 250 ms. The reference was there. The blocks reading zero are far-end
silence, which is correct.

It still has to be fixed, but as a *prerequisite* rather than a cure — see §6.

### 6. What build 123 actually measured, 14 August

Reordering the canceller ahead of the enhancer was necessary and did not fix
it. With a good reference and a confident alignment, on 1 776 loud echo blocks:

```
erle_db          p10 -6.00   median -0.20   p90  6.30      <- removing nothing
aec_confidence   p10  0.91   median  0.96   p90  0.98      <- and sure of it
aec_lag_ms                   median 30.00                  <- 4 changes in 71 s
aec_spread_ms    p10 40      median 140     p90 440
transmitted      1 756 of 1 776  (98.9%)
```

**The filter covers 21.3 ms and the echo is spread over 140 ms, median.** Every
one of those 1 776 blocks had a spread wider than the filter — 100%, not most.
At p90 the spread is 440 ms, twenty times the filter.

That is not a bug. It is a room. An iPhone on a table has a reverberation tail
of hundreds of milliseconds, and `DEFAULT_TAPS` was sized for a helmet, where
the speaker is centimetres from the microphone and there is almost no tail.
`echo_path()` in the tests is **64 taps — 1.3 ms**. The real room is two to
three hundred times longer, and every test in the suite is built on that
kernel.

`_WHY_NO_GROWTH` already records that this filter cannot simply be made longer:
a time-domain NLMS normalised by one total power converges worse the further it
spans, which is why both production cancellers are frequency-domain. So the
architecture cannot reach this case, and no amount of tuning changes that.

**The next step is therefore a different canceller, not a longer one.**

### 7. AEC3 measured on the OPPO, 14 August

[Sonora](https://github.com/dignifiedquire/sonora) is a pure-Rust port of
WebRTC's AudioProcessing (M145), BSD-3-Clause — which GPL-3 can absorb — and
needs no C++ toolchain, which is what makes it worth trying across five
cross-compiled targets. `sonora-aec3` is **0.2.0, published 29 July 2026**, so
it is two weeks old and that is a real risk to weigh.

`tools/aec3bench`, on the phone, 20 s a room:

```
room                         erle dB   mean us    p95 us
helmet, 20 ms tail              42.6       411       486
measured p10, 40 ms             44.6       409       483
measured median, 140 ms         38.4       408       481
measured p90, 440 ms            42.8       400       472
```

Against the same phone's own numbers: the current filter is **970 µs at 1024
taps** and manages **0.0 dB** on the p90 room. AEC3 is **less than half the
cost and 40 dB better**, and it does not degrade as the tail lengthens. It
would take the whole block from 8.8 ms to 8.2.

`core/tests/aec3_cost.rs` runs both side by side on a desktop and agrees:
37–39 dB for AEC3 against 27.8, 0.3, 4.8, 0.0 for ours.

**Two mistakes on the way there, both worth knowing.** The first room used
`testsig::speech` alone — a 120 Hz harmonic stack, exactly periodic, energy at
33 discrete frequencies and none between. On it, ours scored 50 dB and AEC3
scored 1.7, and *both* were artefacts: no adaptive filter can identify a room
from a rank-deficient input, and any filter matching those 33 points scores
perfectly. The generator did not flatter the hypothesis, it flattered the
**incumbent**, because the incumbent had been tuned alongside it. The second
was nearly believing the 1.7: cross-checking AEC3's own `get_statistics()`
against the measured figure is what caught it, and its reported ERL of −30.0 dB
and delay of 288 ms were the tell.

### 8. Still required either way: a continuous render stream

AEC3 and every port of it expect exactly **one 10 ms render frame per 10 ms
capture frame**. Today 44% of blocks push nothing into `echo_reference` because
the far end happened to be quiet, and the queue is cleared wholesale past
500 ms. That has to be a real-time stream before any of the above can be
adopted. It is not the cause of the current fault — §5 measured that — but it
is a precondition for the fix.

### Why the tests could not have caught either

Every echo test hands the canceller a contiguous slice of the far-end signal —
`let r = &far[chunk * BLOCK..(chunk + 1) * BLOCK]`, `echo_alignment.rs:162`.
The reference is perfect and continuous by construction, silences included. The
fault lives entirely in the plumbing between the output callback and the capture
worker, which no test touches. CLAUDE.md's rule about synthetic signals agreeing
with whoever wrote them, landing on the fix rather than on a feature.

### The recording could not say any of this

Thirteen columns and not one of them about the canceller — no ERLE, no lag, no
confidence, not even whether it was switched on. All of the above had to be
inferred from level arithmetic and read out of the source. Seven columns are now
in the decision log, and `echo_ref_samples` is the one that matters: it counts
how much of each block's reference was real before the zero-fill hid the
difference. A block reading 0 had no reference and cannot have cancelled
anything; anything between 0 and 480 is the queue running dry mid-block.

**The next recording from build 123 or later settles the reference question
without any inference at all.**
