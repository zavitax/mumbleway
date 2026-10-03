import 'package:flutter/material.dart';

import '../l10n/app_localizations.dart';
import '../state/app_state.dart';
import '../theme.dart';
import 'error_snack.dart';

/// The passwords this rider holds for a server's closed channels.
///
/// **A Mumble channel has no password.** It has a rule granting entry to a
/// group, written `#name`, and a token is a string that puts the holder in the
/// group of that name. So what a rider is handed as "the password for the
/// clubhouse" is a token, and typing it here is what opens the channel.
///
/// They are kept with the server and presented again on every connect, because
/// a token that has to be retyped at a junction is a token the rider does not
/// have. They take effect at once: the server re-reads them and re-evaluates
/// every channel, so a shut channel opens without reconnecting.
class AccessTokensDialog extends StatefulWidget {
  const AccessTokensDialog({super.key, required this.server});

  final SavedServer server;

  @override
  State<AccessTokensDialog> createState() => _AccessTokensDialogState();
}

class _AccessTokensDialogState extends State<AccessTokensDialog> {
  late List<String> _tokens = [...widget.server.accessTokens];
  final _field = TextEditingController();

  @override
  void dispose() {
    _field.dispose();
    super.dispose();
  }

  void _add() {
    final token = _field.text.trim();
    // Duplicates are not an error worth a message; they are simply not two
    // tokens, and the server would read them as one.
    if (token.isEmpty || _tokens.contains(token)) return;
    setState(() {
      _tokens = [..._tokens, token];
      _field.clear();
    });
  }

  @override
  Widget build(BuildContext context) {
    final l = L.of(context);
    final state = AppStateScope.of(context);

    return AlertDialog(
      title: Text(l.accessTokens),
      content: SizedBox(
        width: 420,
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(l.accessTokensBody, style: const TextStyle(fontSize: 13)),
            const SizedBox(height: 12),
            Row(
              children: [
                Expanded(
                  child: TextField(
                    controller: _field,
                    autofocus: true,
                    onSubmitted: (_) => _add(),
                    decoration: InputDecoration(hintText: l.accessTokenHint),
                  ),
                ),
                const SizedBox(width: 8),
                FilledButton(onPressed: _add, child: Text(l.accessTokenAdd)),
              ],
            ),
            const SizedBox(height: 8),
            if (_tokens.isEmpty)
              Text(l.accessTokensNone, style: const TextStyle(fontSize: 12))
            else
              for (final token in _tokens)
                ListTile(
                  dense: true,
                  contentPadding: EdgeInsets.zero,
                  // Shown rather than hidden behind dots: a token is shared
                  // aloud in a group and a rider has to be able to check they
                  // typed the one they were told.
                  title: Text(token),
                  trailing: IconButton(
                    tooltip: l.accessTokenRemove,
                    icon: const Icon(
                      Icons.delete_outline,
                      color: StatusColors.failed,
                      size: 18,
                    ),
                    onPressed: () =>
                        setState(() => _tokens = [..._tokens]..remove(token)),
                  ),
                ),
          ],
        ),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.pop(context),
          child: Text(l.cancel),
        ),
        FilledButton(
          onPressed: () async {
            final messenger = ScaffoldMessenger.of(context);
            final navigator = Navigator.of(context);
            final error = await state.setAccessTokensFor(
              widget.server.id,
              _tokens,
            );
            navigator.pop();
            if (error != null) showError(messenger, error);
          },
          child: Text(l.save),
        ),
      ],
    );
  }
}
