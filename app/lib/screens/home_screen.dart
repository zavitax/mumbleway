import 'package:flutter/material.dart';

import '../state/app_state.dart';
import '../theme.dart';
import '../widgets/ptt_button.dart';
import '../widgets/server_card.dart';
import 'add_server_screen.dart';
import 'settings_screen.dart';

class HomeScreen extends StatelessWidget {
  const HomeScreen({super.key});

  @override
  Widget build(BuildContext context) {
    final state = AppStateScope.of(context);

    if (state.startupError != null) {
      return _StartupFailure(message: state.startupError!);
    }
    if (!state.ready) {
      return const Scaffold(
        body: Center(child: CircularProgressIndicator()),
      );
    }

    return Scaffold(
      appBar: AppBar(
        title: const Text('MumbleWay'),
        actions: [
          IconButton(
            tooltip: state.deafened ? 'Undeafen' : 'Deafen',
            onPressed: state.toggleDeafen,
            icon: Icon(state.deafened
                ? Icons.hearing_disabled
                : Icons.hearing),
            color: state.deafened ? StatusColors.failed : null,
          ),
          IconButton(
            tooltip: state.muted ? 'Unmute microphone' : 'Mute microphone',
            onPressed: state.toggleMute,
            icon: Icon(state.muted ? Icons.mic_off : Icons.mic),
            color: state.muted ? StatusColors.failed : null,
          ),
          IconButton(
            tooltip: 'Settings',
            onPressed: () => Navigator.push(
              context,
              MaterialPageRoute(builder: (_) => const SettingsScreen()),
            ),
            icon: const Icon(Icons.settings),
          ),
        ],
      ),
      body: SafeArea(
        child: Column(
          children: [
            Expanded(
              child: state.servers.isEmpty
                  ? const _EmptyState()
                  : ListView(
                      padding: const EdgeInsets.only(top: 8, bottom: 12),
                      children: [
                        for (final s in state.servers) ServerCard(server: s),
                        if (state.canAddMore)
                          Padding(
                            padding: const EdgeInsets.fromLTRB(16, 8, 16, 0),
                            child: OutlinedButton.icon(
                              onPressed: () => _addServer(context),
                              icon: const Icon(Icons.add),
                              label: const Text('Add another server'),
                            ),
                          )
                        else
                          Padding(
                            padding: const EdgeInsets.fromLTRB(16, 12, 16, 0),
                            child: Text(
                              'Connected to the maximum of '
                              '${state.maxServers} servers.',
                              textAlign: TextAlign.center,
                              style: TextStyle(
                                fontSize: 12,
                                color: Theme.of(context)
                                    .colorScheme
                                    .onSurfaceVariant,
                              ),
                            ),
                          ),
                      ],
                    ),
            ),
            _TalkPanel(state: state),
          ],
        ),
      ),
      floatingActionButton: state.servers.isEmpty
          ? FloatingActionButton.extended(
              onPressed: () => _addServer(context),
              icon: const Icon(Icons.add),
              label: const Text('Add server'),
            )
          : null,
    );
  }

  Future<void> _addServer(BuildContext context) async {
    await Navigator.push(
      context,
      MaterialPageRoute(builder: (_) => const AddServerScreen()),
    );
  }
}

/// The permanently visible talk controls at the bottom of the screen.
class _TalkPanel extends StatelessWidget {
  const _TalkPanel({required this.state});
  final AppState state;

  @override
  Widget build(BuildContext context) {
    final live = state.runtimes.values.where((r) => r.isLive).length;

    return Container(
      padding: const EdgeInsets.fromLTRB(16, 12, 16, 16),
      decoration: BoxDecoration(
        color: Theme.of(context).colorScheme.surfaceContainerLow,
        borderRadius: const BorderRadius.vertical(top: Radius.circular(22)),
      ),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          const LevelMeter(),
          const SizedBox(height: 12),
          const PttButton(),
          const SizedBox(height: 8),
          Text(
            live == 0
                ? 'Not connected to any server'
                : live == 1
                    ? 'Talking on 1 server'
                    : 'Talking on $live servers simultaneously',
            style: TextStyle(
              fontSize: 12,
              color: Theme.of(context).colorScheme.onSurfaceVariant,
            ),
          ),
        ],
      ),
    );
  }
}

class _EmptyState extends StatelessWidget {
  const _EmptyState();

  @override
  Widget build(BuildContext context) {
    return Center(
      child: Padding(
        padding: const EdgeInsets.all(32),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Icon(Icons.headset_mic_outlined,
                size: 72,
                color: Theme.of(context).colorScheme.onSurfaceVariant),
            const SizedBox(height: 20),
            const Text(
              'No servers yet',
              style: TextStyle(fontSize: 20, fontWeight: FontWeight.w700),
            ),
            const SizedBox(height: 8),
            Text(
              'Add a Mumble server to start talking. You can stay connected to '
              'two at once.',
              textAlign: TextAlign.center,
              style:
                  TextStyle(color: Theme.of(context).colorScheme.onSurfaceVariant),
            ),
          ],
        ),
      ),
    );
  }
}

class _StartupFailure extends StatelessWidget {
  const _StartupFailure({required this.message});
  final String message;

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      body: Center(
        child: Padding(
          padding: const EdgeInsets.all(32),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              const Icon(Icons.mic_off, size: 64, color: StatusColors.failed),
              const SizedBox(height: 20),
              const Text(
                'Audio could not start',
                style: TextStyle(fontSize: 20, fontWeight: FontWeight.w700),
              ),
              const SizedBox(height: 10),
              Text(
                'MumbleWay needs a microphone. Check that one is connected and '
                'that permission is granted, then restart the app.',
                textAlign: TextAlign.center,
                style: TextStyle(
                    color: Theme.of(context).colorScheme.onSurfaceVariant),
              ),
              const SizedBox(height: 16),
              Text(
                message,
                textAlign: TextAlign.center,
                style: const TextStyle(fontSize: 11),
              ),
            ],
          ),
        ),
      ),
    );
  }
}
