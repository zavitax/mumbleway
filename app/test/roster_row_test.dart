import 'dart:typed_data';

import 'package:flutter/material.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:image/image.dart' as img;
import 'package:mumbleway/l10n/app_localizations.dart';
import 'package:mumbleway/src/rust/api/mumbleway.dart';
import 'package:mumbleway/state/app_state.dart';
import 'package:mumbleway/widgets/channel_panel.dart';
import 'package:mumbleway/widgets/connection_quality.dart';

/// What a rider's row says about them, and in what order.
///
/// Three things live in it that each replaced something quieter: their picture
/// where the drawing of a person would be, their note under their name instead
/// of behind an icon, and how their connection is doing beside the control that
/// silences them rather than tucked against their name.
UiUser rider({
  bool muted = false,
  String comment = '',
  UiQuality? quality,
}) => UiUser(
  session: 7,
  name: 'Anna',
  channelId: 0,
  talking: false,
  muted: muted,
  deafened: false,
  localMute: false,
  status: 'silent',
  comment: comment,
  prioritySpeaker: false,
  suppressed: false,
  quality: quality,
);

Uint8List picture() {
  final image = img.Image(width: 24, height: 24);
  img.fill(image, color: img.ColorRgb8(10, 120, 200));
  return Uint8List.fromList(img.encodePng(image));
}

const _fine = UiQuality(
  pingMs: 20,
  udp: true,
  lossUp: 0,
  lossDown: 0,
  windowSecs: 60,
  idleSecs: 0,
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

  AppState connected({Uint8List? avatar}) {
    final state = AppState();
    addTearDown(state.dispose);
    final runtime = ServerRuntime()..status = ConnStatus.connected;
    if (avatar != null) runtime.avatars[7] = avatar;
    state.runtimes['srv'] = runtime;
    return state;
  }

  testWidgets('a picture stands in for the drawing of a person', (t) async {
    final state = connected(avatar: picture());
    await t.pumpWidget(host(state, rider()));
    await t.pump(const Duration(milliseconds: 50));

    expect(find.byType(Image), findsOneWidget);
    // A drawing of a person in the corner of a photograph of one says nothing.
    expect(find.byIcon(Icons.person_outline), findsNothing);
  });

  testWidgets('but the status still rides on it when it says something', (
    t,
  ) async {
    final state = connected(avatar: picture());
    await t.pumpWidget(host(state, rider(muted: true)));
    await t.pump(const Duration(milliseconds: 50));

    expect(find.byType(Image), findsOneWidget);
    // Who is muted is what the row is for, and that is never given up.
    expect(find.byIcon(Icons.mic_off), findsOneWidget);
  });

  testWidgets('without a picture the person icon is the row', (t) async {
    final state = connected();
    await t.pumpWidget(host(state, rider()));
    await t.pump(const Duration(milliseconds: 50));

    expect(find.byType(Image), findsNothing);
    expect(find.byIcon(Icons.person_outline), findsOneWidget);
  });

  testWidgets('the note is under the name and reads as itself', (t) async {
    const note = 'Петля на М4, буду в 18:00';
    final state = connected();
    await t.pumpWidget(host(state, rider(comment: note)));
    await t.pump(const Duration(milliseconds: 50));

    // Written out rather than held behind an icon: what riders put there is
    // where they are and when they are leaving, which the next rider wants
    // without having to discover a tooltip.
    expect(find.text(note), findsOneWidget);
    expect(find.byIcon(Icons.sticky_note_2_outlined), findsNothing);
    final name = t.getRect(find.text('Anna'));
    final written = t.getRect(find.text(note));
    expect(written.top, greaterThanOrEqualTo(name.bottom - 1));
  });

  testWidgets('a long note is cut at two lines and offered in full', (
    t,
  ) async {
    // A server caps a note at 512 characters, and a rider who writes all of
    // them would otherwise own the roster — every other row pushed off a phone
    // screen by one person's paragraph.
    final long = List.filled(40, 'Петля на М4 до вечера').join(', ');
    final state = connected();
    await t.pumpWidget(host(state, rider(comment: long)));
    await t.pump(const Duration(milliseconds: 50));

    final written = t.widget<Text>(find.text(long));
    expect(written.maxLines, 2);
    expect(written.overflow, TextOverflow.ellipsis);

    // And the whole of it behind a press, which is what a phone has instead of
    // a hover.
    final tip = t.widget<Tooltip>(
      find.ancestor(of: find.text(long), matching: find.byType(Tooltip)),
    );
    expect(tip.message, long);
    expect(tip.triggerMode, TooltipTriggerMode.tap);
  });

  testWidgets('the connection sits between the speaker and the menu', (
    t,
  ) async {
    final state = connected();
    await t.pumpWidget(host(state, rider(quality: _fine)));
    await t.pump(const Duration(milliseconds: 50));

    final bars = t.getRect(find.byType(ConnectionQualityBars));
    final speaker = t.getRect(find.byIcon(Icons.volume_up));
    final menu = t.getRect(find.byIcon(Icons.more_vert));
    expect(bars.left, greaterThan(speaker.right));
    expect(bars.right, lessThan(menu.left));
    // At the size of what it stands between, not at the size of a mark on a
    // name, which is where it used to be.
    expect(bars.width, greaterThanOrEqualTo(18));
  });

  testWidgets('and is absent when the server has not measured them', (t) async {
    final state = connected();
    await t.pumpWidget(host(state, rider()));
    await t.pump(const Duration(milliseconds: 50));

    // Nothing beside a row means "no measurement", which is what happens for
    // anybody outside our own channel — see ConnectionQualityBars.
    expect(find.byType(ConnectionQualityBars), findsNothing);
  });
}
