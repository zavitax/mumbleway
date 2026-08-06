import 'package:flutter/material.dart';
import 'package:flutter_svg/flutter_svg.dart';

/// The product lockup: the name stacked in two lines, the helmet riding in
/// front of the second.
///
/// ```
///   Mumble
///      ╭──╮
///      │◗)│ Way
///      ╰──╯
/// ```
///
/// The name breaks across two lines and the second starts well to the right,
/// finishing past where the first ended. That overhang is the idea: the eye
/// crosses the mark diagonally, top-left to bottom-right, the way the road
/// arrives — and "Way" is left hanging off the end rather than tucked into the
/// block, which is what makes it read as travelling rather than as a tidy
/// two-line label. A slight forward lean says the same thing again, quietly.
///
/// The helmet sits in that indent, immediately in front of "Way", rather than
/// beside the whole mark. Set beside the name it was an icon standing next to a
/// word and cost its own width; set here it costs nothing, because the space it
/// occupies is space the overhang had already emptied — and it travels with the
/// word it belongs to instead of floating on its own at the left edge.
///
/// Which is the reason it exists: "MumbleWay" set in one run, with an icon in
/// front, is nine characters and an icon of app bar on a phone that has none to
/// spare, and the bar has vertical room going unused.
///
/// This draws [assetFor], the same file `tool/make_logo.py` generates for
/// everything outside the app. It used to lay the lockup out itself in Dart,
/// which meant two implementations of one design measuring text with two
/// different engines — and they disagreed on the proportions by six percent.
/// There is one artefact now, and the app and the store listing cannot drift.
class Wordmark extends StatelessWidget {
  const Wordmark({super.key, this.height = 34});

  /// Height of the whole mark, descender included.
  final double height;

  /// Ratio of width to height, from the asset's own viewBox.
  ///
  /// Duplicated here because a caller may need the width before the asset has
  /// been read, and because a widget that had to await an asset to report its
  /// size could not sit in an app bar. A test parses the SVG and fails if this
  /// drifts from it, so the copy cannot go stale unnoticed.
  static const double aspectRatio = 537 / 256;

  /// The variant that will read against a surface of [brightness].
  ///
  /// Two files rather than one recoloured at runtime, because the wordmark is
  /// already outlined and there is no text to restyle — only two flat inks and
  /// an accent that stays put in both.
  ///
  /// Neither carries the launcher's rounded tile. That square exists to give
  /// the icon an edge on a home screen; in a logo it is a black brick beside
  /// dark type, or — on a dark app bar, where it matches the background — an
  /// invisible square leaving the helmet to float as a pale blob. Without it
  /// the helmet is a shape like the letters are shapes, in the same ink.
  static String assetFor(Brightness brightness) =>
      brightness == Brightness.dark
      ? 'assets/logo/mumbleway-logo-on-dark.svg'
      : 'assets/logo/mumbleway-logo-on-light.svg';

  /// Size the lockup occupies at [height].
  static Size sizeFor(double height) =>
      Size(height * aspectRatio, height);

  @override
  Widget build(BuildContext context) {
    // The theme's own brightness, not a guess from some nearby colour. An
    // earlier version sniffed the app bar's foreground and got it wrong the
    // first time it was asked outside an app bar — it drew the dark variant on
    // a dark surface, and the mark vanished but for the blue of the visor.
    final brightness = Theme.of(context).brightness;

    return Semantics(
      // "Mumble" and "Way" announced separately would be describing a layout
      // accident rather than the name of the app.
      label: 'MumbleWay',
      image: true,
      child: SvgPicture.asset(
        assetFor(brightness),
        height: height,
        width: height * aspectRatio,
        fit: BoxFit.contain,
        // The bar is still usable without its decoration, and an app that
        // failed to start because a logo would not parse would be absurd.
        placeholderBuilder: (_) =>
            SizedBox(width: height * aspectRatio, height: height),
      ),
    );
  }
}
