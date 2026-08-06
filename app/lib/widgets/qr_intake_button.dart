import 'package:flutter/material.dart';

import '../l10n/app_localizations.dart';
import '../screens/add_server_screen.dart';
import '../screens/scan_qr_screen.dart';
import '../services/qr_intake.dart';
import '../state/app_state.dart';

/// Takes an invitation from a QR code and opens the add-server form on it.
///
/// One button, two routes. On a phone or tablet it opens the camera; anywhere
/// else it asks for an image file, because a desktop's camera points at the
/// person using it rather than at the code in their hand, and a code reaches a
/// laptop as a picture anyway.
///
/// Icon only, by request: it sits beside two labelled buttons in a row that has
/// no width to spare, and a QR glyph is about as unambiguous as an icon gets.
/// The label lives in the tooltip and in the semantics, so it is still readable
/// by a screen reader and on a long press.
class QrIntakeButton extends StatefulWidget {
  const QrIntakeButton({super.key});

  @override
  State<QrIntakeButton> createState() => _QrIntakeButtonState();
}

class _QrIntakeButtonState extends State<QrIntakeButton> {
  bool _busy = false;

  @override
  Widget build(BuildContext context) {
    final l = L.of(context);
    final label = QrReader.cameraAvailable ? l.scanQrCode : l.importQrImage;

    return Tooltip(
      message: label,
      child: OutlinedButton(
        onPressed: _busy ? null : _start,
        style: OutlinedButton.styleFrom(
          // Square-ish, so it reads as one control rather than a labelled
          // button whose label failed to load.
          padding: const EdgeInsets.symmetric(horizontal: 12),
          minimumSize: const Size(52, 40),
        ),
        child: _busy
            ? const SizedBox(
                width: 18,
                height: 18,
                child: CircularProgressIndicator(strokeWidth: 2),
              )
            : Semantics(
                label: label,
                button: true,
                child: const Icon(Icons.qr_code_scanner, size: 22),
              ),
      ),
    );
  }

  Future<void> _start() async {
    final state = AppStateScope.of(context);
    final navigator = Navigator.of(context);
    final messenger = ScaffoldMessenger.of(context);
    final l = L.of(context);
    final fallback = await state.suggestedUsername();

    setState(() => _busy = true);
    try {
      final QrIntake result;
      if (QrReader.cameraAvailable) {
        final text = await navigator.push<String>(
          MaterialPageRoute(builder: (_) => const ScanQrScreen()),
        );
        // Backed out of the camera without scanning anything.
        if (text == null) return;
        result = await QrReader.fromText(text, fallback);
      } else {
        result = await QrReader.fromImageFile(fallback);
      }

      switch (result) {
        case QrCancelled():
          // Dismissed the picker. Saying anything would be noise.
          return;
        case QrRefused(:final reason):
          messenger.showSnackBar(
            SnackBar(content: Text(_wording(l, reason))),
          );
        case QrInvitation(:final server):
          // Opened as a draft rather than saved outright. A code can be
          // photographed off a screen by anyone walking past it, so the last
          // word on whether this server joins the list belongs to the rider
          // looking at the details — not to whatever the camera happened to
          // see.
          await navigator.push(
            MaterialPageRoute(builder: (_) => AddServerScreen(prefill: server)),
          );
      }
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  String _wording(L l, QrRefusal reason) => switch (reason) {
    QrRefusal.noCodeInImage => l.qrNoCodeFound,
    QrRefusal.notAnInvitation => l.qrNotAnInvite,
    QrRefusal.couldNotRead => l.qrNoCodeFound,
  };
}
