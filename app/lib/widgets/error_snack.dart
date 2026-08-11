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
