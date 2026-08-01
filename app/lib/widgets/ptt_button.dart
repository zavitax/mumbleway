import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../src/rust/api/mumbleway.dart';
import '../state/app_state.dart';
import '../theme.dart';

/// Horizontal padding kept clear inside the talk button.
///
/// The label is scaled down to fit within this margin rather than being
/// allowed to touch the edges or overflow, which it otherwise does at large
/// system text sizes and for the longer strings such as "MICROPHONE MUTED".
const double _kLabelMargin = 28;

/// The big talk control.
///
/// Deliberately oversized: this is operated with winter gloves on, often
/// without looking. In push-to-talk mode it keys while held; in the other modes
/// it becomes a live status light showing whether audio is being transmitted.
class PttButton extends StatelessWidget {
  const PttButton({super.key, this.height = 132});

  final double height;

  @override
  Widget build(BuildContext context) {
    final state = AppStateScope.of(context);
    final ptt = state.micMode == MicMode.pushToTalk;
    final live = ptt ? state.transmitting : state.speaking;
    final enabled = !state.muted;

    final color = !enabled
        ? StatusColors.idle
        : live
            ? StatusColors.talking
            : Theme.of(context).colorScheme.surfaceContainerHighest;

    return Semantics(
      button: ptt,
      label: ptt ? 'Push to talk' : 'Transmission indicator',
      child: GestureDetector(
        onTapDown: ptt && enabled ? (_) => _press(state) : null,
        onTapUp: ptt && enabled ? (_) => _release(state) : null,
        onTapCancel: ptt && enabled ? () => _release(state) : null,
        child: AnimatedContainer(
          duration: const Duration(milliseconds: 120),
          height: height,
          decoration: BoxDecoration(
            color: color,
            borderRadius: BorderRadius.circular(24),
            border: Border.all(
              color: live ? StatusColors.talking : Colors.transparent,
              width: 3,
            ),
            boxShadow: live
                ? [
                    BoxShadow(
                      color: StatusColors.talking.withValues(alpha: 0.45),
                      blurRadius: 28,
                      spreadRadius: 2,
                    )
                  ]
                : null,
          ),
          child: Padding(
            // Keeps the contents clear of the rounded corners and the border.
            padding: const EdgeInsets.symmetric(
              horizontal: _kLabelMargin,
              vertical: 12,
            ),
            child: Column(
              mainAxisAlignment: MainAxisAlignment.center,
              mainAxisSize: MainAxisSize.min,
              children: [
                Flexible(
                  child: FittedBox(
                    fit: BoxFit.scaleDown,
                    child: Icon(
                      state.muted
                          ? Icons.mic_off
                          : live
                              ? Icons.mic
                              : Icons.mic_none,
                      size: 44,
                      color: live ? Colors.white : null,
                    ),
                  ),
                ),
                const SizedBox(height: 6),
                // Scale-down rather than wrap or clip: the label must stay on
                // one line and inside the margin at every text scale.
                Flexible(
                  child: FittedBox(
                    fit: BoxFit.scaleDown,
                    child: Text(
                      _label(state, ptt, live),
                      maxLines: 1,
                      softWrap: false,
                      overflow: TextOverflow.visible,
                      textAlign: TextAlign.center,
                      style: TextStyle(
                        fontSize: 15,
                        fontWeight: FontWeight.w800,
                        letterSpacing: 0.6,
                        color: live ? Colors.white : null,
                      ),
                    ),
                  ),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }

  static String _label(AppState state, bool ptt, bool live) {
    if (state.muted) return 'MICROPHONE MUTED';
    if (live) return 'TRANSMITTING';
    if (ptt) return 'HOLD TO TALK';
    return state.micMode == MicMode.continuous ? 'OPEN MIC' : 'VOICE ACTIVATED';
  }

  void _press(AppState state) {
    HapticFeedback.mediumImpact();
    state.setTransmit(true);
  }

  void _release(AppState state) {
    HapticFeedback.lightImpact();
    state.setTransmit(false);
  }
}

/// Horizontal microphone level meter with a speech indicator.
///
/// Shown permanently rather than only in settings: knowing the microphone is
/// live is the single most useful thing on the screen while riding.
class LevelMeter extends StatelessWidget {
  const LevelMeter({super.key, this.showScale = false});

  /// Adds dBFS labelling, used on the audio settings screen.
  final bool showScale;

  @override
  Widget build(BuildContext context) {
    final state = AppStateScope.of(context);
    // Map -60..0 dBFS onto 0..1; below -60 there is nothing worth showing.
    final t = ((state.inputLevelDb + 60) / 60).clamp(0.0, 1.0);

    return Row(
      children: [
        Icon(
          state.muted
              ? Icons.mic_off
              : state.speaking
                  ? Icons.graphic_eq
                  : Icons.mic_none,
          size: 18,
          color: state.muted
              ? StatusColors.failed
              : state.speaking
                  ? StatusColors.talking
                  : StatusColors.idle,
        ),
        const SizedBox(width: 10),
        Expanded(
          child: ClipRRect(
            borderRadius: BorderRadius.circular(999),
            child: TweenAnimationBuilder<double>(
              tween: Tween(begin: 0, end: t),
              duration: const Duration(milliseconds: 90),
              builder: (context, v, _) => LinearProgressIndicator(
                value: v,
                minHeight: 10,
                backgroundColor:
                    Theme.of(context).colorScheme.surfaceContainerHighest,
                valueColor: AlwaysStoppedAnimation(
                  state.muted
                      ? StatusColors.failed
                      : state.speaking
                          ? StatusColors.talking
                          : StatusColors.idle,
                ),
              ),
            ),
          ),
        ),
        if (showScale) ...[
          const SizedBox(width: 10),
          SizedBox(
            width: 62,
            child: Text(
              state.inputLevelDb <= -119
                  ? '—'
                  : '${state.inputLevelDb.toStringAsFixed(0)} dB',
              textAlign: TextAlign.right,
              style: const TextStyle(fontSize: 11, fontFeatures: [
                FontFeature.tabularFigures(),
              ]),
            ),
          ),
        ],
      ],
    );
  }
}
