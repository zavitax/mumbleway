import 'package:flutter/material.dart';

import '../l10n/app_localizations.dart';
import '../src/rust/api/mumbleway.dart';
import '../state/app_state.dart';
import '../theme.dart';
import 'error_snack.dart';

/// The server's ban list, and the one place a ban can be lifted.
///
/// Reading the list needs the same permission as changing it, so anybody who
/// can open this can also act on it — there is no read-only version of this
/// screen to build.
///
/// **Lifting one ban writes the whole list back**, because the protocol has no
/// message for removing a single entry. That is the reason each row carries the
/// server's own copy of itself rather than anything rebuilt from what is drawn:
/// a field lost on the way through the interface would be a ban quietly
/// rewritten, and a row lost would be a ban quietly lifted.
class BanListDialog extends StatefulWidget {
  const BanListDialog({super.key, required this.serverId});

  final String serverId;

  @override
  State<BanListDialog> createState() => _BanListDialogState();
}

class _BanListDialogState extends State<BanListDialog> {
  bool _asked = false;

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    if (_asked) return;
    _asked = true;
    // Asked every time it is opened rather than cached: a ban list is edited
    // by whoever else is administering the server, and a stale one here would
    // be written back over their work.
    AppStateScope.of(context).loadBans(widget.serverId);
  }

  @override
  Widget build(BuildContext context) {
    final l = L.of(context);
    final state = AppStateScope.of(context);
    final bans = state.runtimeFor(widget.serverId).bans;

    return AlertDialog(
      title: Text(l.bannedUsers),
      content: SizedBox(
        width: 420,
        child: bans.isEmpty
            ? Text(l.noBans)
            : ListView.separated(
                shrinkWrap: true,
                itemCount: bans.length,
                separatorBuilder: (_, _) => const Divider(height: 1),
                itemBuilder: (_, i) => _BanRow(
                  serverId: widget.serverId,
                  ban: bans[i],
                ),
              ),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.pop(context),
          child: Text(l.close),
        ),
      ],
    );
  }
}

class _BanRow extends StatelessWidget {
  const _BanRow({required this.serverId, required this.ban});

  final String serverId;
  final UiBan ban;

  @override
  Widget build(BuildContext context) {
    final l = L.of(context);
    final state = AppStateScope.of(context);
    final details = [
      ban.address,
      if (ban.reason.isNotEmpty) ban.reason,
      if (ban.duration == 0) l.banPermanent else l.banLasts(ban.duration ~/ 60),
    ].join(' · ');

    return ListTile(
      dense: true,
      contentPadding: EdgeInsets.zero,
      title: Text(ban.name.isEmpty ? ban.address : ban.name),
      subtitle: Text(details, style: const TextStyle(fontSize: 12)),
      trailing: TextButton(
        onPressed: () async {
          final messenger = ScaffoldMessenger.of(context);
          final error = await state.liftBan(serverId, ban);
          if (error != null) showError(messenger, error);
        },
        child: Text(l.unban, style: const TextStyle(color: StatusColors.talking)),
      ),
    );
  }
}
