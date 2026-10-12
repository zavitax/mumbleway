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
    return Watch<(bool, bool, bool, bool, int, bool)>(
      (state) => (
        state.pocketAvailable,
        state.pocketActive,
        state.anyLive,
        state.capturing,
        state.audioRoute,
        state.pocketTutorialSeen,
      ),
      (context, state) {
        // Offered only once there is somewhere for the audio to go: arming it
        // with nothing connected would close a microphone nobody is listening
        // to and lock the screen a rider needs in order to connect.
        //
        // The same fact the lock is derived from, read from one place — see
        // `AppState.pocketLocked` for the trap that came of having two.
        if (!state.pocketPanelVisible) {
          return const SizedBox.shrink();
        }
        final l = L.of(context);
        final active = state.pocketActive;
        return Container(
          padding: const EdgeInsets.fromLTRB(16, 12, 16, 16),
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
              // **The microphone line shows whether or not it is armed**, and
              // the instruction only once it is. Which microphone is recording
              // is worth knowing at all times; "tap three times" is a lie
              // before arming, because the detector is not running.
              _StateLine(
                armed: active,
                capturing: state.capturing,
                route: state.audioRoute,
              ),
              const SizedBox(height: 10),
              SlideToAct(
                label: active ? l.pocketSlideOff : l.pocketSlideOn,
                colour: active ? StatusColors.failed : StatusColors.connected,
                icon: active ? Icons.lock_open : Icons.lock_outline,
                onConfirmed: () => _confirm(context, active: active),
              ),
              if (active) ...[
                const SizedBox(height: 10),
                _TapFeedback(capturing: state.capturing),
                const SizedBox(height: 8),
                // **The anti-panic line.** Every other control on the screen
                // has just stopped working, which from the outside is exactly
                // what a frozen app looks like.
                Text(
                  l.pocketLockedNote,
                  textAlign: TextAlign.center,
                  style: TextStyle(
                    fontSize: 11,
                    color: Theme.of(context).colorScheme.onSurfaceVariant,
                  ),
                ),
              ],
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

/// What capture is doing, what to do about it, and which microphone is live.
class _StateLine extends StatelessWidget {
  const _StateLine({
    required this.armed,
    required this.capturing,
    required this.route,
  });

  final bool armed;
  final bool capturing;
  final int route;

  /// `Recorded::route`'s numbers — a wire format shared with every recording
  /// already on a rider's phone, so these cases are not free to be renumbered.
  static bool _external(int route) => route == 2 || route == 3 || route == 4;

  /// Which microphone is open, which is not the same as which one is being
  /// transmitted.
  ///
  /// **This used to answer "none" whenever capture was off, and that was
  /// wrong.** Pocket mode spends its time with capture off and a microphone
  /// very much open: the phone's own, which is the only one a tap on the phone
  /// can reach and the entire basis of the gesture. So the line sat on a false
  /// value for the whole of the state it mattered in, which a rider reported
  /// as the microphone never seeming to update.
  ///
  /// Capture off is the phone's own by construction — `activateListening`
  /// asks for the built-in input explicitly — so the route is only consulted
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
    // Closed is always the phone: with capture off there is no hands-free
    // link, so the phone's own microphone is the only one listening and the
    // only one a tap can reach.
    final what = !capturing
        ? l.pocketClosedTapPhone
        : _external(route)
        ? l.pocketOpenTapHelmet
        : l.pocketOpenTapPhone;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        if (armed)
          Row(
            mainAxisAlignment: MainAxisAlignment.center,
            children: [
              Icon(
                capturing ? Icons.mic : Icons.mic_off,
                size: 16,
                color: capturing
                    ? StatusColors.talking
                    : scheme.onSurfaceVariant,
              ),
              const SizedBox(width: 6),
              Flexible(
                child: Text(
                  what,
                  textAlign: TextAlign.center,
                  style: const TextStyle(
                    fontSize: 13,
                    fontWeight: FontWeight.w600,
                  ),
                ),
              ),
            ],
          ),
        if (armed) const SizedBox(height: 2),
        Text(
          _mic(l, route, capturing),
          textAlign: TextAlign.center,
          style: TextStyle(fontSize: 11, color: scheme.onSurfaceVariant),
        ),
      ],
    );
  }
}

/// What the detector is hearing, and how many taps it has counted.
///
/// **The pips are what the tick used to do, moved to where it can work.** The
/// acknowledging tick is a sound played into the rider's ears, and on a helmet
/// it comes back through the helmet's own microphone a round trip later and
/// registers as the next tap. There is no tail that fixes it: taps arrive 200
/// to 330 ms apart and the round trip is of the same order.
///
/// **The meter is the phone's own microphone, as the detector hears it** —
/// band-limited to 100 Hz–4 kHz and measured before anything else touches it,
/// which is the only level that says whether a tap will register. Shown only
/// while listening, because that is when the phone's microphone is the one
/// being tapped; while capturing it is the helmet's, and the talk panel's own
/// meters cover that.
///
/// The mark on the track is the threshold. A tap has to reach it, so a rider
/// can see whether theirs do rather than guess — which is the question three
/// days of this feature have turned on.
class _TapFeedback extends StatefulWidget {
  const _TapFeedback({required this.capturing});

  final bool capturing;

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

  Timer? _tick;
  int _run = 0;
  double _over = 0;

  @override
  void initState() {
    super.initState();
    _tick = Timer.periodic(_period, (_) => _poll());
  }

  @override
  void dispose() {
    _tick?.cancel();
    super.dispose();
  }

  void _poll() {
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
      // No engine; nothing to draw and nothing to report.
    }
  }

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    return Column(
      children: [
        if (!widget.capturing) ...[
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
                              ? StatusColors.talking
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
          const SizedBox(height: 8),
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
                  color: i < _run
                      ? StatusColors.talking
                      : scheme.outlineVariant,
                ),
              ),
          ],
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
