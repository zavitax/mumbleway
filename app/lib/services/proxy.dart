import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:http/http.dart' as http;
import 'package:http/io_client.dart';

/// Where a proxy setting came from.
enum ProxySource {
  /// No proxy in use.
  none,

  /// The operating system's configured proxy.
  system,

  /// `HTTP_PROXY` / `HTTPS_PROXY` environment variables.
  environment,

  /// Entered by the user.
  manual,
}

/// A resolved proxy configuration.
class ProxyConfig {
  const ProxyConfig({required this.source, this.proxy, this.bypass = const []});

  final ProxySource source;

  /// `host:port`, or null when going direct.
  final String? proxy;

  /// Host patterns that bypass the proxy.
  final List<String> bypass;

  bool get isDirect => proxy == null || proxy!.isEmpty;

  String get description => switch (source) {
        ProxySource.none => 'Direct connection',
        ProxySource.system => 'System proxy · $proxy',
        ProxySource.environment => 'Environment proxy · $proxy',
        ProxySource.manual => 'Manual proxy · $proxy',
      };
}

/// Detects the operating system's proxy and builds HTTP clients that use it.
///
/// Dart's own `findProxyFromEnvironment` only reads `HTTP_PROXY` and friends.
/// On Windows those are usually empty while a proxy *is* configured — it lives
/// in the registry, and only applications that ask for it there find one. That
/// difference is exactly why a request can succeed in a browser and fail from
/// the same machine's command line.
///
/// The registry is read with `reg query` rather than through FFI bindings:
/// one short-lived subprocess at startup, against a stable command-line
/// interface, instead of type signatures that shift between package versions.
class SystemProxy {
  SystemProxy._();
  static final SystemProxy instance = SystemProxy._();

  static const _registryPath =
      r'HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings';

  ProxyConfig _config = const ProxyConfig(source: ProxySource.none);
  String? _manual;

  ProxyConfig get config => _config;

  /// Whether to route through a proxy at all. On by default: a machine behind a
  /// proxy usually cannot reach anything without one, and detection reports
  /// "direct" when there is none, so defaulting to on costs nothing.
  ///
  /// Takes effect on the next [refresh].
  bool enabled = true;

  String? get manualProxy => _manual;

  /// Overrides detection with an explicit `host:port`. Null or empty clears it.
  set manualProxy(String? value) {
    _manual = (value == null || value.trim().isEmpty) ? null : value.trim();
  }

  /// Reads the platform configuration. Call once at startup and after changing
  /// [enabled] or [manualProxy].
  Future<ProxyConfig> refresh() async {
    if (!enabled) {
      _config = const ProxyConfig(source: ProxySource.none);
      return _config;
    }
    if (_manual != null) {
      _config = ProxyConfig(source: ProxySource.manual, proxy: _manual);
      return _config;
    }

    // An explicitly exported proxy is a deliberate override of whatever the OS
    // is configured with, so it wins.
    for (final key in [
      'HTTPS_PROXY',
      'https_proxy',
      'HTTP_PROXY',
      'http_proxy',
    ]) {
      final v = Platform.environment[key];
      if (v != null && v.trim().isNotEmpty) {
        _config = ProxyConfig(
          source: ProxySource.environment,
          proxy: stripScheme(v.trim()),
          bypass: splitBypass(Platform.environment['NO_PROXY'] ??
              Platform.environment['no_proxy']),
        );
        return _config;
      }
    }

    if (!kIsWeb && Platform.isWindows) {
      final win = await _readWindowsProxy();
      if (win != null) {
        _config = win;
        return _config;
      }
    }

    _config = const ProxyConfig(source: ProxySource.none);
    return _config;
  }

  Future<ProxyConfig?> _readWindowsProxy() async {
    try {
      final enable = await _regQuery('ProxyEnable');
      // REG_DWORD comes back as hex, e.g. "0x1".
      if (enable == null || int.tryParse(enable.replaceFirst('0x', ''), radix: 16) != 1) {
        return null;
      }
      final server = await _regQuery('ProxyServer');
      if (server == null || server.trim().isEmpty) return null;

      return ProxyConfig(
        source: ProxySource.system,
        proxy: pickFromWindowsValue(server),
        bypass: splitBypass(await _regQuery('ProxyOverride')),
      );
    } catch (_) {
      // No registry, no proxy; going direct is the safe default.
      return null;
    }
  }

  /// Returns the value of a registry entry, or null if absent.
  Future<String?> _regQuery(String name) async {
    final result = await Process.run(
      'reg',
      ['query', _registryPath, '/v', name],
      runInShell: true,
    );
    if (result.exitCode != 0) return null;

    // Output shape: "    ProxyServer    REG_SZ    http://host:port"
    for (final line in (result.stdout as String).split('\n')) {
      final trimmed = line.trim();
      if (!trimmed.startsWith(name)) continue;
      final parts = trimmed.split(RegExp(r'\s{2,}|\t+'));
      if (parts.length >= 3) return parts.sublist(2).join(' ').trim();
    }
    return null;
  }

  /// Windows stores either a single `host:port` or a per-scheme list such as
  /// `http=host:1;https=host:2`. HTTPS is preferred, since that is what this
  /// app actually fetches over.
  @visibleForTesting
  static String pickFromWindowsValue(String value) {
    if (!value.contains('=')) return stripScheme(value);
    for (final scheme in ['https', 'http']) {
      for (final part in value.split(';')) {
        final kv = part.split('=');
        if (kv.length == 2 && kv[0].trim().toLowerCase() == scheme) {
          return stripScheme(kv[1].trim());
        }
      }
    }
    return stripScheme(value.split(';').first);
  }

  @visibleForTesting
  static String stripScheme(String v) =>
      v.replaceFirst(RegExp(r'^\w+://'), '').trim();

  @visibleForTesting
  static List<String> splitBypass(String? raw) {
    if (raw == null || raw.trim().isEmpty) return const [];
    return raw
        .split(RegExp(r'[;,]'))
        .map((s) => s.trim())
        .where((s) => s.isNotEmpty)
        .toList();
  }

  /// Whether [host] should bypass the proxy under [bypass] patterns.
  @visibleForTesting
  static bool hostBypasses(String host, List<String> bypass) {
    for (final pattern in bypass) {
      if (pattern == '<local>') {
        // Windows' shorthand for "any name without a dot".
        if (!host.contains('.')) return true;
        continue;
      }
      final bare = pattern.replaceAll('*', '');
      if (pattern.startsWith('*') && host.endsWith(bare)) return true;
      if (pattern.endsWith('*') && host.startsWith(bare)) return true;
      if (host == pattern) return true;
    }
    return false;
  }

  /// An HTTP client honouring the most recently resolved configuration.
  http.Client createClient() {
    final cfg = _config;
    if (cfg.isDirect) return http.Client();

    // Not a cascade: `..findProxy = <lambda>` evaluates to the lambda's return
    // type, so a following `..` would bind to that rather than the client.
    final inner = HttpClient();
    inner.findProxy = (uri) =>
        hostBypasses(uri.host, cfg.bypass) ? 'DIRECT' : 'PROXY ${cfg.proxy}';
    inner.connectionTimeout = const Duration(seconds: 20);

    return IOClient(inner);
  }
}
