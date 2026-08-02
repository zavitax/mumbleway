import 'dart:io' show Platform;

import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';

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

  /// Whether the platform gives the window its own hang-up control.
  ///
  /// iOS does not: Picture in Picture exposes play/pause and the two skip
  /// buttons and nothing else, and those are spent on talk, mute and deafen.
  bool get hasHangup => hasHangupFor(kind);

  static bool hasHangupFor(FloatingKind kind) =>
      kind != FloatingKind.iosPictureInPicture &&
      kind != FloatingKind.none;

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
        case 'dismissed':
          // The user closed it from the platform side rather than from
          // settings, so the toggle has to follow.
          _showing = false;
          onDismissed?.call();
      }
      return null;
    });
  }

  /// Called when the platform tore the window down on its own.
  VoidCallback? onDismissed;

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

  Future<void> hide() async {
    if (!isSupported) return;
    try {
      await _channel.invokeMethod<bool>('hide');
    } catch (_) {
      // Already gone.
    }
    _showing = false;
  }

  /// Pushes the current call state onto the window.
  ///
  /// Cheap and idempotent; the caller throttles rather than this, because only
  /// the caller knows when something actually changed.
  Future<void> update({
    required List<String> names,
    required bool transmitting,
    required bool connected,
    required bool muted,
    required bool deafened,
  }) async {
    if (!isSupported || !_showing) return;
    try {
      await _channel.invokeMethod<void>('update', {
        'names': names,
        'transmitting': transmitting,
        'connected': connected,
        'muted': muted,
        'deafened': deafened,
      });
    } catch (_) {
      // The service may have been killed; the next show() re-establishes it.
    }
  }

  /// iOS only: whether dismissing the Picture in Picture window also leaves
  /// the server.
  ///
  /// Off by default. Picture in Picture has no spare button for hang-up, and
  /// wiring it to the close button by default would mean tidying the window
  /// away drops the call — a mistake that is silent until someone talks into a
  /// dead connection.
  Future<void> setCloseHangsUp({required bool value}) async {
    if (kind != FloatingKind.iosPictureInPicture) return;
    try {
      await _channel.invokeMethod<void>('setCloseHangsUp', value);
    } catch (_) {
      // Nothing to configure if the controller is not up yet; show() re-sends.
    }
  }
}
