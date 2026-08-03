import 'package:flutter/material.dart';

import '../l10n/app_localizations.dart';
import '../state/app_state.dart';
import '../widgets/language_button.dart';

/// Adds servers from a shared link or a downloadable profile file.
///
/// Two routes because invitations arrive both ways: a `mumble://` link pasted
/// into a chat, or a JSON profile file hosted somewhere for a group to share.
class ImportScreen extends StatefulWidget {
  const ImportScreen({super.key});

  @override
  State<ImportScreen> createState() => _ImportScreenState();
}

class _ImportScreenState extends State<ImportScreen> {
  final _text = TextEditingController();
  final _url = TextEditingController();
  bool _busy = false;

  @override
  void dispose() {
    _text.dispose();
    _url.dispose();
    super.dispose();
  }

  Future<void> _run(Future<String?> Function() action) async {
    setState(() => _busy = true);
    final error = await action();
    if (!mounted) return;
    setState(() => _busy = false);

    ScaffoldMessenger.of(context).showSnackBar(
      SnackBar(content: Text(error ?? L.of(context).serversAdded)),
    );
    if (error == null) Navigator.pop(context);
  }

  @override
  Widget build(BuildContext context) {
    final state = AppStateScope.of(context);

    return Scaffold(
      appBar: AppBar(
        title: Text(L.of(context).importServers),
        actions: const [LanguageButton()],
      ),
      body: ListView(
        padding: const EdgeInsets.all(16),
        children: [
          const _Header('Paste a link or profile'),
          const _Hint(
            'A mumble:// invitation link, or the contents of a JSON profile '
            'file. A profile file may hold several servers at once.',
          ),
          TextField(
            controller: _text,
            minLines: 3,
            maxLines: 8,
            autocorrect: false,
            decoration: const InputDecoration(
              hintText: 'mumble://user@voice.example.com:64738/Riders',
            ),
          ),
          const SizedBox(height: 12),
          FilledButton.icon(
            onPressed: _busy || _text.text.trim().isEmpty
                ? null
                : () => _run(() => state.importFromText(_text.text)),
            icon: const Icon(Icons.playlist_add),
            label: Text(L.of(context).addFromText),
          ),

          const SizedBox(height: 28),
          const _Header('Download a profile file'),
          const _Hint(
            'Fetches a JSON profile file from a web address and adds every '
            'server it contains.',
          ),
          TextField(
            controller: _url,
            autocorrect: false,
            keyboardType: TextInputType.url,
            decoration: const InputDecoration(
              hintText: 'https://example.com/riders.json',
              prefixIcon: Icon(Icons.link),
            ),
          ),
          const SizedBox(height: 12),
          FilledButton.icon(
            onPressed: _busy || _url.text.trim().isEmpty
                ? null
                : () => _run(() => state.importFromUrl(_url.text)),
            icon: _busy
                ? const SizedBox(
                    width: 18,
                    height: 18,
                    child: CircularProgressIndicator(strokeWidth: 2),
                  )
                : const Icon(Icons.download),
            label: const Text('Download and add'),
          ),

          const SizedBox(height: 28),
          _Header(L.of(context).profileFileFormat),
          const _Hint(
            'Either a single object or an array of them. Only the '
            'host is required.',
          ),
          Container(
            padding: const EdgeInsets.all(12),
            decoration: BoxDecoration(
              color: Theme.of(context).colorScheme.surfaceContainerHighest,
              borderRadius: BorderRadius.circular(12),
            ),
            child: const Text(
              '[\n'
              '  {\n'
              '    "name": "Sunday Ride",\n'
              '    "host": "voice.example.com",\n'
              '    "port": 64738,\n'
              '    "username": "rider",\n'
              '    "channel": "Riders"\n'
              '  }\n'
              ']',
              style: TextStyle(fontFamily: 'monospace', fontSize: 11),
            ),
          ),
        ],
      ),
    );
  }
}

class _Header extends StatelessWidget {
  const _Header(this.text);
  final String text;

  @override
  Widget build(BuildContext context) => Padding(
    padding: const EdgeInsets.only(bottom: 4),
    child: Text(
      text,
      style: const TextStyle(fontSize: 15, fontWeight: FontWeight.w700),
    ),
  );
}

class _Hint extends StatelessWidget {
  const _Hint(this.text);
  final String text;

  @override
  Widget build(BuildContext context) => Padding(
    padding: const EdgeInsets.only(bottom: 10),
    child: Text(
      text,
      style: TextStyle(
        fontSize: 12,
        color: Theme.of(context).colorScheme.onSurfaceVariant,
      ),
    ),
  );
}
