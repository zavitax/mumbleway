import 'package:flutter/material.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mumbleway/l10n/app_localizations.dart';
import 'package:mumbleway/src/rust/api/mumbleway.dart';
import 'package:mumbleway/state/app_state.dart';
import 'package:mumbleway/widgets/channel_panel.dart';

/// The server decides what a rider may do, and used to say so only by refusing
/// an action after it was tapped. Now it is asked in advance.
///
/// Three things are pinned here, and each one has an opposite failure: offering
/// an action that cannot work, hiding one that can, and greying out the whole
/// menu in the second before the server has answered — which reads as a broken
/// app and was the reason `known` exists at all.
UiRights rights({
  bool known = true,
  bool muteDeafen = false,
  bool kick = false,
  bool selfRegister = false,
}) => UiRights(
  known: known,
  speak: true,
  muteDeafen: muteDeafen,
  moveUsers: false,
  text: true,
  whisper: true,
  makeChannel: false,
  write: false,
  kick: kick,
  ban: false,
  registerOthers: false,
  selfRegister: selfRegister,
);

UiUser rider({String name = 'Anna', String? mumbleway}) => UiUser(
  session: 7,
  name: name,
  channelId: 0,
  talking: false,
  muted: false,
  deafened: false,
  localMute: false,
  status: 'silent',
  comment: '',
  prioritySpeaker: false,
  suppressed: false,
  mumblewayVersion: mumbleway,
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
      home: Scaffold(
        body: ChannelUserList(serverId: 'srv', users: [user]),
      ),
    ),
  );

  /// Opens the participant menu and reports whether the entry is offered.
  Future<bool> offered(
    WidgetTester tester,
    String label, {
    required UiRights has,
    UiUser? user,
  }) async {
    final state = AppState();
    addTearDown(state.dispose);
    state.runtimes['srv'] = ServerRuntime()
      ..status = ConnStatus.connected
      ..rights = has;
    // Torn all the way down first. Pumping a second `MaterialApp` reuses the
    // element tree, and with it the Navigator — so the menu opened by the
    // previous check is still mounted, and `find.text` reads that one. Every
    // assertion after the first in a test then answers about the wrong rights,
    // convincingly.
    await tester.pumpWidget(const SizedBox.shrink());
    await tester.pump();
    await tester.pumpWidget(host(state, user ?? rider()));
    await tester.pump(const Duration(milliseconds: 50));
    await tester.tap(find.byIcon(Icons.more_vert));
    await tester.pump(const Duration(milliseconds: 400));

    final item = tester.widget<PopupMenuItem<String>>(
      find
          .ancestor(
            of: find.text(label),
            matching: find.byType(PopupMenuItem<String>),
          )
          .first,
    );
    return item.enabled;
  }

  testWidgets('before the server answers, everything is offered', (
    tester,
  ) async {
    final l = await L.delegate.load(const Locale('en'));
    expect(
      await offered(tester, l.kickFromServer, has: rights(known: false)),
      isTrue,
      reason: 'a menu greyed out while waiting reads as a broken app',
    );
  });

  testWidgets('what the server has refused is greyed out', (tester) async {
    final l = await L.delegate.load(const Locale('en'));
    expect(await offered(tester, l.kickFromServer, has: rights()), isFalse);
    expect(
      await offered(tester, l.deafenOnServer, has: rights()),
      isFalse,
      reason: 'deafening has no request to fall back on',
    );
  });

  testWidgets('what the server allows is offered', (tester) async {
    final l = await L.delegate.load(const Locale('en'));
    expect(
      await offered(tester, l.kickFromServer, has: rights(kick: true)),
      isTrue,
    );
    expect(
      await offered(tester, l.muteOnServer, has: rights(muteDeafen: true)),
      isTrue,
    );
  });

  testWidgets('muting a MumbleWay rider needs no permission', (tester) async {
    // The whole point of the request: no permission, and it still works,
    // because it reaches their app rather than the server's ACL.
    final l = await L.delegate.load(const Locale('en'));
    expect(
      await offered(
        tester,
        l.muteOnServer,
        has: rights(),
        user: rider(mumbleway: '1.0.1'),
      ),
      isTrue,
    );
    expect(
      await offered(tester, l.muteOnServer, has: rights()),
      isFalse,
      reason: 'somebody on another Mumble client has no such backup',
    );
  });
}
