import 'dart:io';
import 'dart:typed_data';

import 'package:flutter_test/flutter_test.dart';
import 'package:mumbleway/services/recording_player.dart';

/// Green on the waveform is a claim about what reached the far end, so the
/// thing worth testing is the reading of the log rather than the drawing.
///
/// The `.csv` starts with a comment and then a header, and taking the comment
/// for the header is a mistake this project has already made once — offline,
/// where it reported 0% transmitting on a file that was 36%.
void main() {
  late Directory dir;

  setUp(() async {
    dir = await Directory.systemTemp.createTemp('wave');
  });
  tearDown(() async => dir.delete(recursive: true));

  /// `blocks` of audio, and a log marking the given block indices as sent.
  ///
  /// `firstBlock` is where the log's block column starts, which is not always
  /// zero: recordings made before the rotation fix carry on counting from the
  /// previous file, and those files are on people's phones.
  Future<String> write(int blocks, Set<int> sent,
      {bool log = true, int firstBlock = 0}) async {
    final audio = File('${dir.path}/r.s16');
    final pcm = Int16List(blocks * kRecordingBlock);
    for (var i = 0; i < pcm.length; i++) {
      pcm[i] = 8000; // non-silent, so every bucket draws
    }
    await audio.writeAsBytes(pcm.buffer.asUint8List());
    if (log) {
      final rows = StringBuffer('# mumbleway diagnostic capture\n'
          'block,transmitting,speaking,gate_open,vad,snr_db,level_db,'
          'floor_db,harmonicity,modulation\n');
      for (var b = 0; b < blocks; b++) {
        rows.writeln('${firstBlock + b},${sent.contains(b) ? 1 : 0}'
            ',0,0,0.5,10,-40,-60,0.5,0.4');
      }
      await File('${dir.path}/r.csv').writeAsString(rows.toString());
    }
    return audio.path;
  }

  test('the sent blocks come back as sent buckets', () async {
    // 100 blocks, one second, with the first quarter transmitted.
    final path = await write(100, {for (var i = 0; i < 25; i++) i});
    final w = await scanForTest(path, 20);
    expect(w.buckets, 20);
    expect(w.transmitted, isNotEmpty);
    // Buckets 0-4 cover blocks 0-24.
    expect(w.wasSent(0), isTrue);
    expect(w.wasSent(4), isTrue);
    expect(w.wasSent(10), isFalse);
    expect(w.wasSent(19), isFalse);
  });

  test('the comment line is not mistaken for data', () async {
    // Every block sent. If the header or the comment were parsed as a row, the
    // alignment would slip and the last bucket would come back unsent.
    final path = await write(60, {for (var i = 0; i < 60; i++) i});
    final w = await scanForTest(path, 12);
    for (var i = 0; i < w.buckets; i++) {
      expect(w.wasSent(i), isTrue, reason: 'bucket $i');
    }
  });

  test('no log at all is not the same as nothing sent', () async {
    final path = await write(40, const {}, log: false);
    final w = await scanForTest(path, 8);
    // Empty rather than all-false: the painter shows the ordinary colours
    // instead of claiming the whole recording never went out.
    expect(w.transmitted, isEmpty);
    expect(w.wasSent(0), isFalse);
  });

  test('a rotated log that carries on counting still lines up', () async {
    // The bug this exists for: a ride over 16 MB rotates to a second pair of
    // files, and the writer used to keep counting blocks across the rotation.
    // The second file's log opened at block 17,477 with its audio at sample
    // zero, so every row was clamped to the last bucket and the whole tail
    // drew as if none of it had been transmitted. It is the newest name, so it
    // is the file the listen sheet opens first.
    final path = await write(100, {for (var i = 0; i < 25; i++) i},
        firstBlock: 17477);
    final w = await scanForTest(path, 20);
    expect(w.transmitted, isNotEmpty);
    expect(w.wasSent(0), isTrue, reason: 'the first quarter was transmitted');
    expect(w.wasSent(4), isTrue);
    expect(w.wasSent(10), isFalse);
    expect(w.wasSent(19), isFalse, reason: 'not everything piled into the end');
  });

  test('a bucket counts as sent if any of it was', () async {
    // One block in ten, so every bucket contains exactly one sent block.
    final path = await write(100, {for (var i = 0; i < 100; i += 10) i});
    final w = await scanForTest(path, 10);
    for (var i = 0; i < w.buckets; i++) {
      expect(w.wasSent(i), isTrue, reason: 'bucket $i');
    }
  });
}
