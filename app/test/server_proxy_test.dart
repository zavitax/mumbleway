import 'package:flutter_test/flutter_test.dart';
import 'package:mumbleway/services/server_proxy.dart';
import 'package:mumbleway/src/rust/api/mumbleway.dart';
import 'package:mumbleway/state/app_state.dart';
import 'package:shared_preferences/shared_preferences.dart';

/// How a server decides what to dial through.
///
/// **Four answers and one of them is "ask the app".** The resolution is the
/// only place that knows all three facts — what the entry says, what the app is
/// set to, and whether the rider asked for one to be reached through the other
/// — and getting it wrong is not visible: a server that silently goes direct on
/// a network that blocks it reads as the proxy feature not working, and one
/// that silently goes through a proxy sends a rider's traffic somewhere they
/// did not choose.
void main() {
  setUp(() async {
    SharedPreferences.setMockInitialValues({});
    await ServerProxies.instance.load();
  });

  ServerProxy proxy(String host, {int port = 1080, ProxyScheme? scheme}) =>
      ServerProxy(
        scheme: scheme ?? ProxyScheme.socks5,
        host: host,
        port: port,
        username: null,
        password: null,
        tunnelVoice: false,
      );

  group('resolving a server', () {
    test('direct means direct, whatever the app is set to', () async {
      await ServerProxies.instance.setDefault(
        mode: DefaultProxyMode.custom,
        proxy: proxy('10.0.0.1'),
      );
      expect(
        ServerProxies.instance.chainFor(mode: ServerProxyMode.direct),
        isEmpty,
        reason: 'the local rig must stay direct on a proxied network',
      );
    });

    test('the default is used by a server that names none', () async {
      await ServerProxies.instance.setDefault(
        mode: DefaultProxyMode.custom,
        proxy: proxy('10.0.0.1'),
      );
      final chain = ServerProxies.instance.chainFor(
        mode: ServerProxyMode.useDefault,
      );
      expect(chain.map((p) => p.host), ['10.0.0.1']);
    });

    test('and no default means a direct connection, not a failure', () {
      expect(
        ServerProxies.instance.chainFor(mode: ServerProxyMode.useDefault),
        isEmpty,
      );
    });

    test('a server with its own proxy uses it alone by default', () async {
      await ServerProxies.instance.setDefault(
        mode: DefaultProxyMode.custom,
        proxy: proxy('10.0.0.1'),
      );
      final chain = ServerProxies.instance.chainFor(
        mode: ServerProxyMode.custom,
        own: proxy('192.168.1.1', port: 8080),
      );
      expect(chain.map((p) => p.host), ['192.168.1.1'], reason: 'no chaining asked for');
    });

    test('and is wrapped in the app\'s when the rider asked for that', () async {
      await ServerProxies.instance.setDefault(
        mode: DefaultProxyMode.custom,
        proxy: proxy('10.0.0.1'),
        chainServerProxies: true,
      );
      final chain = ServerProxies.instance.chainFor(
        mode: ServerProxyMode.custom,
        own: proxy('192.168.1.1', port: 8080),
      );
      expect(
        chain.map((p) => p.host),
        ['10.0.0.1', '192.168.1.1'],
        reason: 'outermost first: the app\'s proxy is asked for the server\'s',
      );
    });

    test('a proxy is never dialled through itself', () async {
      // Chaining on, and both settings naming the same machine. Asking it to
      // reach itself is a loop, not a tunnel.
      await ServerProxies.instance.setDefault(
        mode: DefaultProxyMode.custom,
        proxy: proxy('10.0.0.1'),
        chainServerProxies: true,
      );
      final chain = ServerProxies.instance.chainFor(
        mode: ServerProxyMode.custom,
        own: proxy('10.0.0.1'),
      );
      expect(chain.length, 1);
    });

    test('credentials are found by address, not by server', () async {
      final withLogin = ServerProxy(
        scheme: ProxyScheme.socks5,
        host: '10.0.0.1',
        port: 1080,
        username: 'rider',
        password: 'secret',
        tunnelVoice: false,
      );
      await ServerProxies.instance.rememberCredentials(withLogin);

      // A different server naming the same proxy, with nothing typed in.
      final chain = ServerProxies.instance.chainFor(
        mode: ServerProxyMode.custom,
        own: proxy('10.0.0.1'),
      );
      expect(chain.single.username, 'rider');
      expect(chain.single.password, 'secret');
    });
  });

  group('reading what a rider typed', () {
    test('a plain host and port', () {
      final p = parseProxy('10.0.0.1:1080', scheme: ProxyScheme.socks5);
      expect(p?.host, '10.0.0.1');
      expect(p?.port, 1080);
    });

    test('a scheme in front of it is dropped', () {
      final p = parseProxy('http://proxy.example:8000', scheme: ProxyScheme.httpConnect);
      expect(p?.host, 'proxy.example');
      expect(p?.port, 8000);
    });

    test('an IPv6 literal keeps its brackets off and its colons', () {
      final p = parseProxy('[2001:db8::1]:1080', scheme: ProxyScheme.socks5);
      expect(p?.host, '2001:db8::1');
      expect(p?.port, 1080);
    });

    test('no port is no proxy', () {
      // Guessing 8080 would send a rider's traffic somewhere they did not name.
      expect(parseProxy('proxy.example', scheme: ProxyScheme.httpConnect), isNull);
      expect(parseProxy('proxy.example:', scheme: ProxyScheme.httpConnect), isNull);
      expect(parseProxy('proxy.example:70000', scheme: ProxyScheme.httpConnect), isNull);
      expect(parseProxy('  ', scheme: ProxyScheme.httpConnect), isNull);
    });

    test('an encoded proxy round-trips, voice flag included', () {
      final p = ServerProxy(
        scheme: ProxyScheme.socks5,
        host: '10.0.0.1',
        port: 1080,
        username: null,
        password: null,
        tunnelVoice: true,
      );
      final back = decodeProxy(encodeProxy(p));
      expect(back?.scheme, ProxyScheme.socks5);
      expect(back?.host, '10.0.0.1');
      expect(back?.port, 1080);
      expect(back?.tunnelVoice, isTrue);
    });

    test('and the encoded form never carries the credentials', () {
      final p = ServerProxy(
        scheme: ProxyScheme.httpConnect,
        host: 'proxy.example',
        port: 8000,
        username: 'rider',
        password: 'secret',
        tunnelVoice: false,
      );
      expect(encodeProxy(p), isNot(contains('secret')));
      expect(encodeProxy(p), isNot(contains('rider')));
    });
  });

  group('receiving', () {
    // The other half of sharing, and the half that was broken: the link
    // carried the proxy, the parser found it, and the entry built from it had
    // none — so the form opened saying "Global settings" and the rider was
    // handed a server they could not reach.
    test('a proxy in an invitation reaches the entry built from it', () {
      final entry = SavedServer.fromConfig(
        ServerConfig(
          id: 'x',
          name: 'Through SOCKS',
          host: 'voice.example.com',
          port: 6033,
          username: 'rider',
          accessTokens: const ['rideboss'],
          proxyChain: [proxy('10.0.0.1', port: 1080)],
        ),
      );
      expect(entry.proxyMode, ServerProxyMode.custom);
      expect(entry.proxy?.host, '10.0.0.1');
      expect(entry.proxy?.port, 1080);
      // Lost by the same factory, and with the same result: an admin whose
      // token never arrives finds every moderation action greyed out.
      expect(entry.accessTokens, ['rideboss']);
    });

    test('and an invitation without one defers to the app', () {
      final entry = SavedServer.fromConfig(
        ServerConfig(
          id: 'x',
          name: 'Plain',
          host: 'voice.example.com',
          port: 6033,
          username: 'rider',
          accessTokens: const [],
          proxyChain: const [],
        ),
      );
      // Not `direct`: a link cannot say Direct, and assuming it would override
      // a rider's own app-wide setting with the sharer's silence.
      expect(entry.proxyMode, ServerProxyMode.useDefault);
      expect(entry.proxy, isNull);
    });
  });

  group('sharing', () {
    SavedServer entry({
      ServerProxyMode mode = ServerProxyMode.useDefault,
      ServerProxy? own,
    }) => SavedServer(
      name: 'Rig',
      host: 'example.test',
      port: 64738,
      username: 'rider',
      proxyMode: mode,
      proxy: own,
    );

    test('a proxy the rider chose for this server travels', () {
      final p = proxy('10.0.0.1');
      expect(
        AppState.sharedProxy(entry(mode: ServerProxyMode.custom, own: p)),
        [p],
      );
    });

    test('the app-wide default never does', () async {
      // It is a fact about the sharer's network, not about the server, and
      // leaking it would hand strangers a route through a machine nobody
      // volunteered.
      await ServerProxies.instance.setDefault(
        mode: DefaultProxyMode.custom,
        proxy: proxy('10.0.0.1'),
      );
      expect(AppState.sharedProxy(entry(mode: ServerProxyMode.useDefault)), isEmpty);
      expect(AppState.sharedProxy(entry(mode: ServerProxyMode.direct)), isEmpty);
    });
  });

  group('the probe', () {
    test('is skipped exactly when voice goes through the proxy', () async {
      // It is a bare UDP datagram, so where voice cannot go, it cannot go —
      // and sending it anyway reads as "not responding" beside a card that
      // connects perfectly.
      final tunnelled = ServerProxy(
        scheme: ProxyScheme.httpConnect,
        host: 'proxy.example',
        port: 8000,
        username: null,
        password: null,
        tunnelVoice: true,
      );
      final direct = ServerProxy(
        scheme: ProxyScheme.httpConnect,
        host: 'proxy.example',
        port: 8000,
        username: null,
        password: null,
        tunnelVoice: false,
      );

      expect(
        ServerProxies.instance
            .chainFor(mode: ServerProxyMode.custom, own: tunnelled)
            .any((p) => p.tunnelVoice),
        isTrue,
      );
      expect(
        ServerProxies.instance
            .chainFor(mode: ServerProxyMode.custom, own: direct)
            .any((p) => p.tunnelVoice),
        isFalse,
        reason: 'voice still goes direct, so the probe tells the truth',
      );
    });
  });

  group('a saved entry', () {
    SavedServer entry({
      ServerProxyMode mode = ServerProxyMode.useDefault,
      ServerProxy? own,
    }) => SavedServer(
      name: 'Rig',
      host: 'example.test',
      port: 64738,
      username: 'rider',
      proxyMode: mode,
      proxy: own,
    );

    test('survives the round trip through JSON', () {
      final before = entry(
        mode: ServerProxyMode.custom,
        own: proxy('10.0.0.1'),
      );
      final after = SavedServer.fromJson(before.toJson());
      expect(after.proxyMode, ServerProxyMode.custom);
      expect(after.proxy?.host, '10.0.0.1');
      expect(after.proxy?.scheme, ProxyScheme.socks5);
    });

    test('and one saved before proxies existed defers to the app', () {
      final old = SavedServer.fromJson({
        'localId': 'srv',
        'name': 'Rig',
        'host': 'example.test',
        'port': 64738,
        'username': 'rider',
      });
      expect(old.proxyMode, ServerProxyMode.useDefault);
      expect(old.proxy, isNull);
    });

    test('choosing Direct clears the proxy it named', () {
      final was = entry(mode: ServerProxyMode.custom, own: proxy('10.0.0.1'));
      final now = was.copyWith(
        proxyMode: ServerProxyMode.direct,
        clearProxy: true,
      );
      expect(now.proxy, isNull);
      expect(now.proxyMode, ServerProxyMode.direct);
    });

    test('a changed proxy is a changed connection', () {
      // Or a proxy arriving by sync would sit in the settings file and take
      // effect at some unrelated moment later.
      final a = entry(mode: ServerProxyMode.custom, own: proxy('10.0.0.1'));
      final b = entry(mode: ServerProxyMode.custom, own: proxy('10.0.0.2'));
      expect(a.sameConnection(b), isFalse);
      expect(a.sameConnection(a.copyWith(name: 'Another name')), isTrue);
      expect(
        a.sameConnection(a.copyWith(proxyMode: ServerProxyMode.direct)),
        isFalse,
      );
    });
  });
}
