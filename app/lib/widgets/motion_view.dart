import 'dart:async';

import 'package:flutter/material.dart';

import '../services/motion.dart';
import '../src/rust/api/mumbleway.dart';
import '../theme.dart';

/// What the motion sensors and the tap detector are actually doing.
///
/// **Built because "taps are not detected" could not be acted on.** That one
/// sentence covers two faults needing opposite fixes — no samples arriving at
/// all, which is a platform problem, or samples arriving and the thresholds
/// throwing them away, which is a tuning one — and from outside the app they
/// look identical. Nothing on screen could tell them apart.
///
/// So the first row is the sample rate, and it answers that question on its
/// own: **zero means the sensors are not delivering.** Everything below it is
/// only worth reading once that number is not zero.
///
/// Shown whether or not the gesture is switched on, deliberately. The detector
/// runs regardless — a few multiplies per sample — precisely so this panel can
/// be used to work out why nothing fires before anybody commits to the feature.
class MotionView extends StatefulWidget {
  const MotionView({super.key});

  @override
  State<MotionView> createState() => _MotionViewState();
}

class _MotionViewState extends State<MotionView> {
  Timer? _tick;
  UiTapStats _s = UiTapStats(
    enabled: false,
    samples: BigInt.zero,
    candidates: BigInt.zero,
    discardedLong: BigInt.zero,
    discardedMagnitude: BigInt.zero,
    discardedInterval: BigInt.zero,
    gestures: BigInt.zero,
    psiDb: 0,
    floorDb: 0,
    peakDb: 0,
    pending: 0,
    hz: 0,
  );

  @override
  void initState() {
    super.initState();
    // Once a second, like the rest of the panel. A tap is a transient this
    // cannot hope to draw, and trying would cost more than it showed — the
    // counters are what identify the fault, not the waveform.
    _tick = Timer.periodic(const Duration(seconds: 1), (_) => _refresh());
    _refresh();
  }

  @override
  void dispose() {
    _tick?.cancel();
    super.dispose();
  }

  void _refresh() {
    try {
      final s = tapDiagnostics();
      if (mounted) setState(() => _s = s);
    } catch (_) {
      // No engine yet. The zeros already on screen are the honest answer.
    }
  }

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    final samples = _s.samples.toInt();
    final dead = samples == 0;

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Row(
          children: [
            Icon(
              dead ? Icons.sensors_off : Icons.sensors,
              size: 16,
              color: dead ? StatusColors.failed : scheme.onSurfaceVariant,
            ),
            const SizedBox(width: 6),
            Text(
              'Motion',
              style: Theme.of(context).textTheme.labelLarge,
            ),
            const Spacer(),
            Text(
              _s.enabled ? 'gesture on' : 'gesture off',
              style: TextStyle(fontSize: 11, color: scheme.onSurfaceVariant),
            ),
          ],
        ),
        const SizedBox(height: 4),
        // **Three faults look identical from outside and need opposite
        // fixes**, so each gets its own row rather than one "it does not
        // work": the platform saying it has no sensors, nobody having asked
        // for them, and having asked and getting nothing.
        _line(
          context,
          'sensors',
          switch (MotionBridge.instance.availableCached) {
            null => 'not asked yet',
            false =>
              'platform says none — a simulator has no motion hardware',
            true => 'available',
          },
          bad: MotionBridge.instance.availableCached == false,
        ),
        _line(
          context,
          'listening',
          MotionBridge.instance.running ? 'yes' : 'no — nobody asked',
          bad: !MotionBridge.instance.running,
        ),
        // Android should show 200–500 Hz and iOS about 100. Well under either
        // means the platform is capping delivery.
        _line(
          context,
          'rate',
          dead
              ? 'no samples arriving'
              : '${_s.hz.toStringAsFixed(0)} Hz · $samples samples',
          bad: dead,
        ),
        if (!dead) ...[
          // ψ against the floor it has to clear. If these two sit on top of
          // each other no impulse can ever stand out, which is a different
          // fault from an impulse being rejected after standing out.
          _line(
            context,
            'level',
            'ψ ${_s.psiDb.toStringAsFixed(1)} dB · floor '
                '${_s.floorDb.toStringAsFixed(1)} dB · over '
                '${(_s.psiDb - _s.floorDb).toStringAsFixed(1)} dB',
          ),
          _line(
            context,
            'last peak',
            '${_s.peakDb.toStringAsFixed(1)} dB over floor',
          ),
          _line(
            context,
            'taps',
            '${_s.candidates} found · ${_s.pending} banked · '
                '${_s.gestures} gestures',
          ),
          // **The row to read when samples arrive and nothing fires.** A tap
          // through a padded bag rings, and if it rings past the pulse-width
          // limit every one is thrown away while the signal is perfectly good.
          _line(
            context,
            'rejected',
            '${_s.discardedLong} too long · ${_s.discardedMagnitude} unmatched · '
                '${_s.discardedInterval} off-beat',
            bad: _s.discardedLong.toInt() > 0 && _s.candidates == BigInt.zero,
          ),
          if (MotionBridge.instance.dropped > 0)
            _line(
              context,
              'dropped',
              '${MotionBridge.instance.dropped} before the engine',
              bad: true,
            ),
        ],
      ],
    );
  }

  Widget _line(
    BuildContext context,
    String label,
    String value, {
    bool bad = false,
  }) {
    final scheme = Theme.of(context).colorScheme;
    return Padding(
      padding: const EdgeInsets.only(bottom: 2),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          SizedBox(
            width: 68,
            child: Text(
              label,
              style: TextStyle(fontSize: 11, color: scheme.onSurfaceVariant),
            ),
          ),
          Expanded(
            child: Text(
              value,
              style: TextStyle(
                fontSize: 11,
                fontFeatures: const [FontFeature.tabularFigures()],
                color: bad ? StatusColors.failed : scheme.onSurface,
              ),
            ),
          ),
        ],
      ),
    );
  }
}
