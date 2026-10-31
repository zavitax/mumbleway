import 'package:flutter/material.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mumbleway/l10n/app_localizations.dart';
import 'package:mumbleway/src/rust/api/mumbleway.dart' show ConnStatus, MicMode;
import 'package:mumbleway/state/app_state.dart';
import 'package:mumbleway/widgets/pocket_mode.dart';
import 'package:shared_preferences/shared_preferences.dart';

/// The explanation of pocket mode has to stay reachable.
///
/// **It was not.** The four slides are shown on the way into pocket mode and
/// `pocketTutorialSeen` is persisted, so for every ride after the first the
/// only account of the feature in the app was unreachable — and the panel says
/// nothing about what pocket mode is until it is armed, by which point the
/// screen is locked and reading is the last thing a rider should be doing.
///
/// So these assert the two halves that are easy to lose again: that the way in
/// exists once the slides have been seen, and that it is **not** offered while
/// armed, where a dialog would land on top of the one control that unlocks the
/// screen.
void main() {
  setUp(() => SharedPreferences.setMockInitialValues({}));

  Widget host(AppState state, Widget child) => AppStateScope(
    state: state,
    child: MaterialApp(
      localizationsDelegates: const [
        ...L.localizationsDelegates,
        GlobalMaterialLocalizations.delegate,
        GlobalWidgetsLocalizations.delegate,
        GlobalCupertinoLocalizations.delegate,
      ],
      supportedLocales: L.supportedLocales,
      home: Scaffold(body: Align(alignment: Alignment.bottomCenter, child: child)),
    ),
  );

  /// Pocket mode offered, a live server to sit on, and the slides already seen
  /// — which is the state every ride after the first one is in.
  AppState offered() {
    final state = AppState()
      ..pocketMode = true
      ..micMode = MicMode.continuous
      ..pocketTutorialSeen = true;
    addTearDown(state.dispose);
    state.runtimeFor('a').status = ConnStatus.connected;
    return state;
  }

  /// The English copy, read from the bundle rather than repeated here: a test
  /// that hard-codes a string passes after the string has been reworded into
  /// something else.
  Future<L> strings() => L.delegate.load(const Locale('en'));

  testWidgets('the slides can be re-opened from the panel', (tester) async {
    final l = await strings();
    await tester.pumpWidget(host(offered(), const PocketPanel()));

    expect(
      find.byIcon(Icons.help_outline),
      findsOneWidget,
      reason: 'with the slides seen, this is the only way back to them',
    );
    await tester.tap(find.byIcon(Icons.help_outline));
    await tester.pumpAndSettle();

    // The first slide is the one about what the feature is for, which is the
    // question somebody opening this deliberately has.
    expect(find.text(l.pocketTutorialWhyTitle), findsOneWidget);
    // And it cannot arm anything: this was opened to be read.
    expect(find.text(l.close), findsOneWidget);
    expect(find.text(l.cancel), findsNothing);
  });

  testWidgets('nothing offers to open a dialog over a locked screen', (
    tester,
  ) async {
    final state = offered();
    await state.setPocketActive(true);
    await tester.pumpWidget(host(state, const PocketPanel()));
    await tester.pumpAndSettle();

    expect(
      find.byIcon(Icons.help_outline),
      findsNothing,
      reason: 'armed, the slider is the only control that may still work',
    );
  });

  testWidgets('a collapsed panel is the handle and nothing else', (
    tester,
  ) async {
    final state = offered()..pocketPanelCollapsed = true;
    await tester.pumpWidget(host(state, const PocketPanel()));
    await tester.pumpAndSettle();

    expect(
      find.byIcon(Icons.help_outline),
      findsNothing,
      reason: 'the handle keeps its full width for the flick that reopens it',
    );
  });

  // **The settings half of this is not tested, and the reason is worth
  // writing down rather than leaving as an omission.** Reaching the tile means
  // scrolling the settings list, and scrolling it builds `_FingerprintTile`,
  // whose `initState` calls the bridge:
  //
  //   Bad state: flutter_rust_bridge has not been initialized.
  //
  // So no widget test can scroll past the top of that screen. This is the
  // missing seam `.claude/plans/typed-tinkering-fairy.md` names in step 2, and
  // the answer is that seam rather than a mock placed here.

  group('the dialog', () {
    /// Walks to the last slide, whatever the count, by pressing the button
    /// that advances it.
    Future<void> toTheEnd(WidgetTester tester, L l) async {
      while (find.text(l.pocketTutorialNext).evaluate().isNotEmpty) {
        await tester.tap(find.text(l.pocketTutorialNext));
        await tester.pumpAndSettle();
      }
    }

    testWidgets('read to the end, offers nothing but closing', (tester) async {
      final l = await strings();
      await tester.pumpWidget(
        host(offered(), const PocketTutorial(readOnly: true)),
      );
      await toTheEnd(tester, l);

      expect(find.text(l.pocketTutorialStart), findsNothing);
      expect(find.text(l.close), findsOneWidget);
    });

    testWidgets('on the way in, still ends in the switch', (tester) async {
      final l = await strings();
      await tester.pumpWidget(host(offered(), const PocketTutorial()));
      await toTheEnd(tester, l);

      expect(
        find.text(l.pocketTutorialStart),
        findsOneWidget,
        reason: 'the arming flow has to end in arming',
      );
      expect(find.text(l.cancel), findsOneWidget);
    });

    testWidgets('leads with what the mode is for', (tester) async {
      final l = await strings();
      await tester.pumpWidget(host(offered(), const PocketTutorial()));

      expect(find.text(l.pocketTutorialWhyTitle), findsOneWidget);
      expect(
        find.text(l.pocketTutorialTapsTitle),
        findsNothing,
        reason: 'the mechanism comes after the reason, not before it',
      );
    });
  });
}
