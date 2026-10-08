import 'package:flutter/material.dart';

import '../l10n/app_localizations.dart';
import '../services/site_links.dart';
import '../state/app_state.dart';
import '../theme.dart';
import '../widgets/diagnostics_panel.dart';
import '../widgets/error_snack.dart';
import '../widgets/language_button.dart';
import '../widgets/my_avatar.dart';
import '../widgets/ptt_button.dart';
import '../widgets/review_request.dart';
import '../widgets/server_card.dart';
import '../widgets/server_detail_pane.dart';
import '../widgets/wordmark.dart';
import 'add_server_screen.dart';
import 'settings_screen.dart';

/// How wide the overflow menu is drawn, in logical pixels.
///
/// The default is the widest entry, up to five list-tile heights; this is that
/// maximum said out loud, because the picture at the top of the menu is sized
/// from it.
const double _menuWidth = 280;

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
        // The lockup rather than the single-line title. It is a fixed shape
        // rather than text that reflows, so it is allowed to shrink on a bar
        // too narrow for it — which beats the alternatives of clipping the
        // name or letting it overflow into the buttons. It never grows: a
        // wordmark that swelled to fill a tablet's app bar would be the
        // loudest thing on a screen whose subject is the server list.
        //
        // It is also the way to the website, which is where a rider goes to
        // find out what any of this does. Made a link here rather than inside
        // `Wordmark`: the mark is a drawing and should stay one, and this is
        // the only place it is a link.
        title: FittedBox(
          fit: BoxFit.scaleDown,
          alignment: Alignment.centerLeft,
          child: Tooltip(
            message: l.openWebsite,
            child: InkWell(
              onTap: () =>
                  openSite(context, SiteLinks.home(siteLanguage(context))),
              // A bare `Wordmark` gives the reader nothing to announce, so the
              // link needs its own label and role.
              child: Semantics(
                link: true,
                label: l.openWebsite,
                child: const Wordmark(),
              ),
            ),
          ),
        ),
        actions: [
          const LanguageButton(),
          // Turns into a warning when the chain has had to give something up.
          //
          // **The panel cannot be the only place this is said.** A rider whose
          // voice has quietly got worse has no reason to open diagnostics, so
          // the one control that leads there has to be able to say that
          // something is wrong before anybody goes looking for it.
          IconButton(
            tooltip: state.probing
                ? l.diagProbing
                : state.chainDegraded
                ? l.diagChainDegradedShort
                : l.diagnostics,
            onPressed: state.toggleDiagnostics,
            // A spinner until the device has been measured, because the icon it
            // replaces is about to make a claim — plain or warning — that has
            // not been decided yet. Showing either one early would be saying
            // something untrue.
            //
            // **From launch, not from the start of the measurement.** The probe
            // waits for startup to go quiet before it times anything, so gating
            // this on "is it running" left the plain icon up through startup and
            // a five-second settle — the longer half of the window it exists to
            // cover, and the half a rider actually sees. `AppState.probing`
            // carries the whole of it now, and names the paths that end it.
            //
            // Sized to the icon it stands in for, so the toolbar does not shift
            // when the measurement lands.
            icon: state.probing
                ? const SizedBox(
                    width: 20,
                    height: 20,
                    child: CircularProgressIndicator(strokeWidth: 2),
                  )
                : Icon(
                    state.chainDegraded
                        ? Icons.warning_amber_rounded
                        : Icons.monitor_heart_outlined,
                  ),
            color: state.chainDegraded
                ? StatusColors.connecting
                : state.diagnosticsOpen
                ? Theme.of(context).colorScheme.primary
                : null,
          ),
          // **Both buttons say whose decision it was, in the roster's own
          // colours.** Red is ours — we pressed it, and pressing it again
          // undoes it. Amber is somebody else's: an admin's mute or deafen, or
          // a channel that will not carry a voice, none of which this button
          // can lift. Drawn red, as it was, an imposed silence read as a
          // button the rider had pressed and forgotten, and pressing it did
          // nothing they could see.
          IconButton(
            tooltip: state.deafenedByServer
                ? l.youWereDeafened
                : state.deafened
                ? l.undeafen
                : l.deafen,
            onPressed: state.toggleDeafen,
            // A speaker rather than an ear. Deafen silences what *arrives*,
            // and every other app a rider has used puts that behind a speaker
            // with a line through it; an ear is the anatomy rather than the
            // control, and it reads as a hearing-aid setting.
            icon: Icon(
              state.deafened || state.deafenedByServer
                  ? Icons.volume_off
                  : Icons.volume_up,
            ),
            color: state.deafenedByServer
                ? StatusColors.reconnecting
                : state.deafened
                ? StatusColors.failed
                : null,
          ),
          IconButton(
            tooltip: state.silencedByServer
                ? (state.suppressedSomewhere
                      ? l.suppressedTitle
                      : l.youWereMuted)
                : state.muted
                ? l.unmuteMicrophone
                : l.muteMicrophone,
            // **Not a toggle while the silence is somebody else's.** Drawn
            // amber, this button says "an admin muted you" — and it was still
            // wired to self-mute, so a tap changed a different state than the
            // one on screen, the colour did not move, and the microphone
            // silently opened or closed underneath. A control that acts on
            // something other than what it displays is worse than one that
            // does nothing.
            //
            // Only when *no* server can hear us. Muted on one of two, self-mute
            // still decides whether the other one does, so it stays live.
            onPressed: state.silencedEverywhere ? null : state.toggleMute,
            // Kept amber rather than going the usual disabled grey: the reason
            // it cannot be pressed is the thing the colour is saying.
            disabledColor: StatusColors.reconnecting,
            // **`effectivelyMuted`, not `muted`.** Capture being closed is a
            // mute for every practical purpose, and this drew an open
            // microphone through the whole listening state — a rider unable to
            // transmit a word, with the one indicator that exists to say so
            // saying the opposite. It was the first thing reported.
            icon: Icon(
              state.effectivelyMuted || state.silencedByServer
                  ? Icons.mic_off
                  : Icons.mic,
            ),
            color: state.silencedByServer
                ? StatusColors.reconnecting
                : state.muted
                ? StatusColors.failed
                // Closed because nobody has tapped yet is not a fault and must
                // not borrow the red that means "you muted yourself". The
                // ordinary dimmed colour: off, and nothing wrong.
                : !state.capturing
                ? Theme.of(context).disabledColor
                : null,
          ),
          PopupMenuButton<String>(
            tooltip: l.more,
            icon: const Icon(Icons.more_vert),
            // Fixed, where the default is "as wide as the widest entry". The
            // picture at the top is drawn as a fraction of this, and a width
            // that came from the entries would then be coming from the picture
            // that comes from it.
            constraints: const BoxConstraints(
              minWidth: _menuWidth,
              maxWidth: _menuWidth,
            ),
            onSelected: (v) async {
              final messenger = ScaffoldMessenger.of(context);
              switch (v) {
                case 'avatar':
                  final e = await state.pickAvatar();
                  if (e != null) {
                    showError(
                      messenger,
                      e == 'unreadable' ? l.avatarUnreadable : e,
                    );
                  }
                case 'avatarRemove':
                  await state.clearAvatar();
                case 'export':
                  final e = await state.exportServersToFile();
                  if (e != null) {
                    showError(messenger, e);
                  }
                case 'import':
                  final e = await state.importServersFromFile();
                  if (e != null) {
                    showError(messenger, e);
                  }
                case 'website':
                  if (!context.mounted) return;
                  await openSite(
                    context,
                    SiteLinks.home(siteLanguage(context)),
                  );
                case 'settings':
                  if (!context.mounted) return;
                  await Navigator.push(
                    context,
                    MaterialPageRoute(builder: (_) => const SettingsScreen()),
                  );
              }
            },
            itemBuilder: (_) => [
              // The rider's own picture, and the way to change it: one tap on
              // the picture itself, which is the thing being changed.
              //
              // It lives here rather than in settings because it belongs to the
              // rider rather than to any one server or any one setting, and
              // because a picture is something to *see*: a row of prose about
              // it on a page of sliders was both easy to miss and the one row
              // there whose control was a picture rather than a value.
              PopupMenuItem(
                value: 'avatar',
                // The picture decides how tall this is: the default height is
                // one text row, and this entry is a face. The padding goes for
                // the same reason — the picture is measured against the width
                // of the menu, not of what is left inside its margins.
                height: 0,
                padding: EdgeInsets.zero,
                child: AvatarMenuTile(
                  image: state.myAvatar,
                  width: _menuWidth,
                ),
              ),
              // Only when there is one to take down. Without a picture the mark
              // stands in for it, and offering to remove the mark would be
              // offering to remove nothing.
              if (state.myAvatar != null)
                PopupMenuItem(
                  value: 'avatarRemove',
                  child: ListTile(
                    dense: true,
                    contentPadding: EdgeInsets.zero,
                    leading: const Icon(Icons.hide_image_outlined),
                    title: Text(l.avatarRemove),
                  ),
                ),
              const PopupMenuDivider(),
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
                value: 'website',
                child: ListTile(
                  dense: true,
                  contentPadding: EdgeInsets.zero,
                  leading: const Icon(Icons.public),
                  title: Text(l.website),
                ),
              ),
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
                    // **`const`.** See `DiagnosticsPanel._close`: passing
                    // anything here rebuilds the whole panel every time
                    // this screen rebuilds, which is twice a second.
                    child: const DiagnosticsPanel(),
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
        // First in the list rather than floating over the screen.
        //
        // **It used to be the bottom layer of the body's stack**, which on a
        // phone put it in the space below the cards and on anything wider put
        // it *behind* the layout: the detail pane paints over it, so what a
        // rider saw was half a sentence showing through the gap beside the
        // server list and two buttons stranded in an empty pane. Last in the
        // list it was honest but easy to miss — on a window the height of a
        // laptop's it sat below the fold.
        //
        // Here it takes room instead of borrowing it. It scrolls away with the
        // list, it cannot reach the talk panel — which is the one thing on this
        // screen that must never be covered — and it renders to nothing unless
        // `shouldAskForReview` says otherwise, which it never does while a call
        // is up, so it is not in a rider's way on the road.
        const ReviewRequest(),
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
              children: [
                const _MicIdleNotice(),
                const SizedBox(height: 8),
                status,
              ],
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
                // The reason why is dropped when the screen is wider than it
                // is tall. It is reassurance — that nothing is being recorded
                // and the headset keeps its sound quality — which is worth
                // reading once and never again, and in landscape those three
                // extra lines came out of the server list, where the cards are
                // the thing the rider actually came to this screen for. The
                // line above it, which says what will appear here and when,
                // stays in both.
                if (MediaQuery.sizeOf(context).height >=
                    _TalkPanel._shortViewport) ...[
                  const SizedBox(height: 4),
                  Text(
                    l.micIdleWhy,
                    style: TextStyle(
                      fontSize: 11,
                      color: muted.withValues(alpha: 0.75),
                    ),
                  ),
                ],
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
    // Scrollable, and centred only when there is room to centre it.
    //
    // Found on a 2018 phone with a short screen: the column is taller than the
    // space it was given, so it overflowed, and the talk panel below drew over
    // the bottom of it — taking the "Add server" button with it, along with
    // every touch aimed at the button. A new rider on that handset could not
    // add a server at all, which is the only thing this screen exists to ask
    // them to do.
    return LayoutBuilder(
      builder: (context, constraints) => SingleChildScrollView(
        child: ConstrainedBox(
          constraints: BoxConstraints(minHeight: constraints.maxHeight),
          child: _content(context),
        ),
      ),
    );
  }

  Widget _content(BuildContext context) {
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
