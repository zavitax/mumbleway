import 'dart:io' show Platform;

import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';

/// Bridge to the floating push-to-talk island.
///
/// Only Android can draw over other apps. iOS forbids system-wide overlays
/// outright — no entitlement exists for it at any privilege level — so
/// [isSupported] is false there and the UI hides the option rather than
/// offering something that cannot work. Desktop keeps the window itself
/// available, so an overlay adds little.
class OverlayBridge {
  OverlayBridge._();
  static final OverlayBridge instance = OverlayBridge._();

  static const _channel = MethodChannel('mumbleway/overlay');

  /// Called when the island's talk button is pressed or released.
  void Function(bool transmitting)? onTransmit;

  bool _handlerInstalled = false;
  bool _showing = false;

  bool get isShowing => _showing;

  /// Whether a floating island is possible on this platform at all.
  bool get isSupported {
    if (kIsWeb) return false;
    try {
      return Platform.isAndroid;
    } catch (_) {
      return false;
    }
  }

  void _ensureHandler() {
    if (_handlerInstalled) return;
    _handlerInstalled = true;
    _channel.setMethodCallHandler((call) async {
      if (call.method == 'setTransmitting') {
        onTransmit?.call(call.arguments == true);
      }
      return null;
    });
  }

  /// Whether "display over other apps" has been granted.
  Future<bool> hasPermission() async {
    if (!isSupported) return false;
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
    if (!isSupported) return false;
    try {
      return await _channel.invokeMethod<bool>('requestPermission') ?? false;
    } on PlatformException {
      return false;
    } on MissingPluginException {
      return false;
    }
  }

  /// Shows the island. Returns an error message, or null on success.
  Future<String?> show() async {
    if (!isSupported) return 'Floating overlays are not available here.';
    _ensureHandler();
    try {
      await _channel.invokeMethod<bool>('show');
      _showing = true;
      return null;
    } on PlatformException catch (e) {
      if (e.code == 'permission') {
        return 'Allow "display over other apps" first.';
      }
      return e.message ?? 'Could not show the overlay.';
    } on MissingPluginException {
      return 'Floating overlays are not available here.';
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

  /// Pushes the current speakers and transmit state onto the island.
  ///
  /// Cheap and idempotent; the caller throttles rather than this, because only
  /// the caller knows when something actually changed.
  Future<void> update({
    required List<String> names,
    required bool transmitting,
    required bool connected,
  }) async {
    if (!isSupported || !_showing) return;
    try {
      await _channel.invokeMethod<void>('update', {
        'names': names,
        'transmitting': transmitting,
        'connected': connected,
      });
    } catch (_) {
      // The service may have been killed; the next show() re-establishes it.
    }
  }
}
