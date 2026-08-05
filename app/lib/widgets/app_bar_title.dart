import 'package:flutter/material.dart';

/// App bar title with the product icon beside it.
///
/// A widget rather than a copy-pasted `Row` because the icon has to line up
/// identically on every screen; a few pixels of drift between the home screen
/// and a pushed one reads as the title jumping when you navigate.
class AppBarTitle extends StatelessWidget {
  const AppBarTitle(this.title, {super.key, this.showIcon = true});

  final String title;

  /// Set false on screens pushed with a back arrow, where the icon would sit
  /// between the arrow and the title and crowd both.
  final bool showIcon;

  /// Kept in step with the `height` given to the image below. The icon is
  /// square, so one number describes both, and the measurement needs a width
  /// before the image has been laid out.
  static const double _iconSize = 26;
  static const double _gap = 10;

  /// The product name, set in Exo 2 rather than the platform's UI face.
  ///
  /// A wordmark is the one piece of the interface that says which product
  /// this is, and letting it fall to the system font meant it arrived as
  /// Roboto on Android, SF on iOS and Segoe on Windows — three apps wearing
  /// the same icon. Everything else keeps the platform font deliberately:
  /// for ordinary interface text, matching the operating system is worth
  /// more than matching ourselves.
  ///
  /// Exo 2 because the icon is geometric and slightly squared, and this face
  /// is drawn the same way — flat-sided bowls, open apertures, a technical
  /// reading without tipping into a novelty face that would look absurd at
  /// 100 km/h. It also carries Cyrillic, which the platform fonts do too but
  /// most display faces do not, so the Russian build cannot fall back to a
  /// different typeface for want of a glyph.
  ///
  /// Weight 600, not bold: at this size full bold closes up the counters in
  /// the doubled 'm' and 'b', and "MumbleWay" is mostly those.
  /// How much smaller the wordmark sets than the surrounding bar text.
  ///
  /// It is a mark rather than a heading: it says which app this is and is then
  /// never read again, while everything else on the bar is a control somebody
  /// is looking for. Set at full title size it was the loudest thing on a
  /// screen whose actual subject is the server list below it.
  static const double _scale = 0.75;

  /// Tracking, as a fraction of the size rather than a fixed number of pixels.
  ///
  /// Proportional so the letters keep the same relationship to each other at
  /// any text-scale setting; a fixed value tightens a large rendering and
  /// loosens a small one. Negative because the face sets wide, and more
  /// negative than it needs to be for width alone — a wordmark reads as one
  /// object rather than nine letters, and the tighter fit is what does that.
  static const double _tracking = -0.03;

  static TextStyle wordmarkOf(BuildContext context) {
    final base = DefaultTextStyle.of(context).style;
    // The fallback matches Material's titleLarge, which is what an AppBar
    // hands down when nothing else has been set.
    final size = (base.fontSize ?? 22) * _scale;
    return base.copyWith(
      fontFamily: 'Exo2',
      fontVariations: const [FontVariation('wght', 600)],
      fontSize: size,
      letterSpacing: size * _tracking,
    );
  }

  @override
  Widget build(BuildContext context) {
    if (!showIcon) return Text(title, style: wordmarkOf(context));

    return LayoutBuilder(
      builder: (context, constraints) {
        // Either the whole name or none of it. A truncated product name reads
        // as a rendering fault rather than as a name — "MumbleW…" looks like
        // something went wrong — while the icon alone is deliberate, and on a
        // narrow bar the space is better spent on the controls anyway.
        final fits = _fits(context, constraints.maxWidth);

        final icon = Image.asset(
          'assets/icon/mumbleway.png',
          height: _iconSize,
          filterQuality: FilterQuality.medium,
          // The bar is still usable without its decoration, so a missing
          // asset should not take the screen down with it.
          errorBuilder: (_, _, _) => const SizedBox.shrink(),
        );

        // The name still has to reach anyone using a screen reader, which is
        // the one audience for whom dropping it would be a real loss.
        if (!fits) return Semantics(label: title, child: icon);

        return Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            icon,
            const SizedBox(width: _gap),
            Flexible(
              child: Text(
                title,
                style: wordmarkOf(context),
                overflow: TextOverflow.ellipsis,
              ),
            ),
          ],
        );
      },
    );
  }

  /// Whether the icon and the full name fit the width on offer.
  bool _fits(BuildContext context, double available) {
    // An unbounded bar cannot be too narrow for anything.
    if (!available.isFinite) return true;

    // The same style the name is drawn in, not the ambient one. Measuring in
    // a different face than the one rendered is how a title that fits gets
    // hidden, or one that does not gets clipped.
    final painter = TextPainter(
      text: TextSpan(text: title, style: wordmarkOf(context)),
      textDirection: Directionality.of(context),
      textScaler: MediaQuery.textScalerOf(context),
      maxLines: 1,
    )..layout();
    final needed = _iconSize + _gap + painter.width;
    painter.dispose();
    return needed <= available;
  }
}
