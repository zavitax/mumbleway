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
  ///
  /// `mode` and `muted` are the columns added after the Android ride that
  /// transmitted nothing; `legacy` writes a log without them, which is what
  /// every recording made before them looks like.
  Future<String> write(int blocks, Set<int> sent,
      {bool log = true,
      int firstBlock = 0,
      int mode = 0,
      bool muted = false,
      bool legacy = false}) async {
    final audio = File('${dir.path}/r.s16');
    final pcm = Int16List(blocks * kRecordingBlock);
    for (var i = 0; i < pcm.length; i++) {
      pcm[i] = 8000; // non-silent, so every bucket draws
    }
    await audio.writeAsBytes(pcm.buffer.asUint8List());
    if (log) {
      final rows = StringBuffer('# mumbleway diagnostic capture\n'
          'block,transmitting,speaking,gate_open,vad,snr_db,level_db,'
          'floor_db,harmonicity,modulation${legacy ? '' : ',mode,muted'}\n');
      for (var b = 0; b < blocks; b++) {
        rows.writeln('${firstBlock + b},${sent.contains(b) ? 1 : 0}'
            ',0,0,0.5,10,-40,-60,0.5,0.4'
            '${legacy ? '' : ',$mode,${muted ? 1 : 0}'}');
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

  test('the green and the per-block decisions come from one reading', () async {
    // The waveform's colour and the speech-only transport are the same claim
    // shown two ways. They are computed from one list for that reason, and
    // this is the assertion that says so.
    final path = await write(100, {for (var i = 40; i < 60; i++) i});
    final flags = (await decisionsForTest(path)).sent;
    expect(flags.length, 100);
    expect(flags[39], 0);
    expect(flags[40], 1);
    expect(flags[59], 1);
    expect(flags[60], 0);

    final w = await scanForTest(path, 10);
    for (var b = 0; b < 10; b++) {
      final any = [for (var i = b * 10; i < b * 10 + 10; i++) flags[i]]
          .any((f) => f != 0);
      expect(w.wasSent(b), any, reason: 'bucket $b');
    }
  });

  group('why a recording has no green in it', () {
    // The bug this exists for was not a bug. An Android ride came back with
    // 64.9% of blocks marked as speech and not one marked as transmitted, and
    // an all-grey waveform reads as a broken drawing. It was the chain being
    // told not to send — but the log recorded neither of the two settings that
    // do that, so the file could not say so and it took a device to find out.

    test('a muted microphone says so', () async {
      final path = await write(50, const {}, muted: true);
      final d = await decisionsForTest(path);
      expect(d.anySent, isFalse);
      expect(d.reason, NothingSent.muted);
    });

    test('push to talk with the button up says so', () async {
      // 1 is push to talk, from TransmitMode's declaration order.
      final path = await write(50, const {}, mode: 1);
      final d = await decisionsForTest(path);
      expect(d.reason, NothingSent.pushToTalk);
    });

    test('a log from before the columns admits it does not know', () async {
      // Those recordings are on people's phones. Guessing a reason for them
      // would be worse than the grey it replaces.
      final path = await write(50, const {}, legacy: true);
      final d = await decisionsForTest(path);
      expect(d.sent.length, 50, reason: 'the old columns still read');
      expect(d.reason, NothingSent.unexplained);
    });

    test('a gate that simply never opened is not blamed on a setting',
        () async {
      // Voice activated, not muted, and nothing got through. That is a real
      // finding about the gate and must not be reported as a mistake by the
      // rider.
      final path = await write(50, const {});
      expect((await decisionsForTest(path)).reason, NothingSent.unexplained);
    });

    test('nothing is explained when something was sent', () async {
      final path = await write(50, {1, 2, 3}, muted: true);
      final d = await decisionsForTest(path);
      expect(d.anySent, isTrue);
      expect(d.reason, NothingSent.some,
          reason: 'there is nothing to explain when audio went out');
    });

    test('the columns are found by name, not by position', () async {
      // Two columns were added after recordings were already on phones. A
      // reader that counts commas would give every one of those files a
      // different meaning; this asserts the header is what is trusted.
      final audio = File('${dir.path}/r.s16');
      await audio.writeAsBytes(Int16List(4 * kRecordingBlock).buffer
          .asUint8List());
      await File('${dir.path}/r.csv').writeAsString(
        '# mumbleway diagnostic capture\n'
        'block,muted,mode,transmitting\n'
        '0,1,0,0\n1,1,0,0\n2,1,0,0\n3,1,0,0\n',
      );
      final d = await decisionsForTest(audio.path);
      expect(d.sent.length, 4);
      expect(d.anySent, isFalse);
      expect(d.reason, NothingSent.muted);
    });
  });

  group('skipping to what was transmitted', () {
    /// A player with decisions but no file: the arithmetic is what is under
    /// test, and it needs neither an engine nor audio.
    RecordingPlayer withBlocks(List<int> flags) {
      final p = RecordingPlayer();
      p.loadForTest(
        Uint8List.fromList(flags),
        flags.length * kRecordingBlock,
      );
      p.setSpeechOnly(true);
      return p;
    }

    test('silence in front is skipped, and the run ends where it ends', () {
      // Ten blocks; only 4, 5 and 6 went out.
      final p = withBlocks([0, 0, 0, 0, 1, 1, 1, 0, 0, 0]);
      expect(p.speechOnly, isTrue);
      expect(p.nextAudibleForTest(0), 4 * kRecordingBlock);
      expect(p.audibleEndForTest(4 * kRecordingBlock), 7 * kRecordingBlock);
      // From inside the run, nothing moves and the end is unchanged.
      expect(p.nextAudibleForTest(5 * kRecordingBlock + 100),
          5 * kRecordingBlock + 100);
      expect(p.audibleEndForTest(5 * kRecordingBlock), 7 * kRecordingBlock);
    });

    test('past the last transmitted block it runs to the end', () {
      final p = withBlocks([1, 1, 0, 0]);
      // Nothing further was sent, so there is nowhere left to go. The caller
      // reads a position at or past the end as "finished".
      expect(p.nextAudibleForTest(2 * kRecordingBlock),
          greaterThanOrEqualTo(4 * kRecordingBlock));
    });

    test('turning it off plays everything again', () {
      final p = withBlocks([0, 0, 1, 0]);
      p.setSpeechOnly(false);
      expect(p.nextAudibleForTest(0), 0);
      expect(p.audibleEndForTest(0), 4 * kRecordingBlock);
    });

    test('a recording that transmitted nothing cannot be filtered', () {
      // Not a fault — a real outcome, and "play only the transmitted parts" of
      // it is silence. The control is unavailable rather than silent.
      final p = RecordingPlayer();
      p.loadForTest(Uint8List.fromList([0, 0, 0]), 3 * kRecordingBlock);
      expect(p.canSkipSilence, isFalse);
      p.setSpeechOnly(true);
      expect(p.speechOnly, isFalse, reason: 'refused, rather than silent');
    });

    test('audio past the end of the log is played, not skipped', () {
      // A log shorter than its audio is missing information. Treating the
      // absent rows as "not transmitted" would silently drop the tail of a
      // ride, which is the failure the green already has a test against.
      final p = RecordingPlayer();
      p.loadForTest(
        Uint8List.fromList([1, 0]),
        // Four blocks of audio, two blocks of log.
        4 * kRecordingBlock,
      );
      p.setSpeechOnly(true);
      expect(p.nextAudibleForTest(kRecordingBlock), 2 * kRecordingBlock);
      expect(p.audibleEndForTest(2 * kRecordingBlock), 4 * kRecordingBlock);
    });
  });
}
