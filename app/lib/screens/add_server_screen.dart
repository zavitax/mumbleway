import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../src/rust/api/mumbleway.dart';
import '../state/app_state.dart';

/// Form for adding a server, with a few well-known public servers offered as
/// shortcuts so a new user has something to try immediately.
class AddServerScreen extends StatefulWidget {
  const AddServerScreen({super.key});

  @override
  State<AddServerScreen> createState() => _AddServerScreenState();
}

class _AddServerScreenState extends State<AddServerScreen> {
  final _form = GlobalKey<FormState>();
  final _name = TextEditingController();
  final _host = TextEditingController();
  final _port = TextEditingController(text: '64738');
  final _user = TextEditingController();
  final _password = TextEditingController();
  bool _saving = false;

  @override
  void initState() {
    super.initState();
    _port.text = defaultPort().toString();
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
    return Scaffold(
      appBar: AppBar(title: const Text('Add server')),
      body: Form(
        key: _form,
        child: ListView(
          padding: const EdgeInsets.all(16),
          children: [
            const _PublicServerPicker(),
            const SizedBox(height: 8),
            TextFormField(
              controller: _name,
              textInputAction: TextInputAction.next,
              decoration: const InputDecoration(
                labelText: 'Display name',
                hintText: 'Sunday ride',
                prefixIcon: Icon(Icons.label_outline),
              ),
              validator: (v) =>
                  (v == null || v.trim().isEmpty) ? 'Give it a name' : null,
            ),
            const SizedBox(height: 12),
            TextFormField(
              controller: _host,
              textInputAction: TextInputAction.next,
              autocorrect: false,
              keyboardType: TextInputType.url,
              decoration: const InputDecoration(
                labelText: 'Server address',
                hintText: 'mumble.example.com',
                prefixIcon: Icon(Icons.dns_outlined),
              ),
              validator: (v) =>
                  (v == null || v.trim().isEmpty) ? 'Enter an address' : null,
            ),
            const SizedBox(height: 12),
            TextFormField(
              controller: _port,
              textInputAction: TextInputAction.next,
              keyboardType: TextInputType.number,
              inputFormatters: [FilteringTextInputFormatter.digitsOnly],
              decoration: const InputDecoration(
                labelText: 'Port',
                prefixIcon: Icon(Icons.numbers),
              ),
              validator: (v) {
                final p = int.tryParse(v ?? '');
                if (p == null || p < 1 || p > 65535) return 'Port 1-65535';
                return null;
              },
            ),
            const SizedBox(height: 12),
            TextFormField(
              controller: _user,
              textInputAction: TextInputAction.next,
              autocorrect: false,
              decoration: const InputDecoration(
                labelText: 'Username',
                prefixIcon: Icon(Icons.person_outline),
              ),
              validator: (v) =>
                  (v == null || v.trim().isEmpty) ? 'Enter a username' : null,
            ),
            const SizedBox(height: 12),
            TextFormField(
              controller: _password,
              obscureText: true,
              decoration: const InputDecoration(
                labelText: 'Password (optional)',
                helperText: 'Only if the server requires one',
                prefixIcon: Icon(Icons.lock_outline),
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
              label: Text(_saving ? 'Adding…' : 'Add server'),
            ),
          ],
        ),
      ),
    );
  }

  Future<void> _submit() async {
    if (!_form.currentState!.validate()) return;
    setState(() => _saving = true);

    final state = AppStateScope.of(context);
    final error = await state.addNewServer(SavedServer(
      name: _name.text.trim(),
      host: _host.text.trim(),
      port: int.parse(_port.text),
      username: _user.text.trim(),
      password: _password.text.isEmpty ? null : _password.text,
    ));

    if (!mounted) return;
    setState(() => _saving = false);

    if (error != null) {
      ScaffoldMessenger.of(context)
          .showSnackBar(SnackBar(content: Text(error)));
      return;
    }
    Navigator.pop(context);
  }
}

/// Well-known open servers, to make the first run painless.
class _PublicServerPicker extends StatelessWidget {
  const _PublicServerPicker();

  static const _entries = [
    ('Mumble.info (official test)', 'mumble.info', 64738),
    ('GetMumble EU', 'eu.getmumble.com', 64738),
    ('GetMumble US', 'us.getmumble.com', 64738),
  ];

  @override
  Widget build(BuildContext context) {
    return Card(
      margin: EdgeInsets.zero,
      child: Padding(
        padding: const EdgeInsets.all(14),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            const Text('Public servers',
                style: TextStyle(fontWeight: FontWeight.w700)),
            const SizedBox(height: 4),
            Text(
              'Tap to fill in the address, then choose a username.',
              style: TextStyle(
                fontSize: 12,
                color: Theme.of(context).colorScheme.onSurfaceVariant,
              ),
            ),
            const SizedBox(height: 10),
            Wrap(
              spacing: 8,
              runSpacing: 8,
              children: _entries
                  .map((e) => ActionChip(
                        avatar: const Icon(Icons.public, size: 16),
                        label: Text(e.$1),
                        onPressed: () {
                          final s = context
                              .findAncestorStateOfType<_AddServerScreenState>();
                          if (s == null) return;
                          s._name.text = e.$1;
                          s._host.text = e.$2;
                          s._port.text = e.$3.toString();
                        },
                      ))
                  .toList(),
            ),
          ],
        ),
      ),
    );
  }
}
