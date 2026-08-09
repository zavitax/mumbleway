import 'dart:async';
import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:tflite_flutter/tflite_flutter.dart';

import '../src/rust/api/mumbleway.dart';

/// Listens to the background and tells the chain whether it is loud and
/// structured, so `Auto` can pick Helmet on evidence rather than on a level.
///
/// **Why a model at all.** Every other input to the profile decision is a level
/// or a filter measured *after* the suppressor whose behaviour it is meant to
/// influence, so it cannot disagree with it. `docs/MUSIC_GATE.md` is the record
/// of six hand-built features failing against exactly that, and of a 4 MB
/// classifier fed the raw microphone getting it right.
///
/// **What it decides, and what it may not.** One boolean: the background is
/// loud and structured. It is a supporting vote for Helmet, consulted only when
/// the rider has chosen `Auto`, and it never goes near the transmit decision.
/// Being wrong about a profile costs some naturalness; being wrong at the gate
/// cuts a rider off mid-sentence.
///
/// **The class is called `Music` and that is the model's word, not ours.** An
/// engine at speed scores 0.969 on it, which was read as a false positive until
/// the target was stated properly: what the profile wants to know is whether
/// the background is something Helmet handles better, and an engine is.
class BackgroundClassifier {
  BackgroundClassifier();

  /// YAMNet's own input size: 0.975 s at 16 kHz. Not adjustable — the model was
  /// built for it, and the Rust tap produces exactly this.
  static const int windowSamples = 15600;

  /// `Music` in the model's label list, of 521 classes.
  ///
  /// A constant with a test behind it (`test/yamnet_labels_test.dart`), which
  /// reads the label list out of the model file itself. Being wrong here would
  /// not fail loudly — it would score a different class and the app would
  /// behave plausibly and wrongly — and the index is widely quoted, which is
  /// exactly the kind of number that gets copied without checking.
  static const int musicIndex = 132;

  /// Classes the model scores. Asserted against the model's output tensor at
  /// load, so a swapped model cannot silently shift every index.
  static const int classes = 521;

  /// The bar a frame has to clear.
  ///
  /// Measured, not guessed — `tools/vad/yamnet_threshold.py`. Across the corpus
  /// the quiet clip fires 0% at every bar down to 0.05 while music, engine and
  /// voice-over-music fire 63–100%, so the separation is total and this sits in
  /// the flat part of all four curves. A threshold on a plateau is robust; one
  /// on a slope is tuned to the clips it was chosen on.
  static const double bar = 0.30;

  /// How often a window is asked for.
  ///
  /// The thing being detected changes over tens of seconds, and the hold in the
  /// core is 15 s, so inferring faster would spend battery to no effect. It
  /// also has to be well inside the tap's five-second arming or the window goes
  /// cold between polls.
  static const Duration cadence = Duration(seconds: 2);

  /// Where the model can actually run.
  ///
  /// **Not Windows or Linux yet**, and the reason is a missing binary rather
  /// than a missing idea. `tflite_flutter` ships native libraries for Android
  /// and iOS through Gradle and CocoaPods, and a universal macOS dylib inside
  /// the package itself — which the vendored copy in `third_party` now wires
  /// into `Contents/Frameworks` and signs. For Windows it ships nothing at all
  /// and its README says to build your own, so that one waits on a CI job that
  /// builds `libtensorflowlite_c` from source.
  ///
  /// macOS runs it on the CPU whatever the machine: the shipped dylib has no
  /// Core ML or GPU delegate symbols in it. Measured at 2.4 ms an inference on
  /// Apple Silicon, which at one every two seconds is nothing worth avoiding.
  static bool get supportedHere =>
      Platform.isAndroid || Platform.isIOS || Platform.isMacOS;

  Interpreter? _model;
  IsolateInterpreter? _isolate;
  Timer? _timer;
  bool _busy = false;
  bool _starting = false;

  /// Which accelerator the interpreter accepted, or null for plain CPU.
  ///
  /// **This is what was accepted, not what silicon ran it.** Core ML decides
  /// per operation whether to use the Neural Engine, the GPU or the CPU, and
  /// says nothing about which; the GPU delegate likewise falls back on its own.
  /// So this is the strongest honest claim available — the accelerated path was
  /// built — and the panel says it in those terms rather than promising an NPU.
  String? get accelerator => _accelerator;
  String? _accelerator;

  /// True once a model is loaded and running.
  bool get running => _isolate != null;

  /// True when the model is running with no accelerator behind it.
  ///
  /// The state the warning is for: it works, and it is paying for every
  /// inference out of the CPU and therefore the battery.
  bool get onCpuOnly => running && _accelerator == null;

  /// The last verdict, for the panel. Null before the first inference.
  bool? get lastVerdict => _lastVerdict;
  bool? _lastVerdict;

  /// The last score, for the panel.
  double get lastScore => _lastScore;
  double _lastScore = 0;

  /// How long the last inference took, in milliseconds.
  ///
  /// Shown rather than described. What an inference costs is the one thing
  /// about this feature nobody can look up — it depends on the phone, the
  /// delegate and what else is running — and a number measured on the rider's
  /// own device beats any sentence written here. Measured on a Mac through the
  /// same model and runtime it is 2.4 ms, which is why the panel reports the
  /// cost rather than warning about it.
  double get lastInferenceMs => _lastInferenceMs;
  double _lastInferenceMs = 0;

  /// Called when anything above changes, so the panel can redraw.
  VoidCallback? onChanged;

  /// Starts polling, loading the model on the first call.
  ///
  /// Safe to call repeatedly; the second call does nothing.
  Future<void> start() async {
    if (!supportedHere || _isolate != null || _starting) return;
    _starting = true;
    try {
      final options = InterpreterOptions();
      // One delegate attempt, and a plain CPU interpreter if it is refused.
      // A delegate that fails to attach throws at `fromAsset`, so the fallback
      // is a second load rather than a flag.
      try {
        if (Platform.isIOS) {
          options.addDelegate(CoreMlDelegate());
          _accelerator = 'Core ML';
        }
        // **No GPU delegate on Android.** It killed the process on an OPPO
        // A3s (Adreno 506, Android 12): a SIGSEGV inside
        // `TfLiteInterpreterAllocateTensors`, which is native and therefore
        // uncatchable — the `catch` below never runs and the app is simply
        // gone. Sometimes, because it only starts when Automatic is chosen and
        // the devices are open.
        //
        // Not worth another attempt, either. TFLite's own log on the emulator
        // said the delegate could take **31 of 47 operations** and the rest
        // would stay on the CPU, because YAMNet computes its own mel
        // spectrogram and no GPU delegate supports `RFFT2D` or `COMPLEX_ABS`.
        // So the offer was a partial offload of a model that runs once every
        // two seconds, and the price was crashing on the class of phone the
        // acceleration was meant to help.
        _model = await Interpreter.fromAsset(
          'assets/models/yamnet.tflite',
          options: options,
        );
      } catch (e) {
        // Not a failure worth showing a rider: the model still runs, more
        // slowly, and the panel says so through [onCpuOnly].
        debugPrint('background classifier: no accelerator ($e)');
        _accelerator = null;
        _model = await Interpreter.fromAsset('assets/models/yamnet.tflite');
      }

      // The shapes this code assumes, checked against the model that actually
      // loaded. `[15600]` in and `[1, 521]` out — a model with a different
      // input rank would otherwise fail deep inside an isolate every two
      // seconds, where the only symptom is a classifier that never decides.
      final inShape = _model!.getInputTensor(0).shape;
      final outShape = _model!.getOutputTensor(0).shape;
      if (inShape.length != 1 ||
          inShape[0] != windowSamples ||
          outShape.last != classes) {
        throw StateError('unexpected model shape: in $inShape, out $outShape');
      }

      // Off the UI isolate. An inference is tens of milliseconds and this runs
      // while a rider is looking at a call screen that must not stutter.
      _isolate = await IsolateInterpreter.create(address: _model!.address);
      _timer = Timer.periodic(cadence, (_) => _tick());
      onChanged?.call();
    } catch (e) {
      debugPrint('background classifier: could not start ($e)');
      await stop();
    } finally {
      _starting = false;
    }
  }

  /// Stops polling and withdraws the verdict.
  ///
  /// Withdrawing matters. A verdict left behind would keep its 15 s hold alive
  /// and then sit in the chain as a claim nobody is updating — which, if it
  /// said "noisy", would pin Helmet for the rest of the session.
  Future<void> stop() async {
    _timer?.cancel();
    _timer = null;
    await _isolate?.close();
    _isolate = null;
    _model?.close();
    _model = null;
    _accelerator = null;
    _lastVerdict = null;
    _lastScore = 0;
    try {
      clearBackgroundNoisy();
    } catch (_) {
      // No engine to tell. It has forgotten by itself.
    }
    onChanged?.call();
  }

  Future<void> _tick() async {
    // One inference at a time. On a slow phone a run can outlast the cadence,
    // and queueing them would turn a busy moment into a growing backlog.
    if (_busy) return;
    final isolate = _isolate;
    if (isolate == null) return;

    UiWaveform? window;
    try {
      // Asking is also what keeps the tap collecting. Stop calling this and
      // the core stops within five seconds, with no "off" to be missed.
      window = audioWaveform();
    } catch (_) {
      // The engine is not running — no devices open, so there is nothing to
      // classify and nothing to say about it.
      return;
    }
    if (window == null || window.samples.length != windowSamples) return;

    _busy = true;
    try {
      // Flat in, `[1, 521]` out — the model's own shapes, checked at load.
      final output = List.filled(classes, 0.0).reshape([1, classes]);
      final started = DateTime.now();
      await isolate.run(window.samples, output);
      _lastInferenceMs =
          DateTime.now().difference(started).inMicroseconds / 1000.0;
      final score = ((output[0] as List)[musicIndex] as num).toDouble();
      _lastScore = score;
      final noisy = score >= bar;
      _lastVerdict = noisy;
      setBackgroundNoisy(noisy: noisy);
      onChanged?.call();
    } catch (e) {
      debugPrint('background classifier: inference failed ($e)');
    } finally {
      _busy = false;
    }
  }
}
