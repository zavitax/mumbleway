import 'package:flutter/material.dart';

import 'dart:async';

import '../l10n/app_localizations.dart';
import '../src/rust/api/mumbleway.dart';
import '../state/app_state.dart';
import '../theme.dart';
import 'watch.dart';

/// Arming and disarming pocket mode, and saying what it is doing.
///
/// **A slider rather than a button, because the consequence is a locked
/// screen.** Arming closes the microphone and stops every other control
/// responding; a rider who hit that by brushing the screen with a glove would
/// have no obvious way back, and the thing they would try first — pressing
/// buttons — is exactly what stops working. A deliberate drag cannot happen by
/// accident in a pocket, and the same control disarms it, in the same place,
/// so the way out is the way in.
///
/// Everything above the slider is there so the rider never has to look: the
/// state, what to do about it, and **which microphone is actually recording**.
/// That last line exists because `CLAUDE.md` records the fault it prevents —
/// audio carries no record of what captured it, so a phone recording from its
/// own microphone while the rider believes the helmet is live looks identical
/// from the outside.

/// The whole bottom panel, when pocket mode is offered.
class PocketPanel extends StatelessWidget {
  const PocketPanel({super.key});

  @override
  Widget build(BuildContext context) {
    return Watch<(bool, bool, bool, bool, bool, int, bool)>(
      (state) => (
        state.pocketAvailable,
        state.pocketReady,
        state.pocketActive,
        state.anyLive,
        state.capturing,
        state.audioRoute,
        state.pocketPanelCollapsed,
      ),
      (context, state) {
        // Offered only once there is somewhere for the audio to go: arming it
        // with nothing connected would close a microphone nobody is listening
        // to and lock the screen a rider needs in order to connect.
        //
        // The same fact the lock is derived from, read from one place - see
        // `AppState.pocketLocked` for the trap that came of having two.
        if (!state.pocketPanelVisible) {
          return const SizedBox.shrink();
        }
        final l = L.of(context);
        final active = state.pocketActive;
        final collapsed = state.pocketPanelCollapsed;
        return Container(
          padding: const EdgeInsets.fromLTRB(16, 2, 16, 8),
          decoration: BoxDecoration(
            color: Theme.of(context).colorScheme.surfaceContainerHighest,
            borderRadius: const BorderRadius.vertical(
              top: Radius.circular(22),
            ),
          ),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            mainAxisSize: MainAxisSize.min,
            children: [
              // **The handle, and when it is down it is the whole panel.** A
              // rider asked to be able to put this away: it is useful for two
              // seconds of a ride and takes a third of the screen. Pulled
              // down it leaves a notch to pull back up, and the choice is
              // remembered between rides.
              _Handle(
                collapsed: collapsed,
                armed: active,
                onChanged: (v) =>
                    AppStateScope.read(context).setPocketPanelCollapsed(v),
              ),
              // `AnimatedSize` rather than two layouts: the panel is on screen
              // while it changes height, and a jump reads as the app having
              // done something.
              AnimatedSize(
                duration: const Duration(milliseconds: 160),
                curve: Curves.easeOut,
                alignment: Alignment.topCenter,
                child: collapsed
                    ? const SizedBox(width: double.infinity, height: 0)
                    : Column(
                        crossAxisAlignment: CrossAxisAlignment.stretch,
                        mainAxisSize: MainAxisSize.min,
                        children: [
                          _StateLine(
                            capturing: state.capturing,
                            route: state.audioRoute,
                          ),
                          const SizedBox(height: 10),
                          SlideToAct(
                            label: active ? l.pocketSlideOff : l.pocketSlideOn,
                            colour: active
                                ? StatusColors.failed
                                : StatusColors.connected,
                            icon: active
                                ? Icons.lock_open
                                : Icons.lock_outline,
                            onConfirmed: () => _confirm(context, active: active),
                          ),
                          // **A notice, not a refusal, and that distinction
                          // was the whole of a wrong build.** This used to
                          // take the slider away when no headset was
                          // attached, on the reasoning that pocket mode has
                          // nothing to do without one. A rider put the
                          // argument that settles it: they arm pocket mode
                          // *and then* put their helmet on, and a headset
                          // that drops mid-ride — which a BMW's Bluetooth
                          // will do — has to be recoverable by switching it
                          // off and on again, with the panel still armed and
                          // still saying so.
                          //
                          // So arming is always allowed. What changes without
                          // a headset is underneath: capture stays open, no
                          // silence counts down, and nothing listens for
                          // taps. Amber, because amber is this app's word for
                          // "something outside this app decided it" — the
                          // same dress the server's suppression notice wears.
                          //
                          // **Only while armed**, at a rider's request. With
                          // pocket mode off, a notice about what it would
                          // need is advice nobody asked for, next to a slider
                          // that works; once it is on and nothing is
                          // happening, it is the explanation.
                          if (active && !state.pocketReady) ...[
                            const SizedBox(height: 12),
                            const _NoHeadset(),
                          ],
                          if (active && state.pocketReady) ...[
                            // Asked for: the slider and the meter were
                            // touching, and through a glove that reads as one
                            // control.
                            const SizedBox(height: 16),
                            _TapFeedback(
                              capturing: state.capturing,
                              route: state.audioRoute,
                            ),
                          ],
                          if (active) ...[
                            const SizedBox(height: 8),
                            // **The anti-panic line.** Every other control on
                            // the screen has just stopped working, which from
                            // the outside is exactly what a frozen app looks
                            // like.
                            Text(
                              l.pocketLockedNote,
                              textAlign: TextAlign.center,
                              style: TextStyle(
                                fontSize: 11,
                                color: Theme.of(
                                  context,
                                ).colorScheme.onSurfaceVariant,
                              ),
                            ),
                          ],
                        ],
                      ),
              ),
            ],
          ),
        );
      },
    );
  }

  static Future<void> _confirm(
    BuildContext context, {
    required bool active,
  }) async {
    final state = AppStateScope.of(context);
    if (active) {
      await state.setPocketActive(false);
      return;
    }
    // First time: explain it before the screen locks, not after. A rider who
    // has to work out how to get out of this while riding is a rider who was
    // told too late.
    if (!state.pocketTutorialSeen) {
      final go = await showDialog<bool>(
        context: context,
        builder: (_) => const PocketTutorial(),
      );
      await state.markPocketTutorialSeen();
      if (go != true) return;
    }
    await state.setPocketActive(true);
  }
}

/// The grab handle: tap it, or flick it down and up.
///
/// **Both gestures, because neither is obvious on its own.** A handle that
/// only drags is one a rider prods; one that only taps is one they try to
/// drag. The target is the full width of the panel and 28 logical pixels tall,
/// which is reachable with a glove on.
class _Handle extends StatelessWidget {
  const _Handle({
    required this.collapsed,
    required this.armed,
    required this.onChanged,
  });

  final bool collapsed;

  /// **The handle wears the slider's colour**, which a rider asked for and
  /// which is the right answer for a collapsed panel: green while pocket mode
  /// is off, red while it is on, so the notch alone says which state the phone
  /// is in from across the tank. It also has to read as the way back — a
  /// collapsed panel hides the slider, and while armed that slider is the only
  /// live control on the screen.
  final bool armed;

  final ValueChanged<bool> onChanged;

  @override
  Widget build(BuildContext context) {
    return GestureDetector(
      behavior: HitTestBehavior.opaque,
      onTap: () => onChanged(!collapsed),
      // Direction decides, not distance: a flick down puts it away and a flick
      // up brings it back, so the panel always ends in a state rather than
      // part-way through one.
      onVerticalDragEnd: (d) {
        final v = d.primaryVelocity ?? 0;
        if (v > 0) onChanged(true);
        if (v < 0) onChanged(false);
      },
      child: SizedBox(
        height: 28,
        child: Center(
          child: Container(
            width: 44,
            height: 5,
            decoration: BoxDecoration(
              // The slider's own two colours, not a tint of them: these are
              // the same fact at two sizes.
              color: armed ? StatusColors.failed : StatusColors.connected,
              borderRadius: BorderRadius.circular(3),
            ),
          ),
        ),
      ),
    );
  }
}

/// Pocket mode is armed and has nothing to work with.
///
/// **Amber and a sentence, not a closed door.** The five states a rider
/// listed, and they all fall out of one flag — `pocketReady`, which is pocket
/// mode plus a headset:
///
/// | | capture | cooldown |
/// |---|---|---|
/// | armed, no headset | always on | none |
/// | armed, headset | released until tapped | runs |
/// | headset drops while armed | always on | none |
/// | headset arrives while armed | released | runs |
/// | not armed | always on | none |
///
/// So arming without a headset is not a mistake to be prevented; it is how a
/// rider sets the phone down before putting the helmet on, and how they
/// recover a headset that has dropped. This says which of the five they are
/// in, and the microphone stays open underneath it.
class _NoHeadset extends StatelessWidget {
  const _NoHeadset();

  @override
  Widget build(BuildContext context) {
    final l = L.of(context);
    return Container(
      width: double.infinity,
      padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 8),
      decoration: BoxDecoration(
        // The same wash and hairline as the server card's suppression notice,
        // deliberately: both say "something outside this app decided this",
        // and a rider meeting one after the other should not have to work out
        // that they are the same kind of message.
        color: StatusColors.reconnecting.withValues(alpha: 0.13),
        borderRadius: BorderRadius.circular(6),
        border: Border.all(color: StatusColors.noticeBorder),
      ),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          const Icon(
            Icons.headset_off,
            size: 18,
            color: StatusColors.reconnecting,
          ),
          const SizedBox(width: 8),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  l.pocketNoHeadset,
                  style: const TextStyle(
                    color: StatusColors.reconnecting,
                    fontWeight: FontWeight.w600,
                    fontSize: 13,
                  ),
                ),
                const SizedBox(height: 2),
                // The explanation in ordinary body colour: a sentence to read,
                // not a second alarm.
                Text(
                  l.pocketNoHeadsetBody,
                  style: TextStyle(
                    color: Theme.of(context).colorScheme.onSurfaceVariant,
                    fontSize: 12,
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

/// Which microphone is recording, and nothing else.
///
/// **The instruction moved out of here.** It used to say "tap three times" or
/// "stop talking", above the slider, in a line that appeared and disappeared
/// with arming - so the panel changed height as it changed state. It is now a
/// fixed-height block under the three dots, with a countdown in it; see
/// [_TapFeedback].
///
/// What is left is the line `CLAUDE.md` records the fault for: audio carries
/// no record of what captured it, so a phone recording from its own microphone
/// while the rider believes the helmet is live looks identical from outside.
/// The microphone icon beside it is gone at a rider's request - the words say
/// it, and the icon said it again.
class _StateLine extends StatelessWidget {
  const _StateLine({required this.capturing, required this.route});

  final bool capturing;
  final int route;

  /// `Recorded::route`'s numbers - a wire format shared with every recording
  /// already on a rider's phone, so these cases are not free to be renumbered.
  static bool _external(int route) => route == 2 || route == 3 || route == 4;

  /// Which microphone is open, which is not the same as which one is being
  /// transmitted.
  ///
  /// **This used to answer "none" whenever capture was off, and that was
  /// wrong.** Pocket mode spends its time with capture off and a microphone
  /// very much open: the phone's own, which is the only one a tap can reach
  /// and the entire basis of the gesture. So the line sat on a false value for
  /// the whole of the state it mattered in, which a rider reported as the
  /// microphone never seeming to update.
  ///
  /// Capture off is the phone's own by construction - `activateListening`
  /// asks for the built-in input explicitly - so the route is only consulted
  /// for the capturing case, where it names the port that was taken.
  static String _mic(L l, int route, bool capturing) {
    if (!capturing) return l.pocketMicPhone;
    return switch (route) {
      2 => l.pocketMicWired,
      3 => l.pocketMicHelmet,
      4 => l.pocketMicUsb,
      _ => l.pocketMicPhone,
    };
  }

  @override
  Widget build(BuildContext context) {
    final l = L.of(context);
    final scheme = Theme.of(context).colorScheme;
    return Text(
      _mic(l, route, capturing),
      textAlign: TextAlign.center,
      style: TextStyle(fontSize: 11, color: scheme.onSurfaceVariant),
    );
  }
}

/// The meter, the three dots, and what to do next.
///
/// **The meter is the phone's own microphone, as the detector hears it** -
/// band-limited and measured before anything else touches it, which is the
/// only level that says whether a tap will register. The mark on the track is
/// the threshold, so a rider can see whether their taps reach it rather than
/// guess. Shown only while listening, because that is when the phone's
/// microphone is the one being tapped; while capturing it is the helmet's, and
/// the talk panel's own meters cover that.
///
/// **The three dots are what the acknowledging tick used to do.** They still
/// exist beside the three rising notes because a note can be missed under a
/// visor at speed, and two heard out of three is a different diagnosis from
/// none.
///
/// **The nine stage lamps are gone from here.** They were an instrument for
/// finding out why taps on a helmet were not registering, and there is no
/// detector on a helmet any more; the row is still in the diagnostics panel,
/// where somebody looking for it will be looking for it.
class _TapFeedback extends StatefulWidget {
  const _TapFeedback({required this.capturing, required this.route});

  final bool capturing;

  /// `Recorded::route`'s numbers. 2, 3 and 4 are external microphones.
  final int route;

  @override
  State<_TapFeedback> createState() => _TapFeedbackState();
}

class _TapFeedbackState extends State<_TapFeedback> {
  /// Ten a second: a rider is performing a rhythm, and anything slower reads
  /// as the panel lagging the hand. Runs only while pocket mode is armed,
  /// which is the only time this widget exists.
  static const _period = Duration(milliseconds: 100);

  /// How far over the floor the meter draws, in dB. Taps measure 19 to 36 over
  /// it on the recordings, so this shows the whole of the useful range with
  /// the threshold about half way along.
  static const _span = 40.0;

  /// What a tap has to clear, which is `MARGIN_DB` in the detector.
  static const _threshold = 18.0;

  /// The height the instruction block always takes.
  ///
  /// **Fixed on purpose, and a rider asked for it**: a countdown that replaces
  /// one line of text with two moves everything above it, including the slider
  /// they are reaching for. Two lines' worth, so neither state can overflow.
  static const _instructionHeight = 38.0;

  Timer? _tick;
  int _run = 0;
  double _over = 0;

  /// The countdown bar's position, 1 to 0, and the only thing here that moves
  /// between polls.
  ///
  /// **A notifier rather than `setState`, because this now changes ten times a
  /// second.** The bar used to step once a second, which a rider reported as
  /// jumping; smoothing it means repainting at the poll rate, and repainting
  /// the whole panel ten times a second to move one bar is work nobody asked
  /// for. This rebuilds the bar and its label and nothing else.
  final _fraction = ValueNotifier<double>(1);

  /// The last whole second [AppState.captureIdleLeft] reported, and how long
  /// ago it arrived.
  ///
  /// **The integer is authoritative and the interpolation is cosmetic.** The
  /// counter that actually releases capture lives in `AppState` and counts
  /// whole seconds; this fills in between its ticks from a monotonic
  /// stopwatch, so a bar that is a few tens of milliseconds out of step with
  /// the figure beside it cannot drift, stall, or outlive the thing it is
  /// counting down to.
  int _leftSeconds = 0;
  final _sinceReading = Stopwatch();

  @override
  void initState() {
    super.initState();
    _tick = Timer.periodic(_period, (_) => _poll());
  }

  @override
  void dispose() {
    _tick?.cancel();
    _fraction.dispose();
    super.dispose();
  }

  void _poll() {
    if (!mounted) return;
    _advanceCountdown();
    try {
      final s = acousticDiagnostics();
      // Against the floor rather than full scale: the threshold is a margin
      // over the trailing average, so the distance to it is the only reading
      // that means anything. A raw dBFS meter would move with the room.
      final over = (s.meterDb - s.floorDb).clamp(0.0, _span);
      if (s.run == _run && (over - _over).abs() < 0.5) return;
      setState(() {
        _run = s.run;
        _over = over;
      });
    } catch (_) {
      // No engine, so no meter and no dots. The countdown is the app's own and
      // has already been advanced above.
    }
  }

  /// Moves the bar, between the whole seconds the state reports.
  ///
  /// The countdown itself comes from `AppState`: it holds the counter that
  /// actually releases capture, and a second copy here would be a second
  /// answer to the same question. What this adds is the tenths.
  void _advanceCountdown() {
    final state = AppStateScope.read(context);
    final total = state.captureIdleSeconds;
    final left = state.captureIdleLeft;
    if (left != _leftSeconds) {
      _leftSeconds = left;
      _sinceReading
        ..reset()
        ..start();
    }
    if (total <= 0) {
      _fraction.value = 0;
      return;
    }
    // **Bounded to the one second it is interpolating, which it was not.**
    // Subtracting the whole elapsed time from the last reading assumes the
    // reading keeps coming; when the counter is not running the smoothing
    // walked the bar from full to empty on its own, and a rider watched a
    // thirty-second countdown that nothing was behind. A step of at most one
    // second cannot invent more than it is told.
    final step = (_sinceReading.elapsedMilliseconds / 1000.0).clamp(0.0, 1.0);
    // Clamped at zero as well: the state's counter and this stopwatch are not
    // the same clock, so the last tenth of a second before a release can run
    // over.
    final seconds = (_leftSeconds - step).clamp(0.0, total.toDouble());
    _fraction.value = seconds / total;
  }

  /// Whether a tap could be heard at all right now.
  bool get _listening =>
      !widget.capturing && !_StateLine._external(widget.route);

  @override
  Widget build(BuildContext context) {
    final l = L.of(context);
    final scheme = Theme.of(context).colorScheme;
    final total = AppStateScope.of(context).captureIdleSeconds;
    return Column(
      children: [
        if (_listening) ...[
          LayoutBuilder(
            builder: (context, constraints) {
              final w = constraints.maxWidth;
              final loud = _over >= _threshold;
              return SizedBox(
                height: 8,
                child: Stack(
                  children: [
                    DecoratedBox(
                      decoration: BoxDecoration(
                        color: scheme.surfaceContainerHigh,
                        borderRadius: BorderRadius.circular(4),
                      ),
                      child: const SizedBox(width: double.infinity),
                    ),
                    FractionallySizedBox(
                      widthFactor: (_over / _span).clamp(0.0, 1.0),
                      child: DecoratedBox(
                        decoration: BoxDecoration(
                          color: loud
                              ? StatusColors.connected
                              : scheme.onSurfaceVariant,
                          borderRadius: BorderRadius.circular(4),
                        ),
                        child: const SizedBox(height: 8),
                      ),
                    ),
                    // The threshold. A tap has to reach this.
                    Positioned(
                      left: (w * _threshold / _span).clamp(0.0, w - 2),
                      child: Container(
                        width: 2,
                        height: 8,
                        color: scheme.onSurface,
                      ),
                    ),
                  ],
                ),
              );
            },
          ),
          // Asked for: the meter and the dots were touching.
          const SizedBox(height: 14),
        ],
        Row(
          mainAxisAlignment: MainAxisAlignment.center,
          children: [
            for (var i = 0; i < 3; i++)
              AnimatedContainer(
                duration: const Duration(milliseconds: 120),
                width: i < _run ? 16 : 10,
                height: i < _run ? 16 : 10,
                margin: const EdgeInsets.symmetric(horizontal: 5),
                decoration: BoxDecoration(
                  shape: BoxShape.circle,
                  // Green, at a rider's request. Blue was the talking colour
                  // borrowed for want of a better one, and it is the colour of
                  // somebody else's voice everywhere else in this app.
                  color: i < _run
                      ? StatusColors.connected
                      : scheme.outlineVariant,
                ),
              ),
          ],
        ),
        const SizedBox(height: 10),
        // **One block, one height, two states.** Closed: what to do. Open: how
        // long is left before it closes itself, as a bar that empties - which
        // is the one part of pocket mode a rider cannot hear, since the two
        // falling tones say it *has* closed rather than that it is about to.
        SizedBox(
          height: _instructionHeight,
          // **Three states, not two, and the third is the one that caught a
          // rider out.** The microphone can be open with no silence being
          // counted - with no headset, or while a meter or a recording holds
          // it - and a bar emptying then is an instrument reporting on a
          // mechanism that is not running. It says so instead.
          child: !widget.capturing
              ? Center(
                  child: Text(
                    l.pocketTapToSpeak,
                    textAlign: TextAlign.center,
                    style: const TextStyle(
                      fontSize: 13,
                      fontWeight: FontWeight.w600,
                    ),
                  ),
                )
              : !AppStateScope.of(context).captureIdleRunning
              ? Center(
                  child: Text(
                    l.pocketMicOpen,
                    textAlign: TextAlign.center,
                    style: const TextStyle(
                      fontSize: 13,
                      fontWeight: FontWeight.w600,
                    ),
                  ),
                )
              : ValueListenableBuilder<double>(
                  valueListenable: _fraction,
                  builder: (context, fraction, _) => Column(
                    mainAxisAlignment: MainAxisAlignment.center,
                    children: [
                      ClipRRect(
                        borderRadius: BorderRadius.circular(3),
                        child: LinearProgressIndicator(
                          value: fraction,
                          minHeight: 5,
                          backgroundColor: scheme.surfaceContainerHigh,
                          valueColor: const AlwaysStoppedAnimation(
                            StatusColors.connected,
                          ),
                        ),
                      ),
                      const SizedBox(height: 6),
                      Text(
                        // Rounded up, so the figure reads 30 the moment the
                        // bar is full and reaches 0 only when it is empty.
                        l.pocketCountdown((fraction * total).ceil()),
                        textAlign: TextAlign.center,
                        style: const TextStyle(
                          fontSize: 12,
                          fontWeight: FontWeight.w600,
                        ),
                      ),
                    ],
                  ),
                ),
        ),
      ],
    );
  }
}

/// A track with a thumb that has to be dragged most of the way across.
///
/// Ninety per cent of the travel, and it springs back from anything less. A
/// shorter commitment would be reachable by the sort of accidental swipe this
/// control exists to be immune to.
class SlideToAct extends StatefulWidget {
  const SlideToAct({
    super.key,
    required this.label,
    required this.colour,
    required this.icon,
    required this.onConfirmed,
  });

  final String label;
  final Color colour;
  final IconData icon;
  final VoidCallback onConfirmed;

  @override
  State<SlideToAct> createState() => _SlideToActState();
}

class _SlideToActState extends State<SlideToAct> {
  static const double _height = 56;
  static const double _inset = 4;

  /// How far along the thumb has been dragged, in logical pixels.
  double _dx = 0;

  @override
  Widget build(BuildContext context) {
    return LayoutBuilder(
      builder: (context, constraints) {
        final travel = (constraints.maxWidth - _height - _inset * 2).clamp(
          0.0,
          double.infinity,
        );
        final progress = travel == 0 ? 0.0 : (_dx / travel).clamp(0.0, 1.0);
        return Semantics(
          // So the control is operable without a drag at all. A slider is a
          // guard against an accidental touch, not against assistive
          // technology, and `onTap` here is what a screen reader activates.
          button: true,
          label: widget.label,
          onTap: widget.onConfirmed,
          child: Container(
            height: _height + _inset * 2,
            decoration: BoxDecoration(
              color: widget.colour.withValues(alpha: 0.18),
              borderRadius: BorderRadius.circular((_height + _inset * 2) / 2),
              border: Border.all(color: widget.colour.withValues(alpha: 0.5)),
            ),
            child: Stack(
              alignment: Alignment.center,
              children: [
                // Fades out as the thumb covers it, so the label does not sit
                // under the thumb at the end of the travel.
                Opacity(
                  opacity: (1 - progress * 1.6).clamp(0.0, 1.0),
                  child: Padding(
                    padding: EdgeInsets.only(left: _height * 0.9, right: 12),
                    child: Text(
                      widget.label,
                      textAlign: TextAlign.center,
                      style: TextStyle(
                        fontSize: 13,
                        fontWeight: FontWeight.w600,
                        color: widget.colour,
                      ),
                    ),
                  ),
                ),
                Positioned(
                  left: _inset + _dx,
                  child: GestureDetector(
                    onHorizontalDragUpdate: (d) => setState(() {
                      _dx = (_dx + d.delta.dx).clamp(0.0, travel);
                    }),
                    onHorizontalDragEnd: (_) {
                      final fired = travel > 0 && _dx >= travel * 0.9;
                      setState(() => _dx = 0);
                      if (fired) widget.onConfirmed();
                    },
                    // Cancelled as well as ended: a drag that leaves the
                    // widget never reports an end, and the thumb would stay
                    // where it was abandoned.
                    onHorizontalDragCancel: () => setState(() => _dx = 0),
                    child: Container(
                      width: _height,
                      height: _height,
                      decoration: BoxDecoration(
                        color: widget.colour,
                        shape: BoxShape.circle,
                      ),
                      child: Icon(
                        widget.icon,
                        color: Colors.white,
                        size: 26,
                      ),
                    ),
                  ),
                ),
              ],
            ),
          ),
        );
      },
    );
  }
}

/// The three things a rider has to know before the screen locks.
class PocketTutorial extends StatefulWidget {
  const PocketTutorial({super.key});

  @override
  State<PocketTutorial> createState() => _PocketTutorialState();
}

class _PocketTutorialState extends State<PocketTutorial> {
  final _pages = PageController();
  int _page = 0;

  @override
  void dispose() {
    _pages.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final l = L.of(context);
    final slides = [
      (Icons.touch_app, l.pocketTutorialTapsTitle, l.pocketTutorialTapsBody),
      (Icons.lock_outline, l.pocketTutorialLockTitle, l.pocketTutorialLockBody),
      (Icons.hearing, l.pocketTutorialSoundTitle, l.pocketTutorialSoundBody),
    ];
    final last = _page == slides.length - 1;
    return AlertDialog(
      title: Text(l.pocketTutorialTitle),
      content: SizedBox(
        width: 400,
        height: 260,
        child: Column(
          children: [
            Expanded(
              child: PageView(
                controller: _pages,
                onPageChanged: (i) => setState(() => _page = i),
                children: [
                  for (final (icon, title, body) in slides)
                    Column(
                      mainAxisAlignment: MainAxisAlignment.center,
                      children: [
                        Icon(
                          icon,
                          size: 44,
                          color: Theme.of(context).colorScheme.primary,
                        ),
                        const SizedBox(height: 12),
                        Text(
                          title,
                          textAlign: TextAlign.center,
                          style: Theme.of(context).textTheme.titleMedium,
                        ),
                        const SizedBox(height: 8),
                        Text(
                          body,
                          textAlign: TextAlign.center,
                          style: Theme.of(context).textTheme.bodySmall,
                        ),
                      ],
                    ),
                ],
              ),
            ),
            const SizedBox(height: 8),
            Row(
              mainAxisAlignment: MainAxisAlignment.center,
              children: [
                for (var i = 0; i < slides.length; i++)
                  Container(
                    width: 7,
                    height: 7,
                    margin: const EdgeInsets.symmetric(horizontal: 3),
                    decoration: BoxDecoration(
                      shape: BoxShape.circle,
                      color: i == _page
                          ? Theme.of(context).colorScheme.primary
                          : Theme.of(context).colorScheme.outlineVariant,
                    ),
                  ),
              ],
            ),
          ],
        ),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.of(context).pop(false),
          child: Text(L.of(context).cancel),
        ),
        FilledButton(
          onPressed: last
              ? () => Navigator.of(context).pop(true)
              : () => _pages.nextPage(
                  duration: const Duration(milliseconds: 220),
                  curve: Curves.easeOut,
                ),
          child: Text(last ? l.pocketTutorialStart : l.pocketTutorialNext),
        ),
      ],
    );
  }
}
