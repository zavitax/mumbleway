import 'package:flutter_test/flutter_test.dart';
import 'package:mumbleway/src/rust/api/mumbleway.dart' show ConnStatus, MicMode;
import 'package:mumbleway/state/app_state.dart';
import 'package:shared_preferences/shared_preferences.dart';

/// The silence that hands the microphone back.
///
/// **This is the only way out of capture now, which is why it has tests.**
/// Pocket mode used to be left by tapping the helmet three times, and tapping
/// a helmet could not be told from the road — a hands-free link's own AGC and
/// codec smear the transient a strike is recognised by, which four builds of
/// tuning could not get round. So the rider asked for the inverse: half a
/// minute in which they have not spoken, and the microphone goes back to the
/// phone where tapping works.
///
/// What is asserted here is the arithmetic, which is the part a ride cannot
/// check: that a voice holds the microphone open, that the release is measured
/// from the *last* voice rather than from whenever the timer happened to be
/// set, and that the rider's claim on the microphone is dropped rather than
/// the microphone being closed behind its back.
void main() {
  setUp(() => SharedPreferences.setMockInitialValues(<String, Object>{}));

  /// Armed, connected, and the rider's hand on the microphone — with no
  /// engine, so every platform call is a guarded no-op and the reconcile
  /// returns at once.
  Future<AppState> armed() async {
    final state = AppState()
      ..pocketMode = true
      ..micMode = MicMode.continuous;
    state.runtimeFor('a').status = ConnStatus.connected;
    await state.setPocketActive(true);
    await state.toggleCapture();
    return state;
  }

  testWidgets('thirty seconds is the default, and it is the way back', (
    tester,
  ) async {
    final state = AppState();
    expect(state.captureIdleSeconds, 30);
  });

  testWidgets('silence hands the microphone back', (tester) async {
    final state = await armed();
    expect(state.riderWantsMic, isTrue, reason: 'the rider asked for it');

    await tester.pump(const Duration(seconds: 29));
    expect(state.riderWantsMic, isTrue, reason: 'still inside the cooldown');

    await tester.pump(const Duration(seconds: 2));
    expect(
      state.riderWantsMic,
      isFalse,
      reason: 'thirty seconds of silence and the claim is dropped',
    );
  });

  /// **Measured from the last voice, not from the timer.** Re-arming for the
  /// whole period on every word would make the release depend on when the
  /// timer happened to be set, and the voice arrives ten times a second.
  testWidgets('a voice pushes the release out by the whole period', (
    tester,
  ) async {
    final state = await armed();

    for (var i = 0; i < 4; i++) {
      await tester.pump(const Duration(seconds: 20));
      state.noteRiderVoiceForTest();
      expect(state.riderWantsMic, isTrue, reason: 'talking, so it stays open');
    }

    // Twenty seconds after the last word is still inside the half-minute.
    await tester.pump(const Duration(seconds: 20));
    expect(state.riderWantsMic, isTrue);

    await tester.pump(const Duration(seconds: 11));
    expect(state.riderWantsMic, isFalse, reason: 'the silence ran its course');
  });

  testWidgets('a shorter cooldown applies at once, not at the next release', (
    tester,
  ) async {
    final state = await armed();
    await tester.pump(const Duration(seconds: 5));

    await state.updateCaptureIdleSeconds(10);
    await tester.pump(const Duration(seconds: 11));
    expect(
      state.riderWantsMic,
      isFalse,
      reason: 'the old thirty-second timer outlived the setting',
    );
  });

  testWidgets('the period is kept inside what the slider offers', (
    tester,
  ) async {
    final state = AppState();
    await state.updateCaptureIdleSeconds(2);
    expect(state.captureIdleSeconds, 10);
    await state.updateCaptureIdleSeconds(9999);
    expect(state.captureIdleSeconds, 300);
  });
}
