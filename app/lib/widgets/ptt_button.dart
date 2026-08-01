import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../src/rust/api/mumbleway.dart';
import '../state/app_state.dart';
import '../theme.dart';

/// The big talk control.
///
/// Deliberately oversized: this is operated with winter gloves on, often
/// without looking. In push-to-talk mode it keys while held; in the other modes
/// it becomes a live status light showing whether audio is being transmitted.
class PttButton extends StatelessWidget {
  const PttButton({super.key});

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
          height: 132,
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
          child: Column(
            mainAxisAlignment: MainAxisAlignment.center,
            children: [
              Icon(
                state.muted
                    ? Icons.mic_off
                    : live
                        ? Icons.mic
                        : Icons.mic_none,
                size: 44,
                color: live ? Colors.white : null,
              ),
              const SizedBox(height: 6),
              Text(
                state.muted
                    ? 'MICROPHONE MUTED'
                    : ptt
                        ? (live ? 'TRANSMITTING' : 'HOLD TO TALK')
                        : (live ? 'TRANSMITTING' : _idleLabel(state.micMode)),
                style: TextStyle(
                  fontSize: 15,
                  fontWeight: FontWeight.w800,
                  letterSpacing: 0.6,
                  color: live ? Colors.white : null,
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }

  static String _idleLabel(MicMode mode) =>
      mode == MicMode.continuous ? 'OPEN MIC' : 'VOICE ACTIVATED';

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
class LevelMeter extends StatelessWidget {
  const LevelMeter({super.key});

  @override
  Widget build(BuildContext context) {
    final state = AppStateScope.of(context);
    // Map -60..0 dBFS onto 0..1; below -60 there is nothing worth showing.
    final t = ((state.inputLevelDb + 60) / 60).clamp(0.0, 1.0);

    return Row(
      children: [
        Icon(
          state.speaking ? Icons.graphic_eq : Icons.remove,
          size: 18,
          color: state.speaking ? StatusColors.talking : StatusColors.idle,
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
                  state.speaking ? StatusColors.talking : StatusColors.idle,
                ),
              ),
            ),
          ),
        ),
      ],
    );
  }
}
