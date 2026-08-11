import 'dart:io' show File;
import 'dart:ui' as ui;

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:path_provider/path_provider.dart';
import 'package:qr_flutter/qr_flutter.dart';
import 'package:share_plus/share_plus.dart';

import '../l10n/app_localizations.dart';
import '../src/rust/api/mumbleway.dart';
import '../state/app_state.dart';
import '../widgets/app_bar_title.dart';
import '../widgets/error_snack.dart';

/// A server's details as a QR code, to be pointed a phone at.
///
/// The link behind it is the same `mumble://` one the share menu produces, from
/// the same builder in the core — so a code from here opens in the official
/// client, and this app reads codes made by anything else that follows the
/// scheme. It carries the password, because a code somebody has to scan *and*
/// then be told a password separately is not worth the trouble of showing.
///
/// Available on every platform. Making one needs no camera, and the machine
/// with the server details on it is as often a laptop as a phone.
class ServerQrScreen extends StatefulWidget {
  const ServerQrScreen({super.key, required this.server, this.channel});

  final SavedServer server;

  /// Channel the code should land the scanner in. The caller passes whichever
  /// is more useful — where this device is right now, or the saved default.
  final String? channel;

  @override
  State<ServerQrScreen> createState() => _ServerQrScreenState();
}

class _ServerQrScreenState extends State<ServerQrScreen> {
  String? _link;
  String? _error;

  @override
  void initState() {
    super.initState();
    _build();
  }

  Future<void> _build() async {
    try {
      final link = await buildInviteLink(
        config: widget.server.toConfig(),
        channel: widget.channel,
        // The whole point of a code is that scanning it is the only step.
        includePassword: true,
      );
      if (mounted) setState(() => _link = link);
    } catch (e) {
      if (mounted) setState(() => _error = e.toString());
    }
  }

  @override
  Widget build(BuildContext context) {
    final l = L.of(context);
    return Scaffold(
      appBar: AppBar(title: AppBarTitle(l.qrCodeTitle, showIcon: false)),
      body: SafeArea(
        child: _error != null
            ? Center(
                child: Padding(
                  padding: const EdgeInsets.all(24),
                  child: Text(_error!, textAlign: TextAlign.center),
                ),
              )
            : _link == null
            ? const Center(child: CircularProgressIndicator())
            : _Body(server: widget.server, link: _link!),
      ),
    );
  }
}

class _Body extends StatelessWidget {
  const _Body({required this.server, required this.link});

  final SavedServer server;
  final String link;

  /// Side of the code as rendered for sharing, in pixels.
  ///
  /// Generous on purpose: the picture is going to be scanned off whatever
  /// screen it lands on, and a code that has been through a messaging app's
  /// recompression needs the modules to survive it.
  static const double _sharePixels = 1024;

  /// Clear margin left around the code in the shared picture.
  ///
  /// The specification asks for four modules' worth. At this size that is
  /// roughly sixty pixels for a code of the density an invite link produces,
  /// and erring wide costs nothing.
  static const double _quietZone = 64;

  @override
  Widget build(BuildContext context) {
    final l = L.of(context);
    final scheme = Theme.of(context).colorScheme;

    return ListView(
      padding: const EdgeInsets.fromLTRB(20, 16, 20, 28),
      children: [
        Text(
          server.name,
          textAlign: TextAlign.center,
          style: const TextStyle(fontSize: 17, fontWeight: FontWeight.w600),
        ),
        const SizedBox(height: 16),
        Center(
          child: Container(
            padding: const EdgeInsets.all(16),
            decoration: BoxDecoration(
              // White regardless of the app's theme. Scanners look for dark
              // modules on a light field, and a code drawn in the dark palette
              // this app otherwise uses is one many of them will not see.
              color: Colors.white,
              borderRadius: BorderRadius.circular(16),
            ),
            child: QrImageView(
              data: link,
              version: QrVersions.auto,
              size: 250,
              gapless: true,
              backgroundColor: Colors.white,
              eyeStyle: const QrEyeStyle(
                eyeShape: QrEyeShape.square,
                color: Colors.black,
              ),
              dataModuleStyle: const QrDataModuleStyle(
                dataModuleShape: QrDataModuleShape.square,
                color: Colors.black,
              ),
            ),
          ),
        ),
        const SizedBox(height: 18),
        // Only when there is actually a password in it.
        //
        // Public servers have none, and a warning about a secret that is not
        // there is worse than no warning: it appears on most codes, gets read
        // past, and is then not believed on the one code where it is true.
        if ((server.password ?? '').isNotEmpty) ...[
          // Said plainly, and next to the thing it is about. A code on a screen
          // in a cafe is readable from further away than most people expect,
          // and this one is a working password.
          Row(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Icon(
                Icons.warning_amber_rounded,
                size: 18,
                color: scheme.tertiary,
              ),
              const SizedBox(width: 8),
              Expanded(
                child: Text(
                  l.qrCarriesPassword,
                  style: TextStyle(fontSize: 12, color: scheme.onSurfaceVariant),
                ),
              ),
            ],
          ),
          const SizedBox(height: 20),
        ],
        FilledButton.icon(
          onPressed: () => _shareImage(context),
          icon: const Icon(Icons.ios_share),
          label: Text(l.shareQrImage),
        ),
        const SizedBox(height: 8),
        TextButton.icon(
          onPressed: () => _copy(context),
          icon: const Icon(Icons.link, size: 18),
          label: Text(l.copyMumbleUrl),
        ),
        const SizedBox(height: 12),
        // The link itself, selectable. Somebody on a desktop without a phone to
        // hand can read it off, and it is the only way to see what is actually
        // being shared before sharing it.
        SelectableText(
          link,
          style: TextStyle(
            fontSize: 11,
            fontFamily: 'monospace',
            color: scheme.onSurfaceVariant,
          ),
        ),
      ],
    );
  }

  Future<void> _copy(BuildContext context) async {
    final l = L.of(context);
    final messenger = ScaffoldMessenger.of(context);
    await Clipboard.setData(ClipboardData(text: link));
    messenger.showSnackBar(SnackBar(content: Text(l.linkCopied)));
  }

  Future<void> _shareImage(BuildContext context) async {
    final l = L.of(context);
    final messenger = ScaffoldMessenger.of(context);
    try {
      // Painted straight to pixels rather than captured off the screen: a
      // screenshot of the widget would carry the device's scale factor and
      // whatever is behind it, and on a desktop the whole thing may be smaller
      // than the code deserves to be shared at.
      final painter = QrPainter(
        data: link,
        version: QrVersions.auto,
        gapless: true,
        eyeStyle: const QrEyeStyle(
          eyeShape: QrEyeShape.square,
          color: Colors.black,
        ),
        dataModuleStyle: const QrDataModuleStyle(
          dataModuleShape: QrDataModuleShape.square,
          color: Colors.black,
        ),
      );

      // Composited by hand rather than through the painter's own image helper,
      // for two things that helper does not do. The background is filled
      // opaque white — a PNG left transparent is black-on-black the moment it
      // lands in a dark-themed messaging app, which is the one place this
      // picture is going. And the code is inset to leave a quiet zone, the
      // clear margin scanners use to find the code's edges; without it a
      // reader has to guess where the code stops and the message bubble
      // starts, which many will not do.
      final recorder = ui.PictureRecorder();
      final canvas = Canvas(recorder);
      canvas.drawRect(
        const Rect.fromLTWH(0, 0, _sharePixels, _sharePixels),
        Paint()..color = Colors.white,
      );
      canvas.save();
      canvas.translate(_quietZone, _quietZone);
      painter.paint(
        canvas,
        const Size(
          _sharePixels - _quietZone * 2,
          _sharePixels - _quietZone * 2,
        ),
      );
      canvas.restore();

      final image = await recorder.endRecording().toImage(
        _sharePixels.toInt(),
        _sharePixels.toInt(),
      );
      final data = await image.toByteData(format: ui.ImageByteFormat.png);
      image.dispose();
      if (data == null) throw Exception(l.qrCouldNotRender);

      final dir = await getTemporaryDirectory();
      final safe = server.name.replaceAll(RegExp(r'[^A-Za-z0-9_-]'), '_');
      final file = File('${dir.path}/$safe-qr.png');
      await file.writeAsBytes(data.buffer.asUint8List(), flush: true);

      await SharePlus.instance.share(
        ShareParams(
          files: [XFile(file.path, mimeType: 'image/png')],
          subject: l.joinMeOn(server.name),
        ),
      );
    } catch (e) {
      showError(messenger, '$e');
    }
  }
}
