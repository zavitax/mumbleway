import 'dart:io';
import 'dart:ui' as ui;

import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mumbleway/widgets/app_bar_title.dart';
import 'package:mumbleway/widgets/wordmark.dart';

/// Renders the lockup to a PNG so it can be looked at.
///
/// Not a test of anything — a viewer. Kept out of the default run by its
/// filename, which does not end in `_test.dart`.
void main() {
  testWidgets('render', (tester) async {
    // Both the font load and the image encode below have to run on a real
    // event loop; the fake one a widget test installs deadlocks on them.
    await tester.runAsync(() async {
      final loader = FontLoader('Exo2')
        ..addFont(rootBundle.load('assets/fonts/Exo2-Variable.ttf'));
      await loader.load();
    });

    final key = GlobalKey();
    await tester.pumpWidget(
      MaterialApp(
        debugShowCheckedModeBanner: false,
        home: RepaintBoundary(
          key: key,
          child: SizedBox(
            width: 360, // a narrow phone, where the width actually matters
            child: Column(
              mainAxisSize: MainAxisSize.min,
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                // In situ: the real bar, the real buttons beside it.
                AppBar(
                  title: const FittedBox(
                    fit: BoxFit.scaleDown,
                    alignment: Alignment.centerLeft,
                    child: Wordmark(),
                  ),
                  actions: const [
                    Icon(Icons.language),
                    SizedBox(width: 16),
                    Icon(Icons.monitor_heart_outlined),
                    SizedBox(width: 16),
                    Icon(Icons.hearing),
                    SizedBox(width: 16),
                    Icon(Icons.settings),
                    SizedBox(width: 12),
                  ],
                ),
                const SizedBox(height: 6),
                // The one it replaces, for comparison.
                AppBar(
                  title: const AppBarTitle('MumbleWay'),
                  actions: const [
                    Icon(Icons.language),
                    SizedBox(width: 16),
                    Icon(Icons.monitor_heart_outlined),
                    SizedBox(width: 16),
                    Icon(Icons.hearing),
                    SizedBox(width: 16),
                    Icon(Icons.settings),
                    SizedBox(width: 12),
                  ],
                ),
                const SizedBox(height: 22),
                const DefaultTextStyle(
                  style: TextStyle(color: Colors.white, fontSize: 22),
                  child: Padding(
                    padding: EdgeInsets.all(16),
                    child: Column(
                      mainAxisSize: MainAxisSize.min,
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Wordmark(height: 64),
                        SizedBox(height: 18),
                        Wordmark(height: 112),
                      ],
                    ),
                  ),
                ),
              ],
            ),
          ),
        ),
      ),
    );
    // The helmet is an asset image, which resolves through a real bundle read.
    // Precached after the first pump — a context is needed — and then pumped
    // again so the decoded frame is actually painted.
    await tester.runAsync(
      () => precacheImage(
        const AssetImage('assets/icon/mumbleway.png'),
        key.currentContext!,
      ),
    );
    await tester.pumpAndSettle();

    // The real numbers, in the real face — the widget test measures in the
    // harness's fixed-advance font, which is fine for comparing the two
    // layouts but tells you nothing about the actual saving.
    final context = key.currentContext!;
    final lockup = Wordmark.measure(context, height: 32);
    final single = TextPainter(
      text: TextSpan(
        text: 'MumbleWay',
        style: Wordmark.styleFor(context, 22 * 0.75),
      ),
      textDirection: TextDirection.ltr,
      maxLines: 1,
    )..layout();
    // The line-height ratio Flutter resolves for this face, which the SVG
    // generator has to agree with or the two renderings drift apart.
    final probe = TextPainter(
      text: TextSpan(text: 'Way', style: Wordmark.styleFor(context, 100)),
      textDirection: TextDirection.ltr,
      maxLines: 1,
    )..layout();
    // ignore: avoid_print
    print(
      'WIDTHS  lockup=${lockup.width.toStringAsFixed(1)}  '
      'single=${(26 + 10 + single.width).toStringAsFixed(1)}  '
      'lineRatio=${(probe.height / 100).toStringAsFixed(4)}  '
      'aspect=${(lockup.width / 34).toStringAsFixed(3)}  '
      'Way@100=${probe.width.toStringAsFixed(2)}',
    );
    probe.dispose();
    single.dispose();

    final boundary =
        key.currentContext!.findRenderObject()! as RenderRepaintBoundary;
    await tester.runAsync(() async {
      final image = await boundary.toImage(pixelRatio: 3);
      final data = await image.toByteData(format: ui.ImageByteFormat.png);
      final out = File(
        Platform.environment['WORDMARK_OUT'] ?? 'build/wordmark-preview.png',
      );
      await out.parent.create(recursive: true);
      await out.writeAsBytes(data!.buffer.asUint8List());
    });
  });
}
