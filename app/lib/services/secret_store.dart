import 'package:flutter/foundation.dart' show visibleForTesting;
import 'package:flutter/services.dart';

/// Local, hardware- or OS-backed storage for credentials that must not sit in
/// plaintext preferences or leave the device in backups.
///
/// Backed per platform by:
/// * Android — Android Keystore AES-256-GCM wrapping `noBackupFilesDir/secrets.enc`
/// * iOS / macOS — local Keychain (`kSecAttrSynchronizable = false`)
/// * Windows — DPAPI (`CryptProtectData`) into `%LOCALAPPDATA%\MumbleWay\secrets.bin`
///
/// In headless `flutter test` or on unsupported platforms, [isAvailable] stays
/// `false` cleanly so callers can fall back without breaking unit tests.
class SecretStore {
  SecretStore._();
  static final SecretStore instance = SecretStore._();

  static const _channel = MethodChannel('mumbleway/secrets');

  bool _isAvailable = false;
  bool _lastReadFailed = false;

  /// Whether the native `mumbleway/secrets` store responded on the last check.
  bool get isAvailable => _isAvailable;

  /// Whether the last [readAll] reached the native store but failed to read or
  /// decrypt existing secrets.
  bool get lastReadFailed => _lastReadFailed;

  static BinaryMessenger? _messengerOrNull() {
    try {
      return ServicesBinding.instance.defaultBinaryMessenger;
    } catch (_) {
      return null;
    }
  }

  /// Reads all stored key-value pairs (`server:<localId>`, `proxy:<key>`).
  ///
  /// Returns an empty map and sets [isAvailable] to `false` when the native
  /// channel is missing or the Flutter services binding is not initialized.
  /// When the native store responds with an error (for example a lost key or
  /// corrupted ciphertext), [isAvailable] remains `true` and [lastReadFailed]
  /// becomes `true`.
  Future<Map<String, String>> readAll() async {
    if (_messengerOrNull() == null) {
      _isAvailable = false;
      _lastReadFailed = false;
      return const {};
    }
    try {
      final raw = await _channel.invokeMapMethod<String, dynamic>('readAll');
      _isAvailable = true;
      _lastReadFailed = false;
      return {
        for (final e in (raw ?? const {}).entries)
          if (e.value is String) e.key: e.value as String,
      };
    } on MissingPluginException {
      _isAvailable = false;
      _lastReadFailed = false;
      return const {};
    } on PlatformException {
      _isAvailable = true;
      _lastReadFailed = true;
      return const {};
    }
  }

  /// Writes or updates [entries] in the native secret store.
  ///
  /// Returns `true` when the store is available and accepted the write.
  Future<bool> writeAll(Map<String, String> entries) async {
    if (entries.isEmpty) return _isAvailable;
    if (_messengerOrNull() == null) {
      _isAvailable = false;
      _lastReadFailed = false;
      return false;
    }
    try {
      await _channel.invokeMethod<void>('writeAll', entries);
      _isAvailable = true;
      return true;
    } on MissingPluginException {
      _isAvailable = false;
      _lastReadFailed = false;
      return false;
    } on PlatformException {
      _isAvailable = true;
      return false;
    }
  }

  /// Deletes [keys] from the native secret store.
  ///
  /// Returns `true` when the store is available and accepted the delete.
  Future<bool> deleteKeys(List<String> keys) async {
    if (keys.isEmpty) return _isAvailable;
    if (_messengerOrNull() == null) {
      _isAvailable = false;
      _lastReadFailed = false;
      return false;
    }
    try {
      await _channel.invokeMethod<void>('deleteKeys', keys);
      _isAvailable = true;
      return true;
    } on MissingPluginException {
      _isAvailable = false;
      _lastReadFailed = false;
      return false;
    } on PlatformException {
      _isAvailable = true;
      return false;
    }
  }

  @visibleForTesting
  void resetForTesting() {
    _isAvailable = false;
    _lastReadFailed = false;
  }
}
