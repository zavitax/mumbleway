import 'package:flutter/material.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mumbleway/l10n/app_localizations.dart';
import 'package:mumbleway/src/rust/api/mumbleway.dart';
import 'package:mumbleway/state/app_state.dart';
import 'package:mumbleway/widgets/channel_panel.dart';

/// The roster badge marking somebody whose client said it is MumbleWay.
///
/// The handshake that decides this is tested in the core
/// (`core/src/session/peers.rs`). What is tested here is only what reaches a
/// rider: that the badge appears for a reported version and never otherwise,
/// and that it says what it means in words a screen reader can speak.
void main() {
  UiUser user(String name, {String? mumbleway}) => UiUser(
    session: name.hashCode & 0xffff,
    name: name,
    channelId: 0,
    talking: false,
    muted: false,
    deafened: false,
    localMute: false,
    mutedYou: false,
    userId: null,
    selfMuted: false,
    selfDeafened: false,
    status: 'silent',
    comment: '',
    prioritySpeaker: false,
    suppressed: false,
    mumblewayVersion: mumbleway,
  );

  Widget host(List<UiUser> users, {Locale locale = const Locale('en')}) {
    final state = AppState();
    addTearDown(state.dispose);
    return AppStateScope(
      state: state,
      child: MaterialApp(
        locale: locale,
        localizationsDelegates: const [
          ...L.localizationsDelegates,
          GlobalMaterialLocalizations.delegate,
          GlobalWidgetsLocalizations.delegate,
        ],
        supportedLocales: L.supportedLocales,
        home: Scaffold(
          body: SingleChildScrollView(
            child: ChannelUserList(serverId: 'srv', users: users),
          ),
        ),
      ),
    );
  }

  testWidgets('marks somebody who identified themselves', (tester) async {
    await tester.pumpWidget(host([user('Anna', mumbleway: '1.0.1')]));
    await tester.pump();
    expect(find.byType(MumblewayBadge), findsOneWidget);
    expect(find.byTooltip('Uses MumbleWay 1.0.1'), findsOneWidget);
  });

  testWidgets('marks nobody who has not', (tester) async {
    // Absence is not proof — an older MumbleWay says nothing — so the only
    // honest thing to draw for "not known" is nothing at all.
    await tester.pumpWidget(host([user('Boris')]));
    await tester.pump();
    expect(find.byType(MumblewayBadge), findsNothing);
  });

  testWidgets('only the people who said so, in a mixed channel', (
    tester,
  ) async {
    await tester.pumpWidget(
      host([
        user('Anna', mumbleway: '1.0.1'),
        user('Boris'),
        user('Clara', mumbleway: '1.0.2'),
      ]),
    );
    await tester.pump();
    expect(find.byType(MumblewayBadge), findsNWidgets(2));
    expect(find.byTooltip('Uses MumbleWay 1.0.1'), findsOneWidget);
    expect(find.byTooltip('Uses MumbleWay 1.0.2'), findsOneWidget);
  });

  testWidgets('a peer that gave no version still reads cleanly', (
    tester,
  ) async {
    // An empty version identifies the app without the build. The label must
    // not end in a stray space, which a screen reader would pause on.
    await tester.pumpWidget(host([user('Anna', mumbleway: '')]));
    await tester.pump();
    expect(find.byType(MumblewayBadge), findsOneWidget);
    expect(find.byTooltip('Uses MumbleWay'), findsOneWidget);
  });

  testWidgets('says it in Russian too', (tester) async {
    await tester.pumpWidget(
      host([user('Анна', mumbleway: '1.0.1')], locale: const Locale('ru')),
    );
    await tester.pump();
    expect(find.byTooltip('Использует MumbleWay 1.0.1'), findsOneWidget);
  });

  testWidgets('a long name still truncates with the badge in view', (
    tester,
  ) async {
    // The badge sits after the name, so a name long enough to fill the row
    // must give way to it rather than push it off the edge.
    await tester.binding.setSurfaceSize(const Size(360, 640));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    await tester.pumpWidget(
      host([
        user(
          'Vladimir-Konstantinovich-Rostropovich-Longname',
          mumbleway: '1.0.1',
        ),
      ]),
    );
    await tester.pump();

    final badge = tester.getRect(find.byType(MumblewayBadge));
    expect(badge.right, lessThanOrEqualTo(360), reason: 'pushed off the row');
    expect(tester.takeException(), isNull, reason: 'the row overflowed');
  });
}
