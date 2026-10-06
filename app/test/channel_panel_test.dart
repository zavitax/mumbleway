import 'package:flutter/material.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mumbleway/l10n/app_localizations.dart';
import 'package:mumbleway/src/rust/api/mumbleway.dart';
import 'package:mumbleway/state/app_state.dart';
import 'package:mumbleway/widgets/channel_panel.dart';

/// How a channel row is spaced.
///
/// The trailing controls are `IconButton`s — 18pt glyphs centred in compact
/// boxes — so the air the eye reads between them is much wider than the
/// nominal gap in the code. An indicator added with a plain `SizedBox` of a
/// hand-picked width therefore sits tight against its neighbour while the
/// controls beyond it look comfortable, which is exactly what happened to the
/// blocked-speech glyph. The number is measured; this keeps it measured.
void main() {
  UiRights rights({bool speak = true}) => UiRights(
    known: true,
    speak: speak,
    muteDeafen: false,
    moveUsers: false,
    text: true,
    whisper: true,
    makeChannel: true,
    write: true,
    kick: false,
    ban: false,
    registerOthers: false,
    selfRegister: false,
  );

  testWidgets('the blocked-speech glyph is spaced like the row\'s controls', (
    t,
  ) async {
    const channel = UiChannel(
      id: 1,
      name: 'Root',
      description: '',
      userCount: 2,
      maxUsers: 0,
    );
    final state = AppState();
    addTearDown(state.dispose);
    state.runtimes['srv'] = ServerRuntime()
      ..status = ConnStatus.connected
      ..rights = rights()
      ..channelRights = {1: rights(speak: false)};

    await t.pumpWidget(
      AppStateScope(
        state: state,
        child: MaterialApp(
          localizationsDelegates: const [
            ...L.localizationsDelegates,
            GlobalMaterialLocalizations.delegate,
            GlobalWidgetsLocalizations.delegate,
          ],
          supportedLocales: L.supportedLocales,
          home: const Scaffold(
            body: SizedBox(
              width: 420,
              child: ChannelTree(
                serverId: 'srv',
                channels: [channel],
                currentChannelId: null,
                defaultChannelName: null,
              ),
            ),
          ),
        ),
      ),
    );
    await t.pump(const Duration(milliseconds: 50));

    double gap(IconData left, IconData right) =>
        t.getRect(find.byIcon(right)).left - t.getRect(find.byIcon(left)).right;

    // The reference: two of the row's own controls, side by side.
    final controls = gap(Icons.star_border, Icons.more_horiz);
    expect(controls, greaterThan(0), reason: 'the row did not lay out');

    expect(
      gap(Icons.voice_over_off, Icons.person),
      closeTo(controls, 0.5),
      reason: 'the indicator should sit in the same rhythm as the controls; '
          'it was six pixels against twenty-seven',
    );
  });

  testWidgets('and it is spaced the same on both sides', (t) async {
    // Asked for explicitly: it should not read as attached to the channel name.
    const channel = UiChannel(
      id: 1,
      name: 'Root',
      description: '',
      userCount: 2,
      maxUsers: 0,
    );
    final state = AppState();
    addTearDown(state.dispose);
    state.runtimes['srv'] = ServerRuntime()
      ..status = ConnStatus.connected
      ..rights = rights()
      ..channelRights = {1: rights(speak: false)};

    await t.pumpWidget(
      AppStateScope(
        state: state,
        child: MaterialApp(
          localizationsDelegates: const [
            ...L.localizationsDelegates,
            GlobalMaterialLocalizations.delegate,
            GlobalWidgetsLocalizations.delegate,
          ],
          supportedLocales: L.supportedLocales,
          home: const Scaffold(
            body: SizedBox(
              width: 420,
              child: ChannelTree(
                serverId: 'srv',
                channels: [channel],
                currentChannelId: null,
                defaultChannelName: null,
              ),
            ),
          ),
        ),
      ),
    );
    await t.pump(const Duration(milliseconds: 50));

    final glyph = t.getRect(find.byIcon(Icons.voice_over_off));
    final name = t.getRect(find.text('Root'));
    final person = t.getRect(find.byIcon(Icons.person));
    // The name is in an `Expanded`, so its box ends where the gap begins.
    expect(
      glyph.left - name.right,
      closeTo(person.left - glyph.right, 0.5),
    );
  });
}
