import 'dart:typed_data' show Uint8List;

import 'package:flutter/material.dart';
import 'package:flutter_svg/flutter_svg.dart';

import '../screens/channel_acl_screen.dart';

import '../src/rust/api/mumbleway.dart';
import '../l10n/app_localizations.dart';
import '../state/app_state.dart';
import 'server_text.dart';
import 'connection_quality.dart';
import 'voice_meter.dart';
import '../theme.dart';
import 'error_snack.dart';
import 'watch.dart';

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

  /// Whether this rider may change anything about **this** channel.
  ///
  /// Before the server has answered, yes: the same rule as the participant
  /// menu, since a menu that is dead for the first second reads as broken.
  ///
  /// **Per channel, because Write is per channel.** This used to read one
  /// server-wide answer, built from the mask of whichever channel the rider
  /// happened to be standing in, and drew the same menu on every row from it.
  /// So an admin standing in a channel they may not write to saw no menu
  /// anywhere, which is exactly how this was reported.
  static bool _mayManage(AppState state, String serverId, int channelId) {
    final here = state.runtimeFor(serverId).channelRights[channelId];
    if (here == null) {
      // Not answered yet. Ask, and meanwhile offer it.
      state.askChannelRights(serverId, channelId);
      return true;
    }
    return !here.known || here.write || here.makeChannel;
  }

  List<Widget> _buildNode(
    BuildContext context,
    UiChannel channel,
    Map<int?, List<UiChannel>> byParent,
    int depth,
  ) {
    final state = AppStateScope.of(context);
    final isCurrent = channel.id == currentChannelId;
    final listening = state.runtimeFor(serverId).listening.contains(channel.id);
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
                  showError(messenger, error);
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
              // Hearing a channel without going to it. Offered on every
              // channel but the one the rider is in — being in it is already
              // hearing it — and the icon follows the server's answer rather
              // than the tap, since a listen can be refused.
              if (!isCurrent)
                IconButton(
                  iconSize: 18,
                  visualDensity: VisualDensity.compact,
                  constraints: const BoxConstraints(
                    minWidth: 34,
                    minHeight: 34,
                  ),
                  tooltip: listening
                      ? L.of(context).stopListening
                      : L.of(context).listenHere,
                  icon: Icon(
                    listening ? Icons.headset : Icons.headset_outlined,
                    color: listening ? StatusColors.talking : null,
                  ),
                  onPressed: () async {
                    final messenger = ScaffoldMessenger.of(context);
                    final error = await state.toggleListening(
                      serverId,
                      channel.id,
                    );
                    if (error != null) showError(messenger, error);
                  },
                ),
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
              // Managing channels at all is unusual on somebody else's
              // server, so the menu is only drawn where the rider may do
              // something — an always-present menu of greyed entries is a
              // worse answer here than no menu.
              if (_mayManage(state, serverId, channel.id))
                _ChannelMenu(serverId: serverId, channel: channel),
            ],
          ),
        ),
      ),
      // **Only where the rider has a stake in it.** A description is a
      // paragraph, and a paragraph under every row turns a channel list into a
      // wall of prose that hides the list. Under the channel they are standing
      // in, and under the ones they have chosen to listen to, it is the
      // explanation of what they are hearing — which is where an operator put
      // it to be read.
      //
      // Already stripped of its markup by the core, and fetched as a blob when
      // it is 128 bytes or more, so it is here in full or not at all.
      if ((isCurrent || listening) && channel.description.trim().isNotEmpty)
        Padding(
          padding: EdgeInsets.fromLTRB(32.0 + depth * 16, 0, 12, 8),
          child: ServerText.description(
            channel.description,
            style: TextStyle(
              fontSize: 11,
              height: 1.35,
              color: Theme.of(context).colorScheme.onSurfaceVariant,
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

/// Marks somebody whose client identified itself as MumbleWay.
///
/// **The app's own icon, very small**, rather than a generic symbol. It is the
/// picture riders already know from their home screen, so it says "same app as
/// you" without a legend, and nothing else in the roster looks like it.
///
/// Only ever shown on evidence — see `session::peers` in the core. Its absence
/// is not a claim: an older MumbleWay says nothing, and on a server too old to
/// relay the handshake nobody can say anything. For the same reason the core
/// badges our own row only where the handshake runs, so that everybody else's
/// badges mean something.
/// A tooltip that gets out of the way when it has nothing to say.
///
/// The roster's status glyph carries an explanation only in the states that
/// need one; wrapping it unconditionally would put an empty box under every
/// silent rider's icon.
class _StatusTooltip extends StatelessWidget {
  const _StatusTooltip({required this.message, required this.child});

  final String message;
  final Widget child;

  @override
  Widget build(BuildContext context) {
    if (message.isEmpty) return child;
    return Tooltip(
      message: message,
      triggerMode: TooltipTriggerMode.tap,
      showDuration: const Duration(seconds: 6),
      child: child,
    );
  }
}

class MumblewayBadge extends StatelessWidget {
  const MumblewayBadge({super.key, required this.version});

  /// As the peer reported it, already cleaned and cut to length by the core.
  /// May be empty, which still identifies the app, just not which build.
  final String version;

  static const double size = 16;

  @override
  Widget build(BuildContext context) {
    // Trimmed so an empty version does not leave a trailing space in either
    // language.
    final label = L.of(context).peerUsesMumbleway(version).trim();
    return Tooltip(
      message: label,
      child: ClipRRect(
        // The launcher's own rounding at this size, near enough; the SVG is
        // full-bleed because the platforms mask it themselves.
        borderRadius: BorderRadius.circular(size * 0.22),
        child: SvgPicture.asset(
          'assets/icon/mumbleway.svg',
          width: size,
          height: size,
          // The tooltip carries the label for screen readers; the picture adds
          // nothing a second reading would.
          excludeFromSemantics: true,
        ),
      ),
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

    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 2),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.center,
        children: [
          // Who this is, and nothing else.
          //
          // **The state used to ride in the corner of this.** At eleven pixels
          // behind a face it was something to find rather than something to
          // see, which is the opposite of what a roster is for; it is beside
          // the name now, where the eye already is. Their picture when they
          // have one, and the plain person glyph when they have not.
          switch (state.runtimeFor(serverId).avatars[user.session]) {
            final Uint8List image? => ClipOval(
              child: Image.memory(
                image,
                width: 18,
                height: 18,
                fit: BoxFit.cover,
                gaplessPlayback: true,
                // A picture from a stranger's server that will not decode is
                // not a reason to lose the row.
                errorBuilder: (_, _, _) => const Icon(
                  Icons.person_outline,
                  size: 18,
                  color: StatusColors.idle,
                ),
              ),
            ),
            _ => const Icon(
              Icons.person_outline,
              size: 18,
              color: StatusColors.idle,
            ),
          },
          const SizedBox(width: 10),
          Expanded(
            // The badge rides directly after the name rather than at the end
            // of the row, so it reads as something about this person. The name
            // is Flexible so a long one still truncates with the badge visible.
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              mainAxisSize: MainAxisSize.min,
              children: [
                Row(
                  children: [
                    Flexible(
                      child: Text(
                        user.name,
                        overflow: TextOverflow.ellipsis,
                        style: TextStyle(
                          fontSize: 14,
                          fontWeight: speaking
                              ? FontWeight.w700
                              : FontWeight.w400,
                          color: speaking ? StatusColors.talking : null,
                        ),
                      ),
                    ),
                    if (user.mumblewayVersion case final version?) ...[
                      const SizedBox(width: 6),
                      MumblewayBadge(version: version),
                    ],
                    // Their microphone and their hearing, after the name and
                    // the badge and at the size of both. Always both, each
                    // held to read for who decided it.
                    for (final (glyph, colour, words) in statusGlyphs(
                      l,
                      user,
                    )) ...[
                      const SizedBox(width: 6),
                      _StatusTooltip(
                        message: words,
                        child: Icon(glyph, size: 15, color: colour),
                      ),
                    ],
                    // Why the channel goes quiet when this one person starts
                    // talking. Nothing else in the roster would say so.
                    if (user.prioritySpeaker) ...[
                      const SizedBox(width: 6),
                      Tooltip(
                        message: l.prioritySpeaker,
                        triggerMode: TooltipTriggerMode.tap,
                        child: const Icon(
                          Icons.campaign_outlined,
                          size: 14,
                          color: StatusColors.talking,
                        ),
                      ),
                    ],
                  ],
                ),
                // Their own note, under the name and in full.
                //
                // It was an icon holding a tooltip, which is a thing to
                // discover rather than a thing to read — and what riders put
                // there is where they are and when they are leaving, which is
                // the sort of thing the next rider wants without asking for
                // it. The server caps a comment at 512 characters and the
                // client strips its markup, so "in full" is a line or two.
                if (user.comment.isNotEmpty)
                  Padding(
                    padding: const EdgeInsets.only(top: 1),
                    // Two lines, then an ellipsis — and the whole of it behind
                    // a press. A note is capped at 512 characters by the
                    // server, and a rider who writes all of them would
                    // otherwise own the roster: every other row would be
                    // pushed off a phone screen by one person's paragraph.
                    child: Tooltip(
                      message: user.comment,
                      triggerMode: TooltipTriggerMode.tap,
                      showDuration: const Duration(seconds: 8),
                      child: Text(
                        user.comment,
                        maxLines: 2,
                        overflow: TextOverflow.ellipsis,
                        style: TextStyle(
                          fontSize: 11.5,
                          height: 1.25,
                          color: Theme.of(context).colorScheme.onSurfaceVariant,
                        ),
                      ),
                    ),
                  ),
              ],
            ),
          ),
          const SizedBox(width: 8),
          // Same meter as everywhere else, so a given bar length means the
          // same loudness whether it is a participant or your own microphone.
          //
          // **Unless the ladder has given it up.** One meter per person, each
          // moving with every incoming frame, is several animated widgets on a
          // busy channel — and this is the only rung the rider sees without
          // opening the diagnostics panel, which is why it is the last display
          // rung rather than an early one.
          //
          // The name still turns green and bold when somebody talks, so who is
          // speaking survives; the amount does not. The row keeps the meter's
          // width either way, because a layout that reflows when a device gets
          // busy reads as a second fault.
          // **On end, at the height of the glyphs beside it.** Lying across
          // eighty pixels it was the widest thing in the row and the one with
          // least to say: "how loud" is a fraction, and a column six pixels
          // wide says it as well as a bar does while handing the width back to
          // the name, the note and the badges that had been squeezed for it.
          // It takes an icon's worth of room and sits in the middle of it, so
          // the gaps on either side are the gaps between every other pair of
          // things in this row. Six pixels of bar with its own narrow margins
          // read as something that had been squeezed in.
          SizedBox(
            width: 20,
            height: 20,
            child: state.participantMetersDisabled
                ? null
                : Center(
                    child: VoiceMeter(
                      vertical: true,
                      width: 6,
                      height: 20,
                      levelDb:
                          state
                              .runtimeFor(serverId)
                              .speakerLevels[user.session] ??
                          -120.0,
                      muted: user.muted || user.selfMuted || user.localMute,
                    ),
                  ),
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
              color: user.localMute ? StatusColors.yours : null,
            ),
            onPressed: () => state.toggleUserLocalMute(serverId, user),
          ),
          // How this rider's connection is doing, between the control that
          // silences them and the menu that does everything else, at the size
          // of both: the rider who keeps breaking up and the rider you are
          // about to mute are the same row, and this is the half that says
          // which of the two is happening.
          const SizedBox(width: 2),
          ConnectionQualityBars(quality: user.quality, size: 20),
          const SizedBox(width: 2),
          PopupMenuButton<String>(
            tooltip: 'Moderation',
            icon: const Icon(Icons.more_vert, size: 18),
            onSelected: (v) {
              switch (v) {
                case 'mute':
                  state.toggleUserServerMute(serverId, user);
                case 'deafen':
                  state.toggleUserServerDeaf(serverId, user);
                case 'move':
                  _moveElsewhere(context, state);
                case 'register':
                  _registerThem(context, state);
                case 'unregister':
                  _unregisterThem(context, state);
                case 'priority':
                  state.setPriority(serverId, user, on: !user.prioritySpeaker);
                case 'reset':
                  state.clearUserContent(serverId, user);
                case 'details':
                  _showDetails(context, state);
                case 'kick':
                  _confirmKick(context, state);
                case 'ban':
                  _confirmBan(context, state);
                default:
                  // Anything else is one of the server's own entries, keyed by
                  // the identifier it registered. Prefixed so a server cannot
                  // register an action called "kick" and have it run ours.
                  final id = v.startsWith('server:') ? v.substring(7) : null;
                  final action = state
                      .runtimeFor(serverId)
                      .contextActions
                      .where((a) => a.action == id)
                      .firstOrNull;
                  if (action != null) {
                    state.runContextAction(
                      serverId,
                      action,
                      session: user.session,
                    );
                  }
              }
            },
            itemBuilder: (_) {
              // What the server has said we may do here. Until it has said
              // anything, everything is offered: greying out the menu for the
              // second before the first answer arrives reads as a broken app,
              // and a refusal is still handled if we guess wrong.
              final rights = state.runtimeFor(serverId).rights;
              final unanswered = !rights.known;
              return [
                PopupMenuItem(
                  value: 'mute',
                  // Offered without the permission when they run MumbleWay:
                  // the request reaches their app directly and needs none.
                  // This is the case the whole request exists for.
                  enabled:
                      unanswered ||
                      rights.muteDeafen ||
                      user.mumblewayVersion != null,
                  child: Text(user.muted ? l.unmuteOnServer : l.muteOnServer),
                ),
                PopupMenuItem(
                  value: 'deafen',
                  enabled: unanswered || rights.muteDeafen,
                  child: Text(
                    user.deafened ? l.undeafenOnServer : l.deafenOnServer,
                  ),
                ),
                PopupMenuItem(
                  value: 'move',
                  enabled: unanswered || rights.moveUsers,
                  child: Text(l.moveToChannel),
                ),
                PopupMenuItem(
                  value: 'priority',
                  enabled: unanswered || rights.muteDeafen,
                  child: Text(
                    user.prioritySpeaker ? l.priorityRevoke : l.priorityGrant,
                  ),
                ),
                // **One of these two is always the wrong one to offer.**
                // Registering somebody who already has an account and
                // unregistering somebody who has none are both actions whose
                // only possible outcome is a refusal, and the roster knows
                // which it is: the server sends an account id for a registered
                // rider and nothing at all for the rest.
                PopupMenuItem(
                  value: 'register',
                  enabled:
                      (unanswered || rights.registerOthers) &&
                      user.userId == null,
                  child: Text(l.registerThem),
                ),
                PopupMenuItem(
                  value: 'unregister',
                  enabled:
                      (unanswered || rights.registerOthers) &&
                      user.userId != null,
                  child: Text(l.unregisterThem),
                ),
                PopupMenuItem(
                  value: 'reset',
                  // The permission is ResetUserContent on the root channel,
                  // which this client reads but does not carry separately;
                  // an admin holding Ban has it on every server that grants
                  // these together, and a refusal is still shown.
                  enabled: unanswered || rights.ban,
                  child: Text(l.resetContent),
                ),
                PopupMenuItem(value: 'details', child: Text(l.userDetails)),
                const PopupMenuDivider(),
                PopupMenuItem(
                  value: 'kick',
                  enabled: unanswered || rights.kick,
                  child: Text(
                    l.kickFromServer,
                    style: const TextStyle(color: StatusColors.failed),
                  ),
                ),
                // Banning needs a stronger permission than kicking and is the
                // one moderation action with no undo inside itself, so it sits
                // last and asks twice as carefully.
                PopupMenuItem(
                  value: 'ban',
                  enabled: unanswered || rights.ban,
                  child: Text(
                    l.banFromServer,
                    style: const TextStyle(color: StatusColors.failed),
                  ),
                ),
                // Below a divider, and last: these come from a bot or a server
                // plugin, this app has no idea what they do, and they must not
                // sit where a rider expects the actions that are always there.
                for (final action
                    in state
                        .runtimeFor(serverId)
                        .contextActions
                        .where((a) => a.forUser)) ...[
                  const PopupMenuDivider(),
                  PopupMenuItem(
                    value: 'server:${action.action}',
                    child: Text(action.label),
                  ),
                ],
              ];
            },
          ),
        ],
      ),
    );
  }

  /// Gives this rider an account on the server, which is what makes their
  /// name theirs and lets an ACL name them.
  Future<void> _registerThem(BuildContext context, AppState state) async {
    final l = L.of(context);
    final messenger = ScaffoldMessenger.of(context);
    final error = await state.registerRider(serverId, user);
    if (error != null) {
      showError(messenger, error);
    } else {
      messenger.showSnackBar(SnackBar(content: Text(l.registerThemSent)));
    }
  }

  /// Takes their account away again, by the id the server gave it.
  ///
  /// The registration is what an access list names, so removing it quietly
  /// drops them out of every group they were in — which is why this is in the
  /// menu rather than beside the name.
  Future<void> _unregisterThem(BuildContext context, AppState state) async {
    final id = user.userId;
    if (id == null) return;
    final l = L.of(context);
    final messenger = ScaffoldMessenger.of(context);
    final error = await state.unregister(serverId, id);
    if (error != null) {
      showError(messenger, error);
    } else {
      messenger.showSnackBar(SnackBar(content: Text(l.unregisterThemSent)));
    }
  }

  /// What the server will say about this rider.
  ///
  /// Most of it is withheld from anybody but an admin, so the dialog says that
  /// rather than showing empty rows: "the server did not say" is information,
  /// and a blank line is not.
  Future<void> _showDetails(BuildContext context, AppState state) async {
    final l = L.of(context);
    await state.loadUserDetails(serverId, user);
    if (!context.mounted) return;
    await showDialog<void>(
      context: context,
      builder: (c) => AlertDialog(
        title: Text(user.name),
        content: Watch<UiUserDetails?>(
          (s) => s.runtimeFor(serverId).userDetails[user.session],
          (c, s) {
            final d = s.runtimeFor(serverId).userDetails[user.session];
            if (d == null) {
              return Text(l.userDetailsWaiting);
            }
            final rows = <(String, String)>[
              if (d.release.isNotEmpty) (l.userDetailsClient, d.release),
              if (d.os.isNotEmpty)
                (
                  l.userDetailsSystem,
                  [d.os, d.osVersion].where((x) => x.isNotEmpty).join(' '),
                ),
              if (d.address.isNotEmpty) (l.userDetailsAddress, d.address),
              (
                l.userDetailsCertificate,
                d.strongCertificate
                    ? l.userDetailsCertificateStrong
                    : l.userDetailsCertificateSelfSigned,
              ),
              (l.userDetailsOnline, l.minutes(d.onlineSecs ~/ 60)),
              (l.userDetailsIdle, l.minutes(d.idleSecs ~/ 60)),
            ];
            return Column(
              mainAxisSize: MainAxisSize.min,
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                for (final (label, value) in rows)
                  Padding(
                    padding: const EdgeInsets.symmetric(vertical: 2),
                    child: Row(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        SizedBox(
                          width: 110,
                          child: Text(
                            label,
                            style: TextStyle(
                              fontSize: 12,
                              color: Theme.of(c).colorScheme.onSurfaceVariant,
                            ),
                          ),
                        ),
                        Expanded(
                          child: Text(
                            value,
                            style: const TextStyle(fontSize: 12),
                          ),
                        ),
                      ],
                    ),
                  ),
                if (d.release.isEmpty && d.address.isEmpty) ...[
                  const SizedBox(height: 8),
                  Text(
                    l.userDetailsWithheld,
                    style: const TextStyle(fontSize: 12),
                  ),
                ],
              ],
            );
          },
        ),
        actions: [
          TextButton(onPressed: () => Navigator.pop(c), child: Text(l.close)),
        ],
      ),
    );
  }

  /// Banning is kicking that does not wear off, so it asks with the word "ban"
  /// on the button rather than a generic confirmation, and keeps the reason —
  /// the server stores it on the ban, where the next admin reads it.
  Future<void> _confirmBan(BuildContext context, AppState state) async {
    final l = L.of(context);
    final reason = TextEditingController();
    final messenger = ScaffoldMessenger.of(context);

    final confirmed = await showDialog<bool>(
      context: context,
      builder: (c) => AlertDialog(
        title: Text(l.banTitle(user.name)),
        content: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(l.banBody, style: const TextStyle(fontSize: 13)),
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
            child: Text(l.ban),
          ),
        ],
      ),
    );

    if (confirmed != true) {
      reason.dispose();
      return;
    }
    final error = await state.banUserFrom(serverId, user, reason.text);
    reason.dispose();
    if (error != null) {
      showError(messenger, error);
    } else {
      messenger.showSnackBar(SnackBar(content: Text(l.banSent)));
    }
  }

  /// Moves somebody into another channel.
  ///
  /// The list is the channels this server has, with the one they are already
  /// in left out: offering it would be offering an action that does nothing.
  Future<void> _moveElsewhere(BuildContext context, AppState state) async {
    final l = L.of(context);
    final messenger = ScaffoldMessenger.of(context);
    final channels = state
        .runtimeFor(serverId)
        .channels
        .where((c) => c.id != user.channelId)
        .toList();

    final target = await showDialog<int>(
      context: context,
      builder: (c) => SimpleDialog(
        title: Text(l.moveWhere(user.name)),
        children: [
          if (channels.isEmpty)
            Padding(
              padding: const EdgeInsets.symmetric(horizontal: 24, vertical: 8),
              child: Text(l.moveNowhere),
            ),
          for (final channel in channels)
            SimpleDialogOption(
              onPressed: () => Navigator.pop(c, channel.id),
              child: Text(channel.name),
            ),
        ],
      ),
    );
    if (target == null) return;

    final error = await state.moveUserTo(serverId, user, target);
    if (error != null) showError(messenger, error);
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
    if (error != null) {
      showError(messenger, error);
    } else {
      messenger.showSnackBar(SnackBar(content: Text(l.kickSent)));
    }
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
  /// Their microphone and their hearing, as glyph, colour and words.
  ///
  /// **Two glyphs, always, because they are two switches.** A rider closes
  /// their microphone with one button and their ears with another, and the two
  /// say quite different things to whoever reads the row: one is somebody
  /// choosing not to talk, the other is somebody who cannot hear a word you
  /// say. Drawn as one glyph with the other hidden behind it — which is how
  /// this started — a rider who had done both looked like a rider who had done
  /// one, and which one depended on the order of the `if`s.
  ///
  /// **One glyph per state, and the colour says whose decision it was.** Grey
  /// means the thing is on and nothing is wrong, which is worth drawing: a row
  /// with no mark at all reads as "nobody has told us" rather than "they are
  /// fine". Red is the rider's own hand — they chose not to talk, or not to
  /// listen. Amber is somebody else's: an admin, or the channel's own rule,
  /// and that is the one worth catching the eye, because it is the one the
  /// rider on the other end cannot undo by asking.
  ///
  /// The glyph itself does not change with the cause. Two red struck
  /// microphones are not told apart through a visor at speed, and the state —
  /// "they cannot be heard" — is what the row is for; which of them decided it
  /// is in the words, a press away.
  ///
  /// Speech is not in here: the name itself turns and thickens when somebody
  /// talks.
  static List<(IconData, Color, String)> statusGlyphs(L l, UiUser u) {
    const quiet = StatusColors.idle;
    const theirOwn = StatusColors.failed;
    const imposed = StatusColors.reconnecting;
    const yours = StatusColors.yours;

    // **Somebody else's decision is read first, not the rider's.** A rider
    // can be in both states at once, and on this app they usually are: muting
    // a MumbleWay rider sends the server mute *and* asks their app to close
    // its own microphone, so an imposed mute arrives with `self_mute` beside
    // it. Reading their own hand first painted every admin mute in the colour
    // of a decision the rider never made.
    final (IconData, Color, String) mic;
    if (u.localMute) {
      // **Yours, and about the microphone, because that is the switch you
      // moved.** You turned this rider down; the row says so where it says
      // everything else about being heard, in the colour reserved for a
      // decision only you can see — and it is read first because it is true
      // whatever anybody else has done to them, and it is the thing that
      // explains the silence you are sitting in.
      mic = (Icons.mic_off, yours, l.statusMutedForYou);
    } else if (u.muted) {
      mic = (Icons.mic_off, imposed, l.statusMutedByAdmin);
    } else if (u.suppressed) {
      // Silenced by the channel rather than by anybody's hand, and amber for
      // the same reason an admin mute is: it is not this rider's doing and
      // they cannot undo it by changing their mind. A glyph of its own because
      // the cure is different — this one is fixed by moving, not by a button.
      mic = (Icons.voice_over_off, imposed, l.statusSuppressedHere);
    } else if (u.selfMuted) {
      mic = (Icons.mic_off, theirOwn, l.statusTheirMicOff);
    } else {
      mic = (Icons.mic, quiet, l.statusMicOpen);
    }

    // The ears are about *their* hearing, and about yours in it: whether this
    // rider can hear the channel, and whether they are hearing **you**.
    // Silencing somebody for yourself does nothing to what they can hear, so
    // that one is drawn on the microphone above and not here.
    final (IconData, Color, String) ears;
    if (u.mutedYou) {
      // **They have turned you down, and this is the only place it can be
      // seen.** A local mute is invisible by design — the server is not told —
      // so from this end being dropped looks exactly like being heard, and a
      // rider carries on talking to somebody who stopped listening. Their app
      // says so, and it is purple for the same reason the microphone is: it is
      // one rider's private decision about another, and nobody else on the
      // server can see it either.
      ears = (Icons.headset_off, yours, l.statusTheyMutedYou);
    } else if (u.deafened) {
      ears = (Icons.headset_off, imposed, l.statusDeafenedByAdmin);
    } else if (u.selfDeafened) {
      ears = (Icons.headset_off, theirOwn, l.statusTheyHearNothing);
    } else {
      ears = (Icons.headset, quiet, l.statusTheyCanHear);
    }

    return [mic, ears];
  }
}

/// Making, renaming, describing and removing a channel.
///
/// Each entry is offered only where the server says it is allowed, and the two
/// permissions are genuinely different: making a channel under this one is
/// `MakeChannel` *here*, while renaming or removing this one is `Write` on it.
/// A rider may easily have one and not the other.
class _ChannelMenu extends StatelessWidget {
  const _ChannelMenu({required this.serverId, required this.channel});

  final String serverId;
  final UiChannel channel;

  @override
  Widget build(BuildContext context) {
    final l = L.of(context);
    final state = AppStateScope.of(context);
    final rights = state.runtimeFor(serverId).rights;
    final unanswered = !rights.known;

    return PopupMenuButton<String>(
      tooltip: l.channelActions,
      icon: const Icon(Icons.more_horiz, size: 16),
      iconSize: 16,
      onSelected: (v) {
        switch (v) {
          case 'add':
            _makeChannel(context, state);
          case 'rename':
            _rename(context, state);
          case 'describe':
            _describe(context, state);
          case 'permissions':
            Navigator.of(context).push(
              MaterialPageRoute<void>(
                builder: (_) =>
                    ChannelAclScreen(serverId: serverId, channel: channel),
              ),
            );
          case 'remove':
            _confirmRemove(context, state);
        }
      },
      itemBuilder: (_) => [
        PopupMenuItem(
          value: 'add',
          enabled: unanswered || rights.makeChannel,
          child: Text(l.channelAdd),
        ),
        PopupMenuItem(
          value: 'rename',
          enabled: unanswered || rights.write,
          child: Text(l.channelRename),
        ),
        PopupMenuItem(
          value: 'describe',
          enabled: unanswered || rights.write,
          child: Text(l.channelDescribe),
        ),
        PopupMenuItem(
          value: 'permissions',
          enabled: unanswered || rights.write,
          child: Text(l.aclPermissions),
        ),
        const PopupMenuDivider(),
        PopupMenuItem(
          value: 'remove',
          enabled: unanswered || rights.write,
          child: Text(
            l.channelRemove,
            style: const TextStyle(color: StatusColors.failed),
          ),
        ),
      ],
    );
  }

  /// A new channel under this one.
  ///
  /// Temporary is offered and defaulted *on*, because the common case on a
  /// ride is a channel for today: one that nobody has to remember to tidy up,
  /// and that disappears when the last rider leaves it.
  Future<void> _makeChannel(BuildContext context, AppState state) async {
    final l = L.of(context);
    final messenger = ScaffoldMessenger.of(context);
    final name = TextEditingController();
    final description = TextEditingController();
    var temporary = true;

    final made = await showDialog<bool>(
      context: context,
      builder: (c) => StatefulBuilder(
        builder: (c, setState) => AlertDialog(
          title: Text(l.channelAdd),
          content: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              TextField(
                controller: name,
                autofocus: true,
                decoration: InputDecoration(labelText: l.channelName),
              ),
              const SizedBox(height: 12),
              TextField(
                controller: description,
                maxLines: 2,
                decoration: InputDecoration(labelText: l.channelDescription),
              ),
              const SizedBox(height: 8),
              CheckboxListTile(
                contentPadding: EdgeInsets.zero,
                value: temporary,
                onChanged: (v) => setState(() => temporary = v ?? true),
                title: Text(l.channelTemporary),
                subtitle: Text(l.channelTemporaryBody),
              ),
            ],
          ),
          actions: [
            TextButton(
              onPressed: () => Navigator.pop(c, false),
              child: Text(l.cancel),
            ),
            FilledButton(
              onPressed: () => Navigator.pop(c, true),
              child: Text(l.channelCreate),
            ),
          ],
        ),
      ),
    );

    final chosen = name.text.trim();
    final body = description.text.trim();
    name.dispose();
    description.dispose();
    if (made != true || chosen.isEmpty) return;

    final error = await state.createChannelOn(
      serverId,
      parent: channel.id,
      name: chosen,
      description: body,
      temporary: temporary,
    );
    if (error != null) showError(messenger, error);
  }

  Future<void> _rename(BuildContext context, AppState state) async {
    final l = L.of(context);
    final messenger = ScaffoldMessenger.of(context);
    final field = TextEditingController(text: channel.name);
    final ok = await showDialog<bool>(
      context: context,
      builder: (c) => AlertDialog(
        title: Text(l.channelRename),
        content: TextField(
          controller: field,
          autofocus: true,
          decoration: InputDecoration(labelText: l.channelName),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(c, false),
            child: Text(l.cancel),
          ),
          FilledButton(
            onPressed: () => Navigator.pop(c, true),
            child: Text(l.save),
          ),
        ],
      ),
    );
    final chosen = field.text.trim();
    field.dispose();
    if (ok != true || chosen.isEmpty || chosen == channel.name) return;
    final error = await state.editChannelOn(serverId, channel.id, name: chosen);
    if (error != null) showError(messenger, error);
  }

  Future<void> _describe(BuildContext context, AppState state) async {
    final l = L.of(context);
    final messenger = ScaffoldMessenger.of(context);
    final field = TextEditingController(text: channel.description);
    final ok = await showDialog<bool>(
      context: context,
      builder: (c) => AlertDialog(
        title: Text(l.channelDescribe),
        content: TextField(
          controller: field,
          autofocus: true,
          maxLines: 4,
          decoration: InputDecoration(labelText: l.channelDescription),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(c, false),
            child: Text(l.cancel),
          ),
          FilledButton(
            onPressed: () => Navigator.pop(c, true),
            child: Text(l.save),
          ),
        ],
      ),
    );
    final text = field.text.trim();
    field.dispose();
    if (ok != true) return;
    final error = await state.editChannelOn(
      serverId,
      channel.id,
      description: text,
    );
    if (error != null) showError(messenger, error);
  }

  /// Removing takes everything under it with it, which is the part worth
  /// saying out loud before it happens.
  Future<void> _confirmRemove(BuildContext context, AppState state) async {
    final l = L.of(context);
    final messenger = ScaffoldMessenger.of(context);
    final ok = await showDialog<bool>(
      context: context,
      builder: (c) => AlertDialog(
        title: Text(l.channelRemoveTitle(channel.name)),
        content: Text(l.channelRemoveBody),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(c, false),
            child: Text(l.cancel),
          ),
          FilledButton(
            style: FilledButton.styleFrom(backgroundColor: StatusColors.failed),
            onPressed: () => Navigator.pop(c, true),
            child: Text(l.channelRemove),
          ),
        ],
      ),
    );
    if (ok != true) return;
    final error = await state.removeChannelOn(serverId, channel.id);
    if (error != null) showError(messenger, error);
  }
}
