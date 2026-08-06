import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mumbleway/widgets/app_bar_title.dart';
import 'package:mumbleway/widgets/wordmark.dart';

/// Measures the lockup in a real widget tree.
///
/// Fonts are not loaded in tests — every glyph is the test harness's fixed
/// advance — so these numbers are not the shipping ones. That is fine for
/// everything here: the two layouts are compared against *each other* in the
/// same font, and the relationships being asserted (narrower than, ends past,
/// stands as tall as) are the ones the design is made of.
Future<Size> _measure(
  WidgetTester tester,
  Size Function(BuildContext) of, {
  double textScale = 1,
}) async {
  late Size result;
  await tester.pumpWidget(
    MediaQuery(
      data: MediaQueryData(textScaler: TextScaler.linear(textScale)),
      child: Directionality(
        textDirection: TextDirection.ltr,
        child: DefaultTextStyle(
          style: const TextStyle(fontSize: 22),
          child: Builder(
            builder: (context) {
              result = of(context);
              return const SizedBox();
            },
          ),
        ),
      ),
    ),
  );
  return result;
}

/// Width of the single-line lockup this replaced: icon, gap, "MumbleWay".
double _singleLineWidth(BuildContext context, {double iconSize = 26}) {
  const gap = 10.0;
  final painter = TextPainter(
    text: TextSpan(text: 'MumbleWay', style: AppBarTitle.wordmarkOf(context)),
    textDirection: TextDirection.ltr,
    textScaler: MediaQuery.textScalerOf(context),
    maxLines: 1,
  )..layout();
  final width = painter.width;
  painter.dispose();
  return iconSize + gap + width;
}

void main() {
  testWidgets('takes less width than the single line it replaced', (
    tester,
  ) async {
    // The reason this design exists. An app bar on a phone has a fixed budget
    // and "MumbleWay" set in one run spent too much of it, while the bar had
    // vertical room going unused.
    final stacked = await _measure(tester, (c) => Wordmark.measure(c));
    final single = await _measure(
      tester,
      (c) => Size(_singleLineWidth(c), 26),
    );

    expect(
      stacked.width,
      lessThan(single.width),
      reason:
          'the lockup is ${stacked.width.toStringAsFixed(1)} wide against '
          '${single.width.toStringAsFixed(1)} for the single line',
    );
  });

  testWidgets('stays narrower at every text scale a reader may set', (
    tester,
  ) async {
    // Both layouts grow with the reader's text size, and they grow at
    // different rates — six characters against nine. A saving that only held
    // at 100% would evaporate for exactly the readers who need the room most.
    for (final scale in [0.85, 1.0, 1.3, 2.0]) {
      final stacked = await _measure(
        tester,
        (c) => Wordmark.measure(c),
        textScale: scale,
      );
      final single = await _measure(
        tester,
        (c) => Size(_singleLineWidth(c), 26),
        textScale: scale,
      );
      expect(
        stacked.width,
        lessThan(single.width),
        reason: 'at text scale $scale the lockup was not the narrower of the two',
      );
    }
  });

  testWidgets('is no taller than the height it is asked for', (tester) async {
    // It sits in an app bar. A lockup that quietly exceeded its stated height
    // would push the toolbar out or be clipped, and neither failure names
    // itself.
    for (final height in [26.0, 32.0, 64.0]) {
      final size = await _measure(
        tester,
        (c) => Wordmark.measure(c, height: height),
      );
      expect(size.height, height);
      // Still recognisably a horizontal mark rather than a stack.
      expect(size.width, greaterThan(height));
    }
  });

  testWidgets('scales as one object', (tester) async {
    // Doubling the height should double the whole mark, gap included. It used
    // to have a fixed 9px gap, which meant the logo was a different design at
    // 32px and at 400px.
    final small = await _measure(tester, (c) => Wordmark.measure(c, height: 32));
    final large = await _measure(tester, (c) => Wordmark.measure(c, height: 64));
    expect(large.width / small.width, closeTo(2, 0.01));
  });

  testWidgets('renders, and says its whole name to a screen reader', (
    tester,
  ) async {
    final handle = tester.ensureSemantics();
    await tester.pumpWidget(
      const MaterialApp(home: Scaffold(appBar: null, body: Wordmark())),
    );
    expect(tester.takeException(), isNull);
    // "Mumble" and "Way" announced separately would be describing a layout
    // accident rather than the name of the app.
    expect(find.bySemanticsLabel('MumbleWay'), findsOneWidget);
    handle.dispose();
  });
}
