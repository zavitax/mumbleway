import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../l10n/app_localizations.dart';
import '../services/overlay.dart';
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
      child: Stack(
        children: [
          _body(context, state, ptt, live, enabled, color),
          // Sits over the button rather than inside it so the label keeps the
          // full width it needs to scale into.
          if (live)
            const Positioned(top: 12, right: 12, child: OnAirIndicator()),
        ],
      ),
    );
  }

  Widget _body(
    BuildContext context,
    AppState state,
    bool ptt,
    bool live,
    bool enabled,
    Color color,
  ) {
    return GestureDetector(
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
                      _label(context, state, ptt, live),
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
    );
  }

  static String _label(
    BuildContext context,
    AppState state,
    bool ptt,
    bool live,
  ) {
    final l = L.of(context);
    if (state.muted) return l.pttMicrophoneMuted;
    if (live) return l.pttTransmitting;
    if (ptt) return l.pttHoldToTalk;
    return state.micMode == MicMode.continuous
        ? l.pttOpenMic
        : l.pttVoiceActivated;
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

/// A tick on the level meter.
class _Marker extends StatelessWidget {
  const _Marker({
    required this.at,
    required this.width,
    required this.colour,
    required this.height,
  });

  final double at;
  final double width;
  final Color colour;
  final double height;

  @override
  Widget build(BuildContext context) {
    return Positioned(
      left: (width * at - 1).clamp(0.0, width - 2),
      child: Container(
        width: 2,
        height: height,
        decoration: BoxDecoration(
          color: colour,
          borderRadius: BorderRadius.circular(1),
        ),
      ),
    );
  }
}

/// Flashing round "on air" light, shown while audio is going out.
///
/// Blinks rather than sitting steady on purpose: a static colour is easy to
/// stop noticing, and a channel left keyed open — talking to a group that can
/// hear everything — is the failure worth catching. The rate is slow enough
/// not to strobe in peripheral vision on a moving bike.
class OnAirIndicator extends StatefulWidget {
  const OnAirIndicator({super.key, this.size = 14});

  final double size;

  @override
  State<OnAirIndicator> createState() => _OnAirIndicatorState();
}

class _OnAirIndicatorState extends State<OnAirIndicator>
    with SingleTickerProviderStateMixin {
  late final AnimationController _controller = AnimationController(
    vsync: this,
    duration: const Duration(milliseconds: 700),
  )..repeat(reverse: true);

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return FadeTransition(
      opacity: Tween<double>(begin: 1, end: 0.25).animate(_controller),
      child: Semantics(
        label: 'On air',
        child: Container(
          width: widget.size,
          height: widget.size,
          decoration: BoxDecoration(
            color: StatusColors.talking,
            shape: BoxShape.circle,
            boxShadow: [
              BoxShadow(
                color: StatusColors.talking.withValues(alpha: 0.6),
                blurRadius: 8,
                spreadRadius: 1,
              ),
            ],
          ),
        ),
      ),
    );
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
    // Shared with the floating windows so the meter, the threshold and the
    // noise marker cannot end up on different scales.
    final t = OverlayBridge.meterFraction(state.inputLevelDb);

    // The activation threshold only means anything in voice-activated mode,
    // where it tells the rider how far above the engine they have to speak.
    final showThreshold = state.micMode == MicMode.voiceActivity;
    final threshold = OverlayBridge.meterFraction(state.activationThresholdDb);
    final noiseFloor = OverlayBridge.meterFraction(state.noiseFloorDb);

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
          child: LayoutBuilder(
            builder: (context, constraints) => Stack(
              alignment: Alignment.centerLeft,
              children: [
                ClipRRect(
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
                // The tracked background noise, and above it the level voice
                // activation opens at. Both are shown because the gap between
                // them is the margin: at speed the floor climbs, and seeing
                // only the threshold makes that look like a control that has
                // drifted rather than wind.
                if (showThreshold) ...[
                  _Marker(
                    at: noiseFloor,
                    width: constraints.maxWidth,
                    colour: StatusColors.idle,
                    height: 12,
                  ),
                  _Marker(
                    at: threshold,
                    width: constraints.maxWidth,
                    colour: StatusColors.connecting,
                    height: 16,
                  ),
                ],
              ],
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
