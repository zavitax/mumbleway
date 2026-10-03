import 'package:flutter/material.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mumbleway/l10n/app_localizations.dart';
import 'package:mumbleway/screens/home_screen.dart';
import 'package:mumbleway/state/app_state.dart';
import 'package:mumbleway/theme.dart';
import 'package:mumbleway/widgets/review_request.dart';
import 'package:mumbleway/widgets/server_card.dart';
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

  testWidgets('it is shaped like the cards it sits under', (t) async {
    final state = await askingForReview();
    await pumpAt(t, const Size(1280, 720), state);

    // The same margins as a server card, which is what gives it a gap above it
    // and the same edges as the list it has joined. Its own, tighter, left it
    // wider than the cards above and flush against the last of them, so the
    // two read as one card with a line through it.
    final card = t.widget<Card>(
      find.descendant(of: find.byType(ReviewRequest), matching: find.byType(Card)),
    );
    expect(
      card.margin,
      isNull,
      reason: "the theme's margin is what every other card on this screen uses",
    );
    final server = t.getRect(find.byType(ServerCard).first);
    final review = t.getRect(find.byType(ReviewRequest));
    expect(review.left, moreOrLessEquals(server.left, epsilon: 0.5));
    expect(review.right, moreOrLessEquals(server.right, epsilon: 0.5));
    // First in the list. Last, it was honest and easy to miss: on a window the
    // height of a laptop's it sat below the fold with nothing to say it was
    // there.
    expect(review.top, lessThan(server.top));

    // The two buttons centred on each other rather than hung from the top.
    // They are not the same height — this theme sizes a filled button for a
    // gloved thumb and leaves a text button at its own — so aligned to the top
    // the two labels sit on visibly different lines.
    //
    // **Asserted as the property, not as two rectangles.** A widget test draws
    // in a font whose every glyph is a square of the font size, so "Leave a
    // review" measures 196 pixels here and about half that on a screen: a
    // geometric assertion would be about the test font, and the first draft of
    // this one failed on exactly that.
    final wrap = t.widget<Wrap>(
      find.descendant(of: find.byType(ReviewRequest), matching: find.byType(Wrap)),
    );
    expect(wrap.crossAxisAlignment, WrapCrossAlignment.center);
    // And centred in the card.
    //
    // **Both halves, because the property alone is not the thing.** A `Wrap`
    // centres its children inside the box it is given, and a `Wrap` in a column
    // aligned to the start is given exactly the width of its children: the
    // buttons were centred in a box pressed against the left edge, which is
    // left-justified by another name, while `alignment == center` was true the
    // whole time.
    expect(wrap.alignment, WrapAlignment.center);
    final buttons = t.getRect(
      find.descendant(of: find.byType(ReviewRequest), matching: find.byType(Wrap)),
    );
    expect(buttons.center.dx, moreOrLessEquals(review.center.dx, epsilon: 1));

    // And the one worth pressing is coloured, not tonal: on this card the
    // scheme's own filled button is a pale blue on a pale grey.
    final rate = t.widget<FilledButton>(
      find.descendant(of: find.byType(ReviewRequest), matching: find.byType(FilledButton)),
    );
    expect(
      rate.style?.backgroundColor?.resolve({}),
      StatusColors.talking,
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
