import 'package:flutter/material.dart';

import '../l10n/app_localizations.dart';
import '../state/app_state.dart';
import 'channel_panel.dart';
import 'status_badge.dart';

/// Right-hand pane on wide layouts.
///
/// On a phone the channel tree and roster are collapsed inside the server card,
/// because there is no room for them. On a tablet or a wide window there is,
/// and cramming them into a narrow column would waste the space rather than use
/// it — so they get a pane of their own, expanded by default.
class ServerDetailPane extends StatelessWidget {
  const ServerDetailPane({super.key, required this.server});

  final SavedServer? server;

  @override
  Widget build(BuildContext context) {
    final s = server;
    if (s == null) {
      return _Placeholder(
        icon: Icons.dns_outlined,
        title: L.of(context).noServerSelected,
        body: L.of(context).noServerSelectedBody,
      );
    }

    final state = AppStateScope.of(context);
    final rt = state.runtimeFor(s.id);

    if (!rt.isLive) {
      return _Placeholder(
        icon: Icons.link_off,
        title: s.name.isEmpty ? s.host : s.name,
        body: rt.isBusy
            ? L.of(context).statusConnecting
            : L.of(context).connectToSeeChannels,
        trailing: StatusBadge(status: rt.status, detail: rt.detail),
      );
    }

    return ListView(
      padding: const EdgeInsets.fromLTRB(20, 16, 20, 24),
      children: [
        Row(
          children: [
            Expanded(
              child: Text(
                s.name.isEmpty ? s.host : s.name,
                style: const TextStyle(
                  fontSize: 20,
                  fontWeight: FontWeight.w700,
                ),
                overflow: TextOverflow.ellipsis,
              ),
            ),
            StatusBadge(status: rt.status, detail: rt.detail, compact: true),
          ],
        ),
        const SizedBox(height: 6),
        Row(
          children: [
            TransportChip(
              transport: rt.transport,
              pingMs: rt.transport == 'udp' ? rt.udpPingMs : rt.tcpPingMs,
            ),
            const SizedBox(width: 14),
            Icon(
              Icons.tag,
              size: 14,
              color: Theme.of(context).colorScheme.onSurfaceVariant,
            ),
            const SizedBox(width: 4),
            Expanded(
              child: Text(
                rt.currentChannel?.name ?? 'joining…',
                overflow: TextOverflow.ellipsis,
                style: const TextStyle(fontSize: 12),
              ),
            ),
            if (rt.selfName ?? s.username case final me when me.isNotEmpty) ...[
              const SizedBox(width: 6),
              Text(
                '@$me',
                style: TextStyle(
                  fontSize: 12,
                  color: Theme.of(context).colorScheme.onSurfaceVariant,
                ),
              ),
            ],
          ],
        ),

        const SizedBox(height: 18),
        _Heading('In this channel (${rt.channelPeers.length})'),
        ChannelUserList(serverId: s.id, users: rt.channelPeers),

        const SizedBox(height: 22),
        _Heading('Channels'),
        ChannelTree(
          serverId: s.id,
          channels: rt.channels,
          currentChannelId: rt.currentChannelId,
          defaultChannelName: s.defaultChannel,
        ),

        if (rt.welcome.isNotEmpty) ...[
          const SizedBox(height: 22),
          _Heading(L.of(context).welcomeMessage),
          Text(
            // Servers routinely put HTML in this; strip the tags rather than
            // rendering them as literal text.
            rt.welcome.replaceAll(RegExp(r'<[^>]*>'), ' ').trim(),
            style: const TextStyle(fontSize: 12),
          ),
        ],

        if (rt.messages.isNotEmpty) ...[
          const SizedBox(height: 22),
          _Heading(L.of(context).messages),
          for (final m in rt.messages.reversed.take(30))
            Padding(
              padding: const EdgeInsets.symmetric(vertical: 3),
              child: Text(m, style: const TextStyle(fontSize: 12)),
            ),
        ],
      ],
    );
  }
}

class _Heading extends StatelessWidget {
  const _Heading(this.text);
  final String text;

  @override
  Widget build(BuildContext context) => Padding(
    padding: const EdgeInsets.only(bottom: 6),
    child: Text(
      text.toUpperCase(),
      style: TextStyle(
        fontSize: 11,
        fontWeight: FontWeight.w800,
        letterSpacing: 1.0,
        color: Theme.of(context).colorScheme.primary,
      ),
    ),
  );
}

class _Placeholder extends StatelessWidget {
  const _Placeholder({
    required this.icon,
    required this.title,
    required this.body,
    this.trailing,
  });

  final IconData icon;
  final String title;
  final String body;
  final Widget? trailing;

  @override
  Widget build(BuildContext context) {
    return Center(
      child: Padding(
        padding: const EdgeInsets.all(32),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Icon(
              icon,
              size: 56,
              color: Theme.of(context).colorScheme.onSurfaceVariant,
            ),
            const SizedBox(height: 16),
            Text(
              title,
              textAlign: TextAlign.center,
              style: const TextStyle(fontSize: 18, fontWeight: FontWeight.w700),
            ),
            const SizedBox(height: 8),
            Text(
              body,
              textAlign: TextAlign.center,
              style: TextStyle(
                color: Theme.of(context).colorScheme.onSurfaceVariant,
              ),
            ),
            if (trailing != null) ...[const SizedBox(height: 16), trailing!],
          ],
        ),
      ),
    );
  }
}

/// Width at which the two-pane layout takes over.
///
/// 900 logical pixels is about where a phone in landscape and a small tablet
/// diverge: below it a second pane would be too cramped to be worth the
/// horizontal space it steals from the server cards.
const double kWideLayoutBreakpoint = 900;

/// Whether the current context is wide enough for the two-pane layout.
bool isWideLayout(BuildContext context) =>
    MediaQuery.sizeOf(context).width >= kWideLayoutBreakpoint;
