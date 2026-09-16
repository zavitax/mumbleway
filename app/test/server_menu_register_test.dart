import 'package:flutter/material.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mumbleway/l10n/app_localizations.dart';
import 'package:mumbleway/src/rust/api/mumbleway.dart';
import 'package:mumbleway/state/app_state.dart';
import 'package:mumbleway/widgets/server_card.dart';

/// Registering is the one item in the server menu that needs a live session.
///
/// **It is the mirror image of Edit and Remove**, which need the opposite —
/// they rebuild or discard the session and are disabled while one is up.
/// Getting the sense backwards would produce a menu that looks right and
/// offers the action exactly when it cannot work, so the direction is pinned
/// here rather than left to the reader of the widget.
void main() {
  // `localId` is what `ServerCard` reads back as `server.id`, and the runtime
  // below has to be filed under exactly that.
  final server = SavedServer(
    name: 'Clubhouse',
    host: 'mumble.example',
    port: 64738,
    username: 'rider',
    localId: 'srv',
  );

  Widget host(AppState state) => AppStateScope(
    state: state,
    child: MaterialApp(
      localizationsDelegates: const [
        ...L.localizationsDelegates,
        GlobalMaterialLocalizations.delegate,
        GlobalWidgetsLocalizations.delegate,
      ],
      supportedLocales: L.supportedLocales,
      home: Scaffold(
        body: SingleChildScrollView(
          child: ServerCard(server: server, showDetails: false),
        ),
      ),
    ),
  );

  Future<void> openMenu(WidgetTester tester, ConnStatus status) async {
    final state = AppState();
    addTearDown(state.dispose);
    state.runtimes['srv'] = ServerRuntime()..status = status;
    await tester.pumpWidget(host(state));
    await tester.pump(const Duration(milliseconds: 100));
    await tester.tap(find.byIcon(Icons.more_horiz));
    // Fixed pumps rather than `pumpAndSettle`: the card keeps a live status
    // indicator going, so the tree is never quiet and settling never returns.
    // `diagnostic_recording_test.dart` hit the same wall.
    await tester.pump(const Duration(milliseconds: 400));
  }

  /// Whether the menu entry carrying [text] is offered or greyed out.
  bool enabled(WidgetTester tester, String text) {
    final tile = tester.widget<ListTile>(
      find.ancestor(of: find.text(text), matching: find.byType(ListTile)).first,
    );
    return tile.enabled;
  }

  testWidgets('offered while connected', (tester) async {
    await openMenu(tester, ConnStatus.connected);
    expect(find.text('Register on this server'), findsOneWidget);
    expect(enabled(tester, 'Register on this server'), isTrue);
  });

  testWidgets('greyed out, with the reason, while disconnected', (tester) async {
    await openMenu(tester, ConnStatus.idle);
    // Present rather than hidden: the menu keeps its shape whatever the
    // connection is doing, which is the convention the rest of it follows.
    expect(find.text('Register on this server'), findsOneWidget);
    expect(enabled(tester, 'Register on this server'), isFalse);
    expect(find.text('Connect first'), findsOneWidget);
  });

  testWidgets('still unavailable while a connection is only being chased', (
    tester,
  ) async {
    // Reconnecting is not connected. There is no session to register on, and
    // `isLive` is the only state that means there is one.
    await openMenu(tester, ConnStatus.reconnecting);
    expect(enabled(tester, 'Register on this server'), isFalse);
  });

  testWidgets('the sense is opposite to Edit, which needs no session', (
    tester,
  ) async {
    await openMenu(tester, ConnStatus.connected);
    // The two answer different questions and must not agree: registering is a
    // request made *on* a connection, editing replaces the one underneath.
    expect(enabled(tester, 'Register on this server'), isTrue);
    expect(enabled(tester, 'Edit'), isFalse);
  });
}
