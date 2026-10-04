import 'dart:typed_data';

import 'package:flutter/material.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mumbleway/l10n/app_localizations.dart';
import 'package:mumbleway/src/rust/api/mumbleway.dart';
import 'package:mumbleway/state/app_state.dart';
import 'package:mumbleway/widgets/channel_panel.dart';

/// A rider's own note and picture, as they reach the roster.
///
/// The markup stripping is the core's job and is tested there; what is pinned
/// here is that a note shows up at all, written out under the name, and that a
/// picture arriving and being removed both land — the removal being the half
/// that is easy to forget, since an empty image is how the server says "gone".
UiUser rider({String comment = '', int session = 7}) => UiUser(
  session: session,
  name: 'Anna',
  channelId: 0,
  talking: false,
  muted: false,
  deafened: false,
  localMute: false,
  mutedYou: false,
  selfMuted: false,
  selfDeafened: false,
  status: 'silent',
  comment: comment,
  prioritySpeaker: false,
  suppressed: false,
);

/// The smallest thing `Image.memory` will accept: a 1×1 transparent GIF.
final _pixel = Uint8List.fromList([
  0x47, 0x49, 0x46, 0x38, 0x39, 0x61, 0x01, 0x00, 0x01, 0x00, 0x80, 0x00, //
  0x00, 0x00, 0x00, 0x00, 0xff, 0xff, 0xff, 0x21, 0xf9, 0x04, 0x01, 0x00,
  0x00, 0x00, 0x00, 0x2c, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x01, 0x00,
  0x00, 0x02, 0x02, 0x44, 0x01, 0x00, 0x3b,
]);

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

  testWidgets('a note is written under the name, not hidden behind an icon', (
    tester,
  ) async {
    final state = AppState();
    addTearDown(state.dispose);
    state.runtimes['srv'] = ServerRuntime()..status = ConnStatus.connected;

    await tester.pumpWidget(
      host(state, rider(comment: 'On the A9 heading north')),
    );
    await tester.pump(const Duration(milliseconds: 50));

    // **It used to be an icon holding a tooltip**, which is a thing to
    // discover rather than a thing to read. What riders put in a note is where
    // they are and when they are leaving, and the next rider wants that
    // without being told there is something to tap.
    expect(find.text('On the A9 heading north'), findsOneWidget);
    expect(find.byIcon(Icons.sticky_note_2_outlined), findsNothing);
  });

  testWidgets('a rider with no note has nothing under their name', (
    tester,
  ) async {
    final state = AppState();
    addTearDown(state.dispose);
    state.runtimes['srv'] = ServerRuntime()..status = ConnStatus.connected;

    await tester.pumpWidget(host(state, rider()));
    await tester.pump(const Duration(milliseconds: 50));

    expect(find.byIcon(Icons.sticky_note_2_outlined), findsNothing);
    expect(find.text('Anna'), findsOneWidget);
  });

  testWidgets('a picture is drawn, and taking it away takes it off the row', (
    tester,
  ) async {
    final state = AppState();
    addTearDown(state.dispose);
    state.runtimes['srv'] = ServerRuntime()..status = ConnStatus.connected;

    state.onEvent(
      AppEvent.avatar(serverId: 'srv', session: 7, image: _pixel),
    );
    await tester.pumpWidget(host(state, rider()));
    await tester.pump(const Duration(milliseconds: 50));
    expect(find.byType(Image), findsOneWidget);

    // An empty image is how the server says they removed it.
    state.onEvent(
      AppEvent.avatar(serverId: 'srv', session: 7, image: Uint8List(0)),
    );
    await tester.pumpWidget(host(state, rider()));
    await tester.pump(const Duration(milliseconds: 50));
    expect(find.byType(Image), findsNothing);
  });

  test('pictures belong to the server they arrived from', () {
    // Sessions are per connection. A picture filed under the wrong server
    // would eventually be drawn beside a stranger with the same number.
    final state = AppState();
    addTearDown(state.dispose);

    state.onEvent(AppEvent.avatar(serverId: 'a', session: 7, image: _pixel));

    expect(state.runtimeFor('a').avatars[7], isNotNull);
    expect(state.runtimeFor('b').avatars[7], isNull);
  });
}
