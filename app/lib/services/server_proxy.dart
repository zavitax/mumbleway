import 'dart:convert';

import 'package:shared_preferences/shared_preferences.dart';

import '../src/rust/api/mumbleway.dart';
import 'proxy.dart';

/// What a saved server says about reaching it.
///
/// **Four answers, because three of them are not "a proxy".** A server may
/// refuse one on purpose, defer to whatever the app is set to, or name its own
/// — and the difference between the first two is the whole point: a rider who
/// sets a default for a hostile network still wants their local rig dialled
/// directly.
enum ServerProxyMode {
  /// Never through a proxy, whatever the app's default says.
  direct,

  /// Whatever Settings says. The default for every entry, including imports.
  useDefault,

  /// The one named on the entry itself.
  custom,
}

/// What the app does when a server has nothing to say.
enum DefaultProxyMode {
  /// No proxy for anything that does not name one.
  direct,

  /// Whatever the operating system and the environment already say, which is
  /// what [SystemProxy] resolves for downloads.
  ///
  /// **Offered, never assumed.** A proxy a browser uses is usually an HTTP one
  /// whose allow-list stops at 443, so adopting it for a Mumble port by default
  /// would break connections that work today on every machine with a proxy set
  /// in its registry.
  system,

  /// The one named here.
  custom,
}

/// Proxies as the app stores and resolves them.
///
/// The addresses live in ordinary preferences; the usernames and passwords live
/// beside them, **keyed by the proxy rather than by the server**, for two
/// reasons. A credential belongs to the proxy — two servers behind one
/// corporate proxy share it, and the app-wide default has no server to hang it
/// on. And the sync machinery lifts exactly one secret per saved server into
/// the platform's keystore; a second one does not fit that shape, and left in
/// the entry it would travel to iCloud or Android Backup in clear, which the
/// privacy policy says the app does not do.
///
/// None of this is synced. An address that exists on one network is a broken
/// connection on another.
class ServerProxies {
  ServerProxies._();

  static final ServerProxies instance = ServerProxies._();

  static const _prefsDefaultMode = 'mumbleway.proxyServerMode';
  static const _prefsDefault = 'mumbleway.proxyServerDefault';
  static const _prefsChain = 'mumbleway.proxyServerChain';
  static const _prefsCredentials = 'mumbleway.proxyAuth';

  DefaultProxyMode _mode = DefaultProxyMode.direct;
  ServerProxy? _custom;
  bool _chainServerProxies = false;
  Map<String, ({String? user, String? password})> _credentials = {};

  /// What the app does for a server that names no proxy of its own.
  DefaultProxyMode get mode => _mode;

  /// The app-wide proxy, when one is set.
  ServerProxy? get custom => _custom;

  /// Whether a server's own proxy is itself dialled through the app's.
  bool get chainServerProxies => _chainServerProxies;

  Future<void> load() async {
    final prefs = await SharedPreferences.getInstance();
    _mode = switch (prefs.getString(_prefsDefaultMode)) {
      'system' => DefaultProxyMode.system,
      'custom' => DefaultProxyMode.custom,
      _ => DefaultProxyMode.direct,
    };
    _custom = decodeProxy(prefs.getString(_prefsDefault));
    _chainServerProxies = prefs.getBool(_prefsChain) ?? false;
    _credentials = _decodeCredentials(prefs.getString(_prefsCredentials));
  }

  Future<void> setDefault({
    required DefaultProxyMode mode,
    ServerProxy? proxy,
    bool? chainServerProxies,
  }) async {
    _mode = mode;
    _custom = proxy ?? _custom;
    if (chainServerProxies != null) _chainServerProxies = chainServerProxies;
    final prefs = await SharedPreferences.getInstance();
    await prefs.setString(_prefsDefaultMode, switch (mode) {
      DefaultProxyMode.system => 'system',
      DefaultProxyMode.custom => 'custom',
      DefaultProxyMode.direct => 'direct',
    });
    if (_custom case final p?) {
      await prefs.setString(_prefsDefault, encodeProxy(p));
      await rememberCredentials(p);
    }
    await prefs.setBool(_prefsChain, _chainServerProxies);
  }

  /// Keeps a proxy's username and password on this device, under its address.
  Future<void> rememberCredentials(ServerProxy proxy) async {
    final key = keyFor(proxy);
    if ((proxy.username ?? '').isEmpty && (proxy.password ?? '').isEmpty) {
      _credentials.remove(key);
    } else {
      _credentials[key] = (user: proxy.username, password: proxy.password);
    }
    final prefs = await SharedPreferences.getInstance();
    await prefs.setString(
      _prefsCredentials,
      jsonEncode({
        for (final e in _credentials.entries)
          e.key: {'u': e.value.user, 'p': e.value.password},
      }),
    );
  }

  /// The same proxy with whatever credentials this device holds for it.
  ServerProxy withCredentials(ServerProxy proxy) {
    final held = _credentials[keyFor(proxy)];
    if (held == null) return proxy;
    return ServerProxy(
      scheme: proxy.scheme,
      host: proxy.host,
      port: proxy.port,
      username: proxy.username ?? held.user,
      password: proxy.password ?? held.password,
      tunnelVoice: proxy.tunnelVoice,
    );
  }

  /// The app-wide proxy, resolved — `null` where there is none.
  ServerProxy? get resolvedDefault => switch (_mode) {
    DefaultProxyMode.direct => null,
    DefaultProxyMode.custom => _custom == null ? null : withCredentials(_custom!),
    // The downloads proxy already knows how to find one; this reuses the
    // answer rather than the plumbing, and only because a rider asked for it.
    DefaultProxyMode.system => parseProxy(
      SystemProxy.instance.config.proxy,
      scheme: ProxyScheme.httpConnect,
    ),
  };

  /// The chain to dial a server through, outermost first.
  ///
  /// Empty is a direct connection. A server's own proxy is wrapped in the
  /// app's only when the rider asked for that, and a chain of one proxy
  /// through itself collapses — dialling a proxy through itself is a loop, not
  /// a tunnel.
  List<ServerProxy> chainFor({
    required ServerProxyMode mode,
    ServerProxy? own,
  }) {
    final fallback = resolvedDefault;
    switch (mode) {
      case ServerProxyMode.direct:
        return const [];
      case ServerProxyMode.useDefault:
        return fallback == null ? const [] : [fallback];
      case ServerProxyMode.custom:
        if (own == null) return fallback == null ? const [] : [fallback];
        final theirs = withCredentials(own);
        if (!_chainServerProxies || fallback == null) return [theirs];
        if (sameEndpoint(fallback, theirs)) return [theirs];
        return [fallback, theirs];
    }
  }

  /// Two proxies are the same hop when they are the same address, whatever
  /// else differs.
  static bool sameEndpoint(ServerProxy a, ServerProxy b) =>
      a.scheme == b.scheme &&
      a.host.toLowerCase() == b.host.toLowerCase() &&
      a.port == b.port;

  /// The key a credential is filed under: the address and nothing else.
  static String keyFor(ServerProxy proxy) =>
      '${proxy.scheme.name}://${proxy.host.toLowerCase()}:${proxy.port}';

  Map<String, ({String? user, String? password})> _decodeCredentials(String? raw) {
    if (raw == null || raw.isEmpty) return {};
    try {
      final decoded = jsonDecode(raw) as Map<String, dynamic>;
      return {
        for (final e in decoded.entries)
          if (e.value case final Map<String, dynamic> v)
            e.key: (user: v['u'] as String?, password: v['p'] as String?),
      };
    } catch (_) {
      // A settings file somebody edited by hand is not worth a crash at
      // startup; the rider types the password again.
      return {};
    }
  }
}

/// `scheme://host:port` with the credentials left out, for storing and showing.
String encodeProxy(ServerProxy proxy) =>
    '${proxy.scheme.name}://${proxy.host}:${proxy.port}'
    '${proxy.tunnelVoice ? '?voice=1' : ''}';

/// The inverse, forgiving about what a rider may have typed.
ServerProxy? decodeProxy(String? text) {
  if (text == null || text.trim().isEmpty) return null;
  var rest = text.trim();
  var scheme = ProxyScheme.httpConnect;
  var voice = false;

  if (rest.contains('?')) {
    final parts = rest.split('?');
    rest = parts.first;
    voice = parts.last.contains('voice=1');
  }
  final mark = rest.indexOf('://');
  if (mark >= 0) {
    final named = rest.substring(0, mark).toLowerCase();
    scheme = switch (named) {
      'socks5' || 'socks' => ProxyScheme.socks5,
      _ => ProxyScheme.httpConnect,
    };
    rest = rest.substring(mark + 3);
  }
  return parseProxy(rest, scheme: scheme, tunnelVoice: voice);
}

/// Reads a `host:port`, bracketed IPv6 included, into a proxy.
///
/// Returns null rather than guessing: a proxy with no port is not a proxy, and
/// silently picking 8080 would send a rider's traffic somewhere they did not
/// name.
ServerProxy? parseProxy(
  String? hostPort, {
  required ProxyScheme scheme,
  bool tunnelVoice = false,
}) {
  if (hostPort == null) return null;
  // The downloads proxy already strips `http://` and friends; reuse it rather
  // than growing a second parser that disagrees with the first.
  final text = SystemProxy.stripScheme(hostPort.trim());
  if (text.isEmpty) return null;

  String host;
  String port;
  if (text.startsWith('[')) {
    final close = text.indexOf(']');
    if (close < 0 || close + 2 >= text.length || text[close + 1] != ':') {
      return null;
    }
    host = text.substring(1, close);
    port = text.substring(close + 2);
  } else {
    final colon = text.lastIndexOf(':');
    if (colon <= 0 || colon == text.length - 1) return null;
    host = text.substring(0, colon);
    port = text.substring(colon + 1);
  }

  final number = int.tryParse(port.split('/').first);
  if (number == null || number < 1 || number > 65535) return null;
  if (host.trim().isEmpty) return null;

  return ServerProxy(
    scheme: scheme,
    host: host.trim(),
    port: number,
    username: null,
    password: null,
    tunnelVoice: tunnelVoice,
  );
}
