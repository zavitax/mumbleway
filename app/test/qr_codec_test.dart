import 'dart:typed_data';

import 'package:flutter_test/flutter_test.dart';
import 'package:image/image.dart' as img;
import 'package:mumbleway/services/qr_codec.dart';
import 'package:qr/qr.dart';

/// Draws [text] as a QR code into a PNG, the way the display screen does.
///
/// Written here with the raw encoder rather than by rendering the widget so
/// that the test needs no Flutter binding and no golden file: what is being
/// checked is that a code this app produces is one this app can read back.
Uint8List encodePng(String text, {int scale = 6, int quiet = 4, bool invert = false}) {
  final code = QrCode.fromData(
    data: text,
    errorCorrectLevel: QrErrorCorrectLevel.M,
  );
  final matrix = QrImage(code);
  final modules = matrix.moduleCount;
  final size = (modules + quiet * 2) * scale;

  final dark = invert ? 255 : 0;
  final light = invert ? 0 : 255;
  final image = img.Image(width: size, height: size);
  img.fill(image, color: img.ColorRgb8(light, light, light));

  for (var y = 0; y < modules; y++) {
    for (var x = 0; x < modules; x++) {
      if (!matrix.isDark(y, x)) continue;
      img.fillRect(
        image,
        x1: (x + quiet) * scale,
        y1: (y + quiet) * scale,
        x2: (x + quiet + 1) * scale - 1,
        y2: (y + quiet + 1) * scale - 1,
        color: img.ColorRgb8(dark, dark, dark),
      );
    }
  }
  return Uint8List.fromList(img.encodePng(image));
}

void main() {
  const url =
      'mumble://rider:s3cret@voice.example.com:64738/Root/Riders'
      '?version=1.2.0&title=Sunday+Run';

  test('a code this app draws is one it can read back', () {
    expect(QrCodec.decodeImage(encodePng(url)), url);
  });

  test('the link survives the round trip intact', () {
    // The point of the whole feature: everything needed to join, including the
    // password and the channel, comes back out of the picture byte for byte.
    // What the link *means* is the core's business and is tested there; what
    // matters here is that not one character of it is lost in the drawing.
    final back = QrCodec.decodeImage(encodePng(url))!;
    expect(back, url);
    expect(back, contains('rider:s3cret@'));
    expect(back, contains('/Root/Riders'));
  });

  test('a light-on-dark code is read too', () {
    // What a code screenshotted from this app's dark theme looks like.
    expect(QrCodec.decodeImage(encodePng(url, invert: true)), url);
  });

  test('a small rendering still decodes', () {
    expect(QrCodec.decodeImage(encodePng(url, scale: 3)), url);
  });

  test('percent-encoded credentials come back whole', () {
    // The characters a real password contains are exactly the ones a sloppy
    // encoder loses. This is the escaped form the core's builder produces.
    const awkward =
        'mumble://a%20b:p%40ss%2Fword%3A2%25@h.example/Sunday%20Run';
    expect(QrCodec.decodeImage(encodePng(awkward)), awkward);
  });

  test('a long link still fits in one code', () {
    // Invite links grow with the channel path and a long server name, and a
    // code that silently truncated would produce a link that parses and
    // connects somewhere else.
    final long =
        'mumble://${'u' * 40}:${'p' * 40}@a-rather-long-hostname.example.com'
        ':64738/${'Deeply/Nested/Channel/' * 4}?version=1.2.0&title=${'N' * 60}';
    expect(QrCodec.decodeImage(encodePng(long)), long);
  });

  group('refusing what is not a code', () {
    test('a picture with no code in it', () {
      final blank = img.Image(width: 200, height: 200);
      img.fill(blank, color: img.ColorRgb8(200, 200, 200));
      expect(QrCodec.decodeImage(Uint8List.fromList(img.encodePng(blank))), isNull);
    });

    test('bytes that are not an image', () {
      expect(QrCodec.decodeImage(Uint8List.fromList([1, 2, 3, 4, 5])), isNull);
    });

    test('an empty file', () {
      expect(QrCodec.decodeImage(Uint8List(0)), isNull);
    });
  });
}
