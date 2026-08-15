import 'package:flutter/foundation.dart';
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
/// Three bars, because the three quantities are not the same kind of thing and
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
/// * **The Auto chooser's SNR gets its own bar and cannot borrow the first
///   one's.** It is measured against a different floor — the room before the
///   enhancer, where the first bar's floor is what the chain left behind — and
///   on three recordings the two read −19/−60/−53 against −72/−80/−92 dBFS.
///   Marking the profile boundaries on the first bar would put them tens of
///   decibels from where they are, in a picture whose whole claim is that a
///   position can be read off it. So this one is a ratio on a ratio's scale,
///   0 to 60 dB, and its boundaries are drawn where they actually fall.
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

  /// Wide enough for three bars. The analyser beside it is `Expanded`, so
  /// this is taken from it rather than from the panel.
  static const double width = 110;

  /// The top of the Auto gauge's scale, in dB of signal over background.
  ///
  /// Sixty covers everything measured — a quiet room reads in the forties and
  /// a motorway in the low teens — with the upper boundary at 35 sitting just
  /// above the middle, so both bands have room to be seen rather than one
  /// being a sliver at the top.
  static const double _autoScaleDb = 60;

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

    // Independent of `live`: the latch is the chooser's, not the gate's, and
    // it survives the warm-up that makes the other two unreadable.
    final auto = s?.autoSnrDb;

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
          const SizedBox(width: 6),
          // **The chooser, not the gate.** Nothing is latched until somebody
          // speaks and nothing is ever latched under a hand-set profile, so
          // this bar is empty far more often than the other two — which is the
          // honest state and not a fault to paper over.
          Expanded(
            child: _Gauge(
              label: l.diagGaugeAuto,
              caption: auto != null ? '${auto.toStringAsFixed(0)} dB' : '—',
              fill: auto == null ? null : (auto / _autoScaleDb).clamp(0.0, 1.0),
              // Two of them, and they are the point of this bar. Drawn lit when
              // there is a measurement between them and dim when there is not:
              // an unlatched boundary is where a choice *would* be made, and a
              // latched one is where a choice *was*, which are different enough
              // to be worth telling apart at a glance.
              // **No fallback pair here on purpose.** A `?? 20, ?? 35` would
              // put this painter's own copy of the boundaries back in, which
              // is the thing carrying them across the bridge was for. With
              // nothing to draw them from, nothing is drawn.
              bands: s == null
                  ? const []
                  : [
                      s.autoHelmetBelowDb / _autoScaleDb,
                      s.autoStandardBelowDb / _autoScaleDb,
                    ],
              bandsLit: auto != null,
              thresholdAt: null,
              passing: auto != null,
              latched: auto != null,
              colour: _autoColour(auto, s, scheme),
              dimColour: dim,
              grid: scheme.onSurfaceVariant.withValues(alpha: 0.16),
            ),
          ),
        ],
      ),
    );
  }

  /// The colour of the Auto bar, which is the band the latched SNR fell in.
  ///
  /// **The same three colours the rest of the panel uses for how hard a stage
  /// is working**, so a rider who has learned them anywhere else has learned
  /// them here: green is the lightest profile, amber the middle one, red the
  /// heaviest. Red is not an error — a motorway genuinely wants `Helmet` — but
  /// it is the reading that explains a voice sounding processed, which is the
  /// question this bar is looked at to answer.
  Color _autoColour(double? auto, GateSnapshot? s, ColorScheme scheme) {
    if (auto == null || s == null) return scheme.primary;
    if (auto < s.autoHelmetBelowDb) return StatusColors.failed;
    if (auto < s.autoStandardBelowDb) return StatusColors.connecting;
    return StatusColors.connected;
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
    this.bands = const [],
    this.bandsLit = false,
  });

  final String label;
  final String caption;

  /// Fixed divisions up the bar, 0..1, drawn across it.
  ///
  /// Distinct from [thresholdAt], which is one moving line the reading has to
  /// clear. These do not move and nothing clears them: they say which of
  /// several answers a reading falls under.
  final List<double> bands;

  /// Whether the divisions are lit or dim.
  ///
  /// **This is the whole of what "latched" looks like here.** Dim means the
  /// boundaries are merely where a choice would be made; lit means one was
  /// made, and the bar under them is the measurement it was made from.
  final bool bandsLit;

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
              bands: bands,
              bandsLit: bandsLit,
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
    required this.bands,
    required this.bandsLit,
    required this.thresholdAt,
    required this.passing,
    required this.latched,
    required this.colour,
    required this.dimColour,
    required this.grid,
  });

  final double? fill;
  final List<double> bands;
  final bool bandsLit;
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

    // Dim boundaries go under the fill; lit ones go over it, below. A
    // boundary the reading has climbed past is the one most worth seeing, and
    // drawing it first would put it behind exactly then.
    if (!bandsLit) _drawBands(canvas, size, dimColour.withValues(alpha: 0.55));

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

    if (bandsLit) {
      // The one place a threshold changes colour rather than position: these
      // are the same two lines either way, and only the state behind them has
      // moved.
      _drawBands(canvas, size, colour.withValues(alpha: 0.95));
    }

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

  void _drawBands(Canvas canvas, Size size, Color c) {
    final p = Paint()
      ..color = c
      ..strokeWidth = 1;
    for (final b in bands) {
      final y = size.height - b.clamp(0.0, 1.0) * size.height;
      // Dashed, so a fixed division cannot be mistaken for the solid line the
      // other two bars use for a threshold that moves.
      for (double x = 0; x < size.width; x += 4) {
        canvas.drawLine(Offset(x, y), Offset(x + 2, y), p);
      }
    }
  }

  @override
  bool shouldRepaint(_GaugePainter old) =>
      old.fill != fill ||
      old.thresholdAt != thresholdAt ||
      old.passing != passing ||
      old.latched != latched ||
      old.bandsLit != bandsLit ||
      old.colour != colour ||
      !listEquals(old.bands, bands);
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
    required double? autoSnrDb,
    required this.autoHelmetBelowDb,
    required this.autoStandardBelowDb,
    required this.voicedThreshold,
    required this.floorHeld,
    required this.applicable,
  })  : levelDb = _q(levelDb, 1),
        noiseFloorDb = _q(noiseFloorDb, 1),
        opensAtDb = _q(opensAtDb, 1),
        harmonicity = _q(harmonicity, 0.02),
        // Whole decibels. It only moves at a speech onset, so this is not
        // about repaint cost — it is that the bar is sixty pixels for sixty
        // decibels, and a tenth of one is a tenth of a pixel.
        autoSnrDb = autoSnrDb == null ? null : _q(autoSnrDb, 1);

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

  /// The SNR the `Auto` profile was last chosen from, and the two boundaries
  /// it was judged against. All in dB over the background.
  ///
  /// Null until a phrase has been heard under `Auto`. **Measured against a
  /// different floor from [noiseFloorDb]** — the room before the enhancer,
  /// where that one is what the chain left behind — which is why it is drawn
  /// on a bar of its own rather than marked on the first one.
  final double? autoSnrDb;
  final double autoHelmetBelowDb, autoStandardBelowDb;

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
      other.autoSnrDb == autoSnrDb &&
      other.autoHelmetBelowDb == autoHelmetBelowDb &&
      other.autoStandardBelowDb == autoStandardBelowDb &&
      other.voicedThreshold == voicedThreshold &&
      other.floorHeld == floorHeld &&
      other.applicable == applicable;

  @override
  int get hashCode => Object.hash(
        levelDb,
        noiseFloorDb,
        opensAtDb,
        harmonicity,
        autoSnrDb,
        autoHelmetBelowDb,
        autoStandardBelowDb,
        voicedThreshold,
        floorHeld,
        applicable,
      );
}
