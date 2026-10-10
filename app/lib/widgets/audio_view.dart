import 'package:flutter/material.dart';

import '../state/app_state.dart';
import '../theme.dart';

/// What the audio session is doing, and whether it agrees with itself.
///
/// **Every defect in capture-on-demand was found by a rider noticing a
/// sound.** Ten TestFlight builds in four days, and the panel that exists to
/// explain the audio chain said nothing at all about the session: not the
/// route, not whether the microphone was open, and above all not whether what
/// the app had asked for and what it had got were the same thing.
///
/// The disagreement is the fault, in every single case. So it is a row.
class AudioView extends StatelessWidget {
  const AudioView({super.key});

  @override
  Widget build(BuildContext context) {
    final state = AppStateScope.of(context);
    final scheme = Theme.of(context).colorScheme;

    final desired = state.captureDesired;
    final actual = state.capturing;
    final settling = state.captureChanging;
    // Only a *settled* disagreement is a fault. Mid-transition they differ by
    // definition, and reading that as broken is the mistake the iOS handler
    // made for three builds running.
    final disagrees = !settling && desired != actual;

    final claims = state.captureClaims;
    final who = claims.isEmpty
        ? 'nobody'
        : claims.map((c) => c.name).join(' · ');

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Row(
          children: [
            Icon(
              disagrees ? Icons.sync_problem : Icons.graphic_eq,
              size: 16,
              color: disagrees
                  ? StatusColors.failed
                  : scheme.onSurfaceVariant,
            ),
            const SizedBox(width: 6),
            Text('Audio', style: Theme.of(context).textTheme.labelLarge),
            const Spacer(),
            Text(
              state.audioActive ? 'session up' : 'no session',
              style: TextStyle(fontSize: 11, color: scheme.onSurfaceVariant),
            ),
          ],
        ),
        const SizedBox(height: 4),
        _line(context, 'asked by', who),
        // The two rows that matter. Named "wanted" and "open" rather than
        // desired/actual because that is what they are to a rider.
        _line(
          context,
          'microphone',
          settling
              ? 'changing — wanted ${_on(desired)}, open ${_on(actual)}'
              : 'wanted ${_on(desired)} · open ${_on(actual)}',
          bad: disagrees,
        ),
        if (disagrees)
          _line(
            context,
            '',
            'these disagree, and nothing is in flight — this is a fault',
            bad: true,
          ),
        _line(context, 'route', _routeName(state.audioRoute)),
        _line(
          context,
          'gesture',
          state.tapToCapture
              ? 'on · ${state.tapCount} taps · rider wants '
                    '${_on(state.riderWantsMic)}'
              : 'off',
        ),
      ],
    );
  }

  static String _on(bool v) => v ? 'on' : 'off';

  /// `Recorded::route`'s numbers, which are a wire format shared with every
  /// recording already on a rider's phone.
  static String _routeName(int code) => switch (code) {
    0 => 'not reported',
    1 => 'built-in microphone',
    2 => 'wired headset',
    3 => 'Bluetooth hands-free',
    4 => 'USB',
    _ => 'other ($code)',
  };

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
