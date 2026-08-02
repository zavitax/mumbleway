import 'package:flutter/material.dart';

import '../l10n/app_localizations.dart';
import '../state/app_state.dart';
import '../widgets/language_button.dart';

/// Browser for publicly listed Mumble servers.
///
/// Falls back to a small built-in list if the directory cannot be reached, and
/// says so, rather than showing an empty screen with no explanation.
class PublicServersScreen extends StatefulWidget {
  const PublicServersScreen({super.key});

  @override
  State<PublicServersScreen> createState() => _PublicServersScreenState();
}

class _PublicServersScreenState extends State<PublicServersScreen> {
  List<PublicServer>? _servers;
  bool _usedFallback = false;
  String _filter = '';
  String? _error;

  bool _loadStarted = false;

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    // Not initState: reading an InheritedWidget is only legal once
    // dependencies are established, and _load needs AppStateScope.
    if (!_loadStarted) {
      _loadStarted = true;
      _load();
    }
  }

  Future<void> _load() async {
    setState(() {
      _servers = null;
      _error = null;
    });
    try {
      final state = AppStateScope.of(context);
      final (list, fallback) = await state.fetchPublicServers();
      if (!mounted) return;
      setState(() {
        _servers = list;
        _usedFallback = fallback;
      });
    } catch (e) {
      if (!mounted) return;
      setState(() => _error = e.toString());
    }
  }

  @override
  Widget build(BuildContext context) {
    final all = _servers;
    final filtered = all
        ?.where((s) =>
            _filter.isEmpty ||
            s.name.toLowerCase().contains(_filter.toLowerCase()) ||
            s.host.toLowerCase().contains(_filter.toLowerCase()))
        .toList();

    return Scaffold(
      appBar: AppBar(
        title: Text(L.of(context).publicServers),
        actions: [
          const LanguageButton(),
          IconButton(
            onPressed: _load,
            icon: const Icon(Icons.refresh),
            tooltip: L.of(context).reload,
          ),
        ],
      ),
      body: Column(
        children: [
          if (_usedFallback && all != null)
            const _Notice(
              icon: Icons.info_outline,
              text: 'The public directory is not responding, so this is a '
                  'short built-in list. You can still add any server by '
                  'address or link.',
            ),
          Padding(
            padding: const EdgeInsets.fromLTRB(16, 12, 16, 8),
            child: TextField(
              decoration: InputDecoration(
                hintText: L.of(context).search,
                prefixIcon: const Icon(Icons.search),
              ),
              onChanged: (v) => setState(() => _filter = v),
            ),
          ),
          Expanded(
            child: switch ((filtered, _error)) {
              (_, final String e) => _Notice(
                  icon: Icons.error_outline,
                  text: 'Could not load the list: $e',
                ),
              (null, _) => const Center(child: CircularProgressIndicator()),
              (final List<PublicServer> list, _) when list.isEmpty => _Notice(
                  icon: Icons.search_off,
                  text: L.of(context).noServersMatchSearch,
                ),
              (final List<PublicServer> list, _) => ListView.builder(
                  itemCount: list.length,
                  itemBuilder: (_, i) => _PublicServerTile(server: list[i]),
                ),
            },
          ),
        ],
      ),
    );
  }
}

class _PublicServerTile extends StatefulWidget {
  const _PublicServerTile({required this.server});
  final PublicServer server;

  @override
  State<_PublicServerTile> createState() => _PublicServerTileState();
}

class _PublicServerTileState extends State<_PublicServerTile> {
  bool _adding = false;

  @override
  Widget build(BuildContext context) {
    final s = widget.server;
    return ListTile(
      leading: const Icon(Icons.public),
      title: Text(s.name, overflow: TextOverflow.ellipsis),
      subtitle: Text(
        s.country.isEmpty
            ? '${s.host}:${s.port}'
            : '${s.host}:${s.port}  ·  ${s.country}',
        overflow: TextOverflow.ellipsis,
      ),
      trailing: _adding
          ? const SizedBox(
              width: 20,
              height: 20,
              child: CircularProgressIndicator(strokeWidth: 2),
            )
          : IconButton(
              icon: const Icon(Icons.add_circle_outline),
              tooltip: L.of(context).addToMyServers,
              onPressed: _add,
            ),
      onTap: _adding ? null : _add,
    );
  }

  Future<void> _add() async {
    final state = AppStateScope.of(context);
    final messenger = ScaffoldMessenger.of(context);

    // Public servers need a username, and the list does not carry one; ask
    // rather than inventing one that a server may reject as taken.
    final username = await showDialog<String>(
      context: context,
      builder: (_) => const _UsernameDialog(),
    );
    if (username == null || username.trim().isEmpty) return;

    setState(() => _adding = true);
    final error = await state.addNewServer(SavedServer(
      name: widget.server.name,
      host: widget.server.host,
      port: widget.server.port,
      username: username.trim(),
    ));
    if (!mounted) return;
    setState(() => _adding = false);

    messenger.showSnackBar(SnackBar(
      content: Text(error ?? 'Added ${widget.server.name}'),
    ));
  }
}

class _UsernameDialog extends StatefulWidget {
  const _UsernameDialog();

  @override
  State<_UsernameDialog> createState() => _UsernameDialogState();
}

class _UsernameDialogState extends State<_UsernameDialog> {
  final _controller = TextEditingController();

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return AlertDialog(
      title: const Text('Choose a username'),
      content: TextField(
        controller: _controller,
        autofocus: true,
        decoration: const InputDecoration(
          labelText: 'Username',
          helperText: 'How others on the server will see you',
        ),
        onSubmitted: (v) => Navigator.pop(context, v),
      ),
      actions: [
        TextButton(
            onPressed: () => Navigator.pop(context), child: const Text('Cancel')),
        FilledButton(
          onPressed: () => Navigator.pop(context, _controller.text),
          child: Text(L.of(context).add),
        ),
      ],
    );
  }
}

class _Notice extends StatelessWidget {
  const _Notice({required this.icon, required this.text});
  final IconData icon;
  final String text;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.all(16),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Icon(icon, size: 18),
          const SizedBox(width: 10),
          Expanded(child: Text(text, style: const TextStyle(fontSize: 12))),
        ],
      ),
    );
  }
}
