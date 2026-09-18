import 'dart:async';

import 'package:flutter/material.dart';

import '../l10n/app_localizations.dart';
import '../services/remote_mute_notice.dart';
import '../state/app_state.dart';

/// Says who turned the rider's microphone off or on.
///
/// Around the whole app, like `RefusalListener` and for the same reason: the
/// request arrives whenever the other rider sends it, and the rider may be on
/// any screen at the time.
///
/// Not styled as an error. Nothing failed — somebody on the channel asked, and
/// the rider's own mute button is the answer either way.
class RemoteMuteListener extends StatefulWidget {
  const RemoteMuteListener({super.key, required this.child});

  final Widget child;

  @override
  State<RemoteMuteListener> createState() => _RemoteMuteListenerState();
}

class _RemoteMuteListenerState extends State<RemoteMuteListener> {
  StreamSubscription<RemoteMuteNotice>? _sub;

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    if (_sub != null) return;
    _sub = AppStateScope.of(context).remoteMuteNotices.listen(_show);
  }

  @override
  void dispose() {
    _sub?.cancel();
    super.dispose();
  }

  void _show(RemoteMuteNotice notice) {
    if (!mounted) return;
    final messenger = ScaffoldMessenger.maybeOf(context);
    if (messenger == null) return;
    // Six seconds and replace-rather-than-queue, as for errors: read by
    // somebody wearing gloves, and a mute then an unmute should leave the
    // one that is true now on screen, not the stale one first.
    messenger
      ..hideCurrentSnackBar()
      ..showSnackBar(
        SnackBar(
          content: Text(notice.describe(L.of(context))),
          duration: const Duration(seconds: 6),
        ),
      );
  }

  @override
  Widget build(BuildContext context) => widget.child;
}
