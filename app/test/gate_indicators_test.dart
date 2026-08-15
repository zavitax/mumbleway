import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mumbleway/l10n/app_localizations.dart';
import 'package:mumbleway/widgets/gate_indicators.dart';

/// The gauges beside the analyser.
///
/// What is worth testing here is not the drawing — a bar a few dozen pixels
/// tall proves nothing in a test — but the two things that decide *whether* it
/// draws and *whether it changes*: the not-applicable state, and the equality
/// that keeps the notifier silent. The second is the reason this widget exists
/// as a snapshot rather than reading the chain status directly, and a broken
/// `==` would quietly put the panel back to repainting twenty times a second
/// with nothing to show for it.
void main() {
  GateSnapshot snap({
    double level = -30,
    double floor = -60,
    double opensAt = -52,
    double harmonicity = 0.8,
    double voiced = 0.75,
    bool held = false,
    bool applicable = true,
  }) =>
      GateSnapshot(
        levelDb: level,
        noiseFloorDb: floor,
        opensAtDb: opensAt,
        harmonicity: harmonicity,
        voicedThreshold: voiced,
        floorHeld: held,
        applicable: applicable,
      );

  Widget host(GateSnapshot? s) => MaterialApp(
        localizationsDelegates: L.localizationsDelegates,
        supportedLocales: L.supportedLocales,
        home: Scaffold(
          body: SizedBox(
            height: 120,
            child: GateIndicators(snapshot: s, floorDb: -90),
          ),
        ),
      );

  testWidgets('both gauges read out when the chain is running', (t) async {
    await t.pumpWidget(host(snap()));
    await t.pumpAndSettle();
    // -30 level against a -60 floor is 30 dB of headroom.
    expect(find.text('30 dB'), findsOneWidget);
    expect(find.text('0.80'), findsOneWidget);
  });

  testWidgets('an em dash rather than a zero when not applicable', (t) async {
    // Suppression off: no tracked floor, no margin, so there is nothing to
    // measure against. A zero here would be a measurement nobody made.
    await t.pumpWidget(host(snap(applicable: false)));
    await t.pumpAndSettle();
    expect(find.text('—'), findsNWidgets(2));
    expect(find.text('30 dB'), findsNothing);
  });

  testWidgets('nothing at all still draws the section', (t) async {
    // Greyed rather than gone: a section that vanishes reads as a layout bug.
    await t.pumpWidget(host(null));
    await t.pumpAndSettle();
    expect(find.byType(GateIndicators), findsOneWidget);
    expect(find.text('—'), findsNWidgets(2));
  });

  test('the snapshot is silent when nothing a bar can show has moved', () {
    // The whole reason for a snapshot type. A hundredth of a decibel cannot be
    // drawn on this bar, and a value that jitters in the last digit would
    // repaint it on every poll for ever.
    expect(snap(level: -30.0), snap(level: -30.02));
    expect(snap(harmonicity: 0.80), snap(harmonicity: 0.801));
    // And moves when something visible does.
    expect(snap(level: -30.0), isNot(snap(level: -28.0)));
    expect(snap(held: false), isNot(snap(held: true)));
    expect(snap(applicable: true), isNot(snap(applicable: false)));
  });

  test('the snr is the distance between the level and the floor', () {
    // Derived rather than carried, so the bar's height and its caption cannot
    // disagree about what they are showing.
    expect(snap(level: -20, floor: -60).snrDb, closeTo(40, 0.001));
    expect(snap(level: -55, floor: -60).snrDb, closeTo(5, 0.001));
  });
}
