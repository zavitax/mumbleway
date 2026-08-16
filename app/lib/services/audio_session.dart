import 'dart:io' show Platform;

import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';

/// What the platform said when asked for a microphone.
class AudioSessionState {
  const AudioSessionState({
    required this.granted,
    required this.inputChannels,
    required this.sampleRate,
    this.route = 0,
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

  /// Which microphone the platform put us on, as the code the decision log
  /// carries. See `Recorded::route` in `record.rs` for the table; 0 means it
  /// did not say, which is every platform without a session.
  final int route;

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

  /// Whether this platform has something to arrange before the microphone
  /// will work.
  ///
  /// iOS has a session to configure. Android has a permission to ask for, which
  /// the manifest alone never grants — recording without it returns silence
  /// rather than an error, so the meter sat at zero on every device and nothing
  /// said why. Desktop has neither.
  bool get isNeeded {
    if (kIsWeb) return false;
    try {
      return Platform.isIOS || Platform.isAndroid;
    } catch (_) {
      // Platform is unavailable under some test harnesses.
      return false;
    }
  }

  /// Called when another app takes the microphone, and again when it gives it
  /// back.
  ///
  /// **Android hands the loser of a contest for the microphone digital silence,
  /// not an error.** Two apps may capture at once from Android 10 and only one
  /// gets real audio; a navigation app listening for voice commands is the
  /// common case. Nothing in the stream distinguishes that from a quiet room,
  /// so without this the report that arrives is "nobody can hear me" and every
  /// stage of the chain is working perfectly on the silence it was given.
  void Function(bool silenced)? onMicSilenced;

  void _ensureHandler() {
    if (_handlerInstalled) return;
    _handlerInstalled = true;
    _channel.setMethodCallHandler((call) async {
      if (call.method == 'resumed') onResumed?.call();
      if (call.method == 'micSilenced') {
        onMicSilenced?.call(call.arguments == true);
      }
      return null;
    });
  }

  /// Takes the platform's audio session live for a call.
  ///
  /// Separate from [prepare] because the two answer different questions at
  /// different times. Permission is asked for once, at startup, and is a
  /// property of the install; the session is live only while there is a
  /// conversation, and holding it the rest of the time lights the recording
  /// indicator and drags a Bluetooth headset onto the hands-free profile,
  /// where everything else the rider listens to sounds like a telephone.
  ///
  /// Called while a connection is being set up rather than when the first word
  /// is spoken. Activation is not instant and can be refused outright by a
  /// phone call holding a session that will not mix, and neither is worth
  /// discovering half-way through a sentence.
  Future<AudioSessionState> activate({bool voiceProcessing = false}) async {
    if (!isNeeded) return AudioSessionState.notNeeded;
    _ensureHandler();
    try {
      // Passed with the call because iOS can only choose a session mode while
      // it is configuring one, and Android reads it when the capture stream is
      // built. Neither can be told later and act on it now.
      final r = await _channel.invokeMapMethod<String, dynamic>('activate', {
        'voiceProcessing': voiceProcessing,
      });
      return AudioSessionState(
        granted: true,
        inputChannels: r?['ok'] == true
            ? (r?['inputChannels'] as num?)?.toInt() ?? 0
            : 0,
        sampleRate: (r?['sampleRate'] as num?)?.toDouble() ?? 0,
        route: (r?['route'] as num?)?.toInt() ?? 0,
        error: r?['error'] as String?,
      );
    } on MissingPluginException {
      // An older platform side. Let the engine try rather than refusing: it
      // either works or produces the error this exists to explain.
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

  /// Hands the session back when the last call ends.
  ///
  /// Best effort on purpose: failing to put the session down costs battery,
  /// not function, and there is nothing the rider could do about it. The next
  /// [activate] takes it back regardless.
  Future<void> deactivate() async {
    if (!isNeeded) return;
    try {
      await _channel.invokeMethod<bool>('deactivate');
    } catch (_) {
      // Older platform side, or a session that was not ours to release.
    }
  }

  /// Asks for the microphone.
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
