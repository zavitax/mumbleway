import 'package:flutter_test/flutter_test.dart';
import 'package:mumbleway/state/app_state.dart';
import 'package:shared_preferences/shared_preferences.dart';

/// When the app asks for a review, and — more importantly — when it does not.
///
/// The counting is the easy half. The half worth a test is the refusal: this
/// app is used at speed with the phone in a cradle, and a card asking for a
/// favour has no business appearing while a call is up.
void main() {
  setUp(() => SharedPreferences.setMockInitialValues({}));

  Future<AppState> ready({
    int launches = 0,
    int calls = 0,
    bool done = false,
  }) async {
    SharedPreferences.setMockInitialValues({
      'mumbleway.usesLaunches': launches,
      'mumbleway.usesCalls': calls,
      'mumbleway.reviewDone': done,
    });
    final state = AppState();
    addTearDown(state.dispose);
    // The counters are read on load; `markReadyForTesting` stands in for the
    // startup this test does not run.
    await state.debugLoadForTesting();
    state.markReadyForTesting();
    return state;
  }

  test('says nothing until it has something to go on', () async {
    final state = await ready();
    expect(state.shouldAskForReview, isFalse);
  });

  test('asks on the tenth launch', () async {
    // Nine stored, and loading counts this one.
    final state = await ready(launches: 9);
    expect(state.shouldAskForReview, isTrue);
  });

  test('does not ask on the ninth', () async {
    final state = await ready(launches: 7);
    expect(state.shouldAskForReview, isFalse);
  });

  test(
    'asks after three completed calls, without waiting for ten launches',
    () async {
      final state = await ready(calls: 3);
      expect(state.shouldAskForReview, isTrue);
    },
  );

  test('"not now" moves the bar out to seven calls, not three', () async {
    final state = await ready(calls: 3);
    expect(state.shouldAskForReview, isTrue);

    await state.dismissReviewRequest();
    expect(state.shouldAskForReview, isFalse, reason: 'counters not reset');

    // Three more would have been enough the first time. It is not now.
    state.debugAddCallsForTesting(3);
    expect(state.shouldAskForReview, isFalse);

    state.debugAddCallsForTesting(4);
    expect(state.shouldAskForReview, isTrue, reason: 'seven should do it');
  });

  test('once they have gone to the store it never asks again', () async {
    final state = await ready(calls: 3);
    await state.openStoreForReview();
    expect(state.shouldAskForReview, isFalse);

    state.debugAddCallsForTesting(50);
    expect(state.shouldAskForReview, isFalse, reason: 'asked after rating');
  });

  test('a rating is remembered across a restart', () async {
    final first = await ready(calls: 3);
    await first.openStoreForReview();

    final second = await ready(calls: 9, done: true);
    expect(second.shouldAskForReview, isFalse);
  });

  test(
    'a call left open when the app stopped is counted at the next start',
    () async {
      // The marker the last run left behind, dated far enough back to have been
      // a real call. This is the only way "or closes the app" can be honoured:
      // a phone kills a backgrounded app without running anything on the way
      // out, so the call cannot be counted at the time.
      final long = DateTime.now()
          .subtract(const Duration(minutes: 5))
          .millisecondsSinceEpoch;
      SharedPreferences.setMockInitialValues({
        'mumbleway.usesCalls': 2,
        'mumbleway.callOpenSince': long,
      });
      final state = AppState();
      addTearDown(state.dispose);
      await state.debugLoadForTesting();
      state.markReadyForTesting();

      expect(
        state.shouldAskForReview,
        isTrue,
        reason: 'the third call was the one interrupted by closing',
      );
      final prefs = await SharedPreferences.getInstance();
      expect(
        prefs.getInt('mumbleway.callOpenSince'),
        isNull,
        reason: 'the marker must be cleared or it counts again every start',
      );
    },
  );

  test('a call cut short by closing the app does not count', () async {
    final brief = DateTime.now()
        .subtract(const Duration(seconds: 5))
        .millisecondsSinceEpoch;
    SharedPreferences.setMockInitialValues({
      'mumbleway.usesCalls': 2,
      'mumbleway.callOpenSince': brief,
    });
    final state = AppState();
    addTearDown(state.dispose);
    await state.debugLoadForTesting();
    state.markReadyForTesting();

    expect(state.shouldAskForReview, isFalse);
  });

  test('never asks while the audio devices are open', () async {
    // The strongest of the guards: this stands in for a ride in progress.
    final state = await ready(calls: 9);
    expect(state.shouldAskForReview, isTrue);
    state.markAudioActiveForTesting();
    expect(
      state.shouldAskForReview,
      isFalse,
      reason: 'asked for a favour with the microphone live',
    );
  });
}
