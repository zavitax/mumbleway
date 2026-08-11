import 'dart:async';

import 'package:flutter/material.dart';

import '../l10n/app_localizations.dart';
import '../services/server_refusal.dart';
import '../state/app_state.dart';
import 'error_snack.dart';

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
    // Through the shared helper, which is where the colours, the six seconds
    // and the replace-rather-than-queue now live. This used to state all three
    // here, which is how every other failure in the app came to be shown in
    // Material's default white.
    showError(messenger, l.serverRefused(refusal.describe(l)));
  }

  @override
  Widget build(BuildContext context) => widget.child;
}
