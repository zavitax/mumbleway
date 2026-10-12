import 'package:flutter_test/flutter_test.dart';
import 'package:mumbleway/state/app_state.dart';
import 'package:shared_preferences/shared_preferences.dart';

/// Ducking is a duck while somebody is talking, with a tail.
///
/// **It used to be a standing option and that is the bug this replaces.**
/// `.duckOthers` listed on a category ducks from the moment the session is
/// activated until it is deactivated — Apple's own wording — so a rider's
/// music was quieter for every second of a call, and in pocket mode for every
/// second of the ride. The first answer was a switch to turn it off entirely.
///
/// What is asserted here is the shape that let the switch be on by default
/// instead: the duck follows the voices. The timings are the only part a
/// platform cannot be asked about from here, and they are the part that was
/// wrong, so they are what these tests are about.
void main() {
  setUp(() => SharedPreferences.setMockInitialValues(<String, Object>{}));

  /// Fifteen seconds plus a frame, enough to have released.
  const past = Duration(seconds: 16);

  testWidgets('a voice ducks, and the duck outlives it by the cooldown', (
    tester,
  ) async {
    final state = AppState();
    expect(
      state.duckWhenOthersTalk,
      isTrue,
      reason: 'on by default, now that it no longer lasts the whole call',
    );
    expect(state.duckingOthers, isFalse, reason: 'nobody has spoken yet');

    state.noteOthersTalking();
    expect(state.duckingOthers, isTrue);

    await tester.pump(const Duration(seconds: 14));
    expect(
      state.duckingOthers,
      isTrue,
      reason: 'a pause mid-conversation is not the end of it',
    );

    await tester.pump(const Duration(seconds: 2));
    expect(state.duckingOthers, isFalse, reason: 'the cooldown passed');
  });

  /// **The cooldown is re-armed, not accumulated.** Conversation arrives as
  /// half a sentence, a pause, an answer; music that climbs back between them
  /// is a volume pump rather than a conversation.
  testWidgets('every voice pushes the release back', (tester) async {
    final state = AppState();
    state.noteOthersTalking();

    for (var i = 0; i < 5; i++) {
      await tester.pump(const Duration(seconds: 14));
      expect(state.duckingOthers, isTrue, reason: 'exchange still going');
      state.noteOthersTalking();
    }

    await tester.pump(past);
    expect(state.duckingOthers, isFalse);
  });

  testWidgets('with the setting off nothing ducks at all', (tester) async {
    final state = AppState()..duckWhenOthersTalk = false;
    state.noteOthersTalking();
    expect(state.duckingOthers, isFalse);
    // And no timer is left behind, which this test would fail on.
  });

  /// A rider who turns it off is asking for their music back now. Fifteen
  /// seconds of it staying quiet reads as a switch that did nothing.
  testWidgets('turning the setting off withdraws a duck in force', (
    tester,
  ) async {
    final state = AppState();
    state.noteOthersTalking();
    expect(state.duckingOthers, isTrue);

    await state.updateDuckWhenOthersTalk(false);
    expect(state.duckingOthers, isFalse);

    // Nothing re-applies it, and nothing is pending.
    state.noteOthersTalking();
    expect(state.duckingOthers, isFalse);
  });

  /// Switching it back on does not duck on its own: there is nobody talking
  /// at the moment a switch is touched, and a duck with no voice behind it is
  /// the fault this feature started as.
  testWidgets('turning it back on waits for a voice', (tester) async {
    final state = AppState();
    await state.updateDuckWhenOthersTalk(false);
    await state.updateDuckWhenOthersTalk(true);
    expect(state.duckingOthers, isFalse);

    state.noteOthersTalking();
    expect(state.duckingOthers, isTrue);
    await tester.pump(past);
  });
}
