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
/// here is that a note shows up at all without eating the row, and that a
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
  status: 'silent',
  comment: comment,
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

  testWidgets('a note is offered to be read, not spread across the row', (
    tester,
  ) async {
    final state = AppState();
    addTearDown(state.dispose);
    state.runtimes['srv'] = ServerRuntime()..status = ConnStatus.connected;

    await tester.pumpWidget(host(state, rider(comment: 'On the A9 heading north')));
    await tester.pump(const Duration(milliseconds: 50));

    // The icon is there; the sentence is not competing with the name for room.
    expect(find.byIcon(Icons.sticky_note_2_outlined), findsOneWidget);
    expect(find.text('On the A9 heading north'), findsNothing);

    // And it is readable on demand, which on a phone means a tap.
    await tester.tap(find.byIcon(Icons.sticky_note_2_outlined));
    await tester.pump(const Duration(milliseconds: 100));
    expect(find.text('On the A9 heading north'), findsOneWidget);
  });

  testWidgets('a rider with no note has no icon', (tester) async {
    final state = AppState();
    addTearDown(state.dispose);
    state.runtimes['srv'] = ServerRuntime()..status = ConnStatus.connected;

    await tester.pumpWidget(host(state, rider()));
    await tester.pump(const Duration(milliseconds: 50));

    expect(find.byIcon(Icons.sticky_note_2_outlined), findsNothing);
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
