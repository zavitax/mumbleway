import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mumbleway/widgets/app_bar_title.dart';
import 'package:mumbleway/widgets/wordmark.dart';

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

Future<double> _measureSingleLine(WidgetTester tester) async {
  late double result;
  await tester.pumpWidget(
    Directionality(
      textDirection: TextDirection.ltr,
      child: MediaQuery(
        data: const MediaQueryData(),
        child: DefaultTextStyle(
          style: const TextStyle(fontSize: 22),
          child: Builder(
            builder: (context) {
              result = _singleLineWidth(context);
              return const SizedBox();
            },
          ),
        ),
      ),
    ),
  );
  return result;
}

void main() {
  group('the asset', () {
    // The widget cannot measure an SVG without reading it, and an app bar
    // cannot wait for a file. So the aspect is a constant — and a constant
    // copied from a generated file is a constant that goes stale. This is the
    // guard: change the design, re-run tool/make_logo.py, and if the shape
    // moved this fails until the number is updated.
    for (final variant in ['on-dark', 'on-light']) {
      test('$variant declares the aspect the widget assumes', () {
        final file = File('assets/logo/mumbleway-logo-$variant.svg');
        expect(file.existsSync(), isTrue, reason: '${file.path} is missing');

        final box = RegExp(
          r'viewBox="0 0 ([\d.]+) ([\d.]+)"',
        ).firstMatch(file.readAsStringSync());
        expect(box, isNotNull, reason: 'no viewBox in ${file.path}');

        final w = double.parse(box!.group(1)!);
        final h = double.parse(box.group(2)!);
        expect(
          w / h,
          closeTo(Wordmark.aspectRatio, 0.005),
          reason:
              '${file.path} is ${(w / h).toStringAsFixed(3)}:1 but '
              'Wordmark.aspectRatio says '
              '${Wordmark.aspectRatio.toStringAsFixed(3)}. Re-run '
              'tool/make_logo.py, then update the constant.',
        );
      });
    }

    test('the wordmark is outlined, so it needs no font installed', () {
      // A <text> element would render as whatever the viewer happens to have,
      // which for a logo means "not this logo".
      final svg = File(
        'assets/logo/mumbleway-logo-on-dark.svg',
      ).readAsStringSync();
      expect(svg, isNot(contains('<text')));
      expect(svg, isNot(contains('font-family')));
    });

    for (final variant in ['on-dark', 'on-light']) {
      test('$variant has no namespace-prefixed elements', () {
        // The bug this exists to catch rendered one of the two files with
        // <ns0:linearGradient xmlns:ns0="..."> instead of <linearGradient>,
        // depending only on which was written first. Browsers resolve both, so
        // the SVG looked perfect everywhere except in the app, where
        // flutter_svg declined to resolve the prefixed gradient and dropped the
        // visor and the sound waves — silently, leaving a blank white helmet.
        final svg = File(
          'assets/logo/mumbleway-logo-$variant.svg',
        ).readAsStringSync();
        expect(
          RegExp(r'<[a-zA-Z][\w.-]*:').hasMatch(svg),
          isFalse,
          reason: 'namespace-prefixed elements in mumbleway-logo-$variant.svg',
        );
      });

      test('$variant resolves every paint it references', () {
        // A url(#...) pointing at an id that is not in the file paints nothing
        // at all, and nothing at all is indistinguishable from a shape that was
        // meant to be invisible.
        final svg = File(
          'assets/logo/mumbleway-logo-$variant.svg',
        ).readAsStringSync();
        final declared = RegExp(
          'id="([^"]+)"',
        ).allMatches(svg).map((m) => m.group(1)).toSet();
        final referenced = RegExp(
          r'url\(#([^)]+)\)',
        ).allMatches(svg).map((m) => m.group(1)).toSet();
        expect(referenced, isNotEmpty, reason: 'the accent should be a gradient');
        expect(referenced.difference(declared), isEmpty);
      });
    }

    test('the light variant drops the tile and inks the shell', () {
      final dark = File(
        'assets/logo/mumbleway-logo-on-dark.svg',
      ).readAsStringSync();
      final light = File(
        'assets/logo/mumbleway-logo-on-light.svg',
      ).readAsStringSync();
      // The pale shell is what makes sense on a dark tile, and the tile is what
      // makes the helmet a brick on a white page.
      expect(dark, contains('#F4F8FC'));
      expect(light, isNot(contains('#F4F8FC')));
      expect(light, contains('#101822'));
    });
  });

  group('the lockup', () {
    testWidgets('takes less width than the single line it replaced', (
      tester,
    ) async {
      // The reason this design exists. An app bar on a phone has a fixed budget
      // and "MumbleWay" set in one run, with an icon in front of it, spent too
      // much of it — while the bar had vertical room going unused.
      //
      // Measured in the test harness's fixed-advance font, which is not the
      // shipping one; that is fine, because what is being compared is one
      // layout against another in the same font.
      final single = await _measureSingleLine(tester);
      final stacked = Wordmark.sizeFor(34).width;
      expect(
        stacked,
        lessThan(single),
        reason:
            'the lockup is ${stacked.toStringAsFixed(1)} wide against '
            '${single.toStringAsFixed(1)} for the single line',
      );
    });

    test('scales as one object', () {
      // Doubling the height doubles the whole mark. It once had a fixed 9px
      // gap between helmet and name, which made it a different design at 32px
      // and at 400px.
      expect(
        Wordmark.sizeFor(68).width / Wordmark.sizeFor(34).width,
        closeTo(2, 0.001),
      );
      expect(Wordmark.sizeFor(34).height, 34);
    });

    test('picks the variant that reads against the background', () {
      expect(Wordmark.assetFor(Brightness.dark), contains('on-dark'));
      expect(Wordmark.assetFor(Brightness.light), contains('on-light'));
    });

    testWidgets('renders, and says its whole name to a screen reader', (
      tester,
    ) async {
      final handle = tester.ensureSemantics();
      await tester.pumpWidget(
        const MaterialApp(home: Scaffold(body: Wordmark())),
      );
      expect(tester.takeException(), isNull);
      expect(find.bySemanticsLabel('MumbleWay'), findsOneWidget);
      handle.dispose();
    });
  });
}
