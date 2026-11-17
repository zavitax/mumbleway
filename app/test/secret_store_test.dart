import 'dart:convert';

import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mumbleway/services/secret_store.dart';
import 'package:mumbleway/services/server_proxy.dart';
import 'package:mumbleway/src/rust/api/mumbleway.dart';
import 'package:mumbleway/state/app_state.dart';
import 'package:shared_preferences/shared_preferences.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  const secretsChannel = MethodChannel('mumbleway/secrets');

  late Map<String, String> backing;
  late bool failReads;
  late bool failWrites;
  late List<String> channelCalls;

  void installMockChannel() {
    TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
        .setMockMethodCallHandler(secretsChannel, (MethodCall call) async {
          channelCalls.add(call.method);
          switch (call.method) {
            case 'readAll':
              if (failReads) {
                throw PlatformException(
                  code: 'keystore-unreadable',
                  message: 'decrypt failed',
                );
              }
              return Map<String, String>.from(backing);
            case 'writeAll':
              if (failWrites) {
                throw PlatformException(code: 'write-failed', message: 'denied');
              }
              final args = Map<String, dynamic>.from(call.arguments as Map);
              for (final entry in args.entries) {
                if (entry.value is String) {
                  backing[entry.key] = entry.value as String;
                }
              }
              return null;
            case 'deleteKeys':
              final keys = List<String>.from(call.arguments as List);
              for (final key in keys) {
                backing.remove(key);
              }
              return null;
            default:
              throw MissingPluginException();
          }
        });
  }

  setUp(() async {
    backing = <String, String>{};
    failReads = false;
    failWrites = false;
    channelCalls = <String>[];
    SecretStore.instance.resetForTesting();
    SharedPreferences.setMockInitialValues({});
    installMockChannel();
    await ServerProxies.instance.load();
    channelCalls.clear();
  });

  tearDown(() async {
    // Allow unawaited refreshPings() calls from AppState mutations to settle
    // before any state is disposed.
    await Future<void>.delayed(Duration.zero);
    TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
        .setMockMethodCallHandler(secretsChannel, null);
    SecretStore.instance.resetForTesting();
  });

  group('SecretStore channel wrapper', () {
    test('degrades cleanly when the native channel is absent', () async {
      TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
          .setMockMethodCallHandler(secretsChannel, null);
      SecretStore.instance.resetForTesting();

      expect(await SecretStore.instance.readAll(), isEmpty);
      expect(SecretStore.instance.isAvailable, isFalse);
      expect(SecretStore.instance.lastReadFailed, isFalse);
      expect(
        await SecretStore.instance.writeAll({'server:a': 'pw'}),
        isFalse,
      );
      expect(SecretStore.instance.isAvailable, isFalse);
      expect(await SecretStore.instance.deleteKeys(['server:a']), isFalse);
      expect(SecretStore.instance.isAvailable, isFalse);
    });

    test('tracks PlatformException read failures without marking store absent', () async {
      failReads = true;
      expect(await SecretStore.instance.readAll(), isEmpty);
      expect(SecretStore.instance.isAvailable, isTrue);
      expect(SecretStore.instance.lastReadFailed, isTrue);

      failReads = false;
      backing['server:one'] = 'pw1';
      expect(await SecretStore.instance.readAll(), {'server:one': 'pw1'});
      expect(SecretStore.instance.isAvailable, isTrue);
      expect(SecretStore.instance.lastReadFailed, isFalse);
    });

    test('round-trips writeAll, readAll and deleteKeys', () async {
      expect(
        await SecretStore.instance.writeAll({
          'server:one': 'pw1',
          'server:two': 'pw2',
        }),
        isTrue,
      );
      expect(SecretStore.instance.isAvailable, isTrue);
      expect(await SecretStore.instance.readAll(), {
        'server:one': 'pw1',
        'server:two': 'pw2',
      });

      expect(await SecretStore.instance.deleteKeys(['server:one']), isTrue);
      expect(await SecretStore.instance.readAll(), {'server:two': 'pw2'});
    });
  });

  group('server passwords', () {
    test('plaintext passwords are NOT stored in SharedPreferences', () async {
      final state = AppState();
      addTearDown(state.dispose);

      await state.addNewServer(
        SavedServer(
          name: 'Clubhouse',
          host: 'voice.example',
          port: 64738,
          username: 'rider',
          password: 'super-secret-password',
        ),
      );

      expect(backing['server:voice.example:64738'], 'super-secret-password');

      final prefs = await SharedPreferences.getInstance();
      final rawList = prefs.getStringList('mumbleway.servers')!;
      expect(rawList, hasLength(1));
      expect(rawList.single, isNot(contains('super-secret-password')));
      final decoded = jsonDecode(rawList.single) as Map<String, dynamic>;
      expect(decoded.containsKey('password'), isFalse);

      // Updating the password updates SecretStore while keeping prefs clean.
      await state.updateServer(
        state.servers.single.copyWith(password: 'rotated-secret'),
      );
      expect(backing['server:voice.example:64738'], 'rotated-secret');
      final updatedList = prefs.getStringList('mumbleway.servers')!;
      expect(updatedList.single, isNot(contains('rotated-secret')));
      expect(
        (jsonDecode(updatedList.single) as Map<String, dynamic>).containsKey(
          'password',
        ),
        isFalse,
      );

      // A fresh AppState restores the password from SecretStore on startup load.
      final reloaded = AppState();
      addTearDown(reloaded.dispose);
      await reloaded.loadServersForTesting();
      expect(reloaded.servers.single.password, 'rotated-secret');

      // Clearing the password removes it from SecretStore too.
      await reloaded.updateServer(
        reloaded.servers.single.copyWith(clearPassword: true),
      );
      expect(backing.containsKey('server:voice.example:64738'), isFalse);

      await Future<void>.delayed(Duration.zero);
    });

    test('forgetting a server deletes its password from SecretStore', () async {
      final state = AppState();
      addTearDown(state.dispose);

      await state.addNewServer(
        SavedServer(
          name: 'Temp',
          host: 'temp.example',
          port: 64738,
          username: 'rider',
          password: 'temp-password',
        ),
      );
      expect(backing['server:temp.example:64738'], 'temp-password');

      await state.forgetServer('temp.example:64738');
      expect(backing.containsKey('server:temp.example:64738'), isFalse);

      await Future<void>.delayed(Duration.zero);
    });

    test(
      'legacy plaintext passwords are migrated to SecretStore and stripped from SharedPreferences',
      () async {
        SharedPreferences.setMockInitialValues({
          'mumbleway.servers': [
            jsonEncode({
              'localId': 'legacy.example:64738',
              'name': 'Legacy',
              'host': 'legacy.example',
              'port': 64738,
              'username': 'anna',
              'password': 'legacy-password',
            }),
            jsonEncode({
              'localId': 'open.example:64738',
              'name': 'Open',
              'host': 'open.example',
              'port': 64738,
              'username': 'anna',
              'password': null,
            }),
          ],
        });

        final state = AppState();
        addTearDown(state.dispose);
        await state.loadServersForTesting();

        expect(backing['server:legacy.example:64738'], 'legacy-password');
        expect(backing.containsKey('server:open.example:64738'), isFalse);
        expect(state.servers[0].password, 'legacy-password');
        expect(state.servers[1].password, isNull);

        final prefs = await SharedPreferences.getInstance();
        final stored = prefs.getStringList('mumbleway.servers')!;
        expect(stored, hasLength(2));
        for (final entry in stored) {
          expect(entry, isNot(contains('legacy-password')));
          final json = jsonDecode(entry) as Map<String, dynamic>;
          expect(json.containsKey('password'), isFalse);
        }
      },
    );

    test(
      'keeps legacy SharedPreferences passwords if SecretStore migration fails',
      () async {
        failWrites = true;
        SharedPreferences.setMockInitialValues({
          'mumbleway.servers': [
            jsonEncode({
              'localId': 'legacy.example:64738',
              'name': 'Legacy',
              'host': 'legacy.example',
              'port': 64738,
              'username': 'anna',
              'password': 'must-not-lose',
            }),
          ],
        });

        final state = AppState();
        addTearDown(state.dispose);
        await state.loadServersForTesting();

        expect(backing, isEmpty);
        expect(state.servers.single.password, 'must-not-lose');

        final prefs = await SharedPreferences.getInstance();
        final stored = prefs.getStringList('mumbleway.servers')!;
        final json = jsonDecode(stored.single) as Map<String, dynamic>;
        expect(json['password'], 'must-not-lose');
      },
    );

    test(
      'unreadable SecretStore preserves legacy passwords and avoids writeAll or deleteKeys',
      () async {
        backing['server:protected.example:64738'] = 'still-in-native-store';
        failReads = true;
        SecretStore.instance.resetForTesting();
        SharedPreferences.setMockInitialValues({
          'mumbleway.servers': [
            jsonEncode({
              'localId': 'legacy.example:64738',
              'name': 'Legacy &amp; Riders',
              'host': 'legacy.example',
              'port': 64738,
              'username': 'anna',
              'password': 'must-keep-in-prefs',
            }),
          ],
        });

        final state = AppState();
        addTearDown(state.dispose);
        await state.loadServersForTesting();

        expect(SecretStore.instance.isAvailable, isTrue);
        expect(SecretStore.instance.lastReadFailed, isTrue);
        expect(state.servers.single.password, 'must-keep-in-prefs');
        expect(channelCalls, ['readAll']);

        // Subsequent mutations via _persist() must not overwrite or wipe the
        // unreadable native store or strip passwords from SharedPreferences.
        await state.addNewServer(
          SavedServer(
            name: 'Second',
            host: 'second.example',
            port: 64738,
            username: 'rider',
            password: 'second-password',
          ),
        );
        expect(channelCalls, isNot(contains('writeAll')));
        expect(channelCalls, isNot(contains('deleteKeys')));

        await state.forgetServer('legacy.example:64738');
        expect(channelCalls, isNot(contains('writeAll')));
        expect(channelCalls, isNot(contains('deleteKeys')));
        expect(backing, {'server:protected.example:64738': 'still-in-native-store'});

        final prefs = await SharedPreferences.getInstance();
        final stored = prefs.getStringList('mumbleway.servers')!;
        expect(stored, hasLength(1));
        final json = jsonDecode(stored.single) as Map<String, dynamic>;
        expect(json['password'], 'second-password');

        await Future<void>.delayed(Duration.zero);
      },
    );
  });

  group('proxy credentials', () {
    test('proxy credentials are kept out of SharedPreferences', () async {
      const proxyWithLogin = ServerProxy(
        scheme: ProxyScheme.socks5,
        host: '10.0.0.1',
        port: 1080,
        username: 'proxy-user',
        password: 'proxy-password',
        tunnelVoice: false,
      );

      await ServerProxies.instance.setDefault(
        mode: DefaultProxyMode.custom,
        proxy: proxyWithLogin,
      );

      final secretRaw = backing['proxy:socks5://10.0.0.1:1080'];
      expect(secretRaw, isNotNull);
      expect(secretRaw, contains('proxy-password'));

      final prefs = await SharedPreferences.getInstance();
      expect(prefs.getString('mumbleway.proxyAuth'), isNull);
      for (final key in prefs.getKeys()) {
        final value = '${prefs.get(key)}';
        expect(value, isNot(contains('proxy-password')));
        expect(value, isNot(contains('proxy-user')));
      }

      // Reloading restores credentials from SecretStore.
      await ServerProxies.instance.load();
      final resolved = ServerProxies.instance
          .chainFor(mode: ServerProxyMode.useDefault)
          .single;
      expect(resolved.username, 'proxy-user');
      expect(resolved.password, 'proxy-password');

      // Clearing credentials removes them from SecretStore.
      await ServerProxies.instance.rememberCredentials(
        const ServerProxy(
          scheme: ProxyScheme.socks5,
          host: '10.0.0.1',
          port: 1080,
          username: null,
          password: null,
          tunnelVoice: false,
        ),
      );
      expect(backing.containsKey('proxy:socks5://10.0.0.1:1080'), isFalse);
      expect(prefs.getString('mumbleway.proxyAuth'), isNull);
    });

    test(
      'migrates legacy mumbleway.proxyAuth into SecretStore and removes it from SharedPreferences',
      () async {
        SharedPreferences.setMockInitialValues({
          'mumbleway.proxyAuth': jsonEncode({
            'httpConnect://proxy.example:8000': {
              'u': 'legacy-proxy-user',
              'p': 'legacy-proxy-password',
            },
          }),
        });

        await ServerProxies.instance.load();

        expect(
          backing.containsKey('proxy:httpConnect://proxy.example:8000'),
          isTrue,
        );
        final prefs = await SharedPreferences.getInstance();
        expect(prefs.containsKey('mumbleway.proxyAuth'), isFalse);

        final resolved = ServerProxies.instance.withCredentials(
          const ServerProxy(
            scheme: ProxyScheme.httpConnect,
            host: 'proxy.example',
            port: 8000,
            username: null,
            password: null,
            tunnelVoice: false,
          ),
        );
        expect(resolved.username, 'legacy-proxy-user');
        expect(resolved.password, 'legacy-proxy-password');
      },
    );

    test(
      'unreadable SecretStore preserves legacy mumbleway.proxyAuth without migrating or deleting',
      () async {
        backing['proxy:httpConnect://existing.example:8080'] =
            jsonEncode({'u': 'keep', 'p': 'secret'});
        failReads = true;
        SecretStore.instance.resetForTesting();
        SharedPreferences.setMockInitialValues({
          'mumbleway.proxyAuth': jsonEncode({
            'httpConnect://proxy.example:8000': {
              'u': 'legacy-proxy-user',
              'p': 'legacy-proxy-password',
            },
          }),
        });

        await ServerProxies.instance.load();

        expect(SecretStore.instance.isAvailable, isTrue);
        expect(SecretStore.instance.lastReadFailed, isTrue);
        expect(channelCalls, ['readAll']);
        expect(backing, {
          'proxy:httpConnect://existing.example:8080':
              jsonEncode({'u': 'keep', 'p': 'secret'}),
        });

        final prefs = await SharedPreferences.getInstance();
        expect(prefs.containsKey('mumbleway.proxyAuth'), isTrue);
        expect(
          prefs.getString('mumbleway.proxyAuth'),
          contains('legacy-proxy-password'),
        );

        final resolved = ServerProxies.instance.withCredentials(
          const ServerProxy(
            scheme: ProxyScheme.httpConnect,
            host: 'proxy.example',
            port: 8000,
            username: null,
            password: null,
            tunnelVoice: false,
          ),
        );
        expect(resolved.username, 'legacy-proxy-user');
        expect(resolved.password, 'legacy-proxy-password');

        // Editing proxy credentials while the store is unreadable keeps them in
        // SharedPreferences instead of clobbering or clearing the key.
        await ServerProxies.instance.rememberCredentials(
          const ServerProxy(
            scheme: ProxyScheme.socks5,
            host: '10.0.0.2',
            port: 1080,
            username: 'new-user',
            password: 'new-password',
            tunnelVoice: false,
          ),
        );
        expect(channelCalls, ['readAll']);
        expect(prefs.containsKey('mumbleway.proxyAuth'), isTrue);
        expect(
          prefs.getString('mumbleway.proxyAuth'),
          contains('new-password'),
        );
      },
    );
  });
}
