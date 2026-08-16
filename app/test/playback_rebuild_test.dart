import 'dart:io' show Directory, File;

import 'package:flutter/material.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mumbleway/l10n/app_localizations.dart';
import 'package:mumbleway/state/app_state.dart';
import 'package:mumbleway/theme.dart';
import 'package:mumbleway/widgets/recording_preview.dart';

// What one playback tick rebuilds in the listen sheet.
//
// `RecordingPlayer` is a `ChangeNotifier` that fires every 80 ms while a
// recording is playing — 12.5 times a second, for as long as somebody is
// listening to a ride. Everything inside the `ListenableBuilder` watching it
// rebuilds on each one, and what that costs is not visible from the widget
// tree: the notifier is there to move a playhead, and it moves ten controls
// that have nothing to do with the playhead as well.
//
// Driven through `seekToFraction`, which is the same notification the timer
// sends and is reachable from a test, where the timer is not: playback needs
// an audio device.
//
// Where it stood when this was last run, per notification:
//
//   before   165 widgets — everything inside one `ListenableBuilder`
//   after      2 widgets — the clock, when the second it shows changes
//              9 widgets — the scrubber, when the playhead moves
//              0 widgets — the transport controls, unless one of them changed
//
// **The seek here does not move the playhead**, which is the one thing this
// test cannot arrange: `progress` is `_positionSamples / _totalSamples`, and
// the total is zero because opening a recording for playback needs the audio
// engine. So the printed rebuild count omits the scrubber, and the subtree is
// counted separately to say what a real tick costs on top of it.
//
// Nothing here asserts. See `rebuild_cost_test.dart` for why.
void main() {
  late Directory dir;

  setUp(() {
    dir = Directory.systemTemp.createTempSync('mumbleway-playback');
    File(
      '${dir.path}/20260808-1139-000.s16',
    ).writeAsBytesSync(List.filled(96000, 0));
    File(
      '${dir.path}/20260808-1139-000.csv',
    ).writeAsStringSync('block,transmitting\n');
  });

  tearDown(() {
    try {
      dir.deleteSync(recursive: true);
    } catch (_) {
      // Windows will not unlink a file the player still has open.
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
        child: Builder(
          builder: (context) => TextButton(
            onPressed: () => showRecordingPreview(context, dir),
            child: const Text('open'),
          ),
        ),
      ),
    ),
  );

  Future<void> beat(WidgetTester t) async {
    await t.pump();
    await t.pump(const Duration(milliseconds: 400));
  }

  /// Opening the file and scanning its waveform are real I/O on a real
  /// isolate, and a widget test runs on fake time that never advances either.
  Future<void> realWork(WidgetTester t) async {
    for (var i = 0; i < 12; i++) {
      await t.runAsync(
        () => Future<void>.delayed(const Duration(milliseconds: 100)),
      );
      await beat(t);
    }
  }

  testWidgets('what one playback tick rebuilds', (t) async {
    final state = AppState();
    addTearDown(state.dispose);
    t.view.physicalSize = const Size(430, 950);
    t.view.devicePixelRatio = 1.0;
    addTearDown(t.view.resetPhysicalSize);
    addTearDown(t.view.resetDevicePixelRatio);

    await t.pumpWidget(harness(state, dir));
    await t.tap(find.text('open'));
    await beat(t);
    await realWork(t);

    // **By its painter, not `find.byType(CustomPaint).last`.** Material draws
    // with `CustomPaint` all over the place — the first version of this tapped
    // one of the transport buttons instead, which called `setState` on the
    // sheet and reported that every selector had failed.
    final scrubber = find.byWidgetPredicate(
      (w) => w is CustomPaint && '${w.painter.runtimeType}' == '_WavePainter',
    );
    expect(scrubber, findsOneWidget, reason: 'the waveform never appeared');

    final counts = <String, int>{};
    final previous = debugPrint;
    debugPrint = (String? message, {int? wrapWidth}) {
      if (message == null) return;
      final name = message.split(' ').skip(1).firstOrNull;
      if (name != null) counts[name] = (counts[name] ?? 0) + 1;
    };
    final before = scrubber.evaluate().single.widget as CustomPaint;
    debugPrintRebuildDirtyWidgets = true;
    // A seek is the notification a playback tick sends, by the same path.
    await t.tapAt(t.getCenter(scrubber) + const Offset(20, 0));
    await t.pump(const Duration(milliseconds: 16));
    debugPrintRebuildDirtyWidgets = false;
    debugPrint = previous;

    final after = scrubber.evaluate().single.widget as CustomPaint;
    // ignore: avoid_print
    print(
      'TICK the waveform itself rebuilt: '
      '${!identical(before.painter, after.painter)}',
    );

    // What a real tick *does* rebuild, which this test cannot make happen:
    // `progress` is `_positionSamples / _totalSamples` and the total is zero
    // here, because opening a recording for playback needs the audio engine.
    // So the seek above moves the clock and not the playhead. During real
    // playback this subtree is the honest per-tick cost.
    var subtree = 0;
    scrubber.evaluate().single.visitAncestorElements((e) {
      if ('${e.widget.runtimeType}'.startsWith('WhenChanged<double>')) {
        void count(Element child) {
          subtree++;
          child.visitChildren(count);
        }

        e.visitChildren(count);
        return false;
      }
      return true;
    });
    // ignore: avoid_print
    print('TICK the scrubber subtree is $subtree widgets');

    final total = counts.values.fold(0, (a, b) => a + b);
    final worst = counts.entries.toList()
      ..sort((a, b) => b.value.compareTo(a.value));
    // ignore: avoid_print
    print(
      'TICK rebuilds $total widgets of ${t.allElements.length} on the sheet',
    );
    // ignore: avoid_print
    print(
      'TICK mine: ${worst.where((e) => e.key.contains('Preview') || e.key.contains('Scrubber') || e.key.contains('WhenChanged') || e.key.contains('Listenable')).map((e) => '${e.key}x${e.value}').join(', ')}',
    );

    // Repaints, which are the other half and are not rebuilds: the waveform is
    // a `CustomPaint`, so a tick that rebuilt nothing could still repaint the
    // whole sheet if the painter is not inside a boundary of its own.
    final boundaries = find.ancestor(
      of: scrubber,
      matching: find.byType(RepaintBoundary),
    );
    // ignore: avoid_print
    print(
      'TICK the waveform has ${boundaries.evaluate().length} '
      'RepaintBoundary ancestors',
    );
  });
}
