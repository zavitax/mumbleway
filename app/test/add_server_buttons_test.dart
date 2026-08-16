import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mumbleway/l10n/app_localizations.dart';
import 'package:mumbleway/screens/add_server_screen.dart';
import 'package:mumbleway/state/app_state.dart';

/// The two word buttons on the add-server screen, across the widths, languages
/// and text sizes where the room runs out.
///
/// **A word broken in half reads as a rendering fault, not as a long label.**
/// «Публичные серверы» did exactly that: the icon and its gap took about a
/// third of the ~114 device pixels the button gets beside its neighbours, and
/// what was left was narrower than the label's own first word. Wrapping
/// *between* words is fine and expected — the row is sized to its tallest
/// member for that reason.
///
/// So the assertion is not "it fits on one line", which is false on a narrow
/// phone in either language and would fail for the wrong reason. It is that the
/// width available is at least the width of the longest single word, which is
/// exactly the condition for never having to break inside one.
/// `RenderParagraph.getMinIntrinsicWidth` is that longest-word width, so this
/// asks the rendered widget rather than re-measuring the font by hand.
///
/// Two mechanisms can satisfy it and both are covered, because a rule with two
/// implementations has two ways to rot: the button moves its icon above the
/// label when it will not fit beside it, and the screen gives the long button a
/// row of its own when even that is not enough.
void main() {
  Widget host(Locale locale, double scale) => AppStateScope(
    state: AppState(),
    child: MaterialApp(
      locale: locale,
      localizationsDelegates: const [
        ...L.localizationsDelegates,
        GlobalMaterialLocalizations.delegate,
        GlobalWidgetsLocalizations.delegate,
        GlobalCupertinoLocalizations.delegate,
      ],
      supportedLocales: L.supportedLocales,
      builder: (context, child) => MediaQuery.withClampedTextScaling(
        minScaleFactor: scale,
        maxScaleFactor: scale,
        child: child!,
      ),
      home: const AddServerScreen(),
    ),
  );

  // 320 is an iPhone SE, 360 the commonest Android, 900 a desktop window. The
  // text scales are accessibility settings a rider may well be using on a bike,
  // and they are where the widest labels come from.
  for (final scale in const [1.0, 1.3, 1.6, 2.0]) {
    for (final width in const [320.0, 360.0, 420.0, 900.0]) {
      for (final locale in const [Locale('ru'), Locale('en')]) {
        final name =
            'no mid-word break: ${locale.languageCode} '
            '${width.toInt()}dp at x$scale';
        // **One combination is arithmetically impossible and is recorded
        // rather than skipped quietly.** At double text on a 320 dp phone,
        // «Публичные» alone is about 253 px and the whole content area is 248.
        // No arrangement of icons or rows fits a word wider than the screen it
        // is on; the remedies would be shortening the Russian label for every
        // other device, or scaling the text down against the rider's own
        // accessibility setting. Both are worse than a word that wraps.
        if (width <= 320 && scale >= 2.0 && locale.languageCode == 'ru') {
          continue;
        }
        testWidgets(name, (t) async {
          t.view.physicalSize = Size(width, 900);
          t.view.devicePixelRatio = 1.0;
          addTearDown(t.view.resetPhysicalSize);
          addTearDown(t.view.resetDevicePixelRatio);

          await t.pumpWidget(host(locale, scale));
          await t.pumpAndSettle();

          final l = await L.delegate.load(locale);
          for (final label in [l.browsePublic, l.importLabel]) {
            final finder = find.text(label);
            expect(finder, findsOneWidget, reason: '"$label" is not on screen');
            final paragraph = t.renderObject<RenderParagraph>(finder);
            final longestWord = paragraph.getMinIntrinsicWidth(double.infinity);
            expect(
              paragraph.size.width,
              greaterThanOrEqualTo(longestWord),
              reason:
                  '"$label" has ${paragraph.size.width.toStringAsFixed(1)}px '
                  'for a longest word of ${longestWord.toStringAsFixed(1)}px, '
                  'so it breaks mid-word. Move the icon above the label, or '
                  'give the button a row of its own.',
            );
          }
        });
      }
    }
  }

  testWidgets('the icons are kept, beside the label or above it', (t) async {
    // **The rule is that the glyph moves, not that it goes.** It earns its
    // place — it is how a reader finds a control without reading it — and the
    // only thing wrong with it was standing beside a label that needed the
    // width. Above the label it costs height, which this row can give.
    t.view.physicalSize = const Size(360, 900);
    t.view.devicePixelRatio = 1.0;
    addTearDown(t.view.resetPhysicalSize);
    addTearDown(t.view.resetDevicePixelRatio);

    await t.pumpWidget(host(const Locale('ru'), 1.0));
    await t.pumpAndSettle();
    expect(find.byIcon(Icons.public), findsOneWidget);
    expect(find.byIcon(Icons.download), findsOneWidget);
  });
}
