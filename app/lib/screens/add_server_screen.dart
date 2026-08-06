import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../l10n/app_localizations.dart';
import '../src/rust/api/mumbleway.dart';
import '../state/app_state.dart';
import '../widgets/app_bar_title.dart';
import '../widgets/language_button.dart';
import 'import_screen.dart';
import '../widgets/qr_intake_button.dart';
import 'public_servers_screen.dart';

/// Form for adding a server, or editing one already saved.
///
/// One screen for both because the fields are identical and the difference is
/// entirely in what happens on save. Two screens would be two places to keep a
/// validation rule in step.
class AddServerScreen extends StatefulWidget {
  const AddServerScreen({super.key, this.existing, this.prefill});

  /// The server being edited, or null when adding a new one.
  final SavedServer? existing;

  /// Details to start a *new* entry from, rather than one to edit.
  ///
  /// What arrives from a scanned code or a `mumble://` link followed from
  /// outside the app. Deliberately not [existing]: nothing has been saved yet,
  /// the form is still a draft, and the rider can change any of it — or back
  /// out — before anything is written. A link is a suggestion, not an
  /// instruction.
  final SavedServer? prefill;

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

  /// Where the form's contents came from.
  ///
  /// Kept because the entry carries things this form has no field for — the
  /// default channel, the pinned certificate — and they have to survive a save.
  /// It is not simply `widget.prefill`: a code scanned from the shortcuts row
  /// refills the form in place, and the channel from *that* invitation is the
  /// one that should be saved.
  SavedServer? _source;

  @override
  void initState() {
    super.initState();
    final source = widget.existing ?? widget.prefill;
    _source = source;
    if (source == null) {
      _port.text = defaultPort().toString();
      // A name to connect under, so the field is not simply blank and in the
      // way. Derived from the device rather than shared or invented, and only
      // a suggestion — it is an ordinary editable field.
      unawaited(_suggestUsername());
      return;
    }
    _name.text = source.name;
    _host.text = source.host;
    _port.text = source.port.toString();
    _user.text = source.username;
    _password.text = source.password ?? '';
  }

  /// Fills the username field with the device's own suggestion.
  ///
  /// Asynchronous because the answer comes from the platform, so the field is
  /// briefly empty. Only filled if it still is: a rider who types faster than
  /// a method channel answers should not have their name replaced.
  Future<void> _suggestUsername() async {
    final name = await AppStateScope.of(context).suggestedUsername();
    if (!mounted || _user.text.isNotEmpty) return;
    _user.text = name;
  }

  /// Refills the form from a scanned invitation.
  ///
  /// In place rather than on a new screen. This form is where the scan was
  /// started from, so a second copy of it stacked on top is one the rider has
  /// to save and then back out of — and saving popped them onto the empty
  /// first copy rather than onto their server list.
  void _fillFrom(SavedServer server) {
    setState(() {
      _source = server;
      _name.text = server.name;
      _host.text = server.host;
      _port.text = server.port.toString();
      _user.text = server.username;
      _password.text = server.password ?? '';
    });
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
            if (!_editing) ...[
              _Shortcuts(onInvitation: _fillFrom),
              const SizedBox(height: 8),
            ],
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
      // The channel has no field on this form, so it rides along from wherever
      // the entry came from: the saved one when editing, and the link's own
      // when a code or an invitation filled the form in.
      defaultChannel: existing?.defaultChannel ?? _source?.defaultChannel,
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
  const _Shortcuts({required this.onInvitation});

  /// Called with the server a scanned code described, to fill the form this
  /// row sits on.
  final ValueChanged<SavedServer> onInvitation;

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
            // Sized to the tallest of the three rather than each to its own
            // content. Whether a label wraps depends on the width left over
            // and on the language — "Browse public" takes two lines on a
            // narrow phone and one on a wide one — and a row where one button
            // is visibly shorter than its neighbours reads as a mistake, not
            // as a smaller control.
            IntrinsicHeight(
              child: Row(
                crossAxisAlignment: CrossAxisAlignment.stretch,
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
                  const SizedBox(width: 10),
                  // Not Expanded: the other two carry words and need the room,
                  // and a third equal share would squeeze all three. This one
                  // is a glyph and asks for only what a glyph needs.
                  QrIntakeButton(onInvitation: onInvitation),
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }
}
