import 'dart:async';

import 'package:flutter/material.dart';

import '../l10n/app_localizations.dart';
import '../src/rust/api/mumbleway.dart';
import '../screens/add_server_screen.dart';
import '../state/app_state.dart';
import '../theme.dart';
import 'channel_panel.dart';
import 'status_badge.dart';

/// One server: status, live ping, connection controls, channels and who is here.
class ServerCard extends StatelessWidget {
  const ServerCard({
    super.key,
    required this.server,
    this.showDetails = true,
    this.selected = false,
    this.onTap,
  });

  final SavedServer server;

  /// Whether to expand channels and the roster inline. Wide layouts turn this
  /// off because a detail pane shows them with far more room.
  final bool showDetails;

  /// Highlighted as the server the detail pane is showing.
  final bool selected;
  final VoidCallback? onTap;

  @override
  Widget build(BuildContext context) {
    final state = AppStateScope.of(context);
    final rt = state.runtimeFor(server.id);
    final visual = StatusVisual.of(rt.status);
    final l = L.of(context);

    return Card(
      shape: RoundedRectangleBorder(
        borderRadius: BorderRadius.circular(18),
        side: selected
            ? BorderSide(color: Theme.of(context).colorScheme.primary, width: 2)
            : BorderSide.none,
      ),
      child: InkWell(
        onTap: onTap,
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
                                fontSize: 19,
                                fontWeight: FontWeight.w700,
                              ),
                              overflow: TextOverflow.ellipsis,
                            ),
                            const SizedBox(height: 2),
                            Text(
                              '${server.host}:${server.port}  ·  ${server.username}',
                              style: TextStyle(
                                fontSize: 12,
                                color: Theme.of(
                                  context,
                                ).colorScheme.onSurfaceVariant,
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
                        Icon(
                          Icons.tag,
                          size: 14,
                          color: Theme.of(context).colorScheme.onSurfaceVariant,
                        ),
                        const SizedBox(width: 4),
                        Expanded(
                          child: Text(
                            rt.currentChannel?.name ?? l.joining,
                            overflow: TextOverflow.ellipsis,
                            style: const TextStyle(fontSize: 12),
                          ),
                        ),
                      ],
                    ),
                    if (showDetails) ...[
                      const SizedBox(height: 6),
                      _CollapsibleSection(
                        title: l.inThisChannel(rt.channelPeers.length),
                        initiallyExpanded: true,
                        child: ChannelUserList(
                          serverId: server.id,
                          users: rt.channelPeers,
                        ),
                      ),
                      _CollapsibleSection(
                        title: l.channelsHeading(rt.channels.length),
                        child: ChannelTree(
                          serverId: server.id,
                          channels: rt.channels,
                          currentChannelId: rt.currentChannelId,
                          defaultChannelName: server.defaultChannel,
                        ),
                      ),
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
                                label: Text(l.disconnect),
                              )
                            : FilledButton.icon(
                                onPressed: () => state.connect(server.id),
                                icon: const Icon(Icons.play_arrow_rounded),
                                label: Text(l.connect),
                              ),
                      ),
                      const SizedBox(width: 10),
                      SizedBox(
                        width: 52,
                        height: 52,
                        child: PopupMenuButton<String>(
                          tooltip: l.more,
                          icon: const Icon(Icons.more_horiz),
                          onSelected: (v) {
                            switch (v) {
                              case 'edit':
                                Navigator.push(
                                  context,
                                  MaterialPageRoute(
                                    builder: (_) =>
                                        AddServerScreen(existing: server),
                                  ),
                                );
                              case 'link':
                                _share(context, state, rt, asFile: false);
                              case 'file':
                                _share(context, state, rt, asFile: true);
                              case 'duplicate':
                                state.duplicateServer(server);
                              case 'remove':
                                _confirmForget(context, state);
                            }
                          },
                          itemBuilder: (_) => [
                            PopupMenuItem(
                              value: 'edit',
                              child: ListTile(
                                dense: true,
                                contentPadding: EdgeInsets.zero,
                                leading: const Icon(Icons.edit_outlined),
                                title: Text(l.edit),
                              ),
                            ),
                            PopupMenuItem(
                              value: 'link',
                              child: ListTile(
                                dense: true,
                                contentPadding: EdgeInsets.zero,
                                leading: const Icon(Icons.link),
                                title: Text(l.shareInviteLink),
                              ),
                            ),
                            PopupMenuItem(
                              value: 'file',
                              child: ListTile(
                                dense: true,
                                contentPadding: EdgeInsets.zero,
                                leading: const Icon(Icons.description_outlined),
                                title: Text(l.shareProfileFile),
                              ),
                            ),
                            PopupMenuItem(
                              value: 'duplicate',
                              child: ListTile(
                                dense: true,
                                contentPadding: EdgeInsets.zero,
                                leading: const Icon(Icons.copy_all_outlined),
                                title: Text(l.duplicate),
                              ),
                            ),
                            const PopupMenuDivider(),
                            PopupMenuItem(
                              value: 'remove',
                              child: ListTile(
                                dense: true,
                                contentPadding: EdgeInsets.zero,
                                leading: const Icon(
                                  Icons.delete_outline,
                                  color: StatusColors.failed,
                                ),
                                title: Text(
                                  l.remove,
                                  style: const TextStyle(
                                    color: StatusColors.failed,
                                  ),
                                ),
                              ),
                            ),
                          ],
                        ),
                      ),
                    ],
                  ),
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }

  /// Shares an invite, asking first whether to include the password.
  ///
  /// That question is deliberate rather than a checkbox buried in settings: a
  /// link carrying a password grants access to anyone who ever sees it,
  /// including whatever chat app it travels through.
  Future<void> _share(
    BuildContext context,
    AppState state,
    ServerRuntime rt, {
    required bool asFile,
  }) async {
    final l = L.of(context);
    final channel = rt.currentChannel?.name ?? server.defaultChannel;
    final hasPassword = (server.password ?? '').isNotEmpty;
    final messenger = ScaffoldMessenger.of(context);

    var includePassword = false;
    if (hasPassword) {
      final choice = await showDialog<bool>(
        context: context,
        builder: (c) => AlertDialog(
          title: Text(l.includePasswordTitle),
          content: Text(
            'Anyone who receives this can join ${server.name}'
            '${channel == null ? '' : ' and land in $channel'} without being '
            'asked for a password. It stays valid for as long as the password '
            'does, wherever the message ends up.',
            style: const TextStyle(fontSize: 13),
          ),
          actions: [
            TextButton(
              onPressed: () => Navigator.pop(c, false),
              child: Text(l.withoutPassword),
            ),
            FilledButton(
              onPressed: () => Navigator.pop(c, true),
              child: Text(l.includeIt),
            ),
          ],
        ),
      );
      if (choice == null) return; // dismissed
      includePassword = choice;
    }

    final error = asFile
        ? await state.shareInviteFile(
            server,
            channel: channel,
            includePassword: includePassword,
          )
        : await state.shareInviteLink(
            server,
            channel: channel,
            includePassword: includePassword,
          );

    if (error != null) {
      messenger.showSnackBar(SnackBar(content: Text(error)));
    }
  }

  Future<void> _confirmForget(BuildContext context, AppState state) async {
    final l = L.of(context);
    final ok = await showDialog<bool>(
      context: context,
      builder: (c) => AlertDialog(
        title: Text(l.removeServerTitle),
        content: Text(l.removeServerBody(server.name)),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(c, false),
            child: Text(l.cancel),
          ),
          FilledButton(
            onPressed: () => Navigator.pop(c, true),
            child: Text(l.remove),
          ),
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
        children: [
          const Icon(Icons.cloud_off, size: 14, color: StatusColors.idle),
          const SizedBox(width: 6),
          Text(
            L.of(context).probeNotResponding,
            style: const TextStyle(fontSize: 12, color: StatusColors.idle),
          ),
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
        Text(
          '$ping ms',
          style: TextStyle(
            fontSize: 12,
            color: quality,
            fontWeight: FontWeight.w600,
          ),
        ),
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
                    fontSize: 12,
                    fontWeight: FontWeight.w700,
                  ),
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

/// "Retrying in Ns", counted down rather than stated once.
///
/// The core reports the wait when it schedules the attempt and says nothing
/// further until it fires, so a static number sits unchanged for the whole
/// interval and reads as a frozen app at exactly the moment the user is
/// wondering whether it has hung.
class _ReconnectNotice extends StatefulWidget {
  const _ReconnectNotice({required this.rt});
  final ServerRuntime rt;

  @override
  State<_ReconnectNotice> createState() => _ReconnectNoticeState();
}

class _ReconnectNoticeState extends State<_ReconnectNotice> {
  Timer? _tick;

  @override
  void initState() {
    super.initState();
    // Half-second so the displayed second changes promptly after the deadline
    // moves, rather than lagging by up to a full second.
    _tick = Timer.periodic(const Duration(milliseconds: 500), (_) {
      if (mounted) setState(() {});
    });
  }

  @override
  void dispose() {
    _tick?.cancel();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final l = L.of(context);
    final rt = widget.rt;
    final seconds = rt.retrySecondsLeft;
    final detail = rt.detail.isEmpty ? l.connectionLost : rt.detail;
    return _Banner(
      color: StatusColors.reconnecting,
      icon: Icons.sync_problem,
      text: seconds > 0
          ? '$detail ${l.retryingInSeconds(seconds, rt.attempt)}'
          : '$detail ${l.retryingNow(rt.attempt)}',
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
    final l = L.of(context);
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
          Row(
            children: [
              const Icon(Icons.gpp_maybe, color: StatusColors.failed, size: 18),
              const SizedBox(width: 8),
              Expanded(
                child: Text(
                  l.certChangedTitle,
                  style: const TextStyle(fontWeight: FontWeight.w700),
                ),
              ),
            ],
          ),
          const SizedBox(height: 6),
          Text(
            '${l.certChangedBody}\n\n${l.certificateFingerprint}: $short',
            style: const TextStyle(fontSize: 12),
          ),
          const SizedBox(height: 10),
          FilledButton.tonal(
            onPressed: () => state.trustChangedCertificate(server.id),
            child: Text(l.trustNewCertificate),
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
