import 'package:flutter/material.dart';

import '../l10n/app_localizations.dart';
import '../theme.dart';

/// The two numbers the transmit decision is actually made on, drawn beside the
/// spectrum instead of listed under it.
///
/// **A threshold is a comparison, and a comparison between a picture and a
/// number an inch away is one the reader has to do in their head.** The panel
/// already had the SNR and the periodicity as figures in a column; what it
/// could not show was how close either was to the bar it has to clear, which
/// is the only thing worth knowing while watching a gate open and shut.
///
/// Two bars, because the two quantities are not the same kind of thing and
/// pretending otherwise would be the easy mistake here.
///
/// * **SNR shares the analyser's dBFS axis exactly**, and is drawn as the span
///   from the tracked floor up to the current level. Its *height* is therefore
///   the signal-to-noise ratio and its *position* is where that signal sits, so
///   it can be read against the traces to its left without any conversion. The
///   threshold is the floor plus the profile's margin, so it moves with the
///   floor, as the real one does.
/// * **Periodicity has no decibels in it.** It is a correlation, 0 to 1, so it
///   gets its own scale over the same height and says so. Sharing the axis
///   would put a number that is not a level on a level's ruler.
class GateIndicators extends StatelessWidget {
  const GateIndicators({
    super.key,
    required this.snapshot,
    required this.floorDb,
  });

  /// What to draw, or null when there is nothing to say.
  final GateSnapshot? snapshot;

  /// The bottom of the analyser's axis, so the two share a ruler.
  ///
  /// Taken from the spectrum frame rather than assumed, because that is where
  /// the analyser takes it from: a constant here would be right until somebody
  /// changed the other one.
  final double floorDb;

  static const double width = 74;

  @override
  Widget build(BuildContext context) {
    final l = L.of(context);
    final scheme = Theme.of(context).colorScheme;
    final s = snapshot;

    // **Greyed rather than hidden when there is nothing behind it.** A section
    // that vanishes reads as a layout bug; one that is present and dim reads as
    // "not applicable here", which is what it is. With suppression off there is
    // no tracked floor and no margin, so neither bar has a threshold to be
    // measured against.
    final live = s != null && s.applicable;
    final dim = scheme.onSurfaceVariant.withValues(alpha: 0.30);

    return SizedBox(
      width: width,
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Expanded(
            child: _Gauge(
              label: l.diagGaugeSnr,
              caption: live ? '${s.snrDb.toStringAsFixed(0)} dB' : '—',
              fill: live ? _snrFill(s) : null,
              thresholdAt: live ? _snrThreshold(s) : null,
              passing: live && s.levelDb >= s.opensAtDb,
              latched: live && s.floorHeld,
              colour: scheme.primary,
              dimColour: dim,
              grid: scheme.onSurfaceVariant.withValues(alpha: 0.16),
            ),
          ),
          const SizedBox(width: 6),
          Expanded(
            child: _Gauge(
              label: l.diagGaugePitch,
              caption: live ? s.harmonicity.toStringAsFixed(2) : '—',
              fill: live ? s.harmonicity.clamp(0.0, 1.0) : null,
              thresholdAt: live ? s.voicedThreshold.clamp(0.0, 1.0) : null,
              passing: live && s.harmonicity >= s.voicedThreshold,
              latched: false,
              colour: StatusColors.connected,
              dimColour: dim,
              grid: scheme.onSurfaceVariant.withValues(alpha: 0.16),
            ),
          ),
        ],
      ),
    );
  }

  /// Where the level sits on the analyser's axis, 0 at the bottom.
  double _snrFill(GateSnapshot s) =>
      ((s.levelDb - floorDb) / (0 - floorDb)).clamp(0.0, 1.0);

  /// Where the gate's bar sits on the same axis.
  ///
  /// `opensAtDb` is the floor plus the margin actually in force, relief
  /// included, so it moves with the floor exactly as the real threshold does.
  double _snrThreshold(GateSnapshot s) =>
      ((s.opensAtDb - floorDb) / (0 - floorDb)).clamp(0.0, 1.0);
}

/// One vertical bar with a threshold across it.
class _Gauge extends StatelessWidget {
  const _Gauge({
    required this.label,
    required this.caption,
    required this.fill,
    required this.thresholdAt,
    required this.passing,
    required this.latched,
    required this.colour,
    required this.dimColour,
    required this.grid,
  });

  final String label;
  final String caption;

  /// 0..1 up the bar, or null when there is nothing to draw.
  final double? fill;
  final double? thresholdAt;
  final bool passing;

  /// Whether the floor under this reading is being held.
  ///
  /// Drawn as a bracket on the threshold rather than a word: the threshold is
  /// the thing that is latched, and a caption saying so would be read after
  /// the shape has already been understood.
  final bool latched;

  final Color colour, dimColour, grid;

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    return Column(
      children: [
        Expanded(
          child: CustomPaint(
            size: Size.infinite,
            painter: _GaugePainter(
              fill: fill,
              thresholdAt: thresholdAt,
              passing: passing,
              latched: latched,
              colour: colour,
              dimColour: dimColour,
              grid: grid,
            ),
          ),
        ),
        const SizedBox(height: 2),
        Text(
          caption,
          maxLines: 1,
          overflow: TextOverflow.clip,
          style: TextStyle(
            fontSize: 9,
            height: 1.1,
            fontFeatures: const [FontFeature.tabularFigures()],
            color: fill == null ? dimColour : scheme.onSurfaceVariant,
          ),
        ),
        Text(
          label,
          maxLines: 1,
          overflow: TextOverflow.clip,
          style: TextStyle(
            fontSize: 8,
            height: 1.2,
            color: fill == null ? dimColour : scheme.onSurfaceVariant,
          ),
        ),
      ],
    );
  }
}

class _GaugePainter extends CustomPainter {
  const _GaugePainter({
    required this.fill,
    required this.thresholdAt,
    required this.passing,
    required this.latched,
    required this.colour,
    required this.dimColour,
    required this.grid,
  });

  final double? fill;
  final double? thresholdAt;
  final bool passing, latched;
  final Color colour, dimColour, grid;

  @override
  void paint(Canvas canvas, Size size) {
    final track = Paint()..color = grid;
    canvas.drawRRect(
      RRect.fromRectAndRadius(
        Rect.fromLTWH(0, 0, size.width, size.height),
        const Radius.circular(2),
      ),
      track,
    );

    final f = fill;
    if (f == null) {
      // Not applicable: the track alone, with a slash through it, which reads
      // as "nothing to measure" rather than "measured zero".
      canvas.drawLine(
        Offset(0, size.height),
        Offset(size.width, 0),
        Paint()
          ..color = dimColour
          ..strokeWidth = 1,
      );
      return;
    }

    final top = size.height - f * size.height;
    canvas.drawRRect(
      RRect.fromRectAndRadius(
        Rect.fromLTWH(0, top, size.width, size.height - top),
        const Radius.circular(2),
      ),
      Paint()..color = colour.withValues(alpha: passing ? 0.85 : 0.35),
    );

    final t = thresholdAt;
    if (t == null) return;
    final y = size.height - t * size.height;
    canvas.drawLine(
      Offset(0, y),
      Offset(size.width, y),
      Paint()
        ..color = dimColour.withValues(alpha: 0.9)
        ..strokeWidth = 1.5,
    );
    if (latched) {
      // A bracket on the threshold, saying the floor it rides on is pinned.
      final p = Paint()
        ..color = dimColour.withValues(alpha: 0.9)
        ..strokeWidth = 1.5;
      canvas.drawLine(Offset(0, y - 3), Offset(0, y + 3), p);
      canvas.drawLine(
        Offset(size.width, y - 3),
        Offset(size.width, y + 3),
        p,
      );
    }
  }

  @override
  bool shouldRepaint(_GaugePainter old) =>
      old.fill != fill ||
      old.thresholdAt != thresholdAt ||
      old.passing != passing ||
      old.latched != latched;
}

/// Exactly what the two gauges draw, and nothing else.
///
/// **A separate value so the notifier can stay silent.** `UiChainStatus` is
/// rebuilt by the bridge on every poll and is never equal to the last one, so
/// watching it directly would repaint these twenty times a second to draw the
/// same picture — which is the cost the rest of this panel was restructured to
/// avoid. This holds the six numbers that matter, quantised to what the eye can
/// resolve on a bar a few dozen pixels tall.
@immutable
class GateSnapshot {
  GateSnapshot({
    required double levelDb,
    required double noiseFloorDb,
    required double opensAtDb,
    required double harmonicity,
    required this.voicedThreshold,
    required this.floorHeld,
    required this.applicable,
  })  : levelDb = _q(levelDb, 1),
        noiseFloorDb = _q(noiseFloorDb, 1),
        opensAtDb = _q(opensAtDb, 1),
        harmonicity = _q(harmonicity, 0.02);

  /// The signal-to-noise ratio, which is the distance between two of the above
  /// rather than a number of its own. Derived here so the bar's height and its
  /// caption cannot disagree.
  double get snrDb => levelDb - noiseFloorDb;

  /// Rounded to a step, because a bar 60 pixels tall cannot show a hundredth
  /// of a decibel and a value that jitters in the last digit would repaint on
  /// every poll for ever.
  static double _q(double v, double step) => (v / step).roundToDouble() * step;

  final double levelDb, noiseFloorDb, opensAtDb, harmonicity;
  final double voicedThreshold;

  /// Whether the noise floor is currently pinned. See `NoiseFloorTracker`.
  final bool floorHeld;

  /// False when there is no suppression running, so no floor and no margin.
  final bool applicable;

  @override
  bool operator ==(Object other) =>
      other is GateSnapshot &&
      other.levelDb == levelDb &&
      other.noiseFloorDb == noiseFloorDb &&
      other.opensAtDb == opensAtDb &&
      other.harmonicity == harmonicity &&
      other.voicedThreshold == voicedThreshold &&
      other.floorHeld == floorHeld &&
      other.applicable == applicable;

  @override
  int get hashCode => Object.hash(
        levelDb,
        noiseFloorDb,
        opensAtDb,
        harmonicity,
        voicedThreshold,
        floorHeld,
        applicable,
      );
}
