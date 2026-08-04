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
  static TextStyle wordmarkOf(BuildContext context) =>
      DefaultTextStyle.of(context).style.copyWith(
        fontFamily: 'Exo2',
        fontVariations: const [FontVariation('wght', 600)],
        // The face sets a little wide; this returns the name to roughly the
        // width it occupied in the platform font, so nothing else on the bar
        // has to move to accommodate it.
        letterSpacing: -0.2,
      );

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
