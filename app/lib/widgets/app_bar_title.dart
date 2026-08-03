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

  @override
  Widget build(BuildContext context) {
    if (!showIcon) return Text(title);

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
            Flexible(child: Text(title, overflow: TextOverflow.ellipsis)),
          ],
        );
      },
    );
  }

  /// Whether the icon and the full name fit the width on offer.
  bool _fits(BuildContext context, double available) {
    // An unbounded bar cannot be too narrow for anything.
    if (!available.isFinite) return true;

    final painter = TextPainter(
      text: TextSpan(text: title, style: DefaultTextStyle.of(context).style),
      textDirection: Directionality.of(context),
      textScaler: MediaQuery.textScalerOf(context),
      maxLines: 1,
    )..layout();
    final needed = _iconSize + _gap + painter.width;
    painter.dispose();
    return needed <= available;
  }
}
