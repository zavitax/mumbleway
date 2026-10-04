import 'dart:typed_data';

import 'package:flutter/material.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:image/image.dart' as img;
import 'package:mumbleway/l10n/app_localizations.dart';
import 'package:mumbleway/src/rust/api/mumbleway.dart';
import 'package:mumbleway/state/app_state.dart';
import 'package:mumbleway/theme.dart';
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
  bool selfMuted = false,
  bool deafened = false,
  bool selfDeafened = false,
  bool localMute = false,
  String comment = '',
  UiQuality? quality,
}) => UiUser(
  session: 7,
  name: 'Anna',
  channelId: 0,
  talking: false,
  muted: muted,
  deafened: deafened,
  localMute: localMute,
  selfMuted: selfMuted,
  selfDeafened: selfDeafened,
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
      home: Scaffold(
        body: ChannelUserList(serverId: 'srv', users: [user]),
      ),
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

  testWidgets('both switches are drawn, quiet when nothing is wrong', (
    t,
  ) async {
    // **Two glyphs, always.** A rider closes their microphone with one button
    // and their ears with another; grey means the thing is on, which is worth
    // drawing, because a row with no mark at all reads as "nobody has told us"
    // rather than "they are fine".
    final state = connected();
    await t.pumpWidget(host(state, rider()));
    await t.pump(const Duration(milliseconds: 50));

    expect(find.byIcon(Icons.mic), findsOneWidget);
    expect(find.byIcon(Icons.headset), findsOneWidget);
    expect(find.byIcon(Icons.mic_off), findsNothing);
    expect(find.byIcon(Icons.headset_off), findsNothing);
  });

  testWidgets('a rider who did both is shown as having done both', (t) async {
    // One glyph with the other hidden behind it is how this started, and a
    // rider who had done both looked like a rider who had done one.
    final state = connected();
    await t.pumpWidget(host(state, rider(selfMuted: true, selfDeafened: true)));
    await t.pump(const Duration(milliseconds: 50));

    expect(find.byIcon(Icons.mic_off), findsOneWidget);
    expect(find.byIcon(Icons.headset_off), findsOneWidget);
    expect(find.byIcon(Icons.mic), findsNothing);
    expect(find.byIcon(Icons.headset), findsNothing);
  });

  testWidgets('the state is beside the name, not hidden on the picture', (
    t,
  ) async {
    // **It used to ride in the corner of the picture.** At eleven pixels
    // behind a face it was something to find rather than something to see,
    // which is the opposite of what a roster is for.
    final state = connected(avatar: picture());
    await t.pumpWidget(host(state, rider(muted: true)));
    await t.pump(const Duration(milliseconds: 50));

    expect(find.byType(Image), findsOneWidget);
    expect(find.byIcon(Icons.mic_off), findsOneWidget);

    final name = t.getRect(find.text('Anna'));
    final glyph = t.getRect(find.byIcon(Icons.mic_off));
    expect(glyph.left, greaterThan(name.right));
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

  testWidgets('a long note is cut at two lines and offered in full', (t) async {
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

  testWidgets('a rider who turned their own sound off is marked apart', (
    t,
  ) async {
    // The one state in this row that is about *them* hearing rather than about
    // being heard, and the mistake worth saving a rider from: talking to
    // somebody who cannot hear a word of it.
    final state = connected();
    await t.pumpWidget(host(state, rider(selfDeafened: true)));
    await t.pump(const Duration(milliseconds: 50));

    expect(find.byIcon(Icons.headset_off), findsOneWidget);
    // Their microphone is drawn too, and the server closes it when a rider
    // deafens themselves — see the live test.
    expect(find.byIcon(Icons.hearing_disabled), findsNothing);
  });

  testWidgets('and an admin doing it is the same glyph in another colour', (
    t,
  ) async {
    // **One glyph per state, the colour for whose decision it was.** Two red
    // struck icons are not told apart through a visor at speed; what the row
    // is for is "they cannot hear", and amber says the rider on the other end
    // cannot undo it by changing their mind.
    final theirs = connected();
    await t.pumpWidget(host(theirs, rider(selfDeafened: true)));
    await t.pump(const Duration(milliseconds: 50));
    expect(
      t.widget<Icon>(find.byIcon(Icons.headset_off)).color,
      StatusColors.failed,
    );

    await t.pumpWidget(const SizedBox.shrink());
    final admins = connected();
    await t.pumpWidget(host(admins, rider(deafened: true)));
    await t.pump(const Duration(milliseconds: 50));
    expect(find.byIcon(Icons.headset_off), findsOneWidget);
    expect(
      t.widget<Icon>(find.byIcon(Icons.headset_off)).color,
      StatusColors.reconnecting,
    );
  });

  testWidgets('a muted rider is red by their own hand, amber by an admin', (
    t,
  ) async {
    final theirs = connected();
    await t.pumpWidget(host(theirs, rider(selfMuted: true)));
    await t.pump(const Duration(milliseconds: 50));
    expect(
      t.widget<Icon>(find.byIcon(Icons.mic_off)).color,
      StatusColors.failed,
    );

    await t.pumpWidget(const SizedBox.shrink());
    final admins = connected();
    await t.pumpWidget(host(admins, rider(muted: true)));
    await t.pump(const Duration(milliseconds: 50));
    expect(
      t.widget<Icon>(find.byIcon(Icons.mic_off)).color,
      StatusColors.reconnecting,
    );
  });

  testWidgets('an admin mute is not hidden by the rider muting themselves', (
    t,
  ) async {
    // **The ordinary case on this app, not a corner of one.** Muting a rider
    // who runs MumbleWay sends the server mute *and* asks their app to close
    // its own microphone — the request is what reaches a rider the server will
    // not mute for us — so an imposed mute comes back with `self_mute` beside
    // it almost every time. Reading their own hand first painted the admin's
    // decision in the colour of a decision the rider never made, and the words
    // told them they had chosen it.
    final l = await L.delegate.load(const Locale('en'));
    final state = connected();
    await t.pumpWidget(
      host(state, rider(muted: true, selfMuted: true)),
    );
    await t.pump(const Duration(milliseconds: 50));

    expect(
      t.widget<Icon>(find.byIcon(Icons.mic_off)).color,
      StatusColors.reconnecting,
    );
    await t.tap(find.byIcon(Icons.mic_off));
    await t.pump(const Duration(milliseconds: 100));
    expect(find.text(l.statusMutedByAdmin), findsOneWidget);
  });

  testWidgets('and the same for a deafening nobody asked for', (t) async {
    final l = await L.delegate.load(const Locale('en'));
    final state = connected();
    await t.pumpWidget(
      host(state, rider(deafened: true, selfDeafened: true)),
    );
    await t.pump(const Duration(milliseconds: 50));

    expect(
      t.widget<Icon>(find.byIcon(Icons.headset_off)).color,
      StatusColors.reconnecting,
    );
    await t.tap(find.byIcon(Icons.headset_off));
    await t.pump(const Duration(milliseconds: 100));
    expect(find.text(l.statusDeafenedByAdmin), findsOneWidget);
  });

  testWidgets('a rider you turned down is marked in a colour of its own', (
    t,
  ) async {
    // **Your own doing, and invisible to everybody else.** Red would say the
    // rider chose it and amber that an admin did; this is neither, and it is
    // the one state in the row that is about what *you* are hearing. It is
    // read before anything about their own sound, because it is true whatever
    // their sound is doing: you are not hearing them.
    final l = await L.delegate.load(const Locale('en'));
    final state = connected();
    await t.pumpWidget(host(state, rider(localMute: true)));
    await t.pump(const Duration(milliseconds: 50));

    expect(find.byIcon(Icons.headset_off), findsOneWidget);
    expect(
      t.widget<Icon>(find.byIcon(Icons.headset_off)).color,
      StatusColors.yours,
    );
    await t.tap(find.byIcon(Icons.headset_off));
    await t.pump(const Duration(milliseconds: 100));
    expect(find.text(l.statusMutedForYou), findsOneWidget);
  });

  testWidgets('a closed microphone says whose decision it was', (t) async {
    // The glyph is the same either way — it has to be, it is the same state —
    // so the words are what tell a rider whether somebody chose not to talk or
    // was stopped from talking.
    final l = await L.delegate.load(const Locale('en'));

    final theirs = connected();
    await t.pumpWidget(host(theirs, rider(selfMuted: true)));
    await t.pump(const Duration(milliseconds: 50));
    expect(find.byIcon(Icons.mic_off), findsOneWidget);
    await t.tap(find.byIcon(Icons.mic_off));
    await t.pump(const Duration(milliseconds: 100));
    expect(find.text(l.statusTheirMicOff), findsOneWidget);

    await t.pumpWidget(const SizedBox.shrink());
    final admins = connected();
    await t.pumpWidget(host(admins, rider(muted: true)));
    await t.pump(const Duration(milliseconds: 50));
    await t.tap(find.byIcon(Icons.mic_off));
    await t.pump(const Duration(milliseconds: 100));
    expect(find.text(l.statusMutedByAdmin), findsOneWidget);
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
