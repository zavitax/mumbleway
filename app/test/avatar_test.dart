import 'dart:typed_data';

import 'package:flutter_test/flutter_test.dart';
import 'package:image/image.dart' as img;
import 'package:mumbleway/services/avatar.dart';

/// The rider's own picture, on its way to a server that will refuse anything
/// large — with `TextTooLong`, for a picture, which would tell them nothing.
///
/// So what is pinned here is that whatever a rider picks comes out small,
/// square and decodable, and that a file which is not an image is refused here
/// rather than sent and rejected there.
Uint8List _png(int width, int height) {
  final image = img.Image(width: width, height: height);
  img.fill(image, color: img.ColorRgb8(10, 120, 200));
  return img.encodePng(image);
}

void main() {
  test('a camera-sized photograph comes out small', () {
    final prepared = Avatar.prepare(_png(2000, 1500));
    expect(prepared, isNotNull);

    final decoded = img.decodeImage(prepared!)!;
    expect(decoded.width, Avatar.maxEdge);
    expect(decoded.height, Avatar.maxEdge);
    expect(
      prepared.length,
      lessThan(64 * 1024),
      reason: 'servers default to refusing anything over 128 KiB',
    );
  });

  test('a wide picture becomes a face, not a face between two strips', () {
    // Cropped from the middle before scaling: a 16:9 photograph squashed to a
    // square is a portrait nobody recognises.
    final decoded = img.decodeImage(Avatar.prepare(_png(1600, 400))!)!;
    expect(decoded.width, decoded.height);
  });

  test('something already small is left at its own size', () {
    final decoded = img.decodeImage(Avatar.prepare(_png(48, 48))!)!;
    expect(decoded.width, 48, reason: 'scaling up only adds bytes and blur');
  });

  test('a file that is not an image is refused here', () {
    expect(Avatar.prepare(Uint8List.fromList([1, 2, 3, 4])), isNull);
    expect(Avatar.prepare(Uint8List(0)), isNull);
  });

  test('an enormous file is refused before it is decoded', () {
    // Decoding a 50 MB raw photograph on a phone to then throw away 99% of it
    // is a stall the rider did not ask for.
    final huge = Uint8List(Avatar.maxSourceBytes + 1);
    expect(Avatar.prepare(huge), isNull);
  });
}
