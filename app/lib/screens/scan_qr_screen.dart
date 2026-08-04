import 'package:flutter/material.dart';
import 'package:mobile_scanner/mobile_scanner.dart';

import '../l10n/app_localizations.dart';
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
            errorBuilder: (context, error, _) => Center(
              child: Padding(
                padding: const EdgeInsets.all(24),
                child: Text(
                  // A refused camera is the common failure and the only one
                  // the rider can do anything about, so it gets said in words
                  // rather than as a platform error code.
                  error.errorCode == MobileScannerErrorCode.permissionDenied
                      ? l.qrCameraDenied
                      : '${error.errorCode}',
                  textAlign: TextAlign.center,
                ),
              ),
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
