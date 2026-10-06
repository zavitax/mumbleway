import 'package:flutter/material.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mumbleway/l10n/app_localizations.dart';
import 'package:mumbleway/screens/home_screen.dart';
import 'package:mumbleway/src/rust/api/mumbleway.dart';
import 'package:mumbleway/state/app_state.dart';
import 'package:mumbleway/theme.dart';
import 'package:shared_preferences/shared_preferences.dart';

/// The two buttons at the top, when the silence was not the rider's idea.
///
/// **Red is a decision the rider made.** Pressing the microphone closes it and
/// pressing it again opens it, and the colour is the app's word for "you did
/// this". An admin's mute and a channel that will not carry a voice are
/// neither: the button does not lift them, and drawn in the same red they read
/// as something the rider pressed and forgot about — which is a rider pressing
/// a button, seeing nothing happen, and pressing it again.
void main() {
  setUp(() => SharedPreferences.setMockInitialValues({}));

  Widget host(AppState state) => AppStateScope(
    state: state,
    child: MaterialApp(
      localizationsDelegates: const [
        ...L.localizationsDelegates,
        GlobalMaterialLocalizations.delegate,
        GlobalWidgetsLocalizations.delegate,
        GlobalCupertinoLocalizations.delegate,
      ],
      supportedLocales: L.supportedLocales,
      home: const HomeScreen(),
    ),
  );

  UiUser me({bool muted = false, bool deafened = false}) => UiUser(
    session: 7,
    name: 'Anna',
    channelId: 0,
    talking: false,
    muted: muted,
    deafened: deafened,
    selfMuted: false,
    selfDeafened: false,
    localMute: false,
    mutedYou: false,
    userId: null,
    status: 'silent',
    prioritySpeaker: false,
    suppressed: false,
    comment: '',
  );

  /// Connected, with our own entry in the roster — which is where being muted
  /// by somebody else is read from.
  AppState connected({
    bool mutedByAdmin = false,
    bool deafenedByAdmin = false,
    bool suppressed = false,
  }) {
    final state = AppState();
    addTearDown(state.dispose);
    state.markReadyForTesting();
    state.runtimes['srv'] = ServerRuntime()
      ..status = ConnStatus.connected
      ..selfSession = 7
      ..suppressed = suppressed
      ..users = [me(muted: mutedByAdmin, deafened: deafenedByAdmin)];
    return state;
  }

  /// The toolbar's own copy of a glyph. The push-to-talk button draws a
  /// microphone too, and it is a different control saying a different thing.
  Finder inBar(IconData glyph) =>
      find.descendant(of: find.byType(AppBar), matching: find.byIcon(glyph));

  Color? colourOf(WidgetTester t, IconData glyph) => t
      .widget<IconButton>(
        find.ancestor(of: inBar(glyph), matching: find.byType(IconButton)),
      )
      .color;

  testWidgets('an admin muting us shows the microphone closed, in amber', (
    t,
  ) async {
    final state = connected(mutedByAdmin: true);
    await t.pumpWidget(host(state));
    await t.pump(const Duration(milliseconds: 50));

    // Closed, although the rider never closed it.
    expect(inBar(Icons.mic_off), findsOneWidget);
    expect(colourOf(t, Icons.mic_off), StatusColors.reconnecting);
    expect(state.muted, isFalse, reason: 'our own switch has not moved');
  });

  testWidgets('a channel that will not carry a voice reads the same way', (
    t,
  ) async {
    // Different cause, same fact: the rider cannot be heard and cannot fix it
    // from this button. The tooltip is what tells them which it is.
    final state = connected(suppressed: true);
    await t.pumpWidget(host(state));
    await t.pump(const Duration(milliseconds: 50));

    expect(colourOf(t, Icons.mic_off), StatusColors.reconnecting);
  });

  testWidgets('our own mute stays red', (t) async {
    final state = connected();
    state.toggleMute();
    await t.pumpWidget(host(state));
    await t.pump(const Duration(milliseconds: 50));

    expect(inBar(Icons.mic_off), findsOneWidget);
    expect(colourOf(t, Icons.mic_off), StatusColors.failed);
  });

  testWidgets('and an admin deafening us is amber on the speaker', (t) async {
    final state = connected(deafenedByAdmin: true);
    await t.pumpWidget(host(state));
    await t.pump(const Duration(milliseconds: 50));

    expect(inBar(Icons.volume_off), findsOneWidget);
    expect(colourOf(t, Icons.volume_off), StatusColors.reconnecting);
  });

  testWidgets('the microphone button does not toggle an imposed silence', (
    t,
  ) async {
    // **It was still wired to self-mute.** Amber says "an admin muted you",
    // and a tap changed a different state than the one on screen: the colour
    // stayed put, nothing visible moved, and the microphone silently opened or
    // closed underneath. A control that acts on something other than what it
    // displays is worse than one that does nothing.
    final state = connected(mutedByAdmin: true);
    await t.pumpWidget(host(state));
    await t.pump(const Duration(milliseconds: 50));

    final button = t.widget<IconButton>(
      find.ancestor(of: inBar(Icons.mic_off), matching: find.byType(IconButton)),
    );
    expect(button.onPressed, isNull, reason: 'it is an indicator, not a switch');
    // And it keeps saying why rather than going the usual disabled grey.
    expect(button.disabledColor, StatusColors.reconnecting);

    await t.tap(inBar(Icons.mic_off), warnIfMissed: false);
    await t.pump(const Duration(milliseconds: 50));
    expect(state.muted, isFalse, reason: 'our own switch still has not moved');
  });

  testWidgets('a suppressed channel disables it the same way', (t) async {
    final state = connected(suppressed: true);
    await t.pumpWidget(host(state));
    await t.pump(const Duration(milliseconds: 50));
    expect(
      t
          .widget<IconButton>(
            find.ancestor(
              of: inBar(Icons.mic_off),
              matching: find.byType(IconButton),
            ),
          )
          .onPressed,
      isNull,
    );
  });

  testWidgets('the suppression notice is a notice, not a casualty', (t) async {
    // **It was a filled block of the error red with the failure yellow on it**
    // — the same dress a lost connection wears, and the loudest thing on the
    // card. A rider who simply may not speak in this channel read it as the app
    // having broken. Same words, same icon, amber: the register this app
    // already uses for "somebody else decided this", and the colour of the
    // microphone button sitting beside it.
    final state = connected(suppressed: true);
    // The banner lives on the server's card, so there has to be one.
    state.servers.add(
      SavedServer(
        name: 'Clubhouse',
        host: 'mumble.example',
        port: 64738,
        username: 'rider',
        localId: 'srv',
      ),
    );
    await t.pumpWidget(host(state));
    await t.pump(const Duration(milliseconds: 50));

    final l = await L.delegate.load(const Locale('en'));
    final title = t.widget<Text>(find.text(l.suppressedTitle));
    expect(title.style?.color, StatusColors.reconnecting);
    expect(
      title.style?.color,
      isNot(StatusColors.errorForeground),
      reason: 'the failure palette is for failures',
    );

    final box = t
        .widgetList<Container>(find.byType(Container))
        .where((c) => c.decoration is BoxDecoration)
        .map((c) => c.decoration! as BoxDecoration)
        .where((d) => d.color == StatusColors.errorBackground);
    expect(box, isEmpty, reason: 'nothing on this card is painted as an error');

    // And it still says both halves: the fact and what to do about it.
    expect(find.text(l.suppressedTitle), findsOneWidget);
    expect(find.text(l.suppressedBody), findsOneWidget);
  });

  group('on air', () {
    // The meter's colour and the floating window's light both read this, and
    // the whole point of one definition is that they cannot disagree.
    test('an admin muting us is not on air', () {
      final state = connected(mutedByAdmin: true)..micMode = MicMode.continuous;
      expect(
        state.isOnAir,
        isFalse,
        reason: 'the meter was still drawn in the colour that means '
            'somebody is hearing this',
      );
    });

    test('a channel that will not carry a voice is not either', () {
      final state = connected(suppressed: true)..micMode = MicMode.continuous;
      expect(state.isOnAir, isFalse);
    });

    test('and an ordinary connected microphone still is', () {
      final state = connected()..micMode = MicMode.continuous;
      expect(state.isOnAir, isTrue);
    });

    test('muted on one server of two is still on air on the other', () {
      // **The dangerous direction.** Saying "not on air" here would invite a
      // rider to speak freely into a server that is carrying every word.
      final state = connected(mutedByAdmin: true)..micMode = MicMode.continuous;
      state.runtimes['other'] = ServerRuntime()
        ..status = ConnStatus.connected
        ..selfSession = 7
        ..users = [me()];
      expect(state.silencedEverywhere, isFalse);
      expect(state.isOnAir, isTrue);
    });
  });

  testWidgets('nothing imposed leaves both buttons alone', (t) async {
    final state = connected();
    await t.pumpWidget(host(state));
    await t.pump(const Duration(milliseconds: 50));

    expect(inBar(Icons.mic), findsOneWidget);
    expect(inBar(Icons.volume_up), findsOneWidget);
    expect(colourOf(t, Icons.mic), isNull);
    expect(colourOf(t, Icons.volume_up), isNull);
  });
}
