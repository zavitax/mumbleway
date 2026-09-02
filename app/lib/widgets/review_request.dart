import 'package:flutter/material.dart';
import 'package:url_launcher/url_launcher.dart';

import '../l10n/app_localizations.dart';
import '../services/store_links.dart';
import '../state/app_state.dart';
import 'error_snack.dart';
import 'watch.dart';

/// Asks, once in a while, whether the rider would leave a review.
///
/// **A card and not a dialog, and never while anything is connected.** This
/// app is used at speed with the phone in a cradle and the rider's hands on
/// the bars. A modal that has to be dismissed before the interface works again
/// is a bad thing to put in front of somebody in that position, and a bad
/// thing to have appear the moment a call ends on the road. `shouldAskForReview`
/// refuses while a call is up, while one is being chased, and while the audio
/// devices are open; this is a row on the home screen that can be ignored.
///
/// **"Not now" is the whole reason this exists rather than a call to the
/// platform.** `SKStoreReviewController` and Play's In-App Review report
/// nothing — not what the user did, not whether they were shown anything — so
/// a rule like "ask again after seven more calls" cannot be built on them. Our
/// own question has an answer we can hear. See [StoreLinks].
class ReviewRequest extends StatelessWidget {
  const ReviewRequest({super.key});

  @override
  Widget build(BuildContext context) {
    final l = L.of(context);
    final scheme = Theme.of(context).colorScheme;

    // Selects on the one flag, so the row costs a comparison per notification
    // rather than a rebuild — the home screen is notified twice a second for
    // the whole of a ride.
    return Watch<bool>((state) => state.shouldAskForReview, (context, state) {
      if (!state.shouldAskForReview) return const SizedBox.shrink();
      return Card(
        margin: const EdgeInsets.fromLTRB(8, 4, 8, 8),
        color: scheme.surfaceContainerHighest,
        child: Padding(
          padding: const EdgeInsets.fromLTRB(14, 12, 8, 8),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Row(
                children: [
                  Icon(Icons.favorite_outline, size: 18, color: scheme.primary),
                  const SizedBox(width: 10),
                  Expanded(
                    child: Text(
                      l.reviewTitle,
                      style: const TextStyle(
                        fontWeight: FontWeight.w700,
                        fontSize: 15,
                      ),
                    ),
                  ),
                ],
              ),
              const SizedBox(height: 6),
              Text(
                l.reviewBody,
                style: TextStyle(fontSize: 12, color: scheme.onSurfaceVariant),
              ),
              const SizedBox(height: 4),
              Row(
                mainAxisAlignment: MainAxisAlignment.end,
                children: [
                  TextButton(
                    onPressed: state.dismissReviewRequest,
                    child: Text(l.reviewNotNow),
                  ),
                  const SizedBox(width: 4),
                  FilledButton(
                    onPressed: () => _open(context, state),
                    child: Text(l.reviewRate),
                  ),
                ],
              ),
            ],
          ),
        ),
      );
    });
  }

  /// Opens the store, falling back to the web listing where the platform's own
  /// scheme is not handled — Play missing from a device, the Store app absent
  /// from a stripped Windows image. A rider who agreed to leave a review and
  /// was shown nothing would reasonably conclude the app is broken.
  Future<void> _open(BuildContext context, AppState state) async {
    final messenger = ScaffoldMessenger.of(context);
    final l = L.of(context);
    final url = await state.openStoreForReview();
    if (url == null) {
      showError(messenger, l.couldNotOpenLink);
      return;
    }
    var opened = false;
    try {
      opened = await launchUrl(url, mode: LaunchMode.externalApplication);
    } catch (_) {
      opened = false;
    }
    if (!opened) {
      final web = StoreLinks.webFallback();
      if (web != null) {
        try {
          opened = await launchUrl(web, mode: LaunchMode.externalApplication);
        } catch (_) {
          opened = false;
        }
      }
    }
    if (!opened) showError(messenger, l.couldNotOpenLink);
  }
}
