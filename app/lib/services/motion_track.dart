/// The recorder's third track, bucketed for drawing.
///
/// **The point of this is to look at a ride before measuring it.** The tap
/// detector's numbers say *what* it decided; they cannot say what the phone was
/// doing when it decided. A false arm on a motorway and a missed tap through a
/// jacket pocket produce the same two counters and completely different
/// pictures, and the picture is the thing that says which to go and fix.
///
/// ## Why 250 ms
///
/// A bucket has to be short enough that the three impulses of a gesture land in
/// different ones — deliberate tapping sits around 150–250 ms apart — and long
/// enough that a half-hour ride is a few thousand columns rather than a few
/// hundred thousand samples. 250 ms is the coarsest value that still separates
/// the taps of a gesture, so a gesture reads as a run of adjacent candles
/// rather than as one tall one.
///
/// ## Why candles
///
/// The series here are *impulsive*. A mean over 250 ms is exactly the wrong
/// summary for an impulse: a tap is two or three samples out of twenty-five, so
/// averaging divides it by ten and hides the only thing worth seeing. The high
/// and low of the bucket keep it. The average mode is still offered because it
/// is the right summary for the slow things — gravity, a lean, a long
/// undulation — where the extremes are noise and the trend is the signal.
///
/// Direction is open against close, coloured the way a trader would read it,
/// because that is a convention nobody has to be taught.
library;

import 'dart:io';
import 'dart:math' as math;
import 'dart:typed_data';

/// Which quantity a column is drawn from.
///
/// **ψ first, because it is the one the detector actually thresholds.** The
/// others exist to explain it: when ψ spikes and no gesture followed, the
/// question is immediately whether the energy was along gravity (the road) or
/// across it (a hand), and whether the phone rotated (a bag swinging) or only
/// translated (a strike).
enum MotionSeries {
  /// Teager–Kaiser, per axis and summed — `core/src/audio/tap.rs`'s own
  /// operator, recomputed here so the picture and the detector cannot disagree
  /// about what they are looking at.
  psi('ψ  Teager–Kaiser energy', 'm²/s⁴'),

  /// How hard the phone was accelerating, gravity already removed.
  accelMagnitude('|a|  acceleration', 'm/s²'),

  /// The component along world-vertical. Suspension travel arrives here.
  alongGravity('a∥g  along gravity', 'm/s²'),

  /// The component across it. A hand reaching to a thigh arrives here.
  acrossGravity('a⊥g  across gravity', 'm/s²'),

  accelX('aₓ', 'm/s²'),
  accelY('a_y', 'm/s²'),
  accelZ('a_z', 'm/s²'),

  /// A tap translates; a bag on a strap swings. This tells them apart.
  rotationMagnitude('|ω|  rotation', 'rad/s'),

  rotationX('ωₓ', 'rad/s'),
  rotationY('ω_y', 'rad/s'),
  rotationZ('ω_z', 'rad/s'),

  /// Where the phone was pointing. Flat in a pocket, swinging in a bag.
  tiltFromVertical('tilt from vertical', '°');

  const MotionSeries(this.label, this.unit);
  final String label, unit;
}

/// How a bucket is reduced to something drawable.
enum MotionDrawMode {
  /// High, low, and the direction from the bucket's first sample to its last.
  candles,

  /// The mean of the bucket. Right for the slow series, wrong for impulses.
  average,
}

/// One series, already reduced to buckets.
///
/// Five parallel arrays rather than a list of objects: a half-hour ride is
/// about 7200 buckets across twelve series, and that is 86 000 small objects
/// against twelve allocations.
class SeriesBuckets {
  SeriesBuckets(int n)
    : lo = Float32List(n),
      hi = Float32List(n),
      open = Float32List(n),
      close = Float32List(n),
      mean = Float32List(n),
      filled = Uint8List(n);

  final Float32List lo, hi, open, close, mean;

  /// Whether the bucket had any sample in it at all.
  ///
  /// A gap in the motion track is not a flat line, and drawing it as one would
  /// invent stillness the sensors never reported — which is the same mistake
  /// as a decision log with no audio beside it.
  final Uint8List filled;

  /// The largest absolute value anywhere in the series, for scaling.
  double get peak {
    var p = 0.0;
    for (var i = 0; i < hi.length; i++) {
      if (filled[i] == 0) continue;
      p = math.max(p, math.max(hi[i].abs(), lo[i].abs()));
    }
    return p;
  }
}

/// A whole motion track, bucketed.
class MotionTrack {
  MotionTrack({
    required this.series,
    required this.bucketSeconds,
    required this.seconds,
    required this.samples,
    required this.hz,
    this.alignedToAudio = true,
  });

  final Map<MotionSeries, SeriesBuckets> series;
  final double bucketSeconds;

  /// The span the buckets cover, which is the *audio's* duration rather than
  /// the motion track's own — so a column sits under the same instant of the
  /// waveform above it.
  final double seconds;

  final int samples;

  /// Whether the columns line up with the waveform above.
  ///
  /// False when the recording had no audio to align to and the arrival clock
  /// was used instead. The drawing says so, because a timeline that silently
  /// means something else is worse than one that is missing.
  final bool alignedToAudio;

  /// Delivered rate, which is the first thing to check when a picture looks
  /// wrong: iOS gives about 100 Hz, Android 200–500, and an emulator 50.
  final double hz;

  bool get isEmpty => samples == 0 || series.isEmpty;
  int get buckets => isEmpty ? 0 : series.values.first.lo.length;

  static final none = MotionTrack(
    series: const {},
    bucketSeconds: 0.25,
    seconds: 0,
    samples: 0,
    hz: 0,
  );
}

/// Resolution, and see the note on [MotionTrack] for why this number.
const double motionBucketSeconds = 0.25;

/// Reads `{stem}.motion.csv` beside a recording and buckets it.
///
/// Shaped for `compute`: `[audioPath, secondsOfAudio]`. Parsing a half-hour
/// ride is a few hundred thousand lines, which is seconds of work and belongs
/// off the isolate that is drawing a playhead twelve times a second.
Future<MotionTrack> readMotionTrack(List<Object> args) async {
  final audioPath = args[0] as String;
  final seconds = args[1] as double;

  final stem = audioPath.endsWith('.s16')
      ? audioPath.substring(0, audioPath.length - 4)
      : audioPath;
  final file = File('$stem.motion.csv');
  if (!await file.exists()) return MotionTrack.none;

  final lines = await file.readAsLines();

  // **Which clock to place samples on is decided before placing any.**
  //
  // Normally it is the `block` column — the audio block the sample fell
  // within, the one timebase shared with the waveform above, and sharing it is
  // why the column exists. But a recording can have no audio in it: the
  // microphone can be held by another app, and an emulator has none at all. In
  // that case `open_sink` still writes all three files, the `.s16` is zero
  // bytes, and **every motion row carries block 0** — so a block timebase
  // would stack an entire ride into the first column, and an audio duration of
  // zero would refuse to draw it at all.
  //
  // Measured on an emulator: 2338 rows of good sensor data, `.s16` 0 bytes.
  // Falling back to the arrival clock keeps that recording readable, which
  // matters because a ride with no audio is *exactly* the one somebody is
  // looking at the motion of.
  final probe = _timebase(lines);
  final byBlock = probe.blocksAdvance && seconds > 0;
  final span = byBlock
      ? seconds
      : (probe.lastUs - probe.firstUs) / 1e6;
  if (span <= 0) return MotionTrack.none;

  final n = math.max(1, (span / motionBucketSeconds).ceil());
  final series = {for (final s in MotionSeries.values) s: SeriesBuckets(n)};
  final counts = Int32List(n);
  final sums = {for (final s in MotionSeries.values) s: Float64List(n)};

  // Found by name, never by position — the decision log grew two columns after
  // recordings were already on people's phones, and a reader that counts commas
  // gives every older file a different meaning. The same discipline here, for
  // the same reason, before it has had a chance to bite.
  var blockAt = -1, arrivalAt = -1;
  var axAt = -1, ayAt = -1, azAt = -1;
  var gxAt = -1, gyAt = -1, gzAt = -1;
  var rxAt = -1, ryAt = -1, rzAt = -1;

  // ψ needs three consecutive samples per axis, exactly as the detector does.
  final hist = [Float64List(3), Float64List(3), Float64List(3)];
  var seen = 0;
  var samples = 0;
  var firstUs = 0, lastUs = 0;

  double at(List<String> p, int i) =>
      i >= 0 && i < p.length ? (double.tryParse(p[i]) ?? 0) : 0;

  try {
    for (final line in lines) {
      if (line.isEmpty || line.startsWith('#')) continue;
      final p = line.split(',');
      if (p.isEmpty) continue;
      if (int.tryParse(p.first) == null) {
        blockAt = p.indexOf('block');
        arrivalAt = p.indexOf('arrival_us');
        axAt = p.indexOf('ax');
        ayAt = p.indexOf('ay');
        azAt = p.indexOf('az');
        gxAt = p.indexOf('gx');
        gyAt = p.indexOf('gy');
        gzAt = p.indexOf('gz');
        rxAt = p.indexOf('rx');
        ryAt = p.indexOf('ry');
        rzAt = p.indexOf('rz');
        continue;
      }

      final a = [at(p, axAt), at(p, ayAt), at(p, azAt)];
      final g = [at(p, gxAt), at(p, gyAt), at(p, gzAt)];
      final r = [at(p, rxAt), at(p, ryAt), at(p, rzAt)];

      for (var k = 0; k < 3; k++) {
        hist[k][0] = hist[k][1];
        hist[k][1] = hist[k][2];
        hist[k][2] = a[k];
      }
      if (seen < 3) {
        seen++;
        continue;
      }

      samples++;
      final us = at(p, arrivalAt).toInt();
      if (firstUs == 0) firstUs = us;
      lastUs = us;

      // **The block column, not the arrival clock.** It is the audio block this
      // sample fell within, which is the one timebase shared with the waveform
      // above — and sharing it is the whole reason the column exists. The
      // arrival clock is kept for measuring channel jitter, not for placing a
      // sample against audio.
      final t = byBlock
          ? at(p, blockAt) * 0.01
          : (us - probe.firstUs) / 1e6;
      final b = (t / motionBucketSeconds).floor();
      if (b < 0 || b >= n) continue;

      var psi = 0.0;
      for (var k = 0; k < 3; k++) {
        psi += (hist[k][1] * hist[k][1] - hist[k][2] * hist[k][0]).abs();
      }

      final aMag = math.sqrt(a[0] * a[0] + a[1] * a[1] + a[2] * a[2]);
      final gMag = math.sqrt(g[0] * g[0] + g[1] * g[1] + g[2] * g[2]);
      // Along world-vertical, signed: the projection of acceleration onto the
      // gravity unit vector. Signed rather than absolute because up and down
      // are different events — a kerb and a dip are not the same thing.
      final along = gMag > 1e-6
          ? (a[0] * g[0] + a[1] * g[1] + a[2] * g[2]) / gMag
          : 0.0;
      // What is left once that is taken out. Unsigned: across is a plane, not
      // an axis, so a sign here would be a direction nobody chose.
      final across = math.sqrt(math.max(0, aMag * aMag - along * along));
      final rMag = math.sqrt(r[0] * r[0] + r[1] * r[1] + r[2] * r[2]);
      // How far the phone is from upright, in degrees. `gz` against the
      // magnitude: flat on its back in a pocket reads near 0 or 180.
      final tilt = gMag > 1e-6
          ? math.acos((g[2] / gMag).clamp(-1.0, 1.0)) * 180 / math.pi
          : 0.0;

      final values = <MotionSeries, double>{
        MotionSeries.psi: psi,
        MotionSeries.accelMagnitude: aMag,
        MotionSeries.alongGravity: along,
        MotionSeries.acrossGravity: across,
        MotionSeries.accelX: a[0],
        MotionSeries.accelY: a[1],
        MotionSeries.accelZ: a[2],
        MotionSeries.rotationMagnitude: rMag,
        MotionSeries.rotationX: r[0],
        MotionSeries.rotationY: r[1],
        MotionSeries.rotationZ: r[2],
        MotionSeries.tiltFromVertical: tilt,
      };

      final first = counts[b] == 0;
      counts[b]++;
      for (final e in values.entries) {
        final s = series[e.key]!;
        final v = e.value;
        if (first) {
          s.lo[b] = v;
          s.hi[b] = v;
          s.open[b] = v;
          s.filled[b] = 1;
        } else {
          if (v < s.lo[b]) s.lo[b] = v;
          if (v > s.hi[b]) s.hi[b] = v;
        }
        s.close[b] = v;
        sums[e.key]![b] += v;
      }
    }
  } catch (_) {
    // A half-read track is still worth drawing — it is the end of a ride that
    // is missing, and that is visible as the picture stopping.
  }

  if (samples == 0) return MotionTrack.none;
  for (final s in MotionSeries.values) {
    final b = series[s]!;
    final sum = sums[s]!;
    for (var i = 0; i < n; i++) {
      if (counts[i] > 0) b.mean[i] = sum[i] / counts[i];
    }
  }

  final spanUs = lastUs - firstUs;
  return MotionTrack(
    series: series,
    bucketSeconds: motionBucketSeconds,
    seconds: span,
    alignedToAudio: byBlock,
    samples: samples,
    hz: spanUs > 0 && samples > 1 ? (samples - 1) * 1e6 / spanUs : 0,
  );
}

/// What the first pass learned about the file's clocks.
class _Timebase {
  const _Timebase(this.blocksAdvance, this.firstUs, this.lastUs);
  final bool blocksAdvance;
  final int firstUs, lastUs;
}

/// Reads only the two time columns, to choose a timebase before using one.
///
/// Cheap: `readAsLines` has already put the file in memory, so this is a second
/// walk over the same list rather than a second read.
_Timebase _timebase(List<String> lines) {
  var blockAt = -1, arrivalAt = -1;
  var maxBlock = 0;
  var firstUs = 0, lastUs = 0;
  for (final line in lines) {
    if (line.isEmpty || line.startsWith('#')) continue;
    final p = line.split(',');
    if (p.isEmpty) continue;
    if (int.tryParse(p.first) == null) {
      blockAt = p.indexOf('block');
      arrivalAt = p.indexOf('arrival_us');
      continue;
    }
    if (blockAt >= 0 && blockAt < p.length) {
      final b = int.tryParse(p[blockAt]) ?? 0;
      if (b > maxBlock) maxBlock = b;
    }
    if (arrivalAt >= 0 && arrivalAt < p.length) {
      final us = int.tryParse(p[arrivalAt]) ?? 0;
      if (firstUs == 0) firstUs = us;
      lastUs = us;
    }
  }
  return _Timebase(maxBlock > 0, firstUs, lastUs);
}
