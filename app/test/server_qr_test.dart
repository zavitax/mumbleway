import 'dart:io' show Directory, File;
import 'dart:typed_data' show Uint8List;
import 'dart:ui' as ui;

import 'package:flutter_test/flutter_test.dart';
import 'package:mumbleway/screens/server_qr_screen.dart';
import 'package:share_plus/share_plus.dart';

void main() {
  group('qrOffersSaveAs', () {
    // The Windows share sheet has no "save to a folder" target and can fail to
    // open without telling the app, so every desktop gets a save dialog beside
    // it. Phones do not: their sheet already carries "Save image".
    test('offers saving on every desktop', () {
      expect(qrOffersSaveAs('windows'), isTrue);
      expect(qrOffersSaveAs('linux'), isTrue);
      expect(qrOffersSaveAs('macos'), isTrue);
    });

    test('leaves phones to their share sheet', () {
      expect(qrOffersSaveAs('android'), isFalse);
      expect(qrOffersSaveAs('ios'), isFalse);
    });
  });

  test('the file name is safe on every file system', () {
    expect(qrFileName('Obrezki'), 'Obrezki-qr.png');
    expect(qrFileName('Сервер'), 'Сервер-qr.png');
    expect(qrFileName('Мой сервер 1'), 'Мой_сервер_1-qr.png');
    expect(qrFileName('My: server/1?'), 'My_server_1-qr.png');
    expect(qrFileName(r'a<b>c:d"e/f\g|h?i*j'), 'a_b_c_d_e_f_g_h_i_j-qr.png');
    expect(qrFileName('srv\x00\x1f\x7fname'), 'srv_name-qr.png');

    for (final blank in const [
      '',
      '   ',
      '...',
      '___',
      '---',
      r'<>:"/\|?*',
      '\x00\x1f\x7f',
    ]) {
      expect(qrFileName(blank), 'mumbleway-qr.png');
    }
  });

  testWidgets('the picture is a square PNG on an opaque white field', (
    tester,
  ) async {
    await tester.runAsync(() async {
      final png = await renderQrPng('mumble://user@example.org:64738/');
      expect(png, isNotNull);
      // PNG signature, so a file saved with this is one a viewer opens.
      expect(png!.sublist(0, 8), [137, 80, 78, 71, 13, 10, 26, 10]);

      final codec = await ui.instantiateImageCodec(png);
      final image = (await codec.getNextFrame()).image;
      expect(image.width, qrImagePixels.toInt());
      expect(image.height, qrImagePixels.toInt());

      final rgba = (await image.toByteData())!;
      // Top-left pixel lies in the quiet zone: white and fully opaque, not the
      // transparent black that disappears in a dark-themed messenger.
      expect(rgba.getUint32(0), 0xFFFFFFFF);
      image.dispose();
    });
  });

  group('shareQrImageFile', () {
    late Directory tempDir;

    setUp(() => tempDir = Directory.systemTemp.createTempSync('mw-qr-share-'));
    tearDown(() => tempDir.deleteSync(recursive: true));

    test('deletes the temp file when share returns a definitive status', () async {
      for (final status in const [
        ShareResultStatus.success,
        ShareResultStatus.dismissed,
      ]) {
        late String sharedPath;
        String? sharedMime;
        await shareQrImageFile(
          dir: tempDir,
          fileName: 'Srv-qr.png',
          bytes: Uint8List.fromList([1, 2, 3]),
          subject: 'Join me on Srv',
          share: (params) async {
            final xfile = params.files!.single;
            sharedPath = xfile.path;
            sharedMime = xfile.mimeType;
            expect(File(sharedPath).existsSync(), isTrue);
            expect(File(sharedPath).readAsBytesSync(), [1, 2, 3]);
            return status;
          },
        );
        expect(sharedMime, 'image/png');
        expect(File(sharedPath).existsSync(), isFalse);
      }
    });

    test('keeps the temp file on unavailable status, one per share', () async {
      late String firstPath;
      await shareQrImageFile(
        dir: tempDir,
        fileName: 'Srv-qr.png',
        bytes: Uint8List.fromList([1, 2, 3]),
        subject: 'Join me on Srv',
        share: (params) async {
          firstPath = params.files!.single.path;
          return ShareResultStatus.unavailable;
        },
      );
      expect(File(firstPath).existsSync(), isTrue);
      expect(File(firstPath).readAsBytesSync(), [1, 2, 3]);
      expect(firstPath, endsWith('Srv-qr.png'));

      // A second share with the same friendly name — another channel on the
      // same server — must not touch the file the first target may still be
      // reading.
      late String secondPath;
      await shareQrImageFile(
        dir: tempDir,
        fileName: 'Srv-qr.png',
        bytes: Uint8List.fromList([4, 5, 6]),
        subject: 'Join me on Srv',
        share: (params) async {
          secondPath = params.files!.single.path;
          return ShareResultStatus.success;
        },
      );
      expect(secondPath, isNot(firstPath));
      expect(secondPath, endsWith('Srv-qr.png'));
      expect(File(firstPath).readAsBytesSync(), [1, 2, 3]);
      expect(File(secondPath).existsSync(), isFalse);
    });

    test('ages out share directories kept for a delayed target', () async {
      late String keptPath;
      await shareQrImageFile(
        dir: tempDir,
        fileName: 'Srv-qr.png',
        bytes: Uint8List.fromList([1, 2, 3]),
        subject: 'Join me on Srv',
        share: (params) async {
          keptPath = params.files!.single.path;
          return ShareResultStatus.unavailable;
        },
      );
      expect(File(keptPath).existsSync(), isTrue);

      final later = DateTime.now().add(qrShareDirMaxAge * 2);
      await shareQrImageFile(
        dir: tempDir,
        fileName: 'Other-qr.png',
        bytes: Uint8List.fromList([4, 5, 6]),
        subject: 'Join me on Other',
        share: (_) async => ShareResultStatus.success,
        now: () => later,
      );
      expect(File(keptPath).existsSync(), isFalse);
      expect(
        tempDir.listSync().where((e) => e.path.contains(qrShareDirPrefix)),
        isEmpty,
      );
    });

    test('deletes the temp file and rethrows when share throws', () async {
      late String sharedPath;
      var threw = false;
      try {
        await shareQrImageFile(
          dir: tempDir,
          fileName: 'Srv-qr.png',
          bytes: Uint8List.fromList([7, 8, 9]),
          subject: 'Join me on Srv',
          share: (params) async {
            sharedPath = params.files!.single.path;
            expect(File(sharedPath).existsSync(), isTrue);
            throw UnimplementedError('no share sheet');
          },
        );
      } on UnimplementedError {
        threw = true;
      }
      expect(threw, isTrue);
      expect(File(sharedPath).existsSync(), isFalse);
    });
  });
}
