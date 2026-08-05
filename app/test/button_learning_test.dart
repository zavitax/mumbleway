import 'package:flutter_test/flutter_test.dart';
import 'package:mumbleway/services/button_controller.dart';

/// Learning a button from a handlebar remote.
///
/// Remotes present as Bluetooth media keys — previous, next, play/pause — and
/// those arrive by a different route than an ordinary keyboard. Learning used
/// to end only on the keyboard route, so a real remote could be seen, named
/// and shown in the diagnostics panel while the learning screen went on
/// waiting for a press that had already happened.
void main() {
  // A singleton, so each test starts by putting it back to a known state
  // rather than building a fresh one.
  final buttons = ButtonController.instance;
  setUp(() {
    buttons.cancelLearning();
    buttons.setBindings(const []);
    buttons.onTransmit = null;
    buttons.lastMediaKey = null;
  });

  // Android key codes, which is the space the platform channels report in.
  const playPause = 85;
  const next = 87;
  const previous = 88;

  group('learning from a media remote', () {
    test('a media button completes learning', () {
      int? learnedId;
      String? learnedLabel;
      buttons.learnNext((id, label) {
        learnedId = id;
        learnedLabel = label;
      });
      expect(buttons.isLearning, isTrue);

      buttons.handleMediaButton(playPause, true);

      expect(buttons.isLearning, isFalse, reason: 'learning never finished');
      expect(learnedId, ButtonController.mediaKeyId(playPause));
      expect(learnedLabel, isNotNull);
      expect(learnedLabel, isNotEmpty);
    });

    test('each of the buttons a remote actually sends can be learned', () {
      for (final code in [playPause, next, previous]) {
          int? learned;
        buttons.learnNext((id, _) => learned = id);
        buttons.handleMediaButton(code, true);
        expect(
          learned,
          ButtonController.mediaKeyId(code),
          reason: 'key code $code could not be learned',
        );
      }
    });

    test('the press that binds does not also fire what it was bound to', () {
      // Otherwise binding play/pause to the talk button keys the microphone
      // at the moment it is chosen, which on a remote held in a glove is a
      // burst of open mic nobody asked for.
      var transmits = 0;
      buttons.onTransmit = (_) => transmits++;
      buttons.setBindings([
        ButtonBinding(
          keyId: ButtonController.mediaKeyId(playPause),
          action: ButtonAction.pushToTalk,
          label: 'Play/Pause',
        ),
      ]);

      buttons.learnNext((_, _) {});
      buttons.handleMediaButton(playPause, true);
      buttons.handleMediaButton(playPause, false);

      expect(transmits, 0);
    });

    test('the release alone does not finish a binding', () {
      // A remote whose press was consumed elsewhere should not bind on the
      // way back up, or the button learned is whichever one was let go last.
      var learned = false;
      buttons.learnNext((_, _) => learned = true);

      buttons.handleMediaButton(next, false);

      expect(learned, isFalse);
      expect(buttons.isLearning, isTrue);
    });

    test('a press is still reported for diagnosis while learning', () {
      // The panel exists to tell "the remote sends nothing" apart from "the
      // app hears nothing", and that has to keep working during learning.
      buttons.learnNext((_, _) {});
      buttons.handleMediaButton(previous, true);
      expect(buttons.lastMediaKey, isNotNull);
    });

    test('after learning, the button dispatches normally again', () {
      var transmits = 0;
      buttons.onTransmit = (_) => transmits++;
      buttons.learnNext((_, _) {});
      buttons.handleMediaButton(playPause, true);

      buttons.setBindings([
        ButtonBinding(
          keyId: ButtonController.mediaKeyId(playPause),
          action: ButtonAction.pushToTalk,
          label: 'Play/Pause',
        ),
      ]);
      buttons.handleMediaButton(playPause, true);

      expect(transmits, 1);
    });
  });
}
