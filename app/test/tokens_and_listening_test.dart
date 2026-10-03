import 'dart:convert';
import 'dart:typed_data';

import 'package:flutter/material.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mumbleway/l10n/app_localizations.dart';
import 'package:mumbleway/src/rust/api/mumbleway.dart';
import 'package:mumbleway/state/app_state.dart';
import 'package:mumbleway/widgets/channel_panel.dart';
import 'package:shared_preferences/shared_preferences.dart';

/// Two ways of reaching a channel that is not yours: a token that opens a shut
/// one, and listening to one without leaving your own.
///
/// The protocol halves are verified against a real Murmur in
/// `core/tests/live_server.rs` — including the detail that makes tokens work at
/// all, which is that a server writes the group as `#name` and the rider types
/// `name`. What is pinned here is what the app keeps and what it draws.
SavedServer server({List<String> tokens = const []}) => SavedServer(
  name: 'Clubhouse',
  host: 'mumble.example',
  port: 64738,
  username: 'rider',
  localId: 'srv',
  accessTokens: tokens,
);

const _channels = [
  UiChannel(id: 0, name: 'Root', description: '', userCount: 1, maxUsers: 0),
  UiChannel(id: 1, name: 'Garage', description: '', userCount: 0, maxUsers: 0),
];

void main() {
  setUp(() => SharedPreferences.setMockInitialValues({}));

  group('access tokens', () {
    test('are kept with the server, through storage', () {
      final saved = SavedServer.fromJson(
        jsonDecode(jsonEncode(server(tokens: ['vip', 'crew']).toJson()))
            as Map<String, dynamic>,
      );
      expect(saved.accessTokens, ['vip', 'crew']);
    });

    test('an entry from before they existed has none', () {
      expect(SavedServer.fromJson({'localId': 'srv'}).accessTokens, isEmpty);
    });

    test('they are part of how the session is built', () {
      // Unlike a note or the last channel: a token decides which channels a
      // connection can enter, so a changed set is a different connection.
      final before = server(tokens: ['vip']);
      expect(before.sameConnection(before.copyWith(accessTokens: [])), isFalse);
      expect(
        before.sameConnection(before.copyWith(accessTokens: ['vip'])),
        isTrue,
      );
    });

    test('they reach the core with the server', () {
      expect(server(tokens: ['vip']).toConfig().accessTokens, ['vip']);
    });

    test('setting them saves even with nothing connected', () async {
      // The point of keeping them: they have to be there for the *next*
      // handshake, not only for a session that happens to be up.
      final state = AppState();
      addTearDown(state.dispose);
      state.servers.add(server());

      final error = await state.setAccessTokensFor('srv', ['  vip  ', '', 'crew']);

      expect(error, isNull);
      expect(
        state.servers.single.accessTokens,
        ['vip', 'crew'],
        reason: 'trimmed, and the empty one dropped',
      );
    });
  });

  group('listening to another channel', () {
    testWidgets('every channel but the one you are in offers a headphone', (
      tester,
    ) async {
      final state = AppState();
      addTearDown(state.dispose);
      state.runtimes['srv'] = ServerRuntime()..status = ConnStatus.connected;

      await tester.pumpWidget(
        AppStateScope(
          state: state,
          child: MaterialApp(
            localizationsDelegates: const [
              ...L.localizationsDelegates,
              GlobalMaterialLocalizations.delegate,
              GlobalWidgetsLocalizations.delegate,
            ],
            supportedLocales: L.supportedLocales,
            home: const Scaffold(
              body: SingleChildScrollView(
                child: ChannelTree(
                  serverId: 'srv',
                  channels: _channels,
                  currentChannelId: 0,
                  defaultChannelName: null,
                ),
              ),
            ),
          ),
        ),
      );
      await tester.pump(const Duration(milliseconds: 50));

      // One headphone, for Garage: being in Root is already hearing it.
      expect(find.byIcon(Icons.headset_outlined), findsOneWidget);
      expect(find.byIcon(Icons.headset), findsNothing);
    });

    testWidgets('the headphone follows the server, not the tap', (
      tester,
    ) async {
      // A listen can be refused — for want of the Listen permission, or
      // because a limit is reached — so the icon fills in only once the
      // server has said it is listening.
      final state = AppState();
      addTearDown(state.dispose);
      state.runtimes['srv'] = ServerRuntime()..status = ConnStatus.connected;
      state.onEvent(
        AppEvent.listening(
          serverId: 'srv',
          channels: Uint32List.fromList([1]),
        ),
      );

      await tester.pumpWidget(
        AppStateScope(
          state: state,
          child: MaterialApp(
            localizationsDelegates: const [
              ...L.localizationsDelegates,
              GlobalMaterialLocalizations.delegate,
              GlobalWidgetsLocalizations.delegate,
            ],
            supportedLocales: L.supportedLocales,
            home: const Scaffold(
              body: SingleChildScrollView(
                child: ChannelTree(
                  serverId: 'srv',
                  channels: _channels,
                  currentChannelId: 0,
                  defaultChannelName: null,
                ),
              ),
            ),
          ),
        ),
      );
      await tester.pump(const Duration(milliseconds: 50));

      expect(find.byIcon(Icons.headset), findsOneWidget);
      expect(find.byIcon(Icons.headset_outlined), findsNothing);
    });

    test('what is listened to belongs to the server it came from', () {
      final state = AppState();
      addTearDown(state.dispose);

      state.onEvent(
        AppEvent.listening(
          serverId: 'a',
          channels: Uint32List.fromList([1, 2]),
        ),
      );

      expect(state.runtimeFor('a').listening, [1, 2]);
      expect(state.runtimeFor('b').listening, isEmpty);
    });
  });
}
