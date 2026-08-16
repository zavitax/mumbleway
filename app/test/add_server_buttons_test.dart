import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mumbleway/l10n/app_localizations.dart';
import 'package:mumbleway/screens/add_server_screen.dart';
import 'package:mumbleway/state/app_state.dart';

/// The two word buttons on the add-server screen, at the width a narrow phone
/// gives them, in the language that needs the most room.
///
/// **A word broken in half reads as a rendering fault, not as a long label.**
/// «Публичные серверы» did exactly that: the icon and its gap took about 34 of
/// the ~130 device pixels the button gets beside its neighbours, and the
/// remainder was narrower than the first word. Wrapping *between* words is
/// fine and expected — the row is sized to its tallest member for that reason.
///
/// So the assertion is not "it fits on one line", which would be false on a
/// narrow phone in either language and would fail for the wrong reason. It is
/// that the space available is at least as wide as the longest single word,
/// which is exactly the condition for never having to break inside one.
/// `getMinIntrinsicWidth` on a paragraph is that longest-word width, so this
/// asks the rendered widget rather than re-measuring the font by hand.
void main() {
  Widget host(Locale locale) => AppStateScope(
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
      home: const AddServerScreen(),
    ),
  );

  // A narrow phone is where the room runs out and the buttons take two rows;
  // a desktop window keeps all three on one. Both are checked, because the fix
  // for the first is a layout that only applies below a threshold and a
  // threshold has two sides.
  for (final width in const [360.0, 900.0]) {
    for (final locale in const [Locale('ru'), Locale('en')]) {
      testWidgets(
        'no label breaks inside a word in ${locale.languageCode} at ${width.toInt()}dp',
        (t) async {
          t.view.physicalSize = Size(width, 800);
          t.view.devicePixelRatio = 1.0;
          addTearDown(t.view.resetPhysicalSize);
          addTearDown(t.view.resetDevicePixelRatio);

          await t.pumpWidget(host(locale));
          await t.pumpAndSettle();

          final l = await L.delegate.load(locale);
          for (final label in [l.browsePublic, l.importLabel]) {
            final finder = find.text(label);
            expect(
              finder,
              findsOneWidget,
              reason: '"$label" is not on the screen',
            );
            final paragraph = t.renderObject<RenderParagraph>(finder);
            final longestWord = paragraph.getMinIntrinsicWidth(double.infinity);
            expect(
              paragraph.size.width,
              greaterThanOrEqualTo(longestWord),
              reason:
                  '"$label" has ${paragraph.size.width.toStringAsFixed(1)}px for a '
                  'longest word of ${longestWord.toStringAsFixed(1)}px, so it '
                  'breaks mid-word. Drop an icon rather than shortening the words.',
            );
          }
        },
      );
    }
  }

  testWidgets('and neither of them carries an icon', (t) async {
    // The mechanism the test above depends on, asserted directly so that
    // putting an icon back fails here with the reason rather than there with a
    // width.
    await t.pumpWidget(host(const Locale('ru')));
    await t.pumpAndSettle();
    expect(find.byIcon(Icons.public), findsNothing);
    expect(find.byIcon(Icons.download), findsNothing);
  });
}
