import 'dart:io' show Directory;

import 'package:flutter_test/flutter_test.dart';
import 'package:mumbleway/state/app_state.dart';

/// Diagnostic recording has to open the microphone, and give it back.
///
/// The recorder is fed by the engine's capture worker, and that worker does not
/// run until the devices are open. Start the recorder without asking for them
/// and it writes a file that is empty, valid, and indistinguishable from a ride
/// nobody spoke on — which is the precise class of silent failure the feature
/// was built to remove. On Android it fails worse than empty: without the
/// session there is no hands-free link, so the audio would come from the
/// phone's own microphone rather than the headset's. That confusion is what
/// invalidated every measurement made before this existed.
///
/// So the coupling between "recording" and "the devices are open" is the thing
/// under test, and both ways of breaking it are invisible on screen.
void main() {
  late AppState state;

  setUp(() => state = AppState());
  tearDown(() => state.dispose());

  test('nothing holds the microphone open before anything asks', () {
    expect(state.audioHolds, 0);
    expect(state.diagnosticRecording, isFalse);
  });

  test('a recording that cannot open the microphone leaves no hold behind', () async {
    // There is no engine in a unit test, so opening the devices fails — which
    // is exactly the case worth pinning. A rider whose headset is held by
    // another app must get a switch that refuses to move and says why, rather
    // than one that flips on and records silence.
    final error = await state.beginDiagnosticRecording(
      Directory.systemTemp.path,
      'test',
    );

    expect(error, isNotNull, reason: 'a refusal has to reach the rider');
    expect(
      state.diagnosticRecording,
      isFalse,
      reason: 'recording without a microphone writes a file of nothing',
    );
    expect(
      state.audioHolds,
      0,
      reason: 'a leaked hold keeps the microphone open for the whole session',
    );
  });

  test('stopping when nothing is recording gives nothing back', () {
    // Teardown calls this, and so does a rider double-tapping the switch.
    expect(state.endDiagnosticRecording(), 0);
    expect(state.audioHolds, 0);
  });

  test('stopping a recording that never started takes nobody else\'s hold', () async {
    // `holdAudio` raises the count before it awaits the microphone, so this is
    // the real state of a settings screen while the devices are still
    // answering — and the moment when an unbalanced release does its damage.
    final pending = state.holdAudio();
    expect(state.audioHolds, 1);

    expect(state.endDiagnosticRecording(), 0);
    expect(
      state.audioHolds,
      1,
      reason: 'the microphone would shut under a screen still using it',
    );

    // The failed open gives back its own hold, through the door it took it
    // from. Nothing else should have moved the count in the meantime.
    await pending;
    expect(state.audioHolds, 0);
  });
}
