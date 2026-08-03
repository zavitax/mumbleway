import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../l10n/app_localizations.dart';
import '../src/rust/api/mumbleway.dart';
import '../state/app_state.dart';
import '../widgets/app_bar_title.dart';
import '../widgets/language_button.dart';
import 'import_screen.dart';
import 'public_servers_screen.dart';

/// Form for adding a server, or editing one already saved.
///
/// One screen for both because the fields are identical and the difference is
/// entirely in what happens on save. Two screens would be two places to keep a
/// validation rule in step.
class AddServerScreen extends StatefulWidget {
  const AddServerScreen({super.key, this.existing});

  /// The server being edited, or null when adding a new one.
  final SavedServer? existing;

  @override
  State<AddServerScreen> createState() => _AddServerScreenState();
}

class _AddServerScreenState extends State<AddServerScreen> {
  final _form = GlobalKey<FormState>();
  final _name = TextEditingController();
  final _host = TextEditingController();
  final _port = TextEditingController();
  final _user = TextEditingController();
  final _password = TextEditingController();
  bool _saving = false;

  bool get _editing => widget.existing != null;

  @override
  void initState() {
    super.initState();
    final existing = widget.existing;
    if (existing == null) {
      _port.text = defaultPort().toString();
      return;
    }
    _name.text = existing.name;
    _host.text = existing.host;
    _port.text = existing.port.toString();
    _user.text = existing.username;
    _password.text = existing.password ?? '';
  }

  @override
  void dispose() {
    for (final c in [_name, _host, _port, _user, _password]) {
      c.dispose();
    }
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final l = L.of(context);
    return Scaffold(
      appBar: AppBar(
        title: AppBarTitle(
          _editing ? l.editServer : l.addServer,
          showIcon: false,
        ),
        actions: const [LanguageButton()],
      ),
      body: Form(
        key: _form,
        child: ListView(
          padding: const EdgeInsets.all(16),
          children: [
            // Only useful when starting from nothing; while editing they would
            // navigate away from unsaved changes.
            if (!_editing) ...[const _Shortcuts(), const SizedBox(height: 8)],
            TextFormField(
              controller: _name,
              textInputAction: TextInputAction.next,
              decoration: InputDecoration(
                labelText: l.displayName,
                hintText: l.displayNameHint,
                prefixIcon: const Icon(Icons.label_outline),
              ),
              validator: (v) =>
                  (v == null || v.trim().isEmpty) ? l.displayNameMissing : null,
            ),
            const SizedBox(height: 12),
            TextFormField(
              controller: _host,
              textInputAction: TextInputAction.next,
              autocorrect: false,
              keyboardType: TextInputType.url,
              decoration: InputDecoration(
                labelText: l.serverAddress,
                hintText: l.serverAddressHint,
                prefixIcon: const Icon(Icons.dns_outlined),
              ),
              validator: (v) => (v == null || v.trim().isEmpty)
                  ? l.serverAddressMissing
                  : null,
            ),
            const SizedBox(height: 12),
            TextFormField(
              controller: _port,
              textInputAction: TextInputAction.next,
              keyboardType: TextInputType.number,
              inputFormatters: [FilteringTextInputFormatter.digitsOnly],
              decoration: InputDecoration(
                labelText: l.port,
                prefixIcon: const Icon(Icons.numbers),
              ),
              validator: (v) {
                final p = int.tryParse(v ?? '');
                if (p == null || p < 1 || p > 65535) return l.portOutOfRange;
                return null;
              },
            ),
            const SizedBox(height: 12),
            TextFormField(
              controller: _user,
              textInputAction: TextInputAction.next,
              autocorrect: false,
              decoration: InputDecoration(
                labelText: l.username,
                prefixIcon: const Icon(Icons.person_outline),
              ),
              validator: (v) =>
                  (v == null || v.trim().isEmpty) ? l.usernameMissing : null,
            ),
            const SizedBox(height: 12),
            TextFormField(
              controller: _password,
              obscureText: true,
              decoration: InputDecoration(
                labelText: l.passwordOptional,
                helperText: l.passwordHelp,
                prefixIcon: const Icon(Icons.lock_outline),
              ),
            ),
            const SizedBox(height: 24),
            FilledButton.icon(
              onPressed: _saving ? null : _submit,
              icon: _saving
                  ? const SizedBox(
                      width: 18,
                      height: 18,
                      child: CircularProgressIndicator(strokeWidth: 2),
                    )
                  : const Icon(Icons.check),
              label: Text(_label(l)),
            ),
          ],
        ),
      ),
    );
  }

  String _label(L l) {
    if (_saving) return _editing ? l.savingChanges : l.addingServer;
    return _editing ? l.saveChanges : l.addServer;
  }

  Future<void> _submit() async {
    if (!_form.currentState!.validate()) return;
    setState(() => _saving = true);

    final state = AppStateScope.of(context);
    final existing = widget.existing;
    final draft = SavedServer(
      name: _name.text.trim(),
      host: _host.text.trim(),
      port: int.parse(_port.text),
      username: _user.text.trim(),
      password: _password.text.isEmpty ? null : _password.text,
      // Editing keeps the key and everything hanging off it; the pinned
      // certificate and default channel belong to the entry, not the form.
      localId: existing?.localId,
      certFingerprint: existing?.certFingerprint,
      defaultChannel: existing?.defaultChannel,
    );

    final error = existing == null
        ? await state.addNewServer(draft)
        : await state.updateServer(draft);

    if (!mounted) return;
    setState(() => _saving = false);

    if (error != null) {
      ScaffoldMessenger.of(
        context,
      ).showSnackBar(SnackBar(content: Text(error)));
      return;
    }
    Navigator.pop(context);
  }
}

/// Faster routes than typing an address by hand.
class _Shortcuts extends StatelessWidget {
  const _Shortcuts();

  @override
  Widget build(BuildContext context) {
    final l = L.of(context);
    return Card(
      margin: EdgeInsets.zero,
      child: Padding(
        padding: const EdgeInsets.all(14),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              l.quickerWays,
              style: const TextStyle(fontWeight: FontWeight.w700),
            ),
            const SizedBox(height: 10),
            Row(
              children: [
                Expanded(
                  child: OutlinedButton.icon(
                    onPressed: () => Navigator.push(
                      context,
                      MaterialPageRoute(
                        builder: (_) => const PublicServersScreen(),
                      ),
                    ),
                    icon: const Icon(Icons.public),
                    label: Text(l.browsePublic),
                  ),
                ),
                const SizedBox(width: 10),
                Expanded(
                  child: OutlinedButton.icon(
                    onPressed: () => Navigator.push(
                      context,
                      MaterialPageRoute(builder: (_) => const ImportScreen()),
                    ),
                    icon: const Icon(Icons.download),
                    label: Text(l.importLabel),
                  ),
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }
}
