import 'dart:io' show Platform;

import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';

import '../widgets/voice_meter.dart';

/// Which mechanism a platform uses for the floating call window.
///
/// The three are not interchangeable, and the difference leaks into the UI:
/// each one grants a different number of buttons, so the settings screen has
/// to say what the user will actually get.
enum FloatingKind {
  /// Draws over other apps. Arbitrary buttons, needs a special permission.
  androidOverlay,

  /// Picture in Picture. Survives leaving the app, but the system owns the
  /// controls and offers exactly three of them.
  iosPictureInPicture,

  /// An always-on-top panel. Arbitrary buttons, no permission needed.
  macosPanel,

  none,
}

/// Bridge to the floating call window.
///
/// Android draws over other apps, macOS floats a panel, and iOS uses Picture
/// in Picture — the only way Apple lets a third-party app stay visible over
/// another one. All three speak the same method channel so the state layer
/// does not care which is in use, but [kind] is exposed because the iOS
/// control budget is a fact the UI has to explain rather than hide.
class OverlayBridge {
  OverlayBridge._();
  static final OverlayBridge instance = OverlayBridge._();

  static const _channel = MethodChannel('mumbleway/overlay');

  /// Called when the window's talk control is pressed or released.
  void Function(bool transmitting)? onTransmit;

  /// Called when its mute / deafen controls are used. These toggle rather than
  /// set, because the system controls on iOS are momentary buttons with no
  /// state of their own.
  VoidCallback? onToggleMute;
  VoidCallback? onToggleDeafen;
  VoidCallback? onHangup;

  bool _handlerInstalled = false;
  bool _showing = false;

  bool get isShowing => _showing;

  FloatingKind get kind {
    if (kIsWeb) return FloatingKind.none;
    try {
      if (Platform.isAndroid) return FloatingKind.androidOverlay;
      if (Platform.isIOS) return FloatingKind.iosPictureInPicture;
      if (Platform.isMacOS) return FloatingKind.macosPanel;
    } catch (_) {
      // Platform is unavailable under some test harnesses.
    }
    return FloatingKind.none;
  }

  /// Whether a floating window is possible on this platform at all.
  bool get isSupported => kind != FloatingKind.none;

  /// Whether the window offers a deafen control.
  ///
  /// iOS does not. Picture in Picture exposes play/pause and the two skip
  /// buttons and nothing else, and those go to talk, mute and hang up —
  /// deafen is the one that gives way, being a comfort setting rather than a
  /// control the call needs. It stays available in the app.
  bool get hasDeafen => hasDeafenFor(kind);

  static bool hasDeafenFor(FloatingKind kind) =>
      kind != FloatingKind.iosPictureInPicture && kind != FloatingKind.none;

  void _ensureHandler() {
    if (_handlerInstalled) return;
    _handlerInstalled = true;
    _channel.setMethodCallHandler((call) async {
      switch (call.method) {
        case 'setTransmitting':
          onTransmit?.call(call.arguments == true);
        case 'toggleMute':
          onToggleMute?.call();
        case 'toggleDeafen':
          onToggleDeafen?.call();
        case 'hangup':
          onHangup?.call();
        case 'pipStatus':
          onStatus?.call(call.arguments as String?);
        case 'dismissed':
          // Deliberately leaves [_showing] alone. It means "the floating
          // window is switched on", not "a window is on screen this instant" —
          // and those parted company once the window started closing whenever
          // the app came to the front and opening again when it left.
          //
          // Clearing it here stopped every update: the window came back
          // showing the last frame drawn before it closed, with a stale
          // connection state and a talk button that answered to nothing,
          // because nothing was being sent to it any more.
          onDismissed?.call();
      }
      return null;
    });
  }

  /// Called when the platform tore the window down on its own.
  VoidCallback? onDismissed;

  /// Called with why the window did not appear, or null once it has.
  ///
  /// Everything that can go wrong does so well after [show] has returned, so
  /// there is nothing for it to return and nowhere for the reason to land
  /// unless the platform pushes it.
  void Function(String? message)? onStatus;

  /// Whether the permission the window needs has been granted. Only Android
  /// gates this; elsewhere there is nothing to ask for.
  Future<bool> hasPermission() async {
    if (!isSupported) return false;
    if (kind != FloatingKind.androidOverlay) return true;
    try {
      return await _channel.invokeMethod<bool>('hasPermission') ?? false;
    } on PlatformException {
      return false;
    } on MissingPluginException {
      return false;
    }
  }

  /// Opens the system settings page for the overlay permission. Android does
  /// not allow granting it from an in-app prompt.
  Future<bool> requestPermission() async {
    if (kind != FloatingKind.androidOverlay) return true;
    try {
      return await _channel.invokeMethod<bool>('requestPermission') ?? false;
    } on PlatformException {
      return false;
    } on MissingPluginException {
      return false;
    }
  }

  /// Shows the window. Returns an error message, or null on success.
  Future<String?> show() async {
    if (!isSupported) return 'Floating windows are not available here.';
    _ensureHandler();
    try {
      await _channel.invokeMethod<bool>('show');
      _showing = true;
      return null;
    } on PlatformException catch (e) {
      if (e.code == 'permission') {
        return 'Allow "display over other apps" first.';
      }
      return e.message ?? 'Could not show the floating window.';
    } on MissingPluginException {
      return 'Floating windows are not available here.';
    }
  }

  /// Hands the window its wording.
  ///
  /// Separate from [update] because it changes when the language does, which
  /// is roughly never, while update runs ten times a second.
  Future<void> setPhrases(Map<String, String> phrases) async {
    if (!isSupported) return;
    try {
      await _channel.invokeMethod<void>('phrases', phrases);
    } catch (_) {
      // An older platform side, or one that draws no text of its own.
    }
  }

  Future<void> hide() async {
    if (!isSupported) return;
    try {
      await _channel.invokeMethod<bool>('hide');
    } catch (_) {
      // Already gone.
    }
    _showing = false;
  }

  /// Maps a dBFS reading onto the 0..1 range the meters draw in.
  ///
  /// Deliberately the same function the on-screen meters use. The floating
  /// window draws its own meter in native code, and a window showing half full
  /// where the app shows three quarters is worse than showing nothing: the
  /// whole point of a meter is that a length means a loudness.
  static double meterFraction(double db) => VoiceMeter.fractionFor(db);

  /// Pushes the current call state onto the window.
  ///
  /// Cheap and idempotent; the caller throttles rather than this, because only
  /// the caller knows when something actually changed.
  Future<void> update({
    required List<String> names,
    required List<({String name, double levelDb})> speakers,
    required bool transmitting,
    required int micMode,
    required bool live,
    required bool connected,
    required String connectionText,
    required int connectionLevel,
    required String moreSpeakers,
    required int connectedCount,
    required int reconnectingCount,
    required int failedCount,
    required bool muted,
    required bool deafened,
    required double levelDb,
    required double thresholdDb,
    required double noiseFloorDb,
    required bool speaking,
  }) async {
    if (!isSupported || !_showing) return;
    try {
      await _channel.invokeMethod<void>('update', {
        'names': names,
        // Names and levels together, on the same scale as every other meter in
        // the app. A name alone says somebody is connected; a name with a
        // level says they are being heard, which is the thing in doubt when a
        // helmet has gone quiet.
        'speakers': [
          for (final s in speakers)
            {'name': s.name, 'level': meterFraction(s.levelDb)},
        ],
        'transmitting': transmitting,
        'micMode': micMode,
        'live': live,
        'connected': connected,
        'connectionText': connectionText,
        'connectionLevel': connectionLevel,
        'moreSpeakers': moreSpeakers,
        'connectedCount': connectedCount,
        'reconnectingCount': reconnectingCount,
        'failedCount': failedCount,
        'muted': muted,
        'deafened': deafened,
        'level': meterFraction(levelDb),
        'threshold': meterFraction(thresholdDb),
        'noiseFloor': meterFraction(noiseFloorDb),
        'speaking': speaking,
      });
    } catch (_) {
      // The service may have been killed; the next show() re-establishes it.
    }
  }
}
