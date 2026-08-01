import 'package:flutter/material.dart';

import '../src/rust/api/mumbleway.dart';
import '../state/app_state.dart';
import '../theme.dart';
import 'channel_panel.dart';
import 'status_badge.dart';

/// One server: status, live ping, connection controls, channels and who is here.
class ServerCard extends StatelessWidget {
  const ServerCard({super.key, required this.server});

  final SavedServer server;

  @override
  Widget build(BuildContext context) {
    final state = AppStateScope.of(context);
    final rt = state.runtimeFor(server.id);
    final visual = StatusVisual.of(rt.status);

    return Card(
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          // A colour strip along the top edge makes the state readable from a
          // glance at the mount, without focusing on text.
          Container(height: 5, color: visual.color),
          Padding(
            padding: const EdgeInsets.fromLTRB(18, 14, 18, 16),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Row(
                  children: [
                    Expanded(
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          Text(
                            server.name.isEmpty ? server.host : server.name,
                            style: const TextStyle(
                                fontSize: 19, fontWeight: FontWeight.w700),
                            overflow: TextOverflow.ellipsis,
                          ),
                          const SizedBox(height: 2),
                          Text(
                            '${server.host}:${server.port}  ·  ${server.username}',
                            style: TextStyle(
                              fontSize: 12,
                              color:
                                  Theme.of(context).colorScheme.onSurfaceVariant,
                            ),
                            overflow: TextOverflow.ellipsis,
                          ),
                        ],
                      ),
                    ),
                    const SizedBox(width: 10),
                    StatusBadge(status: rt.status, detail: rt.detail),
                  ],
                ),

                const SizedBox(height: 10),
                _ProbeLine(probe: rt.probe),

                if (rt.status == ConnStatus.reconnecting) ...[
                  const SizedBox(height: 12),
                  _ReconnectNotice(rt: rt),
                ],

                if (rt.detail.isNotEmpty && rt.status == ConnStatus.failed) ...[
                  const SizedBox(height: 12),
                  _Banner(
                    color: StatusColors.failed,
                    icon: Icons.error_outline,
                    text: rt.detail,
                  ),
                ],

                if (rt.certificateChanged) ...[
                  const SizedBox(height: 12),
                  _CertificateWarning(server: server, rt: rt),
                ],

                if (rt.isLive) ...[
                  const SizedBox(height: 12),
                  Row(
                    children: [
                      TransportChip(
                        transport: rt.transport,
                        pingMs:
                            rt.transport == 'udp' ? rt.udpPingMs : rt.tcpPingMs,
                      ),
                      const SizedBox(width: 16),
                      Icon(Icons.tag,
                          size: 14,
                          color:
                              Theme.of(context).colorScheme.onSurfaceVariant),
                      const SizedBox(width: 4),
                      Expanded(
                        child: Text(
                          rt.currentChannel?.name ?? 'joining…',
                          overflow: TextOverflow.ellipsis,
                          style: const TextStyle(fontSize: 12),
                        ),
                      ),
                    ],
                  ),
                  const SizedBox(height: 6),
                  _CollapsibleSection(
                    title: 'In this channel (${rt.channelPeers.length})',
                    initiallyExpanded: true,
                    child: ChannelUserList(
                      serverId: server.id,
                      users: rt.channelPeers,
                    ),
                  ),
                  _CollapsibleSection(
                    title: 'Channels (${rt.channels.length})',
                    child: ChannelTree(
                      serverId: server.id,
                      channels: rt.channels,
                      currentChannelId: rt.currentChannelId,
                      defaultChannelName: server.defaultChannel,
                    ),
                  ),
                ],

                const SizedBox(height: 14),
                Row(
                  children: [
                    Expanded(
                      child: rt.isLive || rt.isBusy
                          ? OutlinedButton.icon(
                              onPressed: () => state.disconnect(server.id),
                              icon: const Icon(Icons.stop_circle_outlined),
                              label: const Text('Disconnect'),
                            )
                          : FilledButton.icon(
                              onPressed: () => state.connect(server.id),
                              icon: const Icon(Icons.play_arrow_rounded),
                              label: const Text('Connect'),
                            ),
                    ),
                    const SizedBox(width: 10),
                    IconButton.outlined(
                      onPressed: () => _confirmForget(context, state),
                      icon: const Icon(Icons.delete_outline),
                      tooltip: 'Remove server',
                      style: IconButton.styleFrom(
                        minimumSize: const Size(52, 52),
                      ),
                    ),
                  ],
                ),
              ],
            ),
          ),
        ],
      ),
    );
  }

  Future<void> _confirmForget(BuildContext context, AppState state) async {
    final ok = await showDialog<bool>(
      context: context,
      builder: (c) => AlertDialog(
        title: const Text('Remove server?'),
        content: Text('${server.name} will be removed from your list.'),
        actions: [
          TextButton(
              onPressed: () => Navigator.pop(c, false),
              child: const Text('Cancel')),
          FilledButton(
              onPressed: () => Navigator.pop(c, true),
              child: const Text('Remove')),
        ],
      ),
    );
    if (ok == true) await state.forgetServer(server.id);
  }
}

/// Live reachability line from the unauthenticated status probe. This works
/// whether or not we are connected, so the user can see a server is up before
/// bothering to join it.
class _ProbeLine extends StatelessWidget {
  const _ProbeLine({required this.probe});

  final UiServerStatus? probe;

  @override
  Widget build(BuildContext context) {
    final muted = Theme.of(context).colorScheme.onSurfaceVariant;
    final p = probe;

    if (p == null) {
      return Row(
        children: [
          SizedBox(
            width: 12,
            height: 12,
            child: CircularProgressIndicator(strokeWidth: 2, color: muted),
          ),
          const SizedBox(width: 8),
          Text('Checking…', style: TextStyle(fontSize: 12, color: muted)),
        ],
      );
    }

    if (!p.reachable) {
      return Row(
        children: const [
          Icon(Icons.cloud_off, size: 14, color: StatusColors.idle),
          SizedBox(width: 6),
          Text('Not responding',
              style: TextStyle(fontSize: 12, color: StatusColors.idle)),
        ],
      );
    }

    final ping = p.pingMs.round();
    final quality = ping < 60
        ? StatusColors.connected
        : ping < 150
            ? StatusColors.connecting
            : StatusColors.reconnecting;

    return Row(
      children: [
        Icon(Icons.network_ping, size: 14, color: quality),
        const SizedBox(width: 6),
        Text('$ping ms',
            style: TextStyle(
                fontSize: 12, color: quality, fontWeight: FontWeight.w600)),
        const SizedBox(width: 12),
        Icon(Icons.people_outline, size: 14, color: muted),
        const SizedBox(width: 4),
        Text(
          p.maxUsers > 0 ? '${p.users}/${p.maxUsers}' : '${p.users}',
          style: TextStyle(fontSize: 12, color: muted),
        ),
        if (p.version.isNotEmpty) ...[
          const SizedBox(width: 12),
          Text('v${p.version}', style: TextStyle(fontSize: 11, color: muted)),
        ],
      ],
    );
  }
}

/// A lightweight disclosure that keeps the card compact without the heavy
/// chrome of an ExpansionTile.
class _CollapsibleSection extends StatefulWidget {
  const _CollapsibleSection({
    required this.title,
    required this.child,
    this.initiallyExpanded = false,
  });

  final String title;
  final Widget child;
  final bool initiallyExpanded;

  @override
  State<_CollapsibleSection> createState() => _CollapsibleSectionState();
}

class _CollapsibleSectionState extends State<_CollapsibleSection> {
  late bool _open = widget.initiallyExpanded;

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        InkWell(
          onTap: () => setState(() => _open = !_open),
          child: Padding(
            padding: const EdgeInsets.symmetric(vertical: 8),
            child: Row(
              children: [
                Icon(_open ? Icons.expand_less : Icons.expand_more, size: 18),
                const SizedBox(width: 6),
                Text(
                  widget.title,
                  style: const TextStyle(
                      fontSize: 12, fontWeight: FontWeight.w700),
                ),
              ],
            ),
          ),
        ),
        if (_open) widget.child,
      ],
    );
  }
}

class _ReconnectNotice extends StatelessWidget {
  const _ReconnectNotice({required this.rt});
  final ServerRuntime rt;

  @override
  Widget build(BuildContext context) {
    final secs = (rt.retryInMs / 1000).ceil();
    return _Banner(
      color: StatusColors.reconnecting,
      icon: Icons.sync_problem,
      text: rt.detail.isEmpty
          ? 'Connection lost. Retrying in ${secs}s (attempt ${rt.attempt}).'
          : '${rt.detail}. Retrying in ${secs}s (attempt ${rt.attempt}).',
    );
  }
}

class _CertificateWarning extends StatelessWidget {
  const _CertificateWarning({required this.server, required this.rt});
  final SavedServer server;
  final ServerRuntime rt;

  @override
  Widget build(BuildContext context) {
    final state = AppStateScope.of(context);
    final fp = rt.pendingFingerprint ?? '';
    final short = fp.length > 16 ? '${fp.substring(0, 16)}…' : fp;

    return Container(
      padding: const EdgeInsets.all(12),
      decoration: BoxDecoration(
        color: StatusColors.failed.withValues(alpha: 0.12),
        borderRadius: BorderRadius.circular(12),
        border: Border.all(color: StatusColors.failed.withValues(alpha: 0.5)),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          const Row(
            children: [
              Icon(Icons.gpp_maybe, color: StatusColors.failed, size: 18),
              SizedBox(width: 8),
              Expanded(
                child: Text(
                  'Server certificate changed',
                  style: TextStyle(fontWeight: FontWeight.w700),
                ),
              ),
            ],
          ),
          const SizedBox(height: 6),
          Text(
            'This can mean the server was reinstalled — or that someone is '
            'impersonating it. Only continue if you expected this.\n\nNow: $short',
            style: const TextStyle(fontSize: 12),
          ),
          const SizedBox(height: 10),
          FilledButton.tonal(
            onPressed: () => state.trustChangedCertificate(server.id),
            child: const Text('Trust the new certificate'),
          ),
        ],
      ),
    );
  }
}

class _Banner extends StatelessWidget {
  const _Banner({required this.color, required this.icon, required this.text});
  final Color color;
  final IconData icon;
  final String text;

  @override
  Widget build(BuildContext context) {
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 10),
      decoration: BoxDecoration(
        color: color.withValues(alpha: 0.12),
        borderRadius: BorderRadius.circular(12),
      ),
      child: Row(
        children: [
          Icon(icon, size: 18, color: color),
          const SizedBox(width: 10),
          Expanded(child: Text(text, style: const TextStyle(fontSize: 13))),
        ],
      ),
    );
  }
}
