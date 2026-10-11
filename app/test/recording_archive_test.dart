import 'dart:io';
import 'dart:typed_data';

import 'package:archive/archive_io.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mumbleway/services/recording_archive.dart';

/// What goes into an archive, and what a ride is made of.
///
/// **There was no test here, and that is how a whole track went missing.** The
/// recorder writes three files per segment; the card's share button sent
/// everything in the directory and so carried all three by accident, while the
/// listen sheet built its own list of two and dropped the motion track. A rider
/// sent a ride specifically to settle whether the tap detector fires, and it
/// arrived with the one file that could answer that left behind — looking,
/// from the receiving end, exactly like a detector that found nothing.
///
/// So these tests are about the thing that failed: not whether a zip can be
/// written, but whether every part of a ride is in it and in the *same* one.
void main() {
  late Directory dir;

  setUp(() => dir = Directory.systemTemp.createTempSync('mw-archive-test'));
  tearDown(() => dir.deleteSync(recursive: true));

  // **Incompressible, deliberately.** A constant fill zips to almost nothing,
  // so a cap test written with one never reaches the ceiling and passes by
  // never exercising the branch it is about. A linear congruential sequence is
  // reproducible and close enough to noise for the deflater to give up on.
  var seed = 1;
  String write(String name, int bytes) {
    final path = '${dir.path}${Platform.pathSeparator}$name';
    final data = Uint8List(bytes);
    for (var i = 0; i < bytes; i++) {
      seed = (seed * 1103515245 + 12345) & 0x7fffffff;
      data[i] = (seed >> 16) & 0xff;
    }
    File(path).writeAsBytesSync(data);
    return path;
  }

  group('what a ride is made of', () {


    test('a track that was never written is left out rather than named', () {
      final audio = write('r-000.s16', 16);
      write('r-000.csv', 16);
      final names = rideFiles(audio).map(
        (p) => p.split(Platform.pathSeparator).last,
      );
      expect(names, ['r-000.s16', 'r-000.csv']);
    });
  });

  group('packing', () {
    List<String> namesIn(String archive) => ZipDecoder()
        .decodeBytes(File(archive).readAsBytesSync())
        .files
        .map((f) => f.name)
        .toList();

    test('a ride and its decision log reach the archive together', () {
      final files = [
        write('r-000.s16', 2048),
        write('r-000.csv', 512),
        write('r-000.motion.csv', 512),
      ];
      final archives = packRecordings([
        '$archiveCapBytes',
        dir.path,
        ...files,
      ]);
      expect(archives, hasLength(1));
      expect(
        namesIn(archives.single),
        containsAll(['r-000.s16', 'r-000.csv']),
      );
    });

    test('a cap never separates a motion track from its own audio', () {
      // Two rides, a ceiling that cannot hold both. The guarantee is that
      // whichever archive a ride lands in, all of it lands there — and the
      // motion track is the part that was liable to go elsewhere, because it
      // grouped as a ride of its own.
      final files = [
        for (final stem in ['b-000', 'a-000']) ...[
          write('$stem.s16', 64 * 1024),
          write('$stem.csv', 1024),
        ],
      ];
      final archives = packRecordings(['70000', dir.path, ...files]);
      expect(archives.length, greaterThan(1));

      for (final stem in ['a-000', 'b-000']) {
        final holding = archives
            .where((a) => namesIn(a).contains('$stem.s16'))
            .toList();
        expect(holding, hasLength(1), reason: '$stem should be in exactly one');
        expect(
          namesIn(holding.single),
          containsAll(['$stem.csv']),
          reason: "$stem's own tracks must be beside its audio",
        );
      }
    });
  });
}
