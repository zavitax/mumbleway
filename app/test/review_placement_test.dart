import 'package:flutter/material.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mumbleway/l10n/app_localizations.dart';
import 'package:mumbleway/screens/home_screen.dart';
import 'package:mumbleway/state/app_state.dart';
import 'package:mumbleway/widgets/review_request.dart';
import 'package:shared_preferences/shared_preferences.dart';

/// Where the review card is drawn.
///
/// It used to be the bottom layer of the home screen's stack, which on a window
/// wide enough for the detail pane put it *behind* the layout: the body paints
/// over it, and what reached the screen was half a sentence showing through the
/// gap beside the server list and two buttons stranded in the empty pane. It
/// was reported, correctly, as "I see leave a review under the UI".
///
/// So what is pinned here is that it is part of the list rather than a layer
/// under it — which is also what keeps it off the talk panel.
void main() {
  Future<AppState> askingForReview() async {
    // Nine launches stored and this one makes ten, which is when it asks.
    SharedPreferences.setMockInitialValues({'mumbleway.usesLaunches': 9});
    final state = AppState();
    addTearDown(state.dispose);
    await state.debugLoadForTesting();
    state.markReadyForTesting();
    // A server, because the card lives in the list of them — and because a
    // rider with none has not had the ten launches either.
    state.servers.add(
      SavedServer(
        name: 'Clubhouse',
        host: 'mumble.example',
        port: 64738,
        username: 'rider',
        localId: 'srv',
      ),
    );
    expect(state.shouldAskForReview, isTrue, reason: 'the card must be up');
    return state;
  }

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

  Future<void> pumpAt(WidgetTester t, Size size, AppState state) async {
    t.view.physicalSize = size;
    t.view.devicePixelRatio = 1.0;
    addTearDown(t.view.resetPhysicalSize);
    addTearDown(t.view.resetDevicePixelRatio);
    await t.pumpWidget(host(state));
    await t.pump(const Duration(milliseconds: 50));
  }

  testWidgets('the card sits inside the server list on a wide window', (
    t,
  ) async {
    final state = await askingForReview();
    // Wide enough for the detail pane, which is the layout the fault appeared
    // in: a narrow one has nothing painted over that part of the screen, so the
    // card showed through and looked deliberate.
    await pumpAt(t, const Size(1280, 720), state);

    expect(
      find.descendant(
        of: find.byType(ListView),
        matching: find.byType(ReviewRequest),
      ),
      findsOneWidget,
    );

    // Across, not down: the card is the last thing in a scrolling list, so it
    // may well be laid out below the fold. What the fault was about is width —
    // it reached across the detail pane, under everything drawn there.
    final list = t.getRect(find.byType(ListView).first);
    final card = t.getRect(find.byType(ReviewRequest));
    expect(card.left, greaterThanOrEqualTo(list.left - 1));
    expect(
      card.right,
      lessThanOrEqualTo(list.right + 1),
      reason: 'the card is $card and the list it belongs to is $list',
    );
  });

  testWidgets('and on a phone, where nothing paints over it either', (t) async {
    final state = await askingForReview();
    await pumpAt(t, const Size(430, 950), state);

    expect(
      find.descendant(
        of: find.byType(ListView),
        matching: find.byType(ReviewRequest),
      ),
      findsOneWidget,
    );
  });
}
