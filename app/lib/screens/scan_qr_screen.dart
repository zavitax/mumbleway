import 'package:file_selector/file_selector.dart';
import 'package:flutter/material.dart';
import 'package:mobile_scanner/mobile_scanner.dart';

import '../l10n/app_localizations.dart';
import '../services/qr_codec.dart';
import '../widgets/app_bar_title.dart';

/// Points the camera at a QR code and returns whatever text is in it.
///
/// Deliberately knows nothing about invitations. It reads a code and pops with
/// the string; deciding whether that string is a server is [QrReader]'s job,
/// and keeping the two apart is what lets the same screen serve any future use
/// without growing a mode flag.
///
/// Phones and tablets only. A desktop imports a picture instead — see the
/// button that chooses between them.
class ScanQrScreen extends StatefulWidget {
  const ScanQrScreen({super.key});

  @override
  State<ScanQrScreen> createState() => _ScanQrScreenState();
}

class _ScanQrScreenState extends State<ScanQrScreen> {
  final MobileScannerController _controller = MobileScannerController(
    // Only QR. A scanner that also reads the barcode on a jacket label spends
    // its time on formats nobody here is pointing it at.
    formats: const [BarcodeFormat.qrCode],
    detectionSpeed: DetectionSpeed.noDuplicates,
  );

  /// Guards against the detector firing again between the first hit and the
  /// route actually leaving the stack, which would pop twice.
  bool _handled = false;

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  void _onDetect(BarcodeCapture capture) {
    if (_handled) return;
    for (final barcode in capture.barcodes) {
      final value = barcode.rawValue;
      if (value == null || value.isEmpty) continue;
      _handled = true;
      Navigator.pop(context, value);
      return;
    }
  }

  /// Falls back to reading the code out of a picture.
  ///
  /// A camera that will not open is not the end of the feature. The decoder is
  /// pure Dart and has no idea where its pixels came from, so a phone whose
  /// camera stack refuses a preview can still take a screenshot of the code,
  /// or be sent one, and get exactly the same result.
  Future<void> _pickImage(BuildContext context) async {
    final navigator = Navigator.of(context);
    final messenger = ScaffoldMessenger.of(context);
    final l = L.of(context);

    final file = await openFile(
      acceptedTypeGroups: [
        XTypeGroup(label: 'Images', extensions: QrCodec.imageExtensions),
      ],
    );
    if (file == null) return;

    final text = QrCodec.decodeImage(await file.readAsBytes());
    if (text == null) {
      messenger.showSnackBar(SnackBar(content: Text(l.qrNoCodeFound)));
      return;
    }
    navigator.pop(text);
  }

  @override
  Widget build(BuildContext context) {
    final l = L.of(context);
    return Scaffold(
      appBar: AppBar(title: AppBarTitle(l.scanQrCode, showIcon: false)),
      body: Stack(
        fit: StackFit.expand,
        children: [
          MobileScanner(
            controller: _controller,
            onDetect: _onDetect,
            errorBuilder: (context, error, _) => _CameraTrouble(
              // A refused camera is the common failure and the only one the
              // rider can do anything about, so it gets said in words.
              message: error.errorCode == MobileScannerErrorCode.permissionDenied
                  ? l.qrCameraDenied
                  : l.qrCameraFailed,
              // The code alone says nothing — "genericError" is what a phone
              // reports when the camera stack refused a configuration it does
              // not support, which happens on modest hardware that every other
              // app opens perfectly. The detail underneath it is the part that
              // names the cause, so it is shown rather than swallowed.
              detail: [
                error.errorDetails?.message,
                error.errorDetails?.code?.toString(),
                '${error.errorCode}',
              ].whereType<String>().where((s) => s.isNotEmpty).join(' · '),
              onPickImage: () => _pickImage(context),
            ),
          ),
          // A frame to aim with. The scanner reads the whole picture, so this
          // is guidance rather than a crop — but a camera view with nothing in
          // it gives no clue how close to hold the phone.
          IgnorePointer(
            child: Center(
              child: Container(
                width: 230,
                height: 230,
                decoration: BoxDecoration(
                  border: Border.all(color: Colors.white70, width: 2),
                  borderRadius: BorderRadius.circular(18),
                ),
              ),
            ),
          ),
          Positioned(
            left: 0,
            right: 0,
            bottom: 36,
            child: Center(
              child: Container(
                padding: const EdgeInsets.symmetric(
                  horizontal: 14,
                  vertical: 8,
                ),
                decoration: BoxDecoration(
                  color: Colors.black54,
                  borderRadius: BorderRadius.circular(18),
                ),
                child: Text(
                  l.qrPointAtCode,
                  style: const TextStyle(color: Colors.white, fontSize: 13),
                ),
              ),
            ),
          ),
        ],
      ),
    );
  }
}

/// Shown in place of the preview when the camera will not start.
///
/// Says what happened in words, offers the way round it, and keeps the
/// platform's own explanation visible underneath — small, but selectable, so
/// it can be copied into a bug report. "genericError" on its own has cost one
/// round of guessing already.
class _CameraTrouble extends StatelessWidget {
  const _CameraTrouble({
    required this.message,
    required this.detail,
    required this.onPickImage,
  });

  final String message;
  final String detail;
  final VoidCallback onPickImage;

  @override
  Widget build(BuildContext context) {
    final l = L.of(context);
    final scheme = Theme.of(context).colorScheme;

    return Container(
      color: scheme.surface,
      alignment: Alignment.center,
      child: SingleChildScrollView(
        padding: const EdgeInsets.all(28),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Icon(Icons.videocam_off, size: 40, color: scheme.onSurfaceVariant),
            const SizedBox(height: 14),
            Text(message, textAlign: TextAlign.center),
            const SizedBox(height: 20),
            FilledButton.icon(
              onPressed: onPickImage,
              icon: const Icon(Icons.image_outlined),
              label: Text(l.importQrImage),
            ),
            if (detail.isNotEmpty) ...[
              const SizedBox(height: 22),
              SelectableText(
                detail,
                textAlign: TextAlign.center,
                style: TextStyle(
                  fontSize: 11,
                  fontFamily: 'monospace',
                  color: scheme.onSurfaceVariant,
                ),
              ),
            ],
          ],
        ),
      ),
    );
  }
}
