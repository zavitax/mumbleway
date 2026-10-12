import 'package:flutter/material.dart';

import '../l10n/app_localizations.dart';
import '../src/rust/api/mumbleway.dart' show UiAcousticStats;
import '../theme.dart';

/// The ten gates a frame passes on its way to being a tap, as lamps.
///
/// **Asked for by a rider, in those words: "then I will see where exactly
/// stuff stops".** Every other reading on the tap panel is a number that has
/// to be compared against a threshold by whoever is looking — 11 dB/ms means
/// nothing without knowing that the bar is 14 — and on a helmet there are
/// seven places a tap can be lost, each of which looks identical from outside:
/// nothing happens.
///
/// A lamp is lit when its stage passed something to the next one recently.
/// Read left to right, the row stops at the gate that is losing the taps.
///
/// **`arm` is the lamp that a field reading added.** Six lit and the rest dark
/// was ambiguous: the margin and the attack lamps answer for themselves, and
/// both being lit does not mean they were lit by the same millisecond. `arm`
/// is the one that says they met, which is what the detector actually needs.
class TapStageRow extends StatelessWidget {
  const TapStageRow({super.key, required this.mask});

  /// Bit *n* lit means stage *n* passed something. Zero draws the row dark,
  /// which is what a poller passes when no frames have arrived at all.
  final int mask;

  @override
  Widget build(BuildContext context) {
    final l = L.of(context);
    final scheme = Theme.of(context).colorScheme;
    // The order is the order a frame passes them, which is the whole point:
    // the last lit lamp is the last thing that worked. It matches
    // `core::audio::acoustic_tap::stage`, and the two must not drift — the
    // bitmask is positional.
    final labels = <String>[
      l.tapStageInput,
      l.tapStageBand,
      l.tapStageHearing,
      l.tapStageFloor,
      l.tapStageMargin,
      l.tapStageAttack,
      l.tapStageArmed,
      l.tapStageFellBack,
      l.tapStageTail,
      l.tapStageBanked,
    ];
    return Row(
      children: [
        for (var i = 0; i < labels.length; i++)
          Expanded(
            child: Column(
              children: [
                AnimatedContainer(
                  duration: const Duration(milliseconds: 120),
                  width: mask & (1 << i) != 0 ? 12 : 8,
                  height: mask & (1 << i) != 0 ? 12 : 8,
                  decoration: BoxDecoration(
                    shape: BoxShape.circle,
                    color: mask & (1 << i) != 0
                        ? StatusColors.talking
                        : scheme.outlineVariant,
                  ),
                ),
                const SizedBox(height: 3),
                Text(
                  labels[i],
                  maxLines: 1,
                  overflow: TextOverflow.clip,
                  textAlign: TextAlign.center,
                  style: TextStyle(
                    fontSize: 8,
                    height: 1.1,
                    color: scheme.onSurfaceVariant,
                  ),
                ),
              ],
            ),
          ),
      ],
    );
  }
}

/// Turns the engine's bitmask into something a panel may draw.
///
/// **One rule, and it is not optional.** The mask is held on the detector's
/// own frame clock, which advances only while audio arrives — so a microphone
/// that dies *freezes* the lamps at whatever last passed rather than clearing
/// them, and the row would go on describing a tap from a minute ago. The
/// frame count is what says whether anything is arriving, so a reader that
/// sees it stand still draws the row dark.
///
/// Stateful, because that comparison is between two polls. One per panel.
class TapStageReader {
  BigInt? _frames;

  /// The mask to draw. Zero until a second poll proves frames are moving.
  int read(UiAcousticStats s) {
    final previous = _frames;
    _frames = s.frames;
    if (previous == null || s.frames == previous) return 0;
    return s.stageMask;
  }
}
