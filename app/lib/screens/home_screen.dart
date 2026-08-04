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
                    ? _WideBody(state: state, available: constraints.maxWidth)
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
  const _WideBody({required this.state, required this.available});
  final AppState state;

  /// Width this body actually has, which is the layout's rather than the
  /// screen's: a safe area on a notched phone in landscape takes a bite out of
  /// both edges, and sizing the master column off the screen would push the
  /// detail pane narrower than it was told it could be.
  final double available;

  @override
  Widget build(BuildContext context) {
    return Row(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        SizedBox(
          // Wide enough for a card to stay readable, narrow enough to leave the
          // detail pane the majority of the space. See [masterPaneWidth] for
          // why it is not simply a fixed 400 any more.
          width: masterPaneWidth(available),
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

  /// Height below which the panel folds sideways instead of stacking.
  ///
  /// A phone in landscape is about 390 to 430 points tall. Stacked, this panel
  /// is roughly 240 of them — the talk button alone is 132, because it is meant
  /// to be hit in gloves without looking — which would leave a server list too
  /// short to show one card. Every tall case is well clear of this: a phone in
  /// portrait starts around 660, an iPad in landscape at 768.
  static const double _shortViewport = 600;

  @override
  Widget build(BuildContext context) {
    final live = state.runtimes.values.where((r) => r.isLive).length;
    final short = MediaQuery.sizeOf(context).height < _shortViewport;

    final status = Text(
      live == 0
          ? L.of(context).notConnectedAny
          : live == 1
          ? L.of(context).talkingOnOne
          : L.of(context).talkingOnMany(live),
      textAlign: short ? TextAlign.start : TextAlign.center,
      style: TextStyle(
        fontSize: 12,
        color: Theme.of(context).colorScheme.onSurfaceVariant,
      ),
    );

    return Container(
      padding: EdgeInsets.fromLTRB(16, short ? 8 : 12, 16, short ? 10 : 16),
      decoration: BoxDecoration(
        color: Theme.of(context).colorScheme.surfaceContainerLow,
        borderRadius: const BorderRadius.vertical(top: Radius.circular(22)),
      ),
      // With the microphone shut there is nothing for a meter to show and
      // nothing for the talk button to key. Drawing them anyway gives a bar
      // that never moves and a control that does nothing, which reads as an
      // app that has broken rather than one that is idle — so the panel says
      // what it is waiting for instead.
      child: !state.audioActive
          ? Column(
              mainAxisSize: MainAxisSize.min,
              children: [const _MicIdleNotice(), const SizedBox(height: 8), status],
            )
          : short
          ? _SideBySide(state: state, status: status)
          : _Stacked(state: state, status: status),
    );
  }
}

/// The talk controls with room to breathe: meter, button, status, in a column.
class _Stacked extends StatelessWidget {
  const _Stacked({required this.state, required this.status});
  final AppState state;
  final Widget status;

  @override
  Widget build(BuildContext context) {
    return Column(
      mainAxisSize: MainAxisSize.min,
      children: [
        const LevelMeter(),
        // The talk button only exists for push-to-talk. In the automatic modes
        // it is a status light the meter already provides, so the vertical
        // space goes back to the server list.
        if (state.showTalkButton) ...[
          const SizedBox(height: 12),
          const PttButton(),
        ],
        const SizedBox(height: 8),
        status,
      ],
    );
  }
}

/// The same controls folded sideways for a screen that is wider than it is tall.
///
/// Nothing is dropped — a rider in landscape needs the meter and the connection
/// count exactly as much as one in portrait. The button keeps the trailing
/// side, which is where it sits at the bottom of the stacked layout too, so
/// rotating the device moves it a short way rather than across the screen.
class _SideBySide extends StatelessWidget {
  const _SideBySide({required this.state, required this.status});
  final AppState state;
  final Widget status;

  @override
  Widget build(BuildContext context) {
    final meterAndStatus = Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [const LevelMeter(), const SizedBox(height: 6), status],
    );

    if (!state.showTalkButton) return meterAndStatus;

    return Row(
      crossAxisAlignment: CrossAxisAlignment.center,
      children: [
        Expanded(flex: 3, child: meterAndStatus),
        const SizedBox(width: 16),
        // Shared out rather than fixed. This panel sits in the master column
        // when the two-pane layout is up, which on a landscape phone is about
        // 360 points wide — a fixed button would have left the meter and the
        // connection line squeezed into what was left, on the one screen where
        // this arrangement exists to save space.
        //
        // Still oversized for a gloved thumb, just no longer the full 132: the
        // width it gains sideways buys back most of what the height gives up,
        // and a target this size is one the rider can still find by feel.
        const Expanded(flex: 2, child: PttButton(height: 84)),
      ],
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
