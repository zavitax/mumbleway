import 'package:flutter/material.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mumbleway/l10n/app_localizations.dart';
import 'package:mumbleway/src/rust/api/mumbleway.dart';
import 'package:mumbleway/state/app_state.dart';
import 'package:mumbleway/widgets/channel_panel.dart';
import 'package:mumbleway/widgets/server_card.dart';
import 'package:shared_preferences/shared_preferences.dart';

/// Being suppressed is the one way of going inaudible that nothing else on
/// screen knows about: the meter moves, the gate opens, the mute button says
/// the rider is live, and the server is discarding every word because they
/// lack Speak permission where they are standing.
///
/// So what is pinned here is loudness and persistence. A notice that appears
/// once and goes is wrong — the condition lasts until the rider moves.
UiUser rider({bool suppressed = false}) => UiUser(
  session: 7,
  name: 'Anna',
  channelId: 0,
  talking: false,
  muted: false,
  deafened: false,
  localMute: false,
  selfMuted: false,
  selfDeafened: false,
  status: suppressed ? 'suppressed' : 'silent',
  comment: '',
  prioritySpeaker: false,
  suppressed: suppressed,
);

SavedServer server() => SavedServer(
  name: 'Clubhouse',
  host: 'mumble.example',
  port: 64738,
  username: 'rider',
  localId: 'srv',
);

void main() {
  setUp(() => SharedPreferences.setMockInitialValues({}));

  Widget host(AppState state, Widget child) => AppStateScope(
    state: state,
    child: MaterialApp(
      localizationsDelegates: const [
        ...L.localizationsDelegates,
        GlobalMaterialLocalizations.delegate,
        GlobalWidgetsLocalizations.delegate,
      ],
      supportedLocales: L.supportedLocales,
      home: Scaffold(body: SingleChildScrollView(child: child)),
    ),
  );

  group('the rider is told, and kept told', () {
    testWidgets('the card says so for as long as it is true', (tester) async {
      final state = AppState();
      addTearDown(state.dispose);
      state.servers.add(server());
      state.runtimes['srv'] = ServerRuntime()
        ..status = ConnStatus.connected
        ..suppressed = true;

      await tester.pumpWidget(
        host(state, ServerCard(server: server(), showDetails: false)),
      );
      await tester.pump(const Duration(milliseconds: 100));

      final l = await L.delegate.load(const Locale('en'));
      expect(find.text(l.suppressedTitle), findsOneWidget);
      expect(
        find.text(l.suppressedBody),
        findsOneWidget,
        reason: 'the cure — move — has to be on screen with the problem',
      );
    });

    testWidgets('and says nothing when it is not', (tester) async {
      final state = AppState();
      addTearDown(state.dispose);
      state.servers.add(server());
      state.runtimes['srv'] = ServerRuntime()..status = ConnStatus.connected;

      await tester.pumpWidget(
        host(state, ServerCard(server: server(), showDetails: false)),
      );
      await tester.pump(const Duration(milliseconds: 100));

      final l = await L.delegate.load(const Locale('en'));
      expect(find.text(l.suppressedTitle), findsNothing);
    });

    test('the state follows the server, per server', () {
      final state = AppState();
      addTearDown(state.dispose);

      state.onEvent(const AppEvent.suppressed(serverId: 'a', suppressed: true));

      expect(state.runtimeFor('a').suppressed, isTrue);
      expect(state.runtimeFor('b').suppressed, isFalse);
    });

    test('it is announced every time, not once', () async {
      // Unlike a server's suggestions, this is not advice to take once. A
      // rider who moves into a silent channel twice needs telling twice.
      final state = AppState();
      addTearDown(state.dispose);
      final seen = <bool>[];
      final sub = state.suppressions.listen(seen.add);
      addTearDown(sub.cancel);

      state
        ..onEvent(const AppEvent.suppressed(serverId: 'a', suppressed: true))
        ..onEvent(const AppEvent.suppressed(serverId: 'a', suppressed: false))
        ..onEvent(const AppEvent.suppressed(serverId: 'a', suppressed: true));
      await Future<void>.delayed(Duration.zero);

      expect(seen, [true, false, true]);
    });
  });

  testWidgets('a suppressed rider in the roster is not drawn as merely quiet', (
    tester,
  ) async {
    final state = AppState();
    addTearDown(state.dispose);
    state.runtimes['srv'] = ServerRuntime()..status = ConnStatus.connected;

    await tester.pumpWidget(
      host(
        state,
        ChannelUserList(serverId: 'srv', users: [rider(suppressed: true)]),
      ),
    );
    await tester.pump(const Duration(milliseconds: 50));

    expect(find.byIcon(Icons.voice_over_off), findsOneWidget);
    // **Beside the name, not in place of the rider.** The leading glyph is who
    // this is — their picture, or the person mark when they have none — and
    // the state rides after the name where the eye already is. It used to
    // replace the person mark, which is why this once asserted the opposite.
    expect(find.byIcon(Icons.person_outline), findsOneWidget);
    final name = tester.getRect(find.text('Anna'));
    final glyph = tester.getRect(find.byIcon(Icons.voice_over_off));
    expect(glyph.left, greaterThan(name.right));
  });
}
