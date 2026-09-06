import 'package:flutter/material.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mumbleway/l10n/app_localizations.dart';
import 'package:mumbleway/state/app_state.dart';
import 'package:mumbleway/widgets/review_request.dart';
import 'package:shared_preferences/shared_preferences.dart';

/// The review card as a rider sees it.
///
/// `review_request_test.dart` covers when the app decides to ask.
/// **Nothing covered whether asking produces anything on screen**, which is a
/// different failure: the rule can be right and the card still never appear,
/// because it is wrapped in a selector that has to notice the flag changing.
void main() {
  setUp(() => SharedPreferences.setMockInitialValues({}));

  Future<AppState> ready({int calls = 0}) async {
    SharedPreferences.setMockInitialValues({'mumbleway.usesCalls': calls});
    final state = AppState();
    addTearDown(state.dispose);
    await state.debugLoadForTesting();
    state.markReadyForTesting();
    return state;
  }

  Widget host(AppState state) => AppStateScope(
    state: state,
    child: MaterialApp(
      localizationsDelegates: const [
        ...L.localizationsDelegates,
        GlobalMaterialLocalizations.delegate,
        GlobalWidgetsLocalizations.delegate,
      ],
      supportedLocales: L.supportedLocales,
      home: const Scaffold(body: ReviewRequest()),
    ),
  );

  testWidgets('shows nothing at all when it is not time to ask', (t) async {
    final state = await ready();
    await t.pumpWidget(host(state));
    expect(find.byType(Card), findsNothing);
    expect(find.text('Getting on with MumbleWay?'), findsNothing);
  });

  testWidgets('asks once the threshold is reached', (t) async {
    final state = await ready(calls: 3);
    await t.pumpWidget(host(state));
    expect(find.text('Getting on with MumbleWay?'), findsOneWidget);
    expect(find.text('Not now'), findsOneWidget);
    expect(find.text('Leave a review'), findsOneWidget);
  });

  testWidgets('"Not now" takes the card away without asking again', (t) async {
    final state = await ready(calls: 3);
    await t.pumpWidget(host(state));
    expect(find.text('Getting on with MumbleWay?'), findsOneWidget);

    await t.tap(find.text('Not now'));
    await t.pumpAndSettle();

    // **The point of the test.** The flag flips in the state; if the card is
    // not listening, it sits there having been dismissed.
    expect(find.byType(Card), findsNothing);
    expect(state.shouldAskForReview, isFalse);
  });

  testWidgets('appears without a rebuild when a call pushes it over', (t) async {
    final state = await ready(calls: 2);
    await t.pumpWidget(host(state));
    expect(find.byType(Card), findsNothing);

    state.debugAddCallsForTesting(1);
    await t.pumpAndSettle();

    // Nothing rebuilt the tree from above: the selector has to have noticed.
    expect(find.text('Getting on with MumbleWay?'), findsOneWidget);
  });

  testWidgets('goes away for good once they have gone to the store', (t) async {
    final state = await ready(calls: 3);
    await t.pumpWidget(host(state));
    expect(find.text('Leave a review'), findsOneWidget);

    // Not tapping the button: that reaches `url_launcher`, which has no
    // platform behind it in a test. The state change it causes is the part
    // this widget has to react to.
    await state.openStoreForReview();
    await t.pumpAndSettle();

    expect(find.byType(Card), findsNothing);
    state.debugAddCallsForTesting(50);
    await t.pumpAndSettle();
    expect(find.byType(Card), findsNothing, reason: 'asked after rating');
  });
}
