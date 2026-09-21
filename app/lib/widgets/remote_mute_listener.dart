import 'dart:async';

import 'package:flutter/material.dart';

import '../l10n/app_localizations.dart';
import '../services/remote_mute_notice.dart';
import '../src/rust/api/mumbleway.dart' show MicMode;
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
  StreamSubscription<String>? _suggested;

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    if (_sub != null) return;
    final state = AppStateScope.of(context);
    _sub = state.remoteMuteNotices.listen(_show);
    // The other thing a rider is told rather than asked, and it shares this
    // widget because it shares everything about how it is shown.
    _suggested = state.serverSuggestions.listen(_showSuggestion);
  }

  @override
  void dispose() {
    _sub?.cancel();
    _suggested?.cancel();
    super.dispose();
  }

  /// What the server's administrator asks of riders here.
  ///
  /// Push-to-talk comes with the button that does it, because a suggestion a
  /// rider has to go and find in Settings is one they will not follow. Nothing
  /// changes on its own: the server is asking, not deciding.
  void _showSuggestion(String ask) {
    if (!mounted) return;
    final l = L.of(context);
    final messenger = ScaffoldMessenger.maybeOf(context);
    if (messenger == null) return;
    final state = AppStateScope.of(context);
    // The bandwidth ones carry a number, read from the state as it is now
    // rather than captured when the notice was queued.
    final kbps = (state.audioBitrateBps / 1000).round();
    final message = switch (ask) {
      'ptt' => l.serverSuggestsPushToTalk,
      'positional' => l.serverSuggestsPositional,
      'bitrate' => l.serverCapsBitrate(kbps),
      _ => l.serverCapTooLow,
    };
    messenger
      ..hideCurrentSnackBar()
      ..showSnackBar(
        SnackBar(
          content: Text(message),
          duration: const Duration(seconds: 8),
          action: ask == 'ptt'
              ? SnackBarAction(
                  label: l.serverSuggestsSwitch,
                  onPressed: () => state.updateMicMode(MicMode.pushToTalk),
                )
              : null,
        ),
      );
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
