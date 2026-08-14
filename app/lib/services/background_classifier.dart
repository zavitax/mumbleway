import 'dart:async';
import 'dart:io';

import 'package:archive/archive.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart' show rootBundle;
import 'package:tflite_flutter/tflite_flutter.dart';

import '../src/rust/api/mumbleway.dart';

/// One of the model's classes and what it scored, for the panel.
class ClassScore {
  const ClassScore(this.label, this.score);

  /// The model's own English class name. Not translated, deliberately: these
  /// are the names YAMNet was trained with and the ones any published table of
  /// its classes uses, so a translation would make the panel harder to check
  /// against the model rather than easier.
  final String label;
  final double score;
}

/// The model's label list, read out of the model file itself.
///
/// A `.tflite` is a zip, and the MediaPipe build carries its labels inside it —
/// so there is no second asset that can fall out of step with the weights.
/// `test/yamnet_labels_test.dart` reads the same list to check the one index
/// the chain depends on.
///
/// Top level and taking bytes, because it runs through [compute]: the archive
/// is four megabytes and this happens while a call screen is on the front.
List<String> _labelsFromModel(Uint8List bytes) {
  final zip = ZipDecoder().decodeBytes(bytes);
  final list = zip.files.where((f) => f.name.endsWith('.txt'));
  if (list.isEmpty) return const [];
  return String.fromCharCodes(list.first.content as List<int>)
      .split('\n')
      .map((l) => l.trim())
      .where((l) => l.isNotEmpty)
      .toList();
}

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

  /// `Speech` and `Singing` in the same label list, read out of the model file
  /// by `test/yamnet_labels_test.dart` exactly as `musicIndex` is.
  ///
  /// **These drive the noise floor, not the profile.** The suppressor's floor
  /// estimator assumes the quietest thing it has heard recently is background;
  /// a phrase held longer than its memory breaks that assumption and the floor
  /// climbs onto the voice, which is what "it cuts into my speech" was. While
  /// either of these fires, the floor is not allowed to climb.
  ///
  /// Singing is separate from Speech because YAMNet treats it as a separate
  /// class, and a held note is the *worst* case for the floor rather than a
  /// marginal one: it has no gaps at all for the estimator to find.
  static const int speechIndex = 0;
  static const int singingIndex = 24;

  /// The bar for a voice, lower than [`bar`] on purpose.
  ///
  /// The two verdicts are not symmetrical in what being wrong costs. Deciding
  /// the background is noisy picks a heavier profile; deciding a voice is
  /// present only stops the floor rising, and the floor rising is the fault
  /// being fixed. A false positive costs a floor that stays low for a couple
  /// of seconds — which the suppressor ahead of it is there to handle — and a
  /// false negative costs the middle of somebody's sentence.
  static const double voiceBar = 0.15;

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
      Platform.isAndroid ||
      Platform.isIOS ||
      Platform.isMacOS ||
      // Windows, since `blobs/libtensorflowlite_c-win.dll` is now built and
      // installed beside the executable. Upstream ships no prebuilt for this
      // platform, which is the only reason it was absent -- see
      // `windows/CMakeLists.txt`.
      Platform.isWindows;

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

  /// Whether the last inference heard a voice, for the diagnostics panel.
  bool? get lastVoice => _lastVoice;
  bool? _lastVoice;

  /// The last score, for the panel.
  double get lastScore => _lastScore;
  double _lastScore = 0;

  /// The three classes that scored highest last time, highest first.
  ///
  /// **Context for a decision made on one number.** The chain reads `Music` and
  /// nothing else, and a bare score of 0.83 says nothing about whether the
  /// model heard a stereo or a motorway. Seeing what else it was weighing is
  /// what makes a surprising profile switch explicable rather than arbitrary —
  /// and it is how the `Music` class was understood in the first place, when an
  /// engine at speed turned out to score 0.969 on it.
  ///
  /// Empty until the first inference, and whenever the label list could not be
  /// read.
  List<ClassScore> get top => _top;
  List<ClassScore> _top = const [];

  /// The model's class names, or empty if they could not be read.
  List<String> _labels = const [];

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

      // Names for the panel. Failing to read them is not a reason to give up
      // the classifier: the chain reads an index, not a name, so everything
      // that matters still works and only the panel is poorer for it.
      try {
        final asset = await rootBundle.load('assets/models/yamnet.tflite');
        _labels = await compute(_labelsFromModel, asset.buffer.asUint8List());
      } catch (e) {
        debugPrint('background classifier: no label list ($e)');
        _labels = const [];
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
    _lastVoice = null;
    _lastScore = 0;
    // Withdrawn with the verdict, for the same reason: a list left on screen
    // is a claim about what the microphone is hearing right now, and nothing
    // is listening any more.
    _top = const [];
    try {
      clearBackgroundNoisy();
      // Back to "nobody is classifying", which the chain answers with its own
      // per-block opinion. Leaving a stale `true` here would freeze the noise
      // floor for the rest of the session.
      clearClassifierVoice();
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
      final scores = (output[0] as List).cast<num>();
      final score = scores[musicIndex].toDouble();
      _lastScore = score;
      _top = _highest(scores);
      final noisy = score >= bar;
      _lastVerdict = noisy;
      setBackgroundNoisy(noisy: noisy);

      // Either class counts. They are the same question to the floor — is
      // something making voice-shaped sound right now — and a sung phrase is
      // the harder case, not a lesser one.
      final voice =
          scores[speechIndex].toDouble() >= voiceBar ||
          scores[singingIndex].toDouble() >= voiceBar;
      _lastVoice = voice;
      setClassifierVoice(voice: voice);
      onChanged?.call();
    } catch (e) {
      debugPrint('background classifier: inference failed ($e)');
    } finally {
      _busy = false;
    }
  }

  /// The three highest-scoring classes, highest first.
  ///
  /// A full sort of 521 floats, once every two seconds, which is not worth a
  /// partial selection to avoid. Falls back to the bare index when there is no
  /// label list: a number a reader can look up beats an empty row.
  @visibleForTesting
  List<ClassScore> highestForTest(List<num> scores) => _highest(scores);

  List<ClassScore> _highest(List<num> scores) {
    final order = List<int>.generate(scores.length, (i) => i)
      ..sort((a, b) => scores[b].compareTo(scores[a]));
    return [
      for (final i in order.take(3))
        ClassScore(
          i < _labels.length ? _labels[i] : 'class $i',
          scores[i].toDouble(),
        ),
    ];
  }
}
