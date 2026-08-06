import 'dart:math' as math;

import 'package:flutter/material.dart';

/// The product lockup: the name stacked in two lines, the helmet tucked into
/// the second.
///
/// ```
///   Mumble
///   ╭──╮
///   │◗)│  Way
///   ╰──╯
/// ```
///
/// The name breaks across two lines and the second starts well to the right,
/// finishing past where the first ended. That overhang is the idea: the eye
/// crosses the mark diagonally, top-left to bottom-right, the way the road
/// arrives — and "Way" is left hanging off the end rather than tucked into the
/// block, which is what makes it read as travelling rather than as a tidy
/// two-line label. A slight forward lean says the same thing again, quietly.
///
/// The helmet sits inside that indent rather than in front of the whole mark.
/// Set beside the name it was an icon standing next to a word and cost its own
/// width; set here it costs nothing, because the space it occupies is space the
/// overhang had already emptied. The lockup is the width of "Mumble" and
/// nothing more.
///
/// Which is the reason it exists: "MumbleWay" set in one run is nine characters
/// of app bar on a phone that has none to spare, and the bar has vertical room
/// going unused. There is a test that measures this rather than trusting the
/// arithmetic.
class Wordmark extends StatelessWidget {
  const Wordmark({super.key, this.height = 34});

  /// Height of the whole mark, descender included. Everything else is derived
  /// from it, so the lockup scales as one object.
  final double height;

  /// Gap between the helmet and "Way", as a fraction of the helmet's size.
  static const double _gapRatio = 0.24;

  /// The second line's offset from the first, as a fraction of the font size.
  ///
  /// Tighter than any text would be set. These are two halves of one word, and
  /// normal leading opens a gap that invites them to be read as two.
  static const double _leading = 0.82;

  /// How far "Way" reaches past the end of "Mumble", as a fraction of the width
  /// of "Mumble".
  ///
  /// A floor rather than the whole story: the helmet also has to fit in the
  /// space to the left of "Way", and whichever of the two needs more room wins.
  /// Enough that the overhang is unmistakably deliberate — at 7% it just looked
  /// like the second line had failed to line up.
  static const double _overhang = 0.1;

  /// Forward lean, in radians. About 7°.
  ///
  /// Enough to register as motion, little enough that Exo 2's flat-sided bowls
  /// do not start to look like a different typeface. A true italic would be
  /// better, but the bundled family is a single variable file with a weight
  /// axis and no slant, and shipping a second font for seven degrees is not a
  /// trade worth making.
  static const double _lean = 0.12;

  /// The two halves. Not localised: the name is a brand and is spelled the same
  /// in both catalogues — but split here rather than by slicing `appTitle`, so
  /// a translator who does change it cannot silently produce "Mumbl"/"eWay".
  static const String _first = 'Mumble';
  static const String _second = 'Way';

  /// The face the name is set in. See [AppBarTitle] for why the product name is
  /// the one string in the app that does not use the platform's UI font.
  static TextStyle styleFor(BuildContext context, double fontSize) {
    return DefaultTextStyle.of(context).style.copyWith(
      fontFamily: 'Exo2',
      fontVariations: const [FontVariation('wght', 600)],
      fontSize: fontSize,
      // Negative, because the face sets wide and a wordmark should read as one
      // object rather than as six letters.
      letterSpacing: fontSize * -0.03,
      // Deliberately *not* height: 1. That forces the line box to the font size
      // and so leaves the descender of the "y" hanging below it — which paints
      // outside the widget's own bounds, and got sheared clean off the moment
      // the same layout was drawn into an SVG viewBox. "Way" rendered as "Wav".
      // The font's natural line height contains its own descenders.
    );
  }

  /// Size the lockup occupies at [height], measured in the given context.
  ///
  /// Exposed so the app bar can decide whether it fits before committing to it,
  /// and so a test can compare it against the single line it replaced.
  static Size measure(BuildContext context, {double height = 34}) =>
      Size(_Metrics.of(context, height).width, height);

  @override
  Widget build(BuildContext context) {
    final m = _Metrics.of(context, height);

    // One label for the pair. A screen reader announcing "Mumble" and "Way" as
    // two things would be describing a layout accident rather than the name of
    // the app.
    return Semantics(
      label: '$_first$_second',
      child: ExcludeSemantics(
        child: SizedBox(
          width: m.width,
          height: height,
          child: Stack(
            fit: StackFit.expand,
            children: [
              Positioned(
                left: 0,
                top: m.helmetTop,
                width: m.helmet,
                height: m.helmet,
                child: Image.asset(
                  'assets/icon/mumbleway.png',
                  filterQuality: FilterQuality.medium,
                  // The bar is still usable without its decoration.
                  errorBuilder: (_, _, _) => const SizedBox.shrink(),
                ),
              ),
              // Painted rather than assembled from Text widgets: the overhang,
              // the leading and the lean all have to be exact and to hold at
              // any text scale, and three nudged paddings would drift apart the
              // first time somebody changed the font size.
              CustomPaint(size: Size(m.width, height), painter: _Painter(m)),
            ],
          ),
        ),
      ),
    );
  }
}

/// Everything about the drawing that depends on the font, resolved once.
class _Metrics {
  const _Metrics._({
    required this.first,
    required this.second,
    required this.indent,
    required this.width,
    required this.secondLineTop,
    required this.helmetTop,
    required this.helmet,
  });

  final TextPainter first;
  final TextPainter second;

  /// Where "Way" starts.
  final double indent;

  /// Total width of the mark, lean included.
  final double width;

  /// Top of the second line's box.
  final double secondLineTop;

  /// Top of the helmet: the first line's baseline.
  final double helmetTop;

  /// The helmet's side. Square, and reaches the foot of the mark.
  final double helmet;

  factory _Metrics.of(BuildContext context, double height) {
    final direction = Directionality.maybeOf(context) ?? TextDirection.ltr;
    final scaler = MediaQuery.maybeTextScalerOf(context) ?? TextScaler.noScaling;

    TextPainter paint(String text, double size) => TextPainter(
      text: TextSpan(text: text, style: Wordmark.styleFor(context, size)),
      textDirection: direction,
      textScaler: scaler,
      maxLines: 1,
    )..layout();

    // How tall one line actually is, ascender to descender, in this face at
    // this reader's text scale. Measured rather than assumed: it is a property
    // of the font, and hard-coding 1.0 is what clipped the "y".
    const probe = 100.0;
    final ratio = paint(Wordmark._second, probe).height / probe;

    // Sized so the whole two-line block — descender included — is exactly
    // [height] tall. Nothing paints outside its own box.
    final fontSize = height / (ratio + Wordmark._leading);
    final secondLineTop = fontSize * Wordmark._leading;

    final first = paint(Wordmark._first, fontSize);
    final second = paint(Wordmark._second, fontSize);

    // The helmet hangs from the first line's baseline down to the foot of the
    // mark. Not from the top of the second line, which is where it started and
    // which put its tile straight through the bottom of "Mumble" — the lines
    // are set tighter than one line is tall, so the second line's box begins
    // well above the first line's feet. "Mumble" has no descenders, so its
    // baseline is the lowest its ink reaches and the highest the helmet can
    // start without touching it.
    final helmetTop = first.computeDistanceToActualBaseline(
      TextBaseline.alphabetic,
    );
    final helmet = height - helmetTop;
    final indent = math.max(
      helmet * (1 + Wordmark._gapRatio),
      first.width * (1 + Wordmark._overhang) - second.width,
    );

    return _Metrics._(
      first: first,
      second: second,
      indent: indent,
      // The lean throws the top of the block right of its bottom, so the box
      // has to carry that or the tail of "Mumble" clips.
      width: math.max(first.width, indent + second.width) + height * Wordmark._lean,
      secondLineTop: secondLineTop,
      helmetTop: helmetTop,
      helmet: helmet,
    );
  }
}

class _Painter extends CustomPainter {
  const _Painter(this.m);

  final _Metrics m;

  @override
  void paint(Canvas canvas, Size size) {
    canvas.save();
    // Sheared about the bottom of the block, so the lean throws the mark
    // forward rather than sliding the whole thing sideways. The helmet is left
    // upright — it is a physical object with a rounded tile behind it, and a
    // sheared square reads as a rendering fault rather than as speed.
    canvas.translate(0, size.height);
    canvas.transform(Matrix4.skewX(-Wordmark._lean).storage);
    canvas.translate(0, -size.height);

    m.first.paint(canvas, Offset.zero);
    m.second.paint(canvas, Offset(m.indent, m.secondLineTop));
    canvas.restore();
  }

  @override
  bool shouldRepaint(_Painter old) => old.m != m;
}
