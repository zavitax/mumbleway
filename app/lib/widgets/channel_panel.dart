import 'package:flutter/material.dart';

import '../src/rust/api/mumbleway.dart';
import '../l10n/app_localizations.dart';
import '../state/app_state.dart';
import '../theme.dart';

/// Channel tree for one connected server.
///
/// Tapping a channel joins it now; the star marks the channel joined
/// automatically on every future connect. Those are separate ideas on purpose —
/// a rider often drops into another channel briefly without wanting it to
/// become the default.
class ChannelTree extends StatelessWidget {
  const ChannelTree({
    super.key,
    required this.serverId,
    required this.channels,
    required this.currentChannelId,
    required this.defaultChannelName,
  });

  final String serverId;
  final List<UiChannel> channels;
  final int? currentChannelId;
  final String? defaultChannelName;

  @override
  Widget build(BuildContext context) {
    if (channels.isEmpty) {
      return const Padding(
        padding: EdgeInsets.symmetric(vertical: 8),
        child: Text('No channels yet.', style: TextStyle(fontSize: 12)),
      );
    }

    // Group by parent so the tree can be walked without repeated scans.
    final byParent = <int?, List<UiChannel>>{};
    for (final c in channels) {
      byParent.putIfAbsent(c.parent, () => []).add(c);
    }

    // The root is whichever channel has no parent. Servers normally have
    // exactly one, but guard against a partial tree arriving mid-sync.
    final roots = byParent[null] ?? const <UiChannel>[];
    final orphans = channels
        .where((c) => c.parent != null && !channels.any((p) => p.id == c.parent))
        .toList();

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        for (final r in roots) ..._buildNode(context, r, byParent, 0),
        for (final o in orphans) ..._buildNode(context, o, byParent, 0),
      ],
    );
  }

  List<Widget> _buildNode(
    BuildContext context,
    UiChannel channel,
    Map<int?, List<UiChannel>> byParent,
    int depth,
  ) {
    final state = AppStateScope.of(context);
    final isCurrent = channel.id == currentChannelId;
    final isDefault = defaultChannelName != null &&
        defaultChannelName!.toLowerCase() == channel.name.toLowerCase();
    final full = channel.maxUsers > 0 && channel.userCount >= channel.maxUsers;

    final rows = <Widget>[
      InkWell(
        onTap: isCurrent ? null : () => state.joinChannelOn(serverId, channel.id),
        borderRadius: BorderRadius.circular(10),
        child: Padding(
          padding: EdgeInsets.fromLTRB(8.0 + depth * 16, 8, 4, 8),
          child: Row(
            children: [
              Icon(
                isCurrent ? Icons.radio_button_checked : Icons.tag,
                size: 16,
                color: isCurrent ? StatusColors.connected : null,
              ),
              const SizedBox(width: 8),
              Expanded(
                child: Text(
                  channel.name.isEmpty ? '(root)' : channel.name,
                  overflow: TextOverflow.ellipsis,
                  style: TextStyle(
                    fontSize: 13,
                    fontWeight: isCurrent ? FontWeight.w700 : FontWeight.w400,
                    color: full && !isCurrent
                        ? Theme.of(context).colorScheme.onSurfaceVariant
                        : null,
                  ),
                ),
              ),
              if (channel.userCount > 0) ...[
                Icon(Icons.person,
                    size: 12,
                    color: Theme.of(context).colorScheme.onSurfaceVariant),
                const SizedBox(width: 2),
                Text(
                  channel.maxUsers > 0
                      ? '${channel.userCount}/${channel.maxUsers}'
                      : '${channel.userCount}',
                  style: const TextStyle(fontSize: 11),
                ),
                const SizedBox(width: 6),
              ],
              IconButton(
                iconSize: 18,
                visualDensity: VisualDensity.compact,
                constraints: const BoxConstraints(minWidth: 34, minHeight: 34),
                tooltip: isDefault
                    ? 'Stop joining this channel automatically'
                    : 'Join this channel automatically',
                icon: Icon(
                  isDefault ? Icons.star : Icons.star_border,
                  color: isDefault ? StatusColors.connecting : null,
                ),
                onPressed: () => state.setDefaultChannelFor(
                  serverId,
                  isDefault ? null : channel.name,
                ),
              ),
            ],
          ),
        ),
      ),
    ];

    for (final child in byParent[channel.id] ?? const <UiChannel>[]) {
      rows.addAll(_buildNode(context, child, byParent, depth + 1));
    }
    return rows;
  }
}

/// Live roster of everyone in our current channel.
class ChannelUserList extends StatelessWidget {
  const ChannelUserList({
    super.key,
    required this.serverId,
    required this.users,
  });

  final String serverId;
  final List<UiUser> users;

  @override
  Widget build(BuildContext context) {
    if (users.isEmpty) {
      return Padding(
        padding: const EdgeInsets.symmetric(vertical: 10),
        child: Text(
          'Nobody else is in this channel.',
          style: TextStyle(
            fontSize: 12,
            color: Theme.of(context).colorScheme.onSurfaceVariant,
          ),
        ),
      );
    }

    return Column(
      children: [for (final u in users) _UserRow(serverId: serverId, user: u)],
    );
  }
}

class _UserRow extends StatelessWidget {
  const _UserRow({required this.serverId, required this.user});

  final String serverId;
  final UiUser user;

  @override
  Widget build(BuildContext context) {
    final l = L.of(context);
    final state = AppStateScope.of(context);
    final (icon, color) = _statusVisual(user);

    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 2),
      child: Row(
        children: [
          Icon(icon, size: 18, color: color),
          const SizedBox(width: 10),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  user.name,
                  overflow: TextOverflow.ellipsis,
                  style: TextStyle(
                    fontSize: 14,
                    fontWeight: user.talking ? FontWeight.w700 : FontWeight.w400,
                    color: user.talking ? StatusColors.talking : null,
                  ),
                ),
                Text(
                  user.status,
                  style: TextStyle(
                    fontSize: 11,
                    color: Theme.of(context).colorScheme.onSurfaceVariant,
                  ),
                ),
              ],
            ),
          ),
          // Local mute always works and affects only us, so it is the primary
          // action. Server-side mute needs a permission most users lack, so it
          // lives in the overflow menu.
          IconButton(
            iconSize: 20,
            visualDensity: VisualDensity.compact,
            tooltip: user.localMute ? l.unmuteForMe : l.muteForMe,
            icon: Icon(
              user.localMute ? Icons.volume_off : Icons.volume_up,
              color: user.localMute ? StatusColors.failed : null,
            ),
            onPressed: () => state.toggleUserLocalMute(serverId, user),
          ),
          PopupMenuButton<String>(
            tooltip: 'Moderation',
            icon: const Icon(Icons.more_vert, size: 18),
            onSelected: (v) {
              switch (v) {
                case 'mute':
                  state.toggleUserServerMute(serverId, user);
                case 'deafen':
                  state.toggleUserServerDeaf(serverId, user);
                case 'kick':
                  _confirmKick(context, state);
              }
            },
            itemBuilder: (_) => [
              PopupMenuItem(
                value: 'mute',
                child: Text(user.muted
                    ? 'Unmute on server'
                    : 'Mute on server (for everyone)'),
              ),
              PopupMenuItem(
                value: 'deafen',
                child:
                    Text(user.deafened ? 'Undeafen on server' : 'Deafen on server'),
              ),
              const PopupMenuDivider(),
              PopupMenuItem(
                value: 'kick',
                child: Text(l.kickFromServer,
                    style: const TextStyle(color: StatusColors.failed)),
              ),
            ],
          ),
        ],
      ),
    );
  }

  /// Kicking removes someone from the server for everyone, so it asks first and
  /// offers a reason — the server shows it to the person being removed.
  Future<void> _confirmKick(BuildContext context, AppState state) async {
    final l = L.of(context);
    final reason = TextEditingController();
    final messenger = ScaffoldMessenger.of(context);

    final confirmed = await showDialog<bool>(
      context: context,
      builder: (c) => AlertDialog(
        title: Text('Kick ${user.name}?'),
        content: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            const Text(
              'They will be disconnected from the server. This is not a ban — '
              'they can reconnect straight away.',
              style: TextStyle(fontSize: 13),
            ),
            const SizedBox(height: 14),
            TextField(
              controller: reason,
              autofocus: true,
              decoration: const InputDecoration(
                labelText: 'Reason (optional)',
                hintText: 'Shown to them as they are removed',
              ),
            ),
          ],
        ),
        actions: [
          TextButton(
              onPressed: () => Navigator.pop(c, false),
              child: const Text('Cancel')),
          FilledButton(
            style: FilledButton.styleFrom(
                backgroundColor: StatusColors.failed),
            onPressed: () => Navigator.pop(c, true),
            child: Text(l.kick),
          ),
        ],
      ),
    );

    if (confirmed != true) {
      reason.dispose();
      return;
    }
    final text = reason.text.trim();
    reason.dispose();

    final error = await state.kickUserFrom(serverId, user, text);
    messenger.showSnackBar(SnackBar(
      content: Text(error ??
          'Kick sent. If nothing happens, you lack the Kick permission.'),
    ));
  }

  static (IconData, Color) _statusVisual(UiUser u) {
    if (u.deafened) return (Icons.hearing_disabled, StatusColors.failed);
    if (u.localMute) return (Icons.volume_off, StatusColors.failed);
    if (u.muted) return (Icons.mic_off, StatusColors.idle);
    if (u.talking) return (Icons.volume_up, StatusColors.talking);
    return (Icons.person_outline, StatusColors.idle);
  }
}
