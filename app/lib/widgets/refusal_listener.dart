import 'dart:async';

import 'package:flutter/material.dart';

import '../l10n/app_localizations.dart';
import '../services/server_refusal.dart';
import '../state/app_state.dart';

/// Puts the server's refusals in front of the user.
///
/// Wrapped around the whole app rather than around the screen that asked, and
/// that placement is the point: a refusal arrives asynchronously, long enough
/// after the tap that the user may have moved on. Muting somebody from the
/// roster and then opening settings should still tell them it did not happen.
///
/// It used to go to the chat log as a line from "server", where it read as
/// somebody talking and then scrolled away.
class RefusalListener extends StatefulWidget {
  const RefusalListener({super.key, required this.child});

  final Widget child;

  @override
  State<RefusalListener> createState() => _RefusalListenerState();
}

class _RefusalListenerState extends State<RefusalListener> {
  StreamSubscription<ServerRefusal>? _sub;

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    if (_sub != null) return;
    _sub = AppStateScope.of(context).refusals.listen(_show);
  }

  @override
  void dispose() {
    _sub?.cancel();
    super.dispose();
  }

  void _show(ServerRefusal refusal) {
    if (!mounted) return;
    final l = L.of(context);
    final messenger = ScaffoldMessenger.maybeOf(context);
    if (messenger == null) return;
    // Replaces whatever is showing rather than queueing behind it. Refusals
    // arrive in bursts when a client retries, and a queue would make the user
    // sit through stale ones to reach the current one.
    messenger
      ..hideCurrentSnackBar()
      ..showSnackBar(
        SnackBar(
          // **Say what colour the text is.** The background was set and the
          // foreground was not, so the label kept whatever the surrounding
          // theme gave it -- which on this dark theme is the amber accent, and
          // amber on the error red is close to unreadable. It is also the one
          // message here that has to survive being read at a glance, through a
          // visor, by somebody who has just been refused and does not know why.
          content: Text(
            l.serverRefused(refusal.describe(l)),
            style: TextStyle(
              color: Theme.of(context).colorScheme.onErrorContainer,
            ),
          ),
          backgroundColor: Theme.of(context).colorScheme.errorContainer,
          // Longer than the default. This is the only account of why an action
          // did nothing, and it is being read by somebody who may be wearing
          // gloves and looking at the road.
          duration: const Duration(seconds: 6),
        ),
      );
  }

  @override
  Widget build(BuildContext context) => widget.child;
}
