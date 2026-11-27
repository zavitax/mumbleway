import 'dart:io' show Directory, File, Platform;
import 'dart:typed_data' show Uint8List;
import 'dart:ui' as ui;

import 'package:file_selector/file_selector.dart';
import 'package:flutter/material.dart';
import 'package:path_provider/path_provider.dart';
import 'package:qr_flutter/qr_flutter.dart';
import 'package:share_plus/share_plus.dart';

import '../l10n/app_localizations.dart';
import '../src/rust/api/mumbleway.dart';
import '../services/file_reveal.dart';
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
/// Whether the screen offers to save the code to a file and copy the link.
///
/// On a desktop, as well as the share sheet rather than instead of it. The
/// Windows sheet lists only the apps registered as share targets — a messenger
/// or two, mail, nearby sharing — with no way to put the picture in a folder,
/// and it sometimes fails to open at all with *"Couldn't show all the ways to
/// share"*; `share_plus` reports neither, so the app cannot tell the rider the
/// share went nowhere. Linux has no sheet for files at all. A save dialog and
/// the clipboard are what a keyboard and a file manager expect, and they work.
///
/// Not on a phone: there the sheet *is* the way to a folder ("Save image",
/// "Save to Files"), and a second button would only repeat it.
bool qrOffersSaveAs([String? os]) => const {
  'windows',
  'linux',
  'macos',
}.contains(os ?? Platform.operatingSystem);

final _qrUnsafeChars = RegExp(r'[^\p{L}\p{N}_-]+', unicode: true);
final _qrEdgePunctuation = RegExp(r'^[_-]+|[_-]+$');

/// File name offered for the saved or shared picture of [serverName]'s code.
///
/// Reduced to characters every file system accepts, because the name is the
/// rider's own and Windows refuses half of what people type into one.
String qrFileName(String serverName) {
  final stem = serverName
      .replaceAll(_qrUnsafeChars, '_')
      .replaceAll(_qrEdgePunctuation, '');
  return '${stem.isEmpty ? 'mumbleway' : stem}-qr.png';
}

/// Side of the code as rendered for sharing or saving, in pixels.
///
/// Generous on purpose: the picture is going to be scanned off whatever screen
/// it lands on, and a code that has been through a messaging app's
/// recompression needs the modules to survive it.
const double qrImagePixels = 1024;

/// Clear margin left around the code in the picture.
///
/// The specification asks for four modules' worth. At this size that is
/// roughly sixty pixels for a code of the density an invite link produces,
/// and erring wide costs nothing.
const double _quietZone = 64;

/// Paints the code for [link] as a PNG, black on opaque white.
///
/// Painted straight to pixels rather than captured off the screen: a screenshot
/// of the widget would carry the device's scale factor and whatever is behind
/// it, and on a desktop the whole thing may be smaller than the code deserves
/// to be shared at.
///
/// Composited by hand rather than through the painter's own image helper, for
/// two things that helper does not do. The background is filled opaque white —
/// a PNG left transparent is black-on-black the moment it lands in a
/// dark-themed messaging app, which is the one place this picture is going. And
/// the code is inset to leave a quiet zone, the clear margin scanners use to
/// find the code's edges; without it a reader has to guess where the code stops
/// and the message bubble starts, which many will not do.
///
/// Returns null if the engine could not encode the picture.
Future<Uint8List?> renderQrPng(String link) async {
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

  final recorder = ui.PictureRecorder();
  final canvas = Canvas(recorder);
  canvas.drawRect(
    const Rect.fromLTWH(0, 0, qrImagePixels, qrImagePixels),
    Paint()..color = Colors.white,
  );
  canvas.save();
  canvas.translate(_quietZone, _quietZone);
  painter.paint(
    canvas,
    const Size(qrImagePixels - _quietZone * 2, qrImagePixels - _quietZone * 2),
  );
  canvas.restore();

  final image = await recorder.endRecording().toImage(
    qrImagePixels.toInt(),
    qrImagePixels.toInt(),
  );
  final data = await image.toByteData(format: ui.ImageByteFormat.png);
  image.dispose();
  return data?.buffer.asUint8List();
}

/// Prefix of the per-share directories [shareQrImageFile] creates in `dir`.
const qrShareDirPrefix = 'mumbleway-qr-share-';

/// How long a share directory kept for a delayed share target survives.
const qrShareDirMaxAge = Duration(hours: 1);

/// Writes [bytes] to a fresh directory inside [dir] as [fileName], invokes
/// [share] (defaulting to `SharePlus.instance.share`), and deletes that
/// directory unless the platform returns [ShareResultStatus.unavailable].
///
/// On Windows, `SharePlus.instance.share` hands the path to the OS Share Sheet
/// and returns immediately with `unavailable` before the target app reads it,
/// so deleting the file synchronously in `finally` races the target. The file
/// is kept in that case, and because every share gets its own directory a
/// later share — of another channel on the same server, say — can neither
/// overwrite nor delete a file an earlier target is still reading. Directories
/// kept this way are aged out by later shares after [qrShareDirMaxAge].
Future<void> shareQrImageFile({
  required Directory dir,
  required String fileName,
  required Uint8List bytes,
  required String subject,
  Future<ShareResultStatus> Function(ShareParams params)? share,
  DateTime Function()? now,
}) async {
  await _ageOutQrShareDirs(dir, (now ?? DateTime.now)());
  final shareDir = await dir.createTemp(qrShareDirPrefix);
  final file = File('${shareDir.path}${Platform.pathSeparator}$fileName');
  var deleteTemp = false;
  try {
    await file.writeAsBytes(bytes, flush: true);
    final params = ShareParams(
      files: [XFile(file.path, mimeType: 'image/png')],
      subject: subject,
    );
    final ShareResultStatus status;
    if (share != null) {
      status = await share(params);
    } else {
      final result = await SharePlus.instance.share(params);
      status = result.status;
    }
    deleteTemp = status != ShareResultStatus.unavailable;
  } catch (_) {
    deleteTemp = true;
    rethrow;
  } finally {
    if (deleteTemp) {
      try {
        await shareDir.delete(recursive: true);
      } catch (_) {}
    }
  }
}

/// Removes share directories left for delayed share targets once they are
/// older than [qrShareDirMaxAge]. Best effort: a directory still in use or
/// already gone is skipped.
Future<void> _ageOutQrShareDirs(Directory dir, DateTime now) async {
  try {
    await for (final entry in dir.list(followLinks: false)) {
      if (entry is! Directory) continue;
      final name = entry.uri.pathSegments.where((s) => s.isNotEmpty).last;
      if (!name.startsWith(qrShareDirPrefix)) continue;
      try {
        final modified = (await entry.stat()).modified;
        if (now.difference(modified) > qrShareDirMaxAge) {
          await entry.delete(recursive: true);
        }
      } catch (_) {}
    }
  } catch (_) {}
}

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
        // The same link the share menu builds, so a proxy travels in a
        // scanned code exactly as it does in a message.
        config: widget.server.toConfig(
          proxyChain: AppState.sharedProxy(widget.server),
        ),
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
            : _Body(
                server: widget.server,
                channel: widget.channel,
                link: _link!,
              ),
      ),
    );
  }
}

class _Body extends StatelessWidget {
  const _Body({required this.server, required this.channel, required this.link});

  final SavedServer server;

  /// Carried down so the share button below invites to the same channel the
  /// code does, rather than to the server's default.
  final String? channel;

  final String link;

  @override
  Widget build(BuildContext context) {
    final l = L.of(context);
    final scheme = Theme.of(context).colorScheme;
    final desktop = qrOffersSaveAs();

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
        if (desktop) ...[
          const SizedBox(height: 8),
          OutlinedButton.icon(
            onPressed: () => _saveImage(context),
            icon: const Icon(Icons.save_alt),
            label: Text(l.saveQrImage),
          ),
        ],
        const SizedBox(height: 8),
        // The same share sheet the server card offers, and deliberately the
        // same *link*: the https invitation, not the `mumble://` one in the
        // code above it. A code is scanned by a camera, which acts on the
        // scheme directly; a link is pasted into a messaging app, which is
        // where `mumble://` stops working. Copying the raw scheme to the
        // clipboard was the old behaviour here and was the thing most likely
        // to be pasted into exactly the app that cannot open it.
        TextButton.icon(
          onPressed: () => _share(context),
          icon: const Icon(Icons.link, size: 18),
          label: Text(l.shareInviteLink),
        ),
        if (desktop)
          TextButton.icon(
            onPressed: () => _copy(context),
            icon: const Icon(Icons.copy, size: 18),
            label: Text(l.copyInviteLink),
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

  /// Shares the invitation through the platform's own share sheet.
  ///
  /// **With the password**, matching the code on screen rather than asking
  /// again: this screen already puts a scannable password on display, so a
  /// prompt here would be guarding a door that is standing open. The server
  /// card asks, because there the question is still live.
  Future<void> _share(BuildContext context) async {
    final state = AppStateScope.of(context);
    final messenger = ScaffoldMessenger.of(context);
    final error = await state.shareInviteLink(
      server,
      channel: channel,
      includePassword: true,
    );
    if (error != null) showError(messenger, error);
  }

  /// Puts the same https invitation on the clipboard, with the password —
  /// for the reason [_share] gives.
  Future<void> _copy(BuildContext context) async {
    final l = L.of(context);
    final state = AppStateScope.of(context);
    final messenger = ScaffoldMessenger.of(context);
    final error = await state.copyInviteLink(
      server,
      channel: channel,
      includePassword: true,
    );
    if (error != null) {
      showError(messenger, error);
    } else {
      showNotice(messenger, l.inviteLinkCopied);
    }
  }

  Future<void> _shareImage(BuildContext context) async {
    final l = L.of(context);
    final messenger = ScaffoldMessenger.of(context);
    Uint8List? bytes;
    try {
      bytes = await renderQrPng(link);
      if (bytes == null) throw Exception(l.qrCouldNotRender);

      final dir = await getTemporaryDirectory();
      await shareQrImageFile(
        dir: dir,
        fileName: qrFileName(server.name),
        bytes: bytes,
        subject: l.joinMeOn(server.name),
      );
    } on UnimplementedError {
      // A desktop with no share sheet for files — Linux, or a Windows older
      // than the one that brought the system share UI. Saving is the next best
      // thing, and is offered beside this button there anyway.
      if (context.mounted) await _saveImage(context, bytes: bytes);
    } catch (e) {
      showError(messenger, '$e');
    }
  }

  /// Writes the picture wherever the rider chooses, through the system's own
  /// save dialog.
  Future<void> _saveImage(BuildContext context, {Uint8List? bytes}) async {
    final l = L.of(context);
    final messenger = ScaffoldMessenger.of(context);
    try {
      final location = await getSaveLocation(
        suggestedName: qrFileName(server.name),
        acceptedTypeGroups: const [
          XTypeGroup(label: 'PNG', extensions: ['png']),
        ],
      );
      if (location == null) return; // cancelled

      // The dialog hands back what was typed, and a name typed without its
      // extension would otherwise be a file nothing offers to open.
      var path = location.path;
      if (!path.toLowerCase().endsWith('.png')) path = '$path.png';

      final png = bytes ?? await renderQrPng(link);
      if (png == null) throw Exception(l.qrCouldNotRender);
      await File(path).writeAsBytes(png, flush: true);

      messenger
        ..hideCurrentSnackBar()
        ..showSnackBar(
          SnackBar(
            content: Text(l.qrSavedTo(path)),
            action: SnackBarAction(
              label: l.showInFolder,
              onPressed: () => revealFile(path),
            ),
          ),
        );
    } catch (e) {
      showError(messenger, '$e');
    }
  }
}
