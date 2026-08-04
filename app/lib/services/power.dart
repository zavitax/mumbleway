import 'dart:io' show Platform;

import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';

/// Tells the platform when there is a conversation worth spending power on.
///
/// Every desktop and mobile system has some way for an app to say "do not go
/// to sleep, I am in the middle of something", and every one of them is a
/// promise that costs battery until it is given back:
///
///   * Android — a partial wake lock, held by the foreground service
///   * macOS   — an `NSProcessInfo` activity assertion
///   * Windows — `ES_SYSTEM_REQUIRED` on the platform thread
///
/// iOS is absent because it has no equivalent to ask for: the `audio`
/// background mode already keeps the process alive while a session is live,
/// and nothing else is on offer.
///
/// This lives on its own channel rather than on the overlay's. It was on the
/// overlay's to begin with, because Android happens to keep the wake lock in
/// the same service that draws the floating window — but the two are not the
/// same idea, and the moment Windows needed this it would have meant
/// registering a channel called "overlay" on a platform that has no overlay
/// and never will.
class PowerBridge {
  PowerBridge._();
  static final PowerBridge instance = PowerBridge._();

  static const _channel = MethodChannel('mumbleway/power');

  /// Whether this platform does anything with the answer.
  ///
  /// Asked so that the platforms which do not are not sent a message on every
  /// transition to be dropped on the floor at the other end.
  static bool get isSupported {
    if (kIsWeb) return false;
    try {
      return Platform.isAndroid || Platform.isMacOS || Platform.isWindows;
    } catch (_) {
      // Platform is unavailable under some test harnesses.
      return false;
    }
  }

  /// Reports whether a call is up, or being chased.
  ///
  /// Deliberately not tied to whether a floating window is showing. On Android
  /// the wake lock used to be taken when the app started and released only
  /// when it died — through every ride with nothing connected, and every night
  /// on a bedside table. Tying it to the window instead would have moved the
  /// fault rather than fixed it: a rider who turns the island off in settings
  /// still makes calls, and theirs would have been the calls that died when
  /// the screen went off.
  Future<void> setCallActive(bool active) async {
    if (!isSupported) return;
    try {
      await _channel.invokeMethod<void>('callActive', active);
    } catch (_) {
      // An older platform side, or a service that is not up yet. It is asked
      // again on the next transition, and there is no call to lose in between.
    }
  }
}
