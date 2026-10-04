import 'package:flutter/material.dart';

import '../theme.dart';

/// A mark on a meter, such as the level voice activation opens at.
class VoiceMeterMark {
  const VoiceMeterMark({
    required this.levelDb,
    required this.color,
    this.overhang = 3,
  });

  final double levelDb;
  final Color color;

  /// How far the tick stands proud of the track, top and bottom.
  final double overhang;
}

/// The one voice meter.
///
/// Every level in the app goes through this: the microphone on the home
/// screen, the microphone in settings, and each participant in the roster. A
/// meter is read by shape and colour rather than by number, so two meters that
/// look alike and fill differently are worse than no meter at all — the same
/// bar length has to mean the same loudness wherever it appears.
class VoiceMeter extends StatelessWidget {
  const VoiceMeter({
    super.key,
    required this.levelDb,
    this.muted = false,
    this.monochrome = false,
    this.width,
    this.height = 7,
    this.marks = const [],
    this.vertical = false,
  });

  final double levelDb;

  /// Drains the colour while still showing the level.
  ///
  /// For the input meter, where the question is not only how loud you are but
  /// whether any of it is leaving the device. The meter still moves — a rider
  /// needs to see the microphone is alive — but grey says nobody is hearing it,
  /// and colour arriving is the confirmation that they are. Distinct from
  /// [muted], which empties the meter because there is genuinely no level.
  final bool monochrome;

  /// Greys the meter out and empties it: a muted participant has no level
  /// worth showing, whatever is arriving.
  final bool muted;

  /// Fixed width, or null to fill whatever is available.
  final double? width;
  final double height;

  /// Stands the meter on end: it fills upward from the bottom and is as tall
  /// as it is told, instead of as wide.
  ///
  /// **For a row where the horizontal space belongs to the name.** A roster
  /// row has a name, a note, a badge and up to four glyphs competing for the
  /// width, and a bar lying across eighty pixels of it was the one thing there
  /// that did not need to be long: what it says is "how loud", which a column
  /// says in six pixels and the height of a glyph. The scale is the same one —
  /// same floor, same three gradient stops, same fall rate — so a given
  /// loudness fills the same fraction whichever way the meter is turned.
  final bool vertical;

  /// Ticks drawn over the track, on the same scale as the fill.
  final List<VoiceMeterMark> marks;

  /// Quietest level worth showing. Speech arrives around -30 dBFS, so a floor
  /// of -50 puts a normal voice comfortably past halfway rather than hard
  /// against the left edge.
  static const floorDb = -50.0;

  /// Matches the interval between level reports.
  ///
  /// Levels arrive ten times a second and fall in steps, which on screen is a
  /// visible stutter. Interpolating across exactly one interval turns the steps
  /// into a continuous slide; longer would lag behind the voice, shorter would
  /// leave a gap before the next value arrives. Linear on purpose — easing
  /// between consecutive steps would speed up and slow down within every one.
  static const _tween = Duration(milliseconds: 100);

  /// How fast a meter may fall, in dB per report.
  ///
  /// Reports arrive ten times a second, so this empties a normal speaking
  /// level in about a third of a second: fast enough to read as "they
  /// stopped", slow enough not to flicker between words. Rises are immediate,
  /// since anything slower clips the start of every word.
  ///
  /// The engine stops reporting once every speaker has gone quiet, and knows
  /// how long to keep sending empty reports for from this number and
  /// [silentDb]. Raising either — or lowering this — without revisiting
  /// `SILENT_LEVEL_TAIL` in `api/mumbleway.rs` leaves meters frozen part-way
  /// down instead of falling to nothing.
  static const fallPerReportDb = 9.0;

  /// Level reported when nothing is arriving at all.
  static const silentDb = -120.0;

  /// Follows a reported level, rising at once and falling no faster than the
  /// limit. Shared so every meter in the app fades identically.
  static double follow(double current, double reported) => reported >= current
      ? reported
      : (current - fallPerReportDb).clamp(reported, current);

  static double fractionFor(double db) {
    if (!db.isFinite) return 0;
    return ((db - floorDb) / -floorDb).clamp(0.0, 1.0);
  }

  @override
  Widget build(BuildContext context) {
    final filled = muted ? 0.0 : fractionFor(levelDb);
    final grey = Theme.of(context).colorScheme.onSurfaceVariant;

    final meter = LayoutBuilder(
      builder: (context, constraints) {
        final track = vertical ? constraints.maxHeight : constraints.maxWidth;
        final thickness = vertical ? (width ?? 6) : height;
        return SizedBox(
          height: vertical ? null : height,
          child: Stack(
            clipBehavior: Clip.none,
            children: [
              Positioned.fill(
                child: DecoratedBox(
                  decoration: BoxDecoration(
                    color: grey.withValues(alpha: 0.22),
                    borderRadius: BorderRadius.circular(thickness / 2),
                  ),
                ),
              ),
              ClipRRect(
                borderRadius: BorderRadius.circular(thickness / 2),
                child: TweenAnimationBuilder<double>(
                  tween: Tween(begin: 0, end: filled),
                  duration: _tween,
                  curve: Curves.linear,
                  builder: (context, value, child) => value <= 0.001
                      ? const SizedBox.shrink()
                      : ClipRect(
                          child: Align(
                            alignment: vertical
                                ? Alignment.bottomCenter
                                : Alignment.centerLeft,
                            // Shrinks this Align to a fraction of its child
                            // while the child keeps its full size, so the
                            // gradient always spans the whole track and a given
                            // colour always means the same loudness. Sizing the
                            // gradient to the filled part instead would paint a
                            // quiet talker red at full scale.
                            widthFactor: vertical ? null : value,
                            heightFactor: vertical ? value : null,
                            child: child,
                          ),
                        ),
                  child: SizedBox(
                    width: vertical ? thickness : track,
                    height: vertical ? track : height,
                    child: DecoratedBox(
                      decoration: BoxDecoration(
                        // The same three stops either way, so the shape of the
                        // scale does not change when the colour does: a level
                        // that was two thirds along stays two thirds along, and
                        // only its meaning about being heard changes.
                        gradient: LinearGradient(
                          // Upwards when stood on end, so the quiet end is at
                          // the bottom where the fill starts.
                          begin: vertical
                              ? Alignment.bottomCenter
                              : Alignment.centerLeft,
                          end: vertical
                              ? Alignment.topCenter
                              : Alignment.centerRight,
                          colors: monochrome
                              ? [
                                  grey.withValues(alpha: 0.45),
                                  grey.withValues(alpha: 0.62),
                                  grey.withValues(alpha: 0.85),
                                ]
                              : const [
                                  StatusColors.connected,
                                  StatusColors.connecting,
                                  StatusColors.failed,
                                ],
                          stops: const [0.0, 0.6, 1.0],
                        ),
                      ),
                    ),
                  ),
                ),
              ),
              for (final mark in marks)
                Positioned(
                  left: vertical
                      ? -mark.overhang
                      : (track * fractionFor(mark.levelDb) - 1).clamp(
                          0.0,
                          (track - 2).clamp(0.0, double.infinity),
                        ),
                  right: vertical ? -mark.overhang : null,
                  bottom: vertical
                      ? (track * fractionFor(mark.levelDb) - 1).clamp(
                          0.0,
                          (track - 2).clamp(0.0, double.infinity),
                        )
                      : -mark.overhang,
                  top: vertical ? null : -mark.overhang,
                  child: Container(
                    width: vertical ? null : 2,
                    height: vertical ? 2 : null,
                    decoration: BoxDecoration(
                      color: mark.color,
                      borderRadius: BorderRadius.circular(1),
                    ),
                  ),
                ),
            ],
          ),
        );
      },
    );

    if (vertical) {
      return SizedBox(width: width ?? 6, height: height, child: meter);
    }
    return width == null
        ? meter
        : SizedBox(width: width, height: height, child: meter);
  }
}
