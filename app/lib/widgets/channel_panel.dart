import 'package:flutter/material.dart';

import '../src/rust/api/mumbleway.dart';
import '../l10n/app_localizations.dart';
import '../state/app_state.dart';
import 'voice_meter.dart';
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
      return Padding(
        padding: const EdgeInsets.symmetric(vertical: 8),
        child: Text(
          L.of(context).noChannelsYet,
          style: const TextStyle(fontSize: 12),
        ),
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
        .where(
          (c) => c.parent != null && !channels.any((p) => p.id == c.parent),
        )
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
    final isDefault =
        defaultChannelName != null &&
        defaultChannelName!.toLowerCase() == channel.name.toLowerCase();
    final full = channel.maxUsers > 0 && channel.userCount >= channel.maxUsers;

    final rows = <Widget>[
      InkWell(
        onTap: isCurrent
            ? null
            : () async {
                // Captured before the await: the panel can be rebuilt or
                // dismissed while the server is deciding.
                final messenger = ScaffoldMessenger.of(context);
                final error = await state.joinChannelOn(serverId, channel.id);
                if (error != null) {
                  messenger.showSnackBar(SnackBar(content: Text(error)));
                }
              },
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
                Icon(
                  Icons.person,
                  size: 12,
                  color: Theme.of(context).colorScheme.onSurfaceVariant,
                ),
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
                    ? L.of(context).stopJoiningAutomatically
                    : L.of(context).joinAutomatically,
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
          L.of(context).nobodyElseHere,
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

    // Whether this person is talking comes from the decoded audio, so it
    // arrives with the levels rather than with the roster. Listening here
    // rather than to the state as a whole is what keeps a channel of twenty
    // people from rebuilding all twenty rows ten times a second because one
    // of them is speaking.
    return ListenableBuilder(
      listenable: state.meters,
      builder: (context, _) => _row(context, l, state),
    );
  }

  Widget _row(BuildContext context, L l, AppState state) {
    final speaking = state.runtimeFor(serverId).isSpeaking(user.session);
    final (icon, color) = _statusVisual(user, speaking: speaking);

    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 2),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.center,
        children: [
          Icon(icon, size: 18, color: color),
          const SizedBox(width: 10),
          Expanded(
            child: Text(
              user.name,
              overflow: TextOverflow.ellipsis,
              style: TextStyle(
                fontSize: 14,
                fontWeight: speaking ? FontWeight.w700 : FontWeight.w400,
                color: speaking ? StatusColors.talking : null,
              ),
            ),
          ),
          const SizedBox(width: 8),
          // Same meter as everywhere else, so a given bar length means the
          // same loudness whether it is a participant or your own microphone.
          VoiceMeter(
            width: 81,
            levelDb:
                state.runtimeFor(serverId).speakerLevels[user.session] ??
                -120.0,
            muted: user.muted || user.localMute,
          ),
          const SizedBox(width: 2),
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
                child: Text(user.muted ? l.unmuteOnServer : l.muteOnServer),
              ),
              PopupMenuItem(
                value: 'deafen',
                child: Text(
                  user.deafened ? l.undeafenOnServer : l.deafenOnServer,
                ),
              ),
              const PopupMenuDivider(),
              PopupMenuItem(
                value: 'kick',
                child: Text(
                  l.kickFromServer,
                  style: const TextStyle(color: StatusColors.failed),
                ),
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
            Text(l.kickBody, style: const TextStyle(fontSize: 13)),
            const SizedBox(height: 14),
            TextField(
              controller: reason,
              autofocus: true,
              decoration: InputDecoration(
                labelText: l.kickReasonLabel,
                hintText: l.kickReasonHint,
              ),
            ),
          ],
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(c, false),
            child: Text(l.cancel),
          ),
          FilledButton(
            style: FilledButton.styleFrom(backgroundColor: StatusColors.failed),
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
    messenger.showSnackBar(SnackBar(content: Text(error ?? l.kickSent)));
  }

  /// `speaking` comes from the audio, not the roster: the server never says
  /// who is talking, so `UiUser.talking` only ever changes when the server
  /// happens to send an unrelated roster update.
  /// The whole state of a participant in one glyph, at the head of the row.
  ///
  /// Everything lives here rather than being spread along the row: the icons
  /// line up in a column, so a channel can be scanned down the left edge
  /// instead of read across every entry.
  ///
  /// `speaking` comes from the audio, not the roster: the server never says
  /// who is talking, so `UiUser.talking` only ever changes when the server
  /// happens to send an unrelated roster update.
  static (IconData, Color) _statusVisual(UiUser u, {required bool speaking}) {
    // The same glyph the toolbar uses for the same state. Two icons for one
    // condition is how a roster ends up meaning something different from the
    // button that caused it.
    if (u.deafened) return (Icons.volume_off, StatusColors.failed);
    if (u.localMute) return (Icons.volume_off, StatusColors.failed);
    if (u.muted) return (Icons.mic_off, StatusColors.failed);
    if (speaking) return (Icons.volume_up, StatusColors.talking);
    return (Icons.person_outline, StatusColors.idle);
  }
}
