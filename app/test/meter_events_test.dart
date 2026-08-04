import 'package:flutter_test/flutter_test.dart';
import 'package:mumbleway/src/rust/api/mumbleway.dart';
import 'package:mumbleway/state/app_state.dart';

/// Levels arrive ten times a second, from two separate events, for as long as
/// the microphone is open — which on this app is the whole time it is running.
///
/// Sending those through the state object's own notifier rebuilt every widget
/// in the tree twenty times a second to move two bars. These tests pin down the
/// split, because it is the kind that quietly comes undone: adding a
/// `notifyListeners()` to one of these cases would look harmless, would look
/// correct on screen, and would put the cost straight back.
void main() {
  late AppState state;
  late int mainNotifications;
  late int meterNotifications;

  setUp(() {
    state = AppState();
    mainNotifications = 0;
    meterNotifications = 0;
    state.addListener(() => mainNotifications++);
    state.meters.addListener(() => meterNotifications++);
  });

  tearDown(() => state.dispose());

  AppEvent level(double db) => AppEvent.inputLevel(
    levelDb: db,
    speaking: db > -40,
    thresholdDb: -45,
    noiseFloorDb: -60,
  );

  test('a microphone level does not rebuild the interface', () {
    state.onEvent(level(-20));

    expect(
      mainNotifications,
      0,
      reason: 'the roster, the cards and the title bar are unchanged',
    );
    expect(meterNotifications, 1);
  });

  test('speaker levels do not rebuild the interface', () {
    state.onEvent(
      const AppEvent.speakerLevels(
        levels: [
          UiSpeakerLevel(serverId: 'a', session: 1, levelDb: -18),
        ],
      ),
    );

    expect(mainNotifications, 0);
    expect(meterNotifications, 1);
  });

  test('the levels themselves still reach anything drawing a meter', () {
    state.onEvent(level(-6));

    // Followed rather than assigned — the meter is interpolated towards the
    // reported value — so the test is that it moved, not that it arrived.
    expect(
      state.inputLevelDb,
      greaterThan(-120),
      reason: 'a meter that no longer updates is the way this breaks',
    );
    expect(state.speaking, isTrue);
    expect(state.noiseFloorDb, -60);
    expect(state.activationThresholdDb, -45);
  });

  test('a speaker level reaches the roster it is drawn in', () {
    state.onEvent(
      const AppEvent.speakerLevels(
        levels: [
          UiSpeakerLevel(serverId: 'server-1', session: 7, levelDb: -12),
        ],
      ),
    );

    expect(state.runtimeFor('server-1').speakerLevels[7], greaterThan(-120));
  });

  test('an event that is not a level still rebuilds the interface', () {
    // The counterpart to the tests above: this is the line between the two,
    // and a change that silenced everything would pass all of them but this.
    state.onEvent(
      const AppEvent.welcome(serverId: 'server-1', text: 'Hello'),
    );

    expect(mainNotifications, 1);
    expect(
      meterNotifications,
      0,
      reason: 'meter listeners rebuild at 10 Hz; do not wake them for a roster',
    );
  });

  test('the microphone is shut until something asks for it', () {
    // The engine used to open the microphone as it started and hold it for as
    // long as the app was installed — the recording indicator lit, and a
    // Bluetooth headset pinned to the hands-free profile so that everything
    // else the rider listened to sounded like a telephone. Nothing has asked
    // for a call here, so nothing should be open.
    expect(state.audioActive, isFalse);
  });

  test('a screen that let go of the microphone does not hold it open', () {
    // Releasing more often than holding must not underflow the count into
    // something that can never reach zero again — that would leave the
    // microphone open for the rest of the session, which is the exact fault
    // all of this exists to remove.
    state.releaseAudio();
    state.releaseAudio();
    expect(state.audioActive, isFalse);
  });

  test('a burst of levels leaves the interface untouched throughout', () {
    // One second of audio at the rate the engine actually reports.
    for (var i = 0; i < 10; i++) {
      state.onEvent(level(-30 + i.toDouble()));
      state.onEvent(
        AppEvent.speakerLevels(
          levels: [
            UiSpeakerLevel(
              serverId: 'a',
              session: 1,
              levelDb: -20 - i.toDouble(),
            ),
          ],
        ),
      );
    }

    expect(
      mainNotifications,
      0,
      reason: 'this is what twenty full rebuilds a second used to look like',
    );
    expect(meterNotifications, 20);
  });
}
