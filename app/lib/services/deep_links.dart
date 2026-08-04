import 'dart:async';

import 'package:flutter/services.dart';

/// `mumble://` links opened from outside the app.
///
/// A link tapped in a message, or one a phone's camera app found in a QR code
/// and offered as a notification, arrives as a system intent rather than
/// through anything this app polls. Both platforms deliver it to native code,
/// which forwards it here.
///
/// Two ways in, because a link can arrive before or after the interface
/// exists. A cold start delivers it to the platform first and Flutter second,
/// so the native side holds it and hands it over when [initialLink] asks; a
/// link that arrives while the app is already running comes down [links]. A
/// listener that only did one of the two would work perfectly in testing and
/// miss every link that launched the app.
class DeepLinks {
  DeepLinks._();

  static final DeepLinks instance = DeepLinks._();

  static const MethodChannel _channel = MethodChannel('mumbleway/links');

  final StreamController<String> _controller =
      StreamController<String>.broadcast();

  /// Links arriving while the app is running.
  Stream<String> get links => _controller.stream;

  bool _listening = false;

  /// Starts forwarding, and returns the link the app was launched with.
  ///
  /// Idempotent, because the app state can be rebuilt without the process
  /// restarting and a second handler would deliver every link twice.
  Future<String?> start() async {
    if (!_listening) {
      _listening = true;
      _channel.setMethodCallHandler((call) async {
        if (call.method == 'link') {
          final url = call.arguments as String?;
          if (url != null && url.isNotEmpty) _controller.add(url);
        }
        return null;
      });
    }
    try {
      // Drains whatever the platform was holding. Returns null on desktop,
      // where nothing registers the scheme and the channel is unimplemented.
      return await _channel.invokeMethod<String>('initialLink');
    } on PlatformException {
      return null;
    } on MissingPluginException {
      return null;
    }
  }
}
