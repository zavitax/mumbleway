import 'package:flutter_test/flutter_test.dart';
import 'package:mumbleway/state/app_state.dart';
import 'package:shared_preferences/shared_preferences.dart';

/// The toolbar spinner, and the paths that are allowed to stop it.
///
/// It replaces the diagnostics icon from launch until the device has been
/// measured, because that icon claims either "fine" or "degraded" and neither
/// is known yet. Starting it true is the easy half; the half worth testing is
/// that *something* always turns it off.
///
/// **A spinner that never stops is worse than an icon that is briefly wrong.**
/// It is also invisible in review — the code reads correctly, and the fault
/// only shows on a device, minutes in, as a toolbar that never settles. So
/// each escape is named here, and one removed without a replacement fails.
void main() {
  TestWidgetsFlutterBinding.ensureInitialized();
  setUp(() => SharedPreferences.setMockInitialValues({}));

  test('it spins from construction, before anything has looked', () {
    // No engine, no probe, nothing measured. The plain icon at this moment
    // asserts the chain is fine on a device nobody has timed.
    expect(AppState().probing, isTrue);
  });

  test('the screenshot harness settles it', () {
    // Otherwise every store screenshot shows a progress indicator where the
    // diagnostics icon belongs. The same reason `markReadyForTesting` exists
    // at all — see `store_screenshots_test.dart`.
    expect((AppState()..markReadyForTesting()).probing, isFalse);
  });

  test('opening the devices settles it, probe or no probe', () {
    // A call that starts before the probe has run defers it until the devices
    // shut, which on a ride is twenty minutes away. By then the ladder is
    // measuring the real chain every block and is the better authority, so the
    // icon starts speaking for it rather than spinning for the whole ride.
    final state = AppState();
    expect(state.probing, isTrue);
    state.markAudioActiveForTesting();
    expect(
      state.probing,
      isFalse,
      reason: 'a ride-long spinner reads as a broken toolbar',
    );
    state.dispose();
  });

  test('a failed startup settles it', () async {
    // `_probeWhenIdle` is the last line of the startup `try`, so a throw above
    // it skips the arming and nothing else would ever resolve the flag. There
    // is no chain to measure on this path and the screen already carries the
    // error, which makes the plain icon the honest one.
    //
    // This drives the real `start()` rather than a stand-in for it. There is
    // no Rust library under `flutter test`, so startup throws here for the
    // same reason it would on a device with a broken engine.
    final state = AppState();
    await state.start();
    expect(
      state.startupError,
      isNotNull,
      reason: 'this test is only meaningful if startup actually failed',
    );
    expect(
      state.probing,
      isFalse,
      reason: 'nothing arms the probe once startup has thrown',
    );
    state.dispose();
  });
}
