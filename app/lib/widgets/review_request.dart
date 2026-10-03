import 'package:flutter/material.dart';
import 'package:url_launcher/url_launcher.dart';

import '../l10n/app_localizations.dart';
import '../services/store_links.dart';
import '../state/app_state.dart';
import '../theme.dart';
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
/// The red of the heart on the card. Material's red 600.
const Color _heart = Color(0xFFE53935);

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
        // The theme's margin, which is what every server card above it uses.
        // Its own tighter one left the card wider than the list it had joined
        // and flush against the card above, so the two read as one thing with
        // a line through it.
        color: scheme.surfaceContainerHighest,
        child: Padding(
          padding: const EdgeInsets.fromLTRB(18, 16, 18, 14),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Row(
                children: [
                  // A full heart, and red.
                  //
                  // Not `StatusColors.failed`, which is the same family of red
                  // and means a connection has gone wrong: the six status
                  // colours are read at a glance through a visor and borrowing
                  // one for decoration is how that stops working. This is its
                  // own colour and says nothing about the state of anything.
                  const Icon(Icons.favorite, size: 18, color: _heart),
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
              const SizedBox(height: 10),
              Text(
                l.reviewBody,
                style: TextStyle(fontSize: 12, color: scheme.onSurfaceVariant),
              ),
              const SizedBox(height: 16),
              // Wrapped rather than in a row: the card sits in the server
              // list, which is a column of its own on a wide window and
              // narrower than the screen — and two buttons side by side
              // overflow it by ten pixels in English and more in Russian.
              //
              // Centred across, because the two are not the same height: this
              // theme gives a filled button 52 pixels and leaves a text button
              // at its own, so aligned to the top the words sit on different
              // lines.
              // **The `Wrap` has to be told to fill the width first.** Its
              // own alignment only centres the buttons inside whatever box it
              // is given, and a `Wrap` in a column that aligns to the start is
              // given exactly the width of its children — so the line was
              // centred in a box sitting against the left edge, which is
              // left-justified by another name.
              Center(
                child: Wrap(
                  alignment: WrapAlignment.center,
                  crossAxisAlignment: WrapCrossAlignment.center,
                  spacing: 8,
                  runSpacing: 8,
                  children: [
                    TextButton(
                      // Quieter than the other one on purpose: it is the way out,
                      // not the thing being asked for.
                      style: TextButton.styleFrom(
                        foregroundColor: scheme.onSurfaceVariant,
                        padding: const EdgeInsets.symmetric(horizontal: 12),
                        textStyle: const TextStyle(fontSize: 14),
                      ),
                      onPressed: state.dismissReviewRequest,
                      child: Text(l.reviewNotNow),
                    ),
                    FilledButton(
                      // The app's own blue rather than the scheme's tonal one,
                      // which on this card is a pale blue on a pale grey and
                      // asks to be read twice. This is the one thing on the card
                      // worth pressing.
                      //
                      // Smaller than the theme's button, which is sized for a
                      // gloved thumb on a talk control: at that size the pair
                      // does not fit the master column in either language and
                      // the `Wrap` puts them on two lines, one under the other.
                      // This is a card asking a favour in a car park, not a
                      // control used at speed.
                      style: FilledButton.styleFrom(
                        backgroundColor: StatusColors.talking,
                        foregroundColor: Colors.white,
                        elevation: 2,
                        minimumSize: const Size(0, 48),
                        padding: const EdgeInsets.symmetric(horizontal: 26),
                        textStyle: const TextStyle(
                          fontSize: 15,
                          fontWeight: FontWeight.w700,
                        ),
                      ),
                      onPressed: () => _open(context, state),
                      child: Text(l.reviewRate),
                    ),
                  ],
                ),
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
