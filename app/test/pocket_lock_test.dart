import 'package:flutter_test/flutter_test.dart';
import 'package:mumbleway/src/rust/api/mumbleway.dart' show ConnStatus, MicMode;
import 'package:mumbleway/state/app_state.dart';

/// The lock must never outlive the control that releases it.
///
/// **A rider found the trap.** A server connection dropped while pocket mode
/// was armed, the panel disappeared because nothing was live any more — and
/// every other control stayed dead, with the one slider that could have
/// unlocked them no longer on the screen. There was no way out but to kill the
/// app.
///
/// The cause was two conditions for one fact, in two files: the screen locked
/// on `pocketActive` while the panel decided its own visibility from
/// `pocketAvailable && anyLive`. They agreed in every case anybody tried and
/// disagreed the moment a connection dropped.
///
/// So the lock is now *derived* from the panel's visibility, and this asserts
/// that relationship rather than the one symptom. Adding `anyLive` to the
/// screen's condition would have fixed the report and left the same two
/// expressions free to drift apart on the next reason a panel is not drawn.
void main() {
  /// Armed, connected, and nothing else — no engine, which is why
  /// `setPocketActive` is safe to call here: every platform call it makes is
  /// guarded, and the reconcile returns at once while no devices are open.
  Future<AppState> armed() async {
    final state = AppState()
      ..pocketMode = true
      ..micMode = MicMode.continuous;
    state.runtimeFor('a').status = ConnStatus.connected;
    await state.setPocketActive(true);
    return state;
  }

  group('pocket mode cannot lock a screen it has left', () {
    test('a dropped connection unlocks the screen', () async {
      final state = await armed();
      expect(state.pocketPanelVisible, isTrue, reason: 'the panel is drawn');
      expect(state.pocketLocked, isTrue, reason: 'so the screen is locked');

      // The server goes away. The panel has nothing to sit on.
      state.runtimeFor('a').status = ConnStatus.disconnected;
      expect(state.pocketPanelVisible, isFalse);
      expect(
        state.pocketLocked,
        isFalse,
        reason: 'the lock outlived the slider that releases it',
      );
    });

    test('a reconnection puts both back together', () async {
      final state = await armed();
      state.runtimeFor('a').status = ConnStatus.reconnecting;
      expect(state.pocketLocked, isFalse, reason: 'reconnecting is not live');

      state.runtimeFor('a').status = ConnStatus.connected;
      expect(
        state.pocketLocked,
        isTrue,
        reason: 'pocket mode is still armed, so the lock comes back with it',
      );
    });

    test('withdrawing the setting unlocks the screen', () async {
      final state = await armed();
      expect(state.pocketLocked, isTrue);

      state.pocketMode = false;
      expect(state.pocketPanelVisible, isFalse);
      expect(state.pocketLocked, isFalse);
    });

    test('push to talk takes the panel and the lock together', () async {
      final state = await armed();
      expect(state.pocketLocked, isTrue);

      state.micMode = MicMode.pushToTalk;
      expect(state.pocketAvailable, isFalse, reason: 'the mode excludes it');
      expect(state.pocketLocked, isFalse);
    });

    /// The relationship itself, over every combination: **locked implies
    /// drawn.** The tests above are instances of this; this is the thing a
    /// future change has to keep.
    test('a locked screen always has its slider on it', () async {
      for (final setting in [false, true]) {
        for (final live in [false, true]) {
          for (final arm in [false, true]) {
            for (final mode in MicMode.values) {
              final state = AppState()
                ..pocketMode = setting
                ..micMode = mode;
              if (live) {
                state.runtimeFor('a').status = ConnStatus.connected;
              }
              if (arm) await state.setPocketActive(true);
              if (state.pocketLocked) {
                expect(
                  state.pocketPanelVisible,
                  isTrue,
                  reason:
                      'locked with no slider: setting $setting, live $live, '
                      'arm $arm, mode $mode',
                );
              }
            }
          }
        }
      }
    });
  });
}
