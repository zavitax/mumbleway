import 'dart:typed_data';

import 'package:image/image.dart' as img;
import 'package:zxing2/qrcode.dart';

/// Reads a QR code out of a still image.
///
/// This is the desktop half of the feature. macOS and Windows machines mostly
/// have no camera worth pointing at a phone screen, and the code arrives as a
/// file anyway — a screenshot, or the PNG somebody sent in a message — so they
/// import the picture instead of scanning it. Phones use the camera; see the
/// scanner screen.
///
/// Pure Dart on purpose. The platform decoders differ in what they accept and
/// two of the four platforms have none at all, so one implementation that
/// behaves identically everywhere is worth more here than a native one.
class QrCodec {
  const QrCodec._();

  /// The text inside the first QR code in [bytes], or null if there is none.
  ///
  /// [bytes] is an encoded image — PNG, JPEG, GIF, BMP, WebP — not raw pixels.
  static String? decodeImage(Uint8List bytes) {
    final decoded = _decodeBitmap(bytes);
    if (decoded == null) return null;

    // Two passes. Codes are ordinarily dark on light, and a straight read gets
    // those; a light-on-dark one — which is what a code screenshotted from a
    // dark-themed app looks like, and this app has a dark theme — needs the
    // luminance inverted before the finder patterns are recognisable.
    return _read(decoded) ?? _read(_inverted(decoded));
  }

  /// Whether [name] looks like a file this can read.
  ///
  /// Used to filter the picker rather than to decide anything: a file that
  /// slips through is refused by the decoder a moment later with a message,
  /// which is a better failure than a picker that hides the file somebody is
  /// looking straight at.
  static const List<String> imageExtensions = [
    'png',
    'jpg',
    'jpeg',
    'gif',
    'bmp',
    'webp',
  ];

  static _Bitmap? _decodeBitmap(Uint8List bytes) {
    final img.Image? image;
    try {
      image = img.decodeImage(bytes);
    } catch (_) {
      // Truncated or not an image at all. Either way there is no code in it.
      return null;
    }
    if (image == null || image.width <= 0 || image.height <= 0) return null;

    final width = image.width;
    final height = image.height;
    final pixels = Int32List(width * height);
    var i = 0;
    for (var y = 0; y < height; y++) {
      for (var x = 0; x < width; x++) {
        final p = image.getPixel(x, y);
        // Opaque regardless of what the source said. A code saved with a
        // transparent background reads as black-on-black once the alpha is
        // honoured, which is exactly the picture that will not decode.
        pixels[i++] =
            0xff000000 |
            (p.r.toInt() << 16) |
            (p.g.toInt() << 8) |
            p.b.toInt();
      }
    }
    return _Bitmap(width, height, pixels);
  }

  static _Bitmap _inverted(_Bitmap source) {
    final pixels = Int32List(source.pixels.length);
    for (var i = 0; i < pixels.length; i++) {
      pixels[i] = source.pixels[i] ^ 0x00ffffff;
    }
    return _Bitmap(source.width, source.height, pixels);
  }

  static String? _read(_Bitmap bitmap) {
    try {
      final source = RGBLuminanceSource(
        bitmap.width,
        bitmap.height,
        bitmap.pixels,
      );
      final result = QRCodeReader().decode(
        BinaryBitmap(HybridBinarizer(source)),
      );
      final text = result.text;
      return text.isEmpty ? null : text;
    } catch (_) {
      // zxing throws for "no code here" as well as for a damaged one, and the
      // two are the same answer to the caller: nothing was found.
      return null;
    }
  }
}

class _Bitmap {
  const _Bitmap(this.width, this.height, this.pixels);
  final int width;
  final int height;
  final Int32List pixels;
}
