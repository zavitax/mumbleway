import 'dart:io' show Platform;

import 'package:flutter/foundation.dart' show kIsWeb;

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

  /// Apple's numeric id for MumbleWay, from App Store Connect. The listing is
  /// universal — iPhone, iPad and Mac are one product page — so one id covers
  /// all three and `apps.apple.com` redirects to the reader's own storefront.
  static const appleId = '6797305046';

  /// The Microsoft Store product id, as it appears in the badge link on the
  /// website.
  static const microsoftId = '9PNZ7PWDVLTB';

  static const _androidPackage = 'com.mumbleway.mumbleway';

  /// The page to open, or null where there is no store to review in — a
  /// sideloaded Windows build, a Linux desktop, the web.
  static Uri? review() {
    if (kIsWeb) return null;
    try {
      if (Platform.isIOS || Platform.isMacOS) {
        // `action=write-review` opens the page with the review sheet already
        // up, which is the whole point of sending them there.
        return Uri.parse(
          'https://apps.apple.com/app/id$appleId?action=write-review',
        );
      }
      if (Platform.isAndroid) {
        // The `market:` scheme opens the Play app directly. `url_launcher`
        // reports failure if nothing handles it, and the caller falls back.
        return Uri.parse('market://details?id=$_androidPackage');
      }
      if (Platform.isWindows) {
        return Uri.parse('ms-windows-store://review/?ProductId=$microsoftId');
      }
    } catch (_) {
      // Platform is unavailable under some test harnesses.
    }
    return null;
  }

  /// Where to go when the platform's own scheme did not open — Play not
  /// installed, the Store app missing from a stripped Windows image.
  static Uri? webFallback() {
    if (kIsWeb) return null;
    try {
      if (Platform.isAndroid) {
        return Uri.parse(
          'https://play.google.com/store/apps/details?id=$_androidPackage',
        );
      }
      if (Platform.isWindows) {
        return Uri.parse('https://apps.microsoft.com/detail/$microsoftId');
      }
    } catch (_) {
      // As above.
    }
    return null;
  }
}
