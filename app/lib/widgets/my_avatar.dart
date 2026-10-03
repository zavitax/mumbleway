import 'dart:typed_data';

import 'package:flutter/material.dart';
import 'package:flutter_svg/flutter_svg.dart';

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
