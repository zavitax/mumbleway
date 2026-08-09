import 'package:flutter/material.dart';
import 'package:url_launcher/url_launcher.dart';

import '../l10n/app_localizations.dart';

/// Links from the app to its own website.
///
/// The site is bilingual by whole copies of each page rather than by a query
/// parameter — `/settings.html` and `/ru/settings.html` are two files — so the
/// language has to be chosen when the link is built. Getting it wrong sends a
/// Russian-speaking rider to English documentation, which is the one thing this
/// feature exists to avoid.
class SiteLinks {
  /// No trailing slash: every path below starts with one, and two slashes in
  /// the middle of a GitHub Pages URL is a 404 rather than a redirect.
  static const String base = 'https://zavitax.github.io/mumbleway';

  /// The language segment for a code, empty for the site's default.
  ///
  /// English lives at the root and every other language under its own
  /// directory, which is how the pages are laid out and not a choice made here.
  static String _segment(String languageCode) =>
      languageCode == 'en' ? '' : '/$languageCode';

  static Uri home(String languageCode) =>
      Uri.parse('$base${_segment(languageCode)}');

  static Uri settings(String languageCode) =>
      Uri.parse('$base${_segment(languageCode)}/settings.html');
}

/// The language the site should be opened in, from the app's own choice.
///
/// Reads the locale actually in force — the one the language button sets, not
/// the device's — because a rider who switched the app to Russian is telling us
/// which language they read in. Falls back to the platform locale, then to
/// English, so this can never produce a segment for a language the site does
/// not have.
String siteLanguage(BuildContext context) {
  final code = Localizations.localeOf(context).languageCode;
  return L.supportedLocales
          .any((l) => l.languageCode == code)
      ? code
      : 'en';
}

/// Opens a page of the site in the platform's browser.
///
/// Reports failure rather than swallowing it. A link that silently does nothing
/// is indistinguishable from a tap that missed, and on a device with no browser
/// — or an Android build whose manifest does not declare the query — that is
/// exactly what happens.
Future<void> openSite(BuildContext context, Uri url) async {
  final messenger = ScaffoldMessenger.of(context);
  final l = L.of(context);
  var opened = false;
  try {
    opened = await launchUrl(url, mode: LaunchMode.externalApplication);
  } catch (_) {
    opened = false;
  }
  if (!opened) {
    messenger.showSnackBar(SnackBar(content: Text(l.couldNotOpenLink)));
  }
}
