import 'package:flutter_test/flutter_test.dart';
import 'package:mumbleway/services/store_links.dart';

/// Where "leave a review" actually sends somebody.
///
/// **This is the half of the review prompt that fails silently.** The counting
/// is covered by `review_request_test.dart` and would be obvious if it were
/// wrong — the card appears at the wrong moment, or never. A wrong URL is
/// invisible until the one moment it matters: somebody has agreed to leave a
/// review, taps the button, and lands on nothing. They do not come back.
///
/// Every branch is exercised, not just the one this machine happens to run on.
/// `Platform` cannot be faked, so [StoreLinks.reviewFor] takes the operating
/// system as a string and the real getter is a one-line call to it.
void main() {
  group('review link', () {
    test('Apple opens the page with the review sheet already up', () {
      for (final os in ['ios', 'macos']) {
        final url = StoreLinks.reviewFor(os);
        expect(url, isNotNull, reason: '$os has a store');
        expect(url!.host, 'apps.apple.com');
        expect(url.path, '/app/id${StoreLinks.appleId}');
        // Without this the page opens and the user has to find the control
        // themselves, which is the whole thing this was meant to save them.
        expect(url.queryParameters['action'], 'write-review');
      }
    });

    test('iPhone and Mac share one address', () {
      // One app record, one product page. The listings behind it differ, which
      // does not change the link — see the note on `appleId`.
      expect(StoreLinks.reviewFor('ios'), StoreLinks.reviewFor('macos'));
    });

    test('Android uses the market scheme, so Play opens rather than a browser', () {
      final url = StoreLinks.reviewFor('android');
      expect(url, isNotNull);
      expect(url!.scheme, 'market');
      expect(url.queryParameters['id'], 'com.mumbleway.mumbleway');
    });

    test('Windows asks the Store app for the review page', () {
      final url = StoreLinks.reviewFor('windows');
      expect(url, isNotNull);
      expect(url!.scheme, 'ms-windows-store');
      expect(url.queryParameters['ProductId'], StoreLinks.microsoftId);
    });

    test('somewhere with no store returns null rather than a broken link', () {
      // Linux, and the web, and anything Platform could not name. The card is
      // never shown in these cases; returning null is what makes that true.
      for (final os in ['linux', 'fuchsia', 'web', 'unknown']) {
        expect(StoreLinks.reviewFor(os), isNull, reason: os);
      }
    });
  });

  group('web fallback', () {
    test('Android falls back to the Play page', () {
      final url = StoreLinks.webFallbackFor('android');
      expect(url, isNotNull);
      expect(url!.host, 'play.google.com');
      expect(url.queryParameters['id'], 'com.mumbleway.mumbleway');
    });

    test('Windows falls back to the Store on the web', () {
      final url = StoreLinks.webFallbackFor('windows');
      expect(url, isNotNull);
      expect(url!.host, 'apps.microsoft.com');
      expect(url.path, endsWith(StoreLinks.microsoftId));
    });

    test('Apple has no fallback, because the first address was already the web', () {
      // Not an omission. `apps.apple.com` is the web page; if that will not
      // open, a second URL to the same place fixes nothing.
      expect(StoreLinks.webFallbackFor('ios'), isNull);
      expect(StoreLinks.webFallbackFor('macos'), isNull);
    });
  });

  group('the identifiers', () {
    // These three are the only facts here that live outside the repository,
    // and a typo in one produces a link that opens somebody else's app.
    test('are the ids the stores actually know this app by', () {
      expect(StoreLinks.appleId, '6797305046');
      expect(StoreLinks.microsoftId, '9PNZ7PWDVLTB');
      expect(
        StoreLinks.reviewFor('android')!.queryParameters['id'],
        'com.mumbleway.mumbleway',
        reason: 'must match applicationId in app/android/app/build.gradle.kts',
      );
    });
  });
}
