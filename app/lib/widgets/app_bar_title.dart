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

  @override
  Widget build(BuildContext context) {
    if (!showIcon) return Text(title);
    return Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        Image.asset(
          'assets/icon/mumbleway.png',
          height: 26,
          filterQuality: FilterQuality.medium,
          // The bar is still usable without its decoration, so a missing
          // asset should not take the screen down with it.
          errorBuilder: (_, _, _) => const SizedBox.shrink(),
        ),
        const SizedBox(width: 10),
        Flexible(child: Text(title, overflow: TextOverflow.ellipsis)),
      ],
    );
  }
}
