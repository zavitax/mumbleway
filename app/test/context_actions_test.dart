import 'package:flutter/material.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mumbleway/l10n/app_localizations.dart';
import 'package:mumbleway/src/rust/api/mumbleway.dart';
import 'package:mumbleway/state/app_state.dart';
import 'package:mumbleway/widgets/channel_panel.dart';

/// Entries a server puts into this rider's menus — a recording bot, a ride
/// organiser's script. The app cannot know what any of them does, so what is
/// pinned here is where they are allowed to appear and that one cannot be
/// mistaken for an action of ours.
UiContextAction action(
  String id, {
  String label = 'Start recording',
  bool user = true,
  bool channel = false,
  bool server = false,
}) => UiContextAction(
  action: id,
  label: label,
  forUser: user,
  forChannel: channel,
  forServer: server,
);

UiUser rider() => const UiUser(
  session: 7,
  name: 'Anna',
  channelId: 0,
  talking: false,
  muted: false,
  deafened: false,
  localMute: false,
  status: 'silent',
  comment: '',
  prioritySpeaker: false,
  suppressed: false,
);

void main() {
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
        body: ChannelUserList(serverId: 'srv', users: [rider()]),
      ),
    ),
  );

  Future<void> openMenu(WidgetTester tester, AppState state) async {
    await tester.pumpWidget(host(state));
    await tester.pump(const Duration(milliseconds: 50));
    await tester.tap(find.byIcon(Icons.more_vert));
    await tester.pump(const Duration(milliseconds: 400));
  }

  testWidgets('an action meant for users appears in their menu', (
    tester,
  ) async {
    final state = AppState();
    addTearDown(state.dispose);
    state.runtimes['srv'] = ServerRuntime()
      ..status = ConnStatus.connected
      ..contextActions = [action('bot/rec')];

    await openMenu(tester, state);

    expect(find.text('Start recording'), findsOneWidget);
  });

  testWidgets('an action meant for the server stays out of a rider menu', (
    tester,
  ) async {
    final state = AppState();
    addTearDown(state.dispose);
    state.runtimes['srv'] = ServerRuntime()
      ..status = ConnStatus.connected
      ..contextActions = [
        action('bot/all', label: 'Call everyone', user: false, server: true),
      ];

    await openMenu(tester, state);

    expect(find.text('Call everyone'), findsNothing);
  });

  testWidgets('a server cannot register an entry that runs ours', (
    tester,
  ) async {
    // The menu is keyed by string. An action registered as "kick" must not
    // reach the kick handler, which is why the server's entries carry a
    // prefix that its own identifiers cannot forge.
    final state = AppState();
    addTearDown(state.dispose);
    state.runtimes['srv'] = ServerRuntime()
      ..status = ConnStatus.connected
      ..contextActions = [action('kick', label: 'Definitely not a kick')];

    await openMenu(tester, state);
    final item = tester.widget<PopupMenuItem<String>>(
      find
          .ancestor(
            of: find.text('Definitely not a kick'),
            matching: find.byType(PopupMenuItem<String>),
          )
          .first,
    );

    expect(item.value, 'server:kick');
    expect(item.value, isNot('kick'));
  });

  test('actions belong to the server that registered them', () {
    final state = AppState();
    addTearDown(state.dispose);

    state.onEvent(
      AppEvent.contextActions(serverId: 'a', actions: [action('bot/rec')]),
    );

    expect(state.runtimeFor('a').contextActions, hasLength(1));
    expect(state.runtimeFor('b').contextActions, isEmpty);
  });
}
