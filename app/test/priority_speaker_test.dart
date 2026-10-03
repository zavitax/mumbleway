import 'package:flutter/material.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mumbleway/l10n/app_localizations.dart';
import 'package:mumbleway/src/rust/api/mumbleway.dart';
import 'package:mumbleway/state/app_state.dart';
import 'package:mumbleway/widgets/channel_panel.dart';

/// A priority speaker makes everybody else quieter while they talk, and the
/// server does that without telling anyone. Riders hear the channel duck and
/// have no way to know why — which is the whole reason this badge exists, and
/// the reason it must not appear on an ordinary rider.
UiUser rider({bool priority = false}) => UiUser(
  session: 7,
  name: 'Anna',
  channelId: 0,
  talking: false,
  muted: false,
  deafened: false,
  localMute: false,
  selfMuted: false,
  selfDeafened: false,
  status: 'silent',
  comment: '',
  prioritySpeaker: priority,
  suppressed: false,
);

void main() {
  Widget host(AppState state, UiUser user) => AppStateScope(
    state: state,
    child: MaterialApp(
      localizationsDelegates: const [
        ...L.localizationsDelegates,
        GlobalMaterialLocalizations.delegate,
        GlobalWidgetsLocalizations.delegate,
      ],
      supportedLocales: L.supportedLocales,
      home: Scaffold(body: ChannelUserList(serverId: 'srv', users: [user])),
    ),
  );

  testWidgets('a priority speaker is marked, and says why when asked', (
    tester,
  ) async {
    final state = AppState();
    addTearDown(state.dispose);
    state.runtimes['srv'] = ServerRuntime()..status = ConnStatus.connected;

    await tester.pumpWidget(host(state, rider(priority: true)));
    await tester.pump(const Duration(milliseconds: 50));
    expect(find.byIcon(Icons.campaign_outlined), findsOneWidget);

    await tester.tap(find.byIcon(Icons.campaign_outlined));
    await tester.pump(const Duration(milliseconds: 100));
    final l = await L.delegate.load(const Locale('en'));
    expect(find.text(l.prioritySpeaker), findsOneWidget);
  });

  testWidgets('everybody else is unmarked', (tester) async {
    final state = AppState();
    addTearDown(state.dispose);
    state.runtimes['srv'] = ServerRuntime()..status = ConnStatus.connected;

    await tester.pumpWidget(host(state, rider()));
    await tester.pump(const Duration(milliseconds: 50));

    expect(find.byIcon(Icons.campaign_outlined), findsNothing);
  });
}
