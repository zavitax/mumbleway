import 'package:flutter/material.dart';

import '../l10n/app_localizations.dart';
import '../state/app_state.dart';
import '../theme.dart';
import '../widgets/app_bar_title.dart';
import '../widgets/diagnostics_panel.dart';
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
            tooltip: l.diagnostics,
            onPressed: state.toggleDiagnostics,
            icon: const Icon(Icons.monitor_heart_outlined),
            color: state.diagnosticsOpen
                ? Theme.of(context).colorScheme.primary
                : null,
          ),
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
        child: Stack(
          children: [
            LayoutBuilder(
              builder: (context, constraints) {
                // Above the breakpoint the extra width goes to a detail pane
                // rather than to stretching cards that gain nothing from being
                // wider.
                final wide = constraints.maxWidth >= kWideLayoutBreakpoint;
                return wide
                    ? _WideBody(state: state)
                    : _NarrowBody(state: state);
              },
            ),
            // Slides up over the content rather than displacing it: the panel
            // is consulted while something is going wrong, and shifting the
            // whole screen to read it would move the very thing being watched.
            Positioned(
              left: 0,
              right: 0,
              bottom: 0,
              child: AnimatedSlide(
                duration: const Duration(milliseconds: 220),
                curve: Curves.easeOutCubic,
                // A whole panel-height down, whatever that height happens to
                // be. A fixed offset would leave a tall panel peeking above
                // the edge and a short one travelling further than it needs.
                offset: state.diagnosticsOpen
                    ? Offset.zero
                    : const Offset(0, 1),
                child: IgnorePointer(
                  // Otherwise the hidden panel keeps swallowing taps meant for
                  // the talk button underneath it.
                  ignoring: !state.diagnosticsOpen,
                  child: ConstrainedBox(
                    constraints: BoxConstraints(
                      maxHeight: MediaQuery.of(context).size.height * 0.7,
                    ),
                    child: DiagnosticsPanel(onClose: state.toggleDiagnostics),
                  ),
                ),
              ),
            ),
          ],
        ),
      ),
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
          // With the microphone shut there is nothing for a meter to show and
          // nothing for the talk button to key. Drawing them anyway gives a
          // bar that never moves and a control that does nothing, which reads
          // as an app that has broken rather than one that is idle — so the
          // panel says what it is waiting for instead.
          if (!state.audioActive)
            const _MicIdleNotice()
          else ...[
            const LevelMeter(),
            // The talk button only exists for push-to-talk. In the automatic
            // modes it is a status light the meter already provides, so the
            // vertical space goes back to the server list.
            if (state.showTalkButton) ...[
              const SizedBox(height: 12),
              const PttButton(),
            ],
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

/// Stands in for the meter and the talk button while the microphone is shut.
///
/// Two sentences rather than one. The first says what will appear and when,
/// because a rider looking at a panel that used to hold a large button wants
/// to know it is coming back. The second says why it is not there now — the
/// microphone being closed is the whole point of the change, and left
/// unexplained it looks like something failed to load.
class _MicIdleNotice extends StatelessWidget {
  const _MicIdleNotice();

  @override
  Widget build(BuildContext context) {
    final l = L.of(context);
    final state = AppStateScope.of(context);
    final muted = Theme.of(context).colorScheme.onSurfaceVariant;

    return Padding(
      padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 6),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Icon(Icons.mic_off, size: 18, color: muted),
          const SizedBox(width: 10),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  // Only promises the button to someone who has chosen the
                  // mode that has one. In the hands-free modes it never
                  // appears, and saying it would be a small lie repeated on
                  // every screen.
                  state.showTalkButton
                      ? l.micIdleWithTalkButton
                      : l.micIdleMeterOnly,
                  style: TextStyle(fontSize: 13, color: muted),
                ),
                const SizedBox(height: 4),
                Text(
                  l.micIdleWhy,
                  style: TextStyle(
                    fontSize: 11,
                    color: muted.withValues(alpha: 0.75),
                  ),
                ),
              ],
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
            Icon(
              Icons.headset_mic_outlined,
              size: 72,
              color: Theme.of(context).colorScheme.onSurfaceVariant,
            ),
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
                color: Theme.of(context).colorScheme.onSurfaceVariant,
              ),
            ),
            const SizedBox(height: 24),
            // In the flow rather than floating over it. A floating button is
            // positioned against the window, not against the content, so it sat
            // on top of the talk panel — covering the one control that has to
            // be reachable without looking. Inline, it is in the same place the
            // "add another" button appears once there is a list, so the two
            // states do not move it around.
            FilledButton.icon(
              onPressed: () => HomeScreen._addServer(context),
              icon: const Icon(Icons.add),
              label: Text(L.of(context).addServer),
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
                style: const TextStyle(
                  fontSize: 20,
                  fontWeight: FontWeight.w700,
                ),
              ),
              const SizedBox(height: 10),
              Text(
                L.of(context).audioFailedBody,
                textAlign: TextAlign.center,
                style: TextStyle(
                  color: Theme.of(context).colorScheme.onSurfaceVariant,
                ),
              ),
              const SizedBox(height: 16),
              // Selectable, because this is the only text on the screen that
              // says what actually went wrong, and the headline above it
              // guesses at the microphone whatever the cause really was.
              SelectableText(
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
