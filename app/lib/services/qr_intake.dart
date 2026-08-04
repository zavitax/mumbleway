import 'dart:io' show Platform;

import 'package:file_selector/file_selector.dart';
import 'package:flutter/foundation.dart' show kIsWeb;

import '../src/rust/api/mumbleway.dart';
import '../state/app_state.dart';
import 'qr_codec.dart';

/// What a scanned or imported code turned out to be.
sealed class QrIntake {
  const QrIntake();
}

/// A readable invitation. [server] is a draft, not a saved entry.
class QrInvitation extends QrIntake {
  const QrInvitation(this.server);
  final SavedServer server;
}

/// Something went wrong, in words the rider can act on.
class QrRefused extends QrIntake {
  const QrRefused(this.reason);
  final QrRefusal reason;
}

/// The rider backed out. Not an error, and nothing should be said about it.
class QrCancelled extends QrIntake {
  const QrCancelled();
}

enum QrRefusal { noCodeInImage, notAnInvitation, couldNotRead }

/// Turns a code into a server draft, however the code arrived.
///
/// The parsing itself is the core's `importServers`, which is the same routine
/// behind a pasted link and a downloaded profile file. A code is only another
/// way of carrying the same text, and giving it a second parser would be a
/// second set of rules about what a valid invitation is.
class QrReader {
  const QrReader._();

  /// Whether this device can scan with a camera.
  ///
  /// Phones and tablets only, by request and by sense: a desktop's camera
  /// points at the person using it, not at the phone in their hand, and the
  /// code arrives there as a file anyway. Those import a picture instead.
  static bool get cameraAvailable =>
      !kIsWeb && (Platform.isAndroid || Platform.isIOS);

  /// Reads the text off a code and turns it into a draft server.
  ///
  /// [fallbackUsername] fills in when the link carries no username, which is
  /// the usual shape of a public invitation.
  static Future<QrIntake> fromText(
    String text,
    String fallbackUsername,
  ) async {
    final trimmed = text.trim();
    if (trimmed.isEmpty) return const QrRefused(QrRefusal.notAnInvitation);

    final List<ServerConfig> found;
    try {
      found = await importServers(
        text: trimmed,
        fallbackUsername: fallbackUsername,
      );
    } catch (_) {
      // A QR code holding a Wi-Fi credential, a vCard or a shop's website is
      // not a fault to report in the parser's words. It is simply not one of
      // ours, and that is what the rider needs told.
      return const QrRefused(QrRefusal.notAnInvitation);
    }

    if (found.isEmpty) return const QrRefused(QrRefusal.notAnInvitation);

    // One code, one server. A profile file can carry several and the import
    // screen handles those; the first is the right answer for a code, which
    // is made from a single entry by the screen next door.
    return QrInvitation(SavedServer.fromConfig(found.first));
  }

  /// Asks for an image file and reads a code out of it.
  ///
  /// The desktop route. Returns [QrCancelled] when the picker is dismissed, so
  /// the caller can tell "changed my mind" from "that file had no code in it"
  /// and stay quiet about the first.
  static Future<QrIntake> fromImageFile(String fallbackUsername) async {
    final XFile? file;
    try {
      file = await openFile(
        acceptedTypeGroups: [
          XTypeGroup(label: 'Images', extensions: QrCodec.imageExtensions),
        ],
      );
    } catch (_) {
      return const QrRefused(QrRefusal.couldNotRead);
    }
    if (file == null) return const QrCancelled();

    final String? text;
    try {
      text = QrCodec.decodeImage(await file.readAsBytes());
    } catch (_) {
      return const QrRefused(QrRefusal.couldNotRead);
    }
    if (text == null) return const QrRefused(QrRefusal.noCodeInImage);

    return fromText(text, fallbackUsername);
  }
}
