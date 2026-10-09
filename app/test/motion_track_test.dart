import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:mumbleway/services/motion_track.dart';

/// Reading and bucketing the recorder's third track.
///
/// The drawing is read for shape, and a shape is exactly the kind of thing
/// that looks plausible while being wrong — so what is checked here is the
/// arithmetic underneath it: that a bucket keeps the extreme it was created to
/// keep, that direction means what a trader would assume, and that an
/// unrecorded stretch is not drawn as a still one.
void main() {
  late Directory dir;

  setUp(() => dir = Directory.systemTemp.createTempSync('mw-motion-test'));
  tearDown(() => dir.deleteSync(recursive: true));

  /// Writes a `.motion.csv` with the real header, and the `.s16` beside it
  /// that the reader takes its name from.
  String track(List<String> rows) {
    final stem = '${dir.path}${Platform.pathSeparator}r-000';
    File('$stem.s16').writeAsBytesSync(const []);
    File('$stem.motion.csv').writeAsStringSync(
      '# mumbleway motion track; block is the audio block within THIS segment\n'
      'block,platform_ns,arrival_us,ax,ay,az,gx,gy,gz,rx,ry,rz\n'
      '${rows.join('\n')}\n',
    );
    return '$stem.s16';
  }

  /// One row. `block` is the audio block, so time is `block * 10 ms`.
  String row(int block, double ax, {double az = 0, double rx = 0}) =>
      '$block,${block * 10000000},${block * 10000},'
      '$ax,0.0,$az,0.0,0.0,9.80665,$rx,0.0,0.0';

  test('a bucket is 250 ms of blocks, and the span comes from the audio', () async {
    // 100 blocks of 10 ms is one second: four buckets.
    final path = track([for (var i = 0; i < 100; i++) row(i, 0.01)]);
    final t = await readMotionTrack([path, 1.0]);
    expect(t.isEmpty, isFalse);
    expect(t.buckets, 4);
  });

  test('an impulse survives as the bucket high, and is lost by the mean', () async {
    // One sample in twenty-five is the whole difficulty: this is why candles
    // exist, and the numbers here are the argument for them.
    final rows = [
      for (var i = 0; i < 25; i++) row(i, i == 12 ? 4.0 : 0.0),
    ];
    final t = await readMotionTrack([track(rows), 0.25]);
    final a = t.series[MotionSeries.accelMagnitude]!;
    expect(a.hi[0], closeTo(4.0, 1e-6), reason: 'the high must keep the tap');
    expect(
      a.mean[0],
      lessThan(0.3),
      reason: 'the mean divides a tap away — the reason for the other mode',
    );
  });

  test('direction is open against close, as a trader would read it', () async {
    final rising = [for (var i = 0; i < 25; i++) row(i, i * 0.1)];
    final falling = [for (var i = 0; i < 25; i++) row(i, 2.5 - i * 0.1)];

    final up = await readMotionTrack([track(rising), 0.25]);
    final ua = up.series[MotionSeries.accelMagnitude]!;
    expect(ua.close[0], greaterThan(ua.open[0]));

    dir.deleteSync(recursive: true);
    dir = Directory.systemTemp.createTempSync('mw-motion-test');
    final down = await readMotionTrack([track(falling), 0.25]);
    final da = down.series[MotionSeries.accelMagnitude]!;
    expect(da.close[0], lessThan(da.open[0]));
  });

  test('a gap is unfilled rather than zero', () async {
    // Blocks 0-24 then 75-99: a whole second with the middle half missing.
    final rows = [
      for (var i = 0; i < 25; i++) row(i, 0.5),
      for (var i = 75; i < 100; i++) row(i, 0.5),
    ];
    final t = await readMotionTrack([track(rows), 1.0]);
    final a = t.series[MotionSeries.accelMagnitude]!;
    expect(a.filled[0], 1);
    expect(a.filled[1], 0, reason: 'nothing was recorded here');
    expect(a.filled[2], 0);
    expect(a.filled[3], 1);
  });

  test('gravity splits acceleration into along and across', () async {
    // Gravity is +z in `row`, so an x-only acceleration is entirely across it
    // and a z-only one entirely along it. Getting this backwards would invert
    // the one distinction the picture exists to show — road shock against a
    // reaching hand.
    final across = await readMotionTrack([
      track([for (var i = 0; i < 25; i++) row(i, 2.0)]),
      0.25,
    ]);
    expect(across.series[MotionSeries.acrossGravity]!.hi[0], closeTo(2.0, 1e-3));
    expect(across.series[MotionSeries.alongGravity]!.hi[0], closeTo(0.0, 1e-3));

    dir.deleteSync(recursive: true);
    dir = Directory.systemTemp.createTempSync('mw-motion-test');
    final along = await readMotionTrack([
      track([for (var i = 0; i < 25; i++) row(i, 0.0, az: 2.0)]),
      0.25,
    ]);
    expect(along.series[MotionSeries.alongGravity]!.hi[0], closeTo(2.0, 1e-3));
    expect(along.series[MotionSeries.acrossGravity]!.hi[0], closeTo(0.0, 1e-3));
  });

  test('a recording with no motion track reads as none, not as stillness', () async {
    final stem = '${dir.path}${Platform.pathSeparator}old-000';
    File('$stem.s16').writeAsBytesSync(const []);
    final t = await readMotionTrack(['$stem.s16', 10.0]);
    expect(t.isEmpty, isTrue);
  });

  test('a recording with no audio falls back to the sensor clock', () async {
    // Measured on an emulator, which has no microphone: `open_sink` writes all
    // three files, the `.s16` is zero bytes, and **every motion row carries
    // block 0**. Keying off the block column would stack the whole ride into
    // one column, and taking the span from a zero-length audio file would
    // refuse to draw it at all — on exactly the recording whose motion is the
    // question.
    final stem = '${dir.path}${Platform.pathSeparator}noaudio-000';
    File('$stem.s16').writeAsBytesSync(const []);
    File('$stem.motion.csv').writeAsStringSync(
      '# mumbleway motion track\n'
      'block,platform_ns,arrival_us,ax,ay,az,gx,gy,gz,rx,ry,rz\n'
      '${[
        // One second of 100 Hz with block stuck at 0 throughout.
        for (var i = 0; i < 100; i++)
          '0,0,${i * 10000},${i % 10 == 0 ? 2.0 : 0.0},'
              '0.0,0.0,0.0,0.0,9.80665,0.0,0.0,0.0',
      ].join('\n')}\n',
    );

    final t = await readMotionTrack(['$stem.s16', 0.0]);
    expect(t.isEmpty, isFalse, reason: 'a track with no audio still draws');
    expect(t.alignedToAudio, isFalse, reason: 'and says it is not aligned');
    expect(t.buckets, greaterThan(1), reason: 'spread over its own clock');

    // The impulses must land in different buckets rather than all in the first.
    final a = t.series[MotionSeries.accelMagnitude]!;
    var withPeak = 0;
    for (var i = 0; i < t.buckets; i++) {
      if (a.filled[i] == 1 && a.hi[i] > 1.0) withPeak++;
    }
    expect(withPeak, greaterThan(1));
  });

  test('columns are found by name, so a new one cannot shift the rest', () async {
    // The decision log grew two columns after recordings were already on
    // people's phones. This reader is written so that the same thing here is
    // a non-event, and this is the test that says so.
    final stem = '${dir.path}${Platform.pathSeparator}r-000';
    File('$stem.s16').writeAsBytesSync(const []);
    File('$stem.motion.csv').writeAsStringSync(
      '# mumbleway motion track\n'
      'block,platform_ns,arrival_us,psi,ax,ay,az,gx,gy,gz,rx,ry,rz\n'
      '${[
        for (var i = 0; i < 25; i++)
          '$i,0,${i * 10000},999.0,3.0,0.0,0.0,0.0,0.0,9.80665,0.0,0.0,0.0',
      ].join('\n')}\n',
    );
    final t = await readMotionTrack(['$stem.s16', 0.25]);
    expect(
      t.series[MotionSeries.accelMagnitude]!.hi[0],
      closeTo(3.0, 1e-6),
      reason: 'a column inserted before ax must not shift the reader',
    );
  });
}
