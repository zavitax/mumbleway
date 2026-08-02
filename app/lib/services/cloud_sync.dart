import 'dart:io' show Platform;

import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';

/// Which sync facility a platform offers, if any.
///
/// These are not the same feature wearing different names, and the settings
/// screen says so rather than showing one switch that means something
/// different on each device. What a platform actually provides is not a
/// detail the user can be expected to guess.
enum CloudKind {
  /// iCloud, on iOS and macOS. Continuous and two-way: a server added on the
  /// phone appears on the Mac while both are running.
  icloud,

  /// Android's own backup service. The list rides along with the rest of the
  /// app's data to the user's Google account and comes back when they set up
  /// a new phone — but only then. It is a safety net, not a live link.
  androidBackup,

  /// Nothing. Windows had roaming application data and retired it; there is
  /// no longer a system-provided place to put this. Export and import to a
  /// file still work, and a file saved into OneDrive is the manual version of
  /// the same idea.
  none,
}

/// What one device stores in, and reads back from, the cloud.
class CloudBlob {
  const CloudBlob({required this.payload, required this.secrets});

  /// The server list, as JSON. See `state/server_sync.dart`.
  final String payload;

  /// Server passwords, keyed by `localId`, kept apart from the payload.
  ///
  /// Separated at this boundary because the two want different treatment on
  /// the far side: the list is ordinary synced preferences, while a password
  /// belongs somewhere built to hold one. Which is which is decided by the
  /// platform, not here.
  final Map<String, String> secrets;
}

/// Bridge to whatever the platform offers for syncing the server list.
class CloudSync {
  CloudSync._();
  static final CloudSync instance = CloudSync._();

  static const _channel = MethodChannel('mumbleway/cloud');

  /// Called when another device changed the list.
  ///
  /// Only iCloud raises this; the others have no way to tell us.
  VoidCallback? onRemoteChange;

  bool _handlerInstalled = false;

  CloudKind get kind {
    if (kIsWeb) return CloudKind.none;
    try {
      if (Platform.isIOS || Platform.isMacOS) return CloudKind.icloud;
      if (Platform.isAndroid) return CloudKind.androidBackup;
    } catch (_) {
      // Platform is unavailable under some test harnesses.
    }
    return CloudKind.none;
  }

  /// Whether this platform syncs while the app is running.
  ///
  /// [CloudKind.androidBackup] deliberately fails this: it is real, and it is
  /// worth having, but nothing about it happens on a timescale the app can
  /// observe, so there is no state to poll and no upload to trigger.
  bool get isLive => kind == CloudKind.icloud;

  /// Whether the facility is not merely present but usable — signed in, and
  /// switched on for this app.
  Future<bool> isReady() async {
    if (!isLive) return false;
    try {
      return await _channel.invokeMethod<bool>('available') ?? false;
    } on PlatformException {
      return false;
    } on MissingPluginException {
      return false;
    }
  }

  void _ensureHandler() {
    if (_handlerInstalled) return;
    _handlerInstalled = true;
    _channel.setMethodCallHandler((call) async {
      if (call.method == 'remoteChanged') onRemoteChange?.call();
      return null;
    });
  }

  /// Reads the cloud's copy, or null if there is none or it cannot be read.
  Future<CloudBlob?> read() async {
    if (!isLive) return null;
    _ensureHandler();
    try {
      final r = await _channel.invokeMapMethod<String, dynamic>('read');
      final payload = r?['payload'];
      if (payload is! String || payload.isEmpty) return null;
      return CloudBlob(
        payload: payload,
        secrets: {
          for (final e in (r?['secrets'] as Map? ?? const {}).entries)
            if (e.value is String) '${e.key}': e.value as String,
        },
      );
    } catch (_) {
      return null;
    }
  }

  /// Publishes this device's copy. Returns an error message, or null.
  ///
  /// [liveIds] is every server the list still contains, so the platform can
  /// clear the stored password of one that has gone. A deleted server whose
  /// password outlived it would be waiting to be handed back if the same
  /// address were ever added again.
  Future<String?> write(CloudBlob blob, {required List<String> liveIds}) async {
    if (!isLive) return null;
    _ensureHandler();
    try {
      await _channel.invokeMethod<void>('write', {
        'payload': blob.payload,
        'secrets': blob.secrets,
        'liveIds': liveIds,
      });
      return null;
    } on PlatformException catch (e) {
      return e.message ?? 'iCloud refused the change.';
    } on MissingPluginException {
      return null;
    }
  }
}
