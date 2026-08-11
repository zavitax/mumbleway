# Testing the Windows classifier — handover

Written 2026-08-11, revised the same day when the check below was finally run
and **the first build turned out to export nothing at all**. See "The DLL that
exported nothing" — it is the trap most likely to be walked into again, because
every part of it looks like success.

**Not a site page.** No front matter, so Jekyll leaves it alone.

## What is done

| | |
|---|---|
| `libtensorflowlite_c-win.dll` | built from TensorFlow **r2.17** at `C:\src\tensorflow`, output in `C:\src\tflite-win-build`, vendored at `app/blobs/`. 3.69 MB |
| Install into the bundle | upstream's own `install()` block in `app/windows/CMakeLists.txt`; confirmed landing in `build/windows/x64/runner/Release/blobs/` |
| `BackgroundClassifier.supportedHere` | now includes `Platform.isWindows` |
| Licences | `docs/licences.md` and `docs/ru/licences.md` both name the Windows build |
| **The library loads and the model runs** | proved twice, see below |

Rebuilding the DLL is `scratchpad/build_tflite_win.cmd` if it is still around;
if not, the recipe is under "Building it again" below — and it **needs the
patch**, which is not optional.

## What has been proved, and how

Two checks, deliberately different in kind.

**Outside the app**, `scratchpad/tflite_probe.dart` opens the vendored DLL by
path with `dart:ffi` and drives the C API by hand: `TfLiteVersion` → 2.17.1,
`yamnet.tflite` loads, tensors allocate, input shape `[15600]` and output shape
`[1, 521]` — the two the app asserts at load — and an inference on a block of
silence returns in 9.0 ms with class 494 at 0.80 and `Music` at 0.0039, which is
the right answer for silence. It goes round `tflite_flutter` on purpose: the
package resolves the library from `Platform.resolvedExecutable` and needs a
Flutter binding for `rootBundle`, so a failure there could mean the DLL is wrong
*or* that a test harness could not find an asset, and this has to be able to
fail for one reason only.

**Inside the app**, with a connected session and `Automatic` chosen, the DLL
appears in the process's own loaded modules:

```powershell
(Get-Process mumbleway).Modules |
  Where-Object { $_.ModuleName -match 'tensorflow' } | Select ModuleName, FileName
```

That is the stronger of the two, because the module list can only contain what
`DynamicLibrary.open` actually opened, and `_syncClassifier` only reaches that
call when all four conditions below hold. It needs no clicking, and it is the
check to reach for when somebody else is using the window.

What is **still** unwitnessed is the three scored rows in the panel — the model
running on live microphone audio rather than on a probe's silence. That is a
screenshot, and it is what remains of this task.

## The four conditions

**The model is opened lazily and only under one condition.** `_syncClassifier`
in `app_state.dart` wants *four* things true at once, and all four are easy to
have wrong while thinking the feature is broken:

1. the devices are open — so **connected to a server**, not merely running;
2. the noise profile is **Automatic**, not Helmet or anything else;
3. the rider has not switched it off;
4. the platform supports it — which is the part that just changed.

So a Windows build that never connects will never touch the DLL, and will look
exactly like one where the DLL is fine.

### The check, in order

1. **Set the profile to Automatic.** Settings → Noise cancellation → Automatic.
   Or, with the app closed, `flutter.mumbleway.noise` in
   `%APPDATA%\com.mumbleway\mumbleway\shared_preferences.json`: `4` is
   Automatic, `3` is Helmet. **Edit it with Python, not a PowerShell pipeline** —
   `Set-Content -Encoding utf8` adds a BOM and the app then reads nothing. That
   happened; the file survived, but it is a real trap.
2. **Connect to a server** and leave it connected.
3. **Open the diagnostics panel** — the toolbar icon left of the speaker.
4. **Look above the chain dots.** Three rows, each a label, a bar and a score
   from 0 to 1, is the classifier running. See below for what each other
   outcome means.

### Reading the result

| What the panel shows | What it means |
|---|---|
| Three rows with labels and scores | **It works.** The DLL loaded, YAMNet is running. |
| A spinner and "listening to the background…" | The model loaded and has not produced a verdict yet. Give it a few seconds; it runs about once every two seconds. |
| The profile line, but nothing above the dots | The classifier is not *running*. Either the profile is not Automatic, or `start()` threw — check the engine log at the bottom of the panel. |
| "Background detection is not available on this platform" | `supportedHere` is false. On Windows that now means the build predates this work. |
| An amber note about no accelerator, with a millisecond figure | Normal and expected on Windows: no delegate is attached there, so it runs on the processor and the panel reports what that costs. |

**A failure to load is not a crash.** `start()` catches and falls back, so the
symptom is a classifier that silently never appears — which is why the table
above matters more than it looks. If the DLL is the wrong ABI or has a missing
dependency, that is where it shows.

### If it does not load

`DynamicLibrary.open` failing is reported through `debugPrint`, which a release
build does not show. Two ways to see it:

```powershell
# Release, with stdout captured -- the engine log goes here too.
$p = Start-Process .\mumbleway.exe -PassThru -RedirectStandardOutput out.txt -RedirectStandardError err.txt
```

```powershell
# Or run a debug build, where debugPrint reaches the console.
flutter run -d windows
```

Then check the DLL itself, in this order. **Exports first** — that is the fault
that has actually happened, and it is invisible from the loader's side:

```powershell
dumpbin /exports build\windows\x64\runner\Release\blobs\libtensorflowlite_c-win.dll
```

`TfLiteVersion` and `TfLiteInterpreterCreate` should be in the list. An empty
list, or "no exports", is the trap described above. `scratchpad/tflite_probe.dart`
answers the same question and rather more besides.

Only then the dependencies, since a TFLite build can want a Visual C++ runtime
the target machine has not got:

```powershell
dumpbin /dependents build\windows\x64\runner\Release\blobs\libtensorflowlite_c-win.dll
```

## The DLL that exported nothing

The first build produced `tensorflowlite_c.dll`, 1.28 MB, and everything about
it looked right: the build was green, the file was the expected shape, it copied
into the bundle, and **it loaded**. `DynamicLibrary.open` succeeded. Only the
first `lookupFunction` failed:

```
Invalid argument(s): Failed to lookup symbol 'TfLiteVersion':
The specified procedure could not be found. (error code: 127)
```

It had **no export directory at all** — not a missing symbol, no symbols. Two
things beside it said so and neither is loud: there was no `tensorflowlite_c.lib`
next to the DLL, because an import library is only produced for a DLL that
exports something, and 1.28 MB is about a third of what a real one weighs,
because the linker had dropped everything nothing reached.

The cause is one line in a file that belongs to neither end of it.
`tensorflow/lite/CMakeLists.txt` appends `-DTFL_STATIC_LIBRARY_BUILD` to
`tensorflow-lite`'s **PUBLIC** compile options whenever `BUILD_SHARED_LIBS` is
off — which it is here, since the whole shape of this build is a shared C shim
over a static core. PUBLIC means it is inherited by the shim, and
`core/c/c_api_types.h` tests it **before** `TFL_COMPILE_LIBRARY`:

```c
#ifdef SWIG
#define TFL_CAPI_EXPORT
#elif defined(TFL_STATIC_LIBRARY_BUILD)
#define TFL_CAPI_EXPORT              /* <- taken, and the story ends here */
#else
#if defined(_WIN32)
#ifdef TFL_COMPILE_LIBRARY
#define TFL_CAPI_EXPORT __declspec(dllexport)
```

So `target_compile_definitions(tensorflowlite_c PRIVATE TFL_COMPILE_LIBRARY)`,
which upstream sets three lines away and which is exactly right, never gets
reached. The fix drops the flag from the *interface* only, which leaves the core
compiling as the static library it is and changes only the four C API
translation units:

```cmake
# in tensorflow/lite/c/CMakeLists.txt, after target_link_libraries
get_target_property(_tflite_iface tensorflow-lite INTERFACE_COMPILE_OPTIONS)
if (_tflite_iface)
  list(REMOVE_ITEM _tflite_iface "-DTFL_STATIC_LIBRARY_BUILD")
  set_target_properties(tensorflow-lite
    PROPERTIES INTERFACE_COMPILE_OPTIONS "${_tflite_iface}")
endif()
```

**`/UTFL_STATIC_LIBRARY_BUILD` does not work**, which is worth recording because
it is the obvious first attempt and it fails quietly: CMake emits a target's
inherited interface options *after* its own, so the `-D` lands after the `/U`
and MSVC takes them in order. The rebuild looked identical — same byte count,
same missing `.lib` — which is precisely how a no-op presents.

With the patch the DLL is **3.69 MB** with an import library beside it. Both
numbers are worth checking after any rebuild; they are the cheapest possible
smoke test, and neither the compiler nor the linker will say a word.

## Building it again

Three things, all learned the hard way and all recorded in `9051ee3`:

- **Ninja, not the Visual Studio generator.** Eigen includes
  `CMakeDetermineFortranCompiler` unconditionally, and under the VS generator
  CMake's Fortran probe is an Intel `.vfproj` Windows cannot open — so configure
  pops Explorer "how do you want to open this file?" dialogs. Nothing is wrong;
  the answer is "there is no Fortran compiler", which is correct and wanted.
  Under Ninja it is an ordinary compile that fails quietly.
- **Do not pass `-DCMAKE_CXX_STANDARD`.** TFLite hardcodes 17 in two of its own
  `CMakeLists.txt` and wins, so the flag is silently ignored and only misleads
  whoever reads the command back.
- **Log the whole build.** Tailing the last lines hid the real error for an hour
  on the first attempt.

```
cmake -S C:\src\tensorflow\tensorflow\lite\c -B C:\src\tflite-win-build ^
      -G Ninja -DCMAKE_BUILD_TYPE=Release -DTFLITE_ENABLE_XNNPACK=ON
cmake --build C:\src\tflite-win-build --target tensorflowlite_c
```

666 targets, about eight minutes, one 1.28 MB DLL. `vcvars64.bat` first, and
Ninja is in `C:\Android\sdk\cmake\3.22.1\bin`.

## The classifier diagnostics, and what they are for

Three separate things in the panel report on this, and they are easy to confuse.

**The profile line**, above the dots. Shows for all five settings now, not only
Automatic — it used to appear only under Auto, which produced three separate
reports of a "missing" profile line when the profile was simply set by hand.
Under Auto it reads *Auto is using X* and the name is a live verdict; set by
hand it reads *Profile X* and is an instruction that will not change.

**`(pinned)`**, grey, beside an amber profile name. The relief ladder's
`NoClassifier` rung has stopped the inference, so Auto keeps whatever it last
chose and cannot change its mind again. This is the one ladder rung whose cost
is invisible in every other number on the panel: a rider who set Auto and rode
from a car park onto a motorway stays on the car park's profile. Only ever
shown under Auto — a hand-picked profile was never going to change, so calling
it pinned would report a loss that did not happen.

**The three rows** are the evidence behind the profile line. A label has to
clear **0.30** to count as heard. Reading the bars against each other is the
point: "Automatic chose Helmet" is an answer without a reason, and three scores
say whether it was confident or whether the field was level.

### Two flags that sound the same and are not

- `classifier_top_disabled` — a *display* rung. The model still runs and Auto
  still reads it; only the three rows stop being drawn.
- `classifier_disabled` — the model stops running. This is what `(pinned)`
  reports.

Getting these the wrong way round would make the panel claim Auto had stopped
adapting when it had merely stopped *showing its working*, which is the more
alarming of the two claims and the wrong one.

## Also worth knowing

- **macOS is supported and has never been seen to run.** `supportedHere` has
  included macOS for some time and an 11 MB dylib is vendored for it, but the
  Mac has had Helmet set by hand every time it was looked at, so the classifier
  never started. The same four-condition check above applies there, and it is
  worth doing — "nominally supported" and "working" are not the same claim.
- **Windows has no accelerator and that is fine.** No GPU delegate is attached
  there. For YAMNet a delegate is a poor trade anyway: TFLite's own log on
  Android said it could take **31 of 47 operations**, because the model computes
  its own mel spectrogram and no GPU delegate implements `RFFT2D` or
  `COMPLEX_ABS` — and the attempt SIGSEGV'd inside
  `TfLiteInterpreterAllocateTensors` on an Adreno 506, natively, where a Dart
  `catch` never runs. Against ~2.4 ms once every two seconds there is nothing
  to win.
- **The screenshots still want retaking**, and the Windows diagnostics panel
  should be shot *after* this is confirmed working, so the three classifier rows
  are in the picture. `docs/assets/img/shots/diagnostics-desktop.webp` is
  current in every other respect.
