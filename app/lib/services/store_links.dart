import 'dart:io' show Platform;

import 'package:flutter/foundation.dart' show kIsWeb, visibleForTesting;

/// Where to send somebody who has agreed to leave a review.
///
/// **The store page, not a native review sheet.** Apple's
/// `SKStoreReviewController` and Google Play's In-App Review both put a rating
/// prompt in front of the user without leaving the app, which is nicer — and
/// both refuse to say whether they showed anything or what the user did, and
/// both are rate-limited by the operating system on its own terms. An app that
/// cannot tell the difference between "asked and declined" and "never shown"
/// cannot honour a rule like "ask again if skipped".
///
/// So the asking is ours and the writing is theirs. The card records the
/// answer it can see; this opens the page. Adding `in_app_review` later would
/// buy the nicer sheet at the cost of a dependency on four platforms and the
/// loss of that signal, which is a trade to make deliberately rather than by
/// reaching for the obvious package.
class StoreLinks {
  const StoreLinks._();

  /// Apple's numeric id for MumbleWay, from App Store Connect. One id and one
  /// URL cover iPhone, iPad and Mac, and `apps.apple.com` redirects to the
  /// reader's own storefront.
  ///
  /// **One page does not mean one listing.** iOS and macOS are separate version
  /// records behind that id, and they hold different keywords, different
  /// descriptions and different promotional text — see `docs/STORE_SURVEY.md`,
  /// which found them drifted apart. It makes no difference to this link, and
  /// it is worth knowing before assuming a change on one is a change on both.
  static const appleId = '6797305046';

  /// The Microsoft Store product id, as it appears in the badge link on the
  /// website.
  static const microsoftId = '9PNZ7PWDVLTB';

  static const _androidPackage = 'com.mumbleway.mumbleway';

  /// The operating system, as one string, or `web` / `unknown`.
  ///
  /// Split out so the link builders below are pure functions of it. `Platform`
  /// cannot be faked, so a test running on Windows could only ever exercise
  /// the Windows branch — and the branch that matters most is whichever one
  /// the tester is not on.
  static String get _os {
    if (kIsWeb) return 'web';
    try {
      return Platform.operatingSystem;
    } catch (_) {
      // Platform is unavailable under some test harnesses.
      return 'unknown';
    }
  }

  /// The page to open, or null where there is no store to review in — a
  /// sideloaded Windows build, a Linux desktop, the web.
  static Uri? review() => reviewFor(_os);

  /// Where to go when the platform's own scheme did not open — Play not
  /// installed, the Store app missing from a stripped Windows image.
  static Uri? webFallback() => webFallbackFor(_os);

  @visibleForTesting
  static Uri? reviewFor(String os) {
    switch (os) {
      case 'ios':
      case 'macos':
        // `action=write-review` opens the page with the review sheet already
        // up, which is the whole point of sending them there.
        return Uri.parse(
          'https://apps.apple.com/app/id$appleId?action=write-review',
        );
      case 'android':
        // The `market:` scheme opens the Play app directly. `url_launcher`
        // reports failure if nothing handles it, and the caller falls back.
        return Uri.parse('market://details?id=$_androidPackage');
      case 'windows':
        return Uri.parse('ms-windows-store://review/?ProductId=$microsoftId');
      default:
        return null;
    }
  }

  @visibleForTesting
  static Uri? webFallbackFor(String os) {
    switch (os) {
      case 'android':
        return Uri.parse(
          'https://play.google.com/store/apps/details?id=$_androidPackage',
        );
      case 'windows':
        return Uri.parse('https://apps.microsoft.com/detail/$microsoftId');
      default:
        // Apple has no second address: `apps.apple.com` is already the web
        // page, so a failure there is not something another URL fixes.
        return null;
    }
  }
}
