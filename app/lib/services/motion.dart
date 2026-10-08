import 'dart:io' show Platform;

import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';

import '../src/rust/api/mumbleway.dart' as rust;

/// The phone's own motion, on its way to the recorder and the tap detector.
///
/// **A channel into the existing native files rather than a plugin**, and the
/// rate is the reason. `sensors_plus` and its alternatives deliver at a cadence
/// of their own choosing; this needs an explicit one, because the entire
/// question of whether a tap can be told from a pothole turns on it. A tap's
/// mechanical transient runs 20–80 ms, so at 100 Hz it is three to eight
/// samples and its onset is a step rather than a slope. Android gives 200–500 Hz
/// and iOS tops out at 100, and that asymmetry is a finding rather than a
/// detail — see `docs/CAPTURE_ON_DEMAND.md`.
///
/// It also keeps the dependency list where this project likes it, and leaves
/// the Android 12 `HIGH_SAMPLING_RATE_SENSORS` permission visible in the
/// manifest instead of buried in a package.
///
/// ## What it does with a sample
///
/// Hands it straight to the engine, which writes it to the recorder's third
/// track if a session is running and ignores it otherwise. **Nothing here is
/// conditional on tap detection being switched on**, which is the requirement
/// easiest to get wrong: measuring *false* positives needs rides with no taps
/// in them, so the negative corpus can only be gathered while the feature is
/// off.
class MotionBridge {
  MotionBridge._();
  static final MotionBridge instance = MotionBridge._();

  static const _channel = MethodChannel('mumbleway/motion');

  bool _handlerInstalled = false;
  bool _running = false;

  /// Readings this process could not hand on, counted rather than hidden.
  ///
  /// A gap nobody counted looks exactly like a stretch of road where nothing
  /// happened, which is the reading a tap detector would then be scored
  /// against.
  int get dropped => _dropped;
  int _dropped = 0;

  /// Whether this platform has motion sensors to offer at all.
  bool get isSupported {
    if (kIsWeb) return false;
    try {
      return Platform.isAndroid || Platform.isIOS;
    } catch (_) {
      return false;
    }
  }

  /// Called when a sample completed a tap gesture.
  ///
  /// The detection happens in Rust beside the rest of the signal processing —
  /// see `core/src/audio/tap.rs` — and arrives as the return value of the same
  /// call that hands the sample over, rather than through a channel of its own:
  /// the sample is already crossing the boundary, and a callback the other way
  /// would have to be marshalled back onto the thread this is already on.
  VoidCallback? onTapGesture;

  Future<bool> available() async {
    if (!isSupported) return false;
    try {
      return await _channel.invokeMethod<bool>('available') ?? false;
    } catch (_) {
      return false;
    }
  }

  Future<void> start() async {
    if (!isSupported || _running) return;
    _ensureHandler();
    try {
      await _channel.invokeMethod<bool>('start');
      _running = true;
    } on MissingPluginException {
      // An older platform side. Not a fault worth surfacing: the feature that
      // needs this says it is unavailable rather than failing when used.
    } catch (_) {}
  }

  Future<void> stop() async {
    if (!_running) return;
    _running = false;
    try {
      await _channel.invokeMethod<bool>('stop');
    } catch (_) {}
  }

  void _ensureHandler() {
    if (_handlerInstalled) return;
    _handlerInstalled = true;
    _channel.setMethodCallHandler((call) async {
      if (call.method != 'sample') return null;
      final a = call.arguments;
      if (a is! Map) return null;

      final accel = _three(a['a']);
      final gravity = _three(a['g']);
      final rotation = _three(a['r']);
      final stamp = (a['t'] as num?)?.toInt() ?? 0;

      try {
        final gesture = rust.pushMotion(
          platformNs: BigInt.from(stamp),
          accel: accel,
          gravity: gravity,
          rotation: rotation,
        );
        if (gesture) onTapGesture?.call();
      } catch (_) {
        // No engine yet, or it has gone. Counted rather than thrown: this runs
        // at up to 400 Hz and an exception per sample would drown the log in
        // the one situation where the log is what somebody is reading.
        _dropped++;
      }
      return null;
    });
  }

  static List<double> _three(Object? v) {
    if (v is! List) return const [0.0, 0.0, 0.0];
    double at(int i) =>
        i < v.length ? ((v[i] as num?)?.toDouble() ?? 0.0) : 0.0;
    return [at(0), at(1), at(2)];
  }
}
