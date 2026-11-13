import 'package:flutter/material.dart';

import '../l10n/app_localizations.dart';

/// The pill that marks a feature as experimental.
///
/// **Deliberately not a `Chip`.** A `Chip` is 32 px tall, carries its own ink
/// and tap target, and is a control the rider is meant to press. This is a
/// label on a heading — pressing it does nothing — and at a section header's
/// 12 px a Chip beside it makes the heading look like the smaller of the two.
///
/// It takes its colour from `tertiary`, which is the one role in the scheme
/// this app does not otherwise use: `primary` is every heading and every
/// filled button, `error` is the armed slider and the fault lamps in
/// diagnostics. A badge that borrowed either would read as "broken" or as
/// "press me", and it is neither.
///
/// The word itself comes from `l.experimental` rather than the caller, so the
/// second feature to need one cannot quietly word it differently.
class ExperimentalBadge extends StatelessWidget {
  const ExperimentalBadge({super.key});

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 2),
      decoration: BoxDecoration(
        color: scheme.tertiaryContainer,
        borderRadius: BorderRadius.circular(4),
      ),
      child: Text(
        L.of(context).experimental.toUpperCase(),
        style: TextStyle(
          fontSize: 10,
          fontWeight: FontWeight.w800,
          letterSpacing: 0.8,
          color: scheme.onTertiaryContainer,
        ),
      ),
    );
  }
}
