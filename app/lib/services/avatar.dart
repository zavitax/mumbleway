import 'dart:io';
import 'dart:typed_data';

import 'package:image/image.dart' as img;
import 'package:path_provider/path_provider.dart';

/// The rider's own picture: one for the rider, kept on this device.
///
/// **Not per server, although Mumble stores it per server.** There is no
/// identity in Mumble that spans servers — each one keeps its own copy against
/// its own account — so the copy that lasts is the local one, and it is sent to
/// each server as that server connects. A rider sets their face once.
///
/// Everything here is deliberately small. A roster draws this at eighteen
/// pixels; a camera photograph is four thousand across and several megabytes,
/// and a server will refuse it outright — with `TextTooLong`, for a picture,
/// which tells the rider nothing. So an incoming file is decoded, squared,
/// scaled to [maxEdge] and written out as PNG, and what travels is a few
/// kilobytes whatever was chosen.
class Avatar {
  const Avatar._();

  /// Longest edge kept, in pixels.
  ///
  /// The roster draws 18 and a future larger view might want 64; 128 is enough
  /// for both on a dense screen and still a small PNG. Mumble's own default
  /// ceiling is 128 KiB for the whole message, which this stays far inside.
  static const maxEdge = 128;

  /// Refuses to even decode a file larger than this.
  ///
  /// A RAW photograph can be 50 MB, and decoding one on a phone to then throw
  /// away 99% of it is a stall the rider did not ask for.
  static const maxSourceBytes = 12 * 1024 * 1024;

  /// Turns whatever was chosen into something worth sending.
  ///
  /// Returns null when the bytes are not an image this can read — which
  /// includes a file that is simply not one, since a picker lets a rider
  /// choose anything.
  static Uint8List? prepare(Uint8List source) {
    if (source.isEmpty || source.length > maxSourceBytes) return null;
    // **Decoding has to be guarded, not merely checked.** `decodeImage` does
    // not always answer null for something that is not an image: it probes
    // each format in turn, and a few bytes of nonsense reach the PSD reader's
    // header parse and come back as a RangeError. A rider picking the wrong
    // file from a gallery is an ordinary mistake, not a crash.
    final img.Image? decoded;
    try {
      decoded = img.decodeImage(source);
    } catch (_) {
      return null;
    }
    if (decoded == null) return null;

    // Squared from the middle first, so a wide photograph becomes a face
    // rather than a face with two strips of sky.
    final edge = decoded.width < decoded.height ? decoded.width : decoded.height;
    final square = img.copyCrop(
      decoded,
      x: (decoded.width - edge) ~/ 2,
      y: (decoded.height - edge) ~/ 2,
      width: edge,
      height: edge,
    );
    final scaled = edge > maxEdge
        ? img.copyResize(square, width: maxEdge, height: maxEdge)
        : square;
    return img.encodePng(scaled);
  }

  static Future<File> _file() async {
    final dir = await getApplicationSupportDirectory();
    return File('${dir.path}/avatar.png');
  }

  /// The rider's picture, or null if they have none.
  static Future<Uint8List?> load() async {
    try {
      final file = await _file();
      if (!await file.exists()) return null;
      final bytes = await file.readAsBytes();
      return bytes.isEmpty ? null : bytes;
    } catch (_) {
      // A picture that cannot be read is not worth failing a startup over.
      return null;
    }
  }

  /// Keeps `image` as the rider's picture. Empty removes it.
  static Future<void> save(Uint8List image) async {
    final file = await _file();
    if (image.isEmpty) {
      if (await file.exists()) await file.delete();
      return;
    }
    await file.writeAsBytes(image, flush: true);
  }
}
