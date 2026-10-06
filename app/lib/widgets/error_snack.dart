import 'package:flutter/material.dart';

import '../theme.dart';

/// Shows a failure, in the one colour pair the app uses for failures.
///
/// A function rather than a convention, because a convention had already been
/// written down twice and applied once: the refusal snackbar was given a
/// readable foreground and every other error in the app carried on arriving as
/// Material's default, which on this dark theme is **a white card**. A rider who
/// has just been refused sees the loudest thing on the screen in the one colour
/// that says nothing about what happened.
///
/// Everything about it is deliberately not the default:
///
/// * **[StatusColors.errorForeground] on [StatusColors.errorBackground]** —
///   yellow on a deep red, stated together in one place so the contrast between
///   them stays a decision rather than an accident of two themes meeting.
/// * **Six seconds, not four.** This is often the only account of why an action
///   did nothing, and it is read by somebody wearing gloves and looking at a
///   road.
/// * **Replaces rather than queues.** Failures arrive in bursts when something
///   retries, and a queue makes the rider sit through stale ones to reach the
///   one that is true now.
/// Something a rider should know, that is nobody's fault.
///
/// **Amber, not the error red.** The deep red and its yellow are for a failure
/// — a refusal, a connection lost, a thing that went wrong. A channel that will
/// not carry a voice is none of those: it is a rule, working as its operator
/// intended, and dressed as a fatal error it reads as the app having broken.
/// Amber is already this app's word for "somebody else decided this", on the
/// microphone button and in the roster, so the notice matches the glyph it is
/// explaining.
void showNotice(ScaffoldMessengerState messenger, String message) {
  messenger
    ..hideCurrentSnackBar()
    ..showSnackBar(
      SnackBar(
        content: Text(
          message,
          // **Set explicitly.** Left to the theme it came out as the default
          // dark text on a saturated orange, which is unreadable.
          style: const TextStyle(
            color: StatusColors.noticeForeground,
            fontWeight: FontWeight.w600,
          ),
        ),
        backgroundColor: StatusColors.noticeBackground,
        // The panel on the card has one; without it the toast is a dark slab
        // and the two stop looking like the same message.
        shape: RoundedRectangleBorder(
          borderRadius: BorderRadius.circular(6),
          side: const BorderSide(color: StatusColors.noticeBorder),
        ),
        duration: const Duration(seconds: 6),
      ),
    );
}

void showError(ScaffoldMessengerState messenger, String message) {
  messenger
    ..hideCurrentSnackBar()
    ..showSnackBar(
      SnackBar(
        content: Text(
          message,
          style: const TextStyle(
            color: StatusColors.errorForeground,
            fontWeight: FontWeight.w600,
          ),
        ),
        backgroundColor: StatusColors.errorBackground,
        duration: const Duration(seconds: 6),
      ),
    );
}
