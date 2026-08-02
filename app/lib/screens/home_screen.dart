import 'package:flutter/material.dart';

import '../l10n/app_localizations.dart';
import '../state/app_state.dart';
import '../theme.dart';
import '../widgets/app_bar_title.dart';
import '../widgets/language_button.dart';
import '../widgets/ptt_button.dart';
import '../widgets/server_card.dart';
import '../widgets/server_detail_pane.dart';
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
      return const Scaffold(body: Center(child: CircularProgressIndicator()));
    }

    final l = L.of(context);

    return Scaffold(
      appBar: AppBar(
        title: AppBarTitle(l.appTitle),
        actions: [
          const LanguageButton(),
          IconButton(
            tooltip: state.deafened ? l.undeafen : l.deafen,
            onPressed: state.toggleDeafen,
            icon: Icon(state.deafened ? Icons.hearing_disabled : Icons.hearing),
            color: state.deafened ? StatusColors.failed : null,
          ),
          IconButton(
            tooltip: state.muted ? l.unmuteMicrophone : l.muteMicrophone,
            onPressed: state.toggleMute,
            icon: Icon(state.muted ? Icons.mic_off : Icons.mic),
            color: state.muted ? StatusColors.failed : null,
          ),
          PopupMenuButton<String>(
            tooltip: l.more,
            icon: const Icon(Icons.more_vert),
            onSelected: (v) async {
              final messenger = ScaffoldMessenger.of(context);
              switch (v) {
                case 'export':
                  final e = await state.exportServersToFile();
                  if (e != null) {
                    messenger.showSnackBar(SnackBar(content: Text(e)));
                  }
                case 'import':
                  final e = await state.importServersFromFile();
                  if (e != null) {
                    messenger.showSnackBar(SnackBar(content: Text(e)));
                  }
                case 'settings':
                  if (!context.mounted) return;
                  await Navigator.push(
                    context,
                    MaterialPageRoute(builder: (_) => const SettingsScreen()),
                  );
              }
            },
            itemBuilder: (_) => [
              PopupMenuItem(
                value: 'export',
                child: ListTile(
                  dense: true,
                  contentPadding: EdgeInsets.zero,
                  leading: const Icon(Icons.upload_file),
                  title: Text(l.exportServers),
                ),
              ),
              PopupMenuItem(
                value: 'import',
                child: ListTile(
                  dense: true,
                  contentPadding: EdgeInsets.zero,
                  leading: const Icon(Icons.file_open),
                  title: Text(l.importFromFile),
                ),
              ),
              const PopupMenuDivider(),
              PopupMenuItem(
                value: 'settings',
                child: ListTile(
                  dense: true,
                  contentPadding: EdgeInsets.zero,
                  leading: const Icon(Icons.settings),
                  title: Text(l.settings),
                ),
              ),
            ],
          ),
        ],
      ),
      body: SafeArea(
        child: LayoutBuilder(
          builder: (context, constraints) {
            // Above the breakpoint the extra width goes to a detail pane rather
            // than to stretching cards that gain nothing from being wider.
            final wide = constraints.maxWidth >= kWideLayoutBreakpoint;
            return wide ? _WideBody(state: state) : _NarrowBody(state: state);
          },
        ),
      ),
      floatingActionButton: state.servers.isEmpty
          ? FloatingActionButton.extended(
              onPressed: () => _addServer(context),
              icon: const Icon(Icons.add),
              label: Text(l.addServer),
            )
          : null,
    );
  }

  static Future<void> _addServer(BuildContext context) async {
    await Navigator.push(
      context,
      MaterialPageRoute(builder: (_) => const AddServerScreen()),
    );
  }
}

/// Phone layout: one column, cards expand inline.
class _NarrowBody extends StatelessWidget {
  const _NarrowBody({required this.state});
  final AppState state;

  @override
  Widget build(BuildContext context) {
    return Column(
      children: [
        Expanded(
          child: state.servers.isEmpty
              ? const _EmptyState()
              : _ServerList(state: state, showDetails: true),
        ),
        _TalkPanel(state: state),
      ],
    );
  }
}

/// Tablet and wide-window layout: a master list beside a detail pane.
class _WideBody extends StatelessWidget {
  const _WideBody({required this.state});
  final AppState state;

  @override
  Widget build(BuildContext context) {
    return Row(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        SizedBox(
          // Wide enough for a card to stay readable, narrow enough to leave the
          // detail pane the majority of the space.
          width: 400,
          child: Column(
            children: [
              Expanded(
                child: state.servers.isEmpty
                    ? const _EmptyState()
                    : _ServerList(state: state, showDetails: false),
              ),
              _TalkPanel(state: state),
            ],
          ),
        ),
        const VerticalDivider(width: 1),
        Expanded(child: ServerDetailPane(server: state.selectedServer)),
      ],
    );
  }
}

class _ServerList extends StatelessWidget {
  const _ServerList({required this.state, required this.showDetails});

  final AppState state;
  final bool showDetails;

  @override
  Widget build(BuildContext context) {
    return ListView(
      padding: const EdgeInsets.only(top: 8, bottom: 12),
      children: [
        for (final s in state.servers)
          ServerCard(
            server: s,
            showDetails: showDetails,
            selected: !showDetails && state.selectedServerId == s.id,
            onTap: showDetails ? null : () => state.selectServer(s.id),
          ),
        Padding(
          padding: const EdgeInsets.fromLTRB(16, 8, 16, 0),
          child: OutlinedButton.icon(
            onPressed: () => HomeScreen._addServer(context),
            icon: const Icon(Icons.add),
            label: Text(L.of(context).addAnotherServer),
          ),
        ),
        if (!state.canAddMore)
          Padding(
            padding: const EdgeInsets.fromLTRB(16, 10, 16, 0),
            child: Text(
              L.of(context).maxServersNote(state.maxServers),
              textAlign: TextAlign.center,
              style: TextStyle(
                fontSize: 11,
                color: Theme.of(context).colorScheme.onSurfaceVariant,
              ),
            ),
          ),
      ],
    );
  }
}

/// The permanently visible talk controls.
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
          // The talk button only exists for push-to-talk. In the automatic
          // modes it is a status light the meter already provides, so the
          // vertical space goes back to the server list.
          if (state.showTalkButton) ...[
            const SizedBox(height: 12),
            const PttButton(),
          ],
          const SizedBox(height: 8),
          Text(
            live == 0
                ? L.of(context).notConnectedAny
                : live == 1
                    ? L.of(context).talkingOnOne
                    : L.of(context).talkingOnMany(live),
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
            Text(
              L.of(context).noServersTitle,
              style: const TextStyle(fontSize: 20, fontWeight: FontWeight.w700),
            ),
            const SizedBox(height: 8),
            Text(
              L.of(context).noServersBody,
              textAlign: TextAlign.center,
              style: TextStyle(
                  color: Theme.of(context).colorScheme.onSurfaceVariant),
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
              Text(
                L.of(context).audioFailedTitle,
                style: const TextStyle(fontSize: 20, fontWeight: FontWeight.w700),
              ),
              const SizedBox(height: 10),
              Text(
                L.of(context).audioFailedBody,
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
