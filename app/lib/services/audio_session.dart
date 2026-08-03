import 'dart:io' show Platform;

import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';

/// What the platform said when asked for a microphone.
class AudioSessionState {
  const AudioSessionState({
    required this.granted,
    required this.inputChannels,
    required this.sampleRate,
    this.error,
  });

  /// Whether the user has allowed recording.
  final bool granted;

  /// How many input channels the hardware offers once the session is live.
  ///
  /// Zero after a successful configuration means there is genuinely nothing to
  /// record from — no built-in microphone, or a headset that has just been
  /// unplugged. Worth distinguishing from a refusal, because the two need
  /// different things from the user.
  ///
  /// Negative means the question was never put, on a platform with no session
  /// to put it to. Not the same as zero, and treating it as zero would refuse
  /// to start the engine on every desktop.
  final int inputChannels;

  final double sampleRate;

  /// What the platform said went wrong, if configuring the session failed
  /// outright rather than merely being refused.
  final String? error;

  bool get usable => granted && inputChannels != 0;

  /// The state on platforms that have no session to configure.
  static const notNeeded = AudioSessionState(
    granted: true,
    inputChannels: -1,
    sampleRate: 0,
  );
}

/// Prepares the iOS audio session before anything tries to open the microphone.
///
/// iOS starts every app in a playback-only category, and in that state it
/// reports zero input channels rather than an error. The audio engine then
/// fails inside CoreAudio with "channel count must be at least 1" — accurate
/// about the symptom, silent about the cause, and impossible to reproduce
/// anywhere but a real device, since no other platform has a session at all.
class AudioSessionBridge {
  AudioSessionBridge._();
  static final AudioSessionBridge instance = AudioSessionBridge._();

  static const _channel = MethodChannel('mumbleway/audioSession');

  /// Called when a phone call or a Siri request ended and the session came
  /// back. The engine may need restarting; the platform side has already put
  /// the session itself back.
  VoidCallback? onResumed;

  bool _handlerInstalled = false;

  bool get isNeeded {
    if (kIsWeb) return false;
    try {
      return Platform.isIOS;
    } catch (_) {
      // Platform is unavailable under some test harnesses.
      return false;
    }
  }

  void _ensureHandler() {
    if (_handlerInstalled) return;
    _handlerInstalled = true;
    _channel.setMethodCallHandler((call) async {
      if (call.method == 'resumed') onResumed?.call();
      return null;
    });
  }

  /// Asks for the microphone and activates the session.
  ///
  /// Must complete before the engine starts. Everywhere else this is a no-op
  /// that reports success, so the caller has one code path.
  Future<AudioSessionState> prepare() async {
    if (!isNeeded) return AudioSessionState.notNeeded;
    _ensureHandler();
    try {
      final r = await _channel.invokeMapMethod<String, dynamic>('prepare');
      return AudioSessionState(
        granted: r?['granted'] == true,
        inputChannels: (r?['inputChannels'] as num?)?.toInt() ?? 0,
        sampleRate: (r?['sampleRate'] as num?)?.toDouble() ?? 0,
      );
    } on MissingPluginException {
      // An older build of the native side. Let the engine try: it either works
      // or produces the error this exists to explain, and refusing to start
      // would be worse than either.
      return AudioSessionState.notNeeded;
    } on PlatformException catch (e) {
      return AudioSessionState(
        granted: true,
        inputChannels: 0,
        sampleRate: 0,
        error: e.message,
      );
    }
  }
}
