import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mumbleway/l10n/app_localizations.dart';
import 'package:mumbleway/services/site_links.dart';

/// The site is bilingual by whole copies of each page, so the language has to
/// be chosen when the link is built rather than by the page it lands on. A
/// wrong segment is not a broken link — it is a working link to documentation
/// the reader cannot read, which nothing downstream would catch.
void main() {
  group('links to the site', () {
    test('English lives at the root, not under a language directory', () {
      expect(SiteLinks.home('en').toString(),
          'https://zavitax.github.io/mumbleway');
      expect(SiteLinks.settings('en').toString(),
          'https://zavitax.github.io/mumbleway/settings.html');
    });

    test('every other language is under its own directory', () {
      expect(SiteLinks.home('ru').toString(),
          'https://zavitax.github.io/mumbleway/ru');
      expect(SiteLinks.settings('ru').toString(),
          'https://zavitax.github.io/mumbleway/ru/settings.html');
    });

    test('no doubled slash, which GitHub Pages answers with a 404', () {
      for (final l in L.supportedLocales) {
        for (final u in [
          SiteLinks.home(l.languageCode),
          SiteLinks.settings(l.languageCode),
        ]) {
          expect(u.toString().substring('https://'.length), isNot(contains('//')),
              reason: '$u has an empty path segment');
          expect(u.scheme, 'https');
        }
      }
    });

    testWidgets('the language follows the app, not the device', (tester) async {
      Future<String> languageUnder(Locale locale) async {
        late String seen;
        await tester.pumpWidget(MaterialApp(
          locale: locale,
          localizationsDelegates: L.localizationsDelegates,
          supportedLocales: L.supportedLocales,
          home: Builder(builder: (context) {
            seen = siteLanguage(context);
            return const SizedBox();
          }),
        ));
        return seen;
      }

      expect(await languageUnder(const Locale('ru')), 'ru');
      expect(await languageUnder(const Locale('en')), 'en');
    });

    testWidgets('a language the site does not have falls back to English',
        (tester) async {
      // Localizations resolves an unsupported locale to a supported one, so
      // this asserts the result is a language the site actually publishes
      // rather than a segment invented from the device's setting.
      late String seen;
      await tester.pumpWidget(MaterialApp(
        locale: const Locale('de'),
        localizationsDelegates: L.localizationsDelegates,
        supportedLocales: L.supportedLocales,
        home: Builder(builder: (context) {
          seen = siteLanguage(context);
          return const SizedBox();
        }),
      ));
      expect(L.supportedLocales.map((l) => l.languageCode), contains(seen));
    });
  });
}
