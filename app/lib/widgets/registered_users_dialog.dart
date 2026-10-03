import 'package:flutter/material.dart';

import '../l10n/app_localizations.dart';
import '../src/rust/api/mumbleway.dart';
import '../state/app_state.dart';
import '../theme.dart';
import 'error_snack.dart';

/// Everybody this server has an account for.
///
/// Registration is what makes a name belong to somebody: an unregistered name
/// is free for anyone to take the moment its holder disconnects, and no ACL can
/// name them. This is the admin's view of that list, and the only place an
/// account can be taken away.
///
/// **Removing one is a list, not a message.** The protocol has no "unregister"
/// — a `UserList` carrying that user with no name is how it is spelled — so the
/// list is re-read from the server afterwards rather than edited here, and what
/// the screen shows is always what the server last said.
class RegisteredUsersDialog extends StatefulWidget {
  const RegisteredUsersDialog({super.key, required this.serverId});

  final String serverId;

  @override
  State<RegisteredUsersDialog> createState() => _RegisteredUsersDialogState();
}

class _RegisteredUsersDialogState extends State<RegisteredUsersDialog> {
  bool _asked = false;

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    if (_asked) return;
    _asked = true;
    AppStateScope.of(context).loadRegistered(widget.serverId);
  }

  @override
  Widget build(BuildContext context) {
    final l = L.of(context);
    final state = AppStateScope.of(context);
    final users = state.runtimeFor(widget.serverId).registered;

    return AlertDialog(
      title: Text(l.registeredUsers),
      content: SizedBox(
        width: 420,
        child: users.isEmpty
            ? Text(l.registeredNobody)
            : ListView.separated(
                shrinkWrap: true,
                itemCount: users.length,
                separatorBuilder: (_, _) => const Divider(height: 1),
                itemBuilder: (_, i) => _Row(
                  serverId: widget.serverId,
                  user: users[i],
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

class _Row extends StatelessWidget {
  const _Row({required this.serverId, required this.user});

  final String serverId;
  final UiRegisteredUser user;

  @override
  Widget build(BuildContext context) {
    final l = L.of(context);
    final state = AppStateScope.of(context);
    return ListTile(
      dense: true,
      contentPadding: EdgeInsets.zero,
      title: Text(user.name.isEmpty ? '#${user.userId}' : user.name),
      subtitle: user.lastSeen.isEmpty
          ? null
          : Text(
              l.registeredLastSeen(user.lastSeen),
              style: const TextStyle(fontSize: 12),
            ),
      trailing: TextButton(
        onPressed: () async {
          final messenger = ScaffoldMessenger.of(context);
          final error = await state.unregister(serverId, user.userId);
          if (error != null) showError(messenger, error);
        },
        child: Text(
          l.registeredUnregister,
          style: const TextStyle(color: StatusColors.failed),
        ),
      ),
    );
  }
}
