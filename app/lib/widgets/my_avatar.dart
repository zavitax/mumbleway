import 'dart:typed_data';

import 'package:flutter/material.dart';
import 'package:flutter_svg/flutter_svg.dart';

import '../l10n/app_localizations.dart';

/// The rider's own picture, round, at [size].
///
/// With nothing chosen this is the app's own mark rather than a grey
/// silhouette. A rider who has not picked a picture is not anonymous — they are
/// simply using MumbleWay — and the mark says that, where an empty avatar says
/// only that something is missing.
class MyAvatar extends StatelessWidget {
  const MyAvatar({super.key, required this.image, this.size = 32});

  /// The picture, as it is kept on this device. Null or empty means none.
  final Uint8List? image;

  final double size;

  @override
  Widget build(BuildContext context) {
    final picture = image;
    return SizedBox(
      width: size,
      height: size,
      child: ClipOval(
        child: picture == null || picture.isEmpty
            ? _mark()
            : Image.memory(
                picture,
                width: size,
                height: size,
                fit: BoxFit.cover,
                gaplessPlayback: true,
                // A picture that will not decode is not worth losing the menu
                // over, and the mark is what would be there without it anyway.
                errorBuilder: (_, _, _) => _mark(),
              ),
      ),
    );
  }

  Widget _mark() => SvgPicture.asset(
    'assets/icon/mumbleway.svg',
    width: size,
    height: size,
    fit: BoxFit.cover,
    placeholderBuilder: (_) => SizedBox(width: size, height: size),
  );
}

/// The picture as it appears in the overflow menu: large, round, and its own
/// button.
///
/// It is drawn at half of [width], with a quarter of [width] as the margin
/// around it, which keeps it a picture rather than an icon and keeps the menu
/// from being mostly face. What pressing it does is said over the picture on
/// hover rather than beside it: the picture is the control, and a label
/// standing next to it all the time would make the entry read as a setting
/// with a thumbnail.
///
/// There is no hover on a phone, so the same words are the accessibility label,
/// which is what a screen reader announces.
///
/// **[width] is passed in rather than measured.** A popup menu asks its entries
/// how wide they would like to be and then makes itself that wide, so an entry
/// that sized itself from the width it was given would be defining its own
/// answer. `LayoutBuilder` says so plainly — it throws "does not support
/// returning intrinsic dimensions" from inside the menu's own measuring pass.
/// The menu is given a fixed width instead, and this is told what it is.
class AvatarMenuTile extends StatefulWidget {
  const AvatarMenuTile({super.key, required this.image, required this.width});

  final Uint8List? image;

  /// The width this entry is drawn in, which the menu fixes.
  final double width;

  @override
  State<AvatarMenuTile> createState() => _AvatarMenuTileState();
}

class _AvatarMenuTileState extends State<AvatarMenuTile> {
  bool _hovered = false;

  @override
  Widget build(BuildContext context) {
    final l = L.of(context);

    final size = widget.width / 2;
    // The margin is a quarter of the width on every side, so the picture sits
    // in the middle of a square of air rather than in a row.
    final margin = widget.width / 4;

    return Semantics(
      button: true,
      label: l.avatarChange,
      child: MouseRegion(
        onEnter: (_) => setState(() => _hovered = true),
        onExit: (_) => setState(() => _hovered = false),
        child: SizedBox(
          width: widget.width,
          child: Padding(
            padding: EdgeInsets.all(margin),
            child: Center(
              child: SizedBox(
                width: size,
                height: size,
                child: Stack(
                  fit: StackFit.expand,
                  children: [
                    MyAvatar(image: widget.image, size: size),
                    if (_hovered)
                      ClipOval(
                        child: ColoredBox(
                          color: Colors.black.withValues(alpha: 0.55),
                          child: Center(
                            child: Padding(
                              padding: const EdgeInsets.symmetric(
                                horizontal: 12,
                              ),
                              child: Text(
                                l.avatarChange,
                                textAlign: TextAlign.center,
                                style: const TextStyle(
                                  color: Colors.white,
                                  fontWeight: FontWeight.w600,
                                ),
                              ),
                            ),
                          ),
                        ),
                      ),
                  ],
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }
}
