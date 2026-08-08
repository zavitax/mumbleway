import 'dart:io' show Directory, File;

import 'package:flutter/material.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mumbleway/l10n/app_localizations.dart';
import 'package:mumbleway/state/app_state.dart';
import 'package:mumbleway/theme.dart';
import 'package:mumbleway/widgets/recording_preview.dart';
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

        // The card opens folded, and everything this test is about — the
        // status line and the two buttons — is inside the fold. Opening it is
        // the state worth testing: folded, there is nothing to squeeze.
        await tester.tap(find.byIcon(Icons.expand_more));
        await tester.pump(const Duration(milliseconds: 200));

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

  group('deleting one recording from the listen sheet', () {
    // The audio and its `.csv` are one thing split across two files. Audio
    // without the decision log is a recording nobody can say anything about,
    // and the log names the fault without anybody listening to a voice — so
    // the pair is the unit everywhere it is handled. The sheet deletes by
    // stem, which is the one place that rule is easy to get wrong: the button
    // sits beside a chip showing only the `.s16`.
    late Directory dir;

    setUp(() {
      dir = Directory.systemTemp.createTempSync('mumbleway-preview');
      for (final stem in ['20260808-1139-000', '20260808-1141-000']) {
        // Non-empty, so opening it is a real read rather than a special case.
        File('${dir.path}/$stem.s16').writeAsBytesSync(List.filled(9600, 0));
        File('${dir.path}/$stem.csv').writeAsStringSync('block,transmitting\n');
      }
    });

    tearDown(() {
      try {
        dir.deleteSync(recursive: true);
      } catch (_) {
        // Windows will not unlink a file the player still has open, so a test
        // that failed before dismissing the sheet leaves its temp directory
        // behind. Reporting that here would bury the reason the test failed.
      }
    });

    Widget harness(AppState state, Directory dir) => MaterialApp(
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
          // Below the scope, deliberately. `showRecordingPreview` reads the
          // app state from the context it is handed, and a context taken from
          // above the scope cannot see it — which is the same mistake the
          // sheet itself would make if it read the state from its own
          // ancestry, since a modal route hangs off the Navigator.
          child: Builder(
            builder: (context) => TextButton(
              onPressed: () => showRecordingPreview(context, dir),
              child: const Text('open'),
            ),
          ),
        ),
      ),
    );

    /// Pumps a fixed span rather than settling.
    ///
    /// The waveform is scanned on a background isolate and the sheet shows a
    /// spinner until it lands, so `pumpAndSettle` never returns — there is
    /// always another frame of animation owed. Fixed pumps are enough for the
    /// sheet and dialog transitions, which is all these tests drive.
    Future<void> beat(WidgetTester tester) async {
      await tester.pump();
      await tester.pump(const Duration(milliseconds: 400));
    }

    /// Lets real work finish.
    ///
    /// Opening the file and scanning its waveform are real I/O on a real
    /// isolate, and a widget test runs on fake time that never advances
    /// either. Without this the sheet is still opening its file when the tap
    /// arrives — which is a real state, and one the sheet now handles, but not
    /// the state these tests are about.
    Future<void> realWork(WidgetTester tester) async {
      // Several turns rather than one long sleep. Opening the file, spawning
      // the isolate that scans the waveform and closing the handle again are
      // separate pieces of real work, and each one only starts once the
      // previous has been pumped back into the widget.
      for (var i = 0; i < 12; i++) {
        await tester.runAsync(
          () => Future<void>.delayed(const Duration(milliseconds: 100)),
        );
        await beat(tester);
      }
    }

    testWidgets('takes the decision log with the audio', (tester) async {
      await tester.pumpWidget(harness(state, dir));
      await tester.tap(find.text('open'));
      await beat(tester);
      await realWork(tester);

      // Newest first, so the sheet opens on 1141 and that is what goes.
      await tester.tap(find.byIcon(Icons.delete_outline));
      await beat(tester);

      // Asks first. The ride cannot be recorded again, and this button is a
      // thumb's width from a transport control that gets pressed repeatedly.
      expect(find.text('Delete this recording?'), findsOneWidget);
      expect(
        File('${dir.path}/20260808-1141-000.s16').existsSync(),
        isTrue,
        reason: 'nothing may go before the question is answered',
      );

      // Two different clocks, in this order, and both are needed.
      //
      // The tap and the pump run on fake time: the dialog's dismissal is an
      // animation, and until it has been pumped the `showDialog` future has
      // not completed, so the delete has not been asked for at all. The unlink
      // that follows then waits on a real file handle closing, which fake time
      // never advances — hence `runAsync` after it, not around it.
      await tester.tap(find.widgetWithText(FilledButton, 'Delete'));
      await beat(tester);
      await realWork(tester);

      expect(File('${dir.path}/20260808-1141-000.s16').existsSync(), isFalse);
      expect(
        File('${dir.path}/20260808-1141-000.csv').existsSync(),
        isFalse,
        reason: 'a log left behind describes a ride nobody can hear',
      );

      // The other ride is untouched, both halves of it.
      expect(File('${dir.path}/20260808-1139-000.s16').existsSync(), isTrue);
      expect(File('${dir.path}/20260808-1139-000.csv').existsSync(), isTrue);
    });

    testWidgets('cancelling keeps both files', (tester) async {
      await tester.pumpWidget(harness(state, dir));
      await tester.tap(find.text('open'));
      await beat(tester);
      await realWork(tester);

      await tester.tap(find.byIcon(Icons.delete_outline));
      await beat(tester);
      await tester.tap(find.widgetWithText(TextButton, 'Cancel'));
      await beat(tester);

      for (final stem in ['20260808-1139-000', '20260808-1141-000']) {
        expect(File('${dir.path}/$stem.s16').existsSync(), isTrue);
        expect(File('${dir.path}/$stem.csv').existsSync(), isTrue);
      }
    });
  });
}
