import 'dart:io' show Directory;

import 'package:flutter/material.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mumbleway/l10n/app_localizations.dart';
import 'package:mumbleway/state/app_state.dart';
import 'package:mumbleway/theme.dart';
import 'package:mumbleway/widgets/recording_toggle.dart';

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

  group('the panel fits the narrowest phone', () {
    // The status line and the two buttons were one Row. The buttons take their
    // intrinsic width, the text got what was left, and on a narrow panel with
    // Russian labels that was a few pixels -- so the text wrapped one character
    // per line. It looked like a font bug and was a layout one.
    //
    // Russian is the case that broke, because both labels are longer than
    // their English equivalents. Testing English alone would have passed.
    Widget harness(Locale locale, AppState state) => MaterialApp(
      locale: locale,
      theme: buildTheme(Brightness.dark),
      supportedLocales: AppState.supportedLocales,
      localizationsDelegates: const [
        L.delegate,
        GlobalMaterialLocalizations.delegate,
        GlobalWidgetsLocalizations.delegate,
        GlobalCupertinoLocalizations.delegate,
      ],
      home: Scaffold(
        body: AppStateScope(
          state: state,
          child: const SingleChildScrollView(child: RecordingToggle()),
        ),
      ),
    );

    for (final locale in const [Locale('en'), Locale('ru')]) {
      testWidgets('in ${locale.languageCode}, on a 320pt screen', (tester) async {
        // 320 logical pixels is the narrowest phone still worth supporting,
        // and the diagnostics panel is full width, so this is the real case.
        tester.view.physicalSize = const Size(320, 640);
        tester.view.devicePixelRatio = 1.0;
        addTearDown(tester.view.resetPhysicalSize);
        addTearDown(tester.view.resetDevicePixelRatio);

        await tester.pumpWidget(harness(locale, state));
        await tester.pump(const Duration(milliseconds: 100));

        // No overflow. takeException would hold a FlutterError describing a
        // RenderFlex that ran off the side.
        expect(tester.takeException(), isNull);

        // And the status line has room to set. One character per line is what
        // the bug looked like, so the assertion is on the width the text was
        // actually given rather than on how it happens to wrap.
        final status = find.byWidgetPredicate(
          (w) => w is Text && (w.style?.fontSize == 12),
        );
        expect(status, findsWidgets);
        for (final element in status.evaluate()) {
          final width = (element.renderObject! as RenderBox).size.width;
          expect(
            width,
            greaterThan(120),
            reason: 'the status line was squeezed to ${width.toStringAsFixed(0)}pt, '
                'which wraps it a letter at a time',
          );
        }
      });
    }
  });
}
