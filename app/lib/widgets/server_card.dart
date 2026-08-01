import 'package:flutter/material.dart';

import '../src/rust/api/mumbleway.dart';
import '../state/app_state.dart';
import '../theme.dart';
import 'status_badge.dart';

/// One server: status, connection controls, and who is in the channel.
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
                              color: Theme.of(context)
                                  .colorScheme
                                  .onSurfaceVariant,
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

                if (rt.status == ConnStatus.reconnecting) ...[
                  const SizedBox(height: 12),
                  _ReconnectNotice(rt: rt),
                ],

                if (rt.detail.isNotEmpty &&
                    rt.status == ConnStatus.failed) ...[
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
                        pingMs: rt.transport == 'udp'
                            ? rt.udpPingMs
                            : rt.tcpPingMs,
                      ),
                      const SizedBox(width: 16),
                      Icon(Icons.people_outline,
                          size: 14,
                          color: Theme.of(context).colorScheme.onSurfaceVariant),
                      const SizedBox(width: 4),
                      Text('${rt.users.length}',
                          style: const TextStyle(fontSize: 12)),
                    ],
                  ),
                  if (rt.users.isNotEmpty) ...[
                    const SizedBox(height: 10),
                    _UserStrip(users: rt.users),
                  ],
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

/// Compact list of who is present, highlighting whoever is speaking.
class _UserStrip extends StatelessWidget {
  const _UserStrip({required this.users});
  final List<UiUser> users;

  @override
  Widget build(BuildContext context) {
    return Wrap(
      spacing: 8,
      runSpacing: 8,
      children: users.map((u) {
        final color = u.talking
            ? StatusColors.talking
            : Theme.of(context).colorScheme.surfaceContainerHighest;
        return Container(
          padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 6),
          decoration: BoxDecoration(
            color: color.withValues(alpha: u.talking ? 0.25 : 1.0),
            borderRadius: BorderRadius.circular(999),
            border: u.talking
                ? Border.all(color: StatusColors.talking, width: 1.5)
                : null,
          ),
          child: Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              Icon(
                u.deafened
                    ? Icons.hearing_disabled
                    : u.muted
                        ? Icons.mic_off
                        : u.talking
                            ? Icons.volume_up
                            : Icons.person,
                size: 14,
                color: u.talking ? StatusColors.talking : null,
              ),
              const SizedBox(width: 6),
              Text(u.name, style: const TextStyle(fontSize: 12)),
            ],
          ),
        );
      }).toList(),
    );
  }
}
