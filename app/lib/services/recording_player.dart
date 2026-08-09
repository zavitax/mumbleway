import 'dart:async';
import 'dart:io';

import 'package:flutter/foundation.dart';

import '../src/rust/api/mumbleway.dart';

/// What the recorder writes, and therefore what this reads.
///
/// Headerless by design — see `core/src/audio/record.rs`. Nothing in the file
/// says any of this, so it is stated in one place rather than assumed in
/// several.
const int kRecordingRate = 48000;

/// Samples per decision-log row. One 10 ms block, the same one the chain works
/// in — the `.csv` has a row per block and the audio has no marker in it, so
/// this is the only thing tying the two together.
const int kRecordingBlock = 480;
const int _bytesPerSample = 2;

/// Peaks for drawing a recording, and the length it was measured over.
class Waveform {
  const Waveform(this.minima, this.maxima, this.duration, this.transmitted);

  /// One entry per bucket, in the range -1..1.
  final Float32List minima;
  final Float32List maxima;
  final Duration duration;

  /// Whether any of the bucket went on the wire, from the `.csv` beside the
  /// audio. Empty when there is no log to read.
  ///
  /// *Any*, not *most*: the question a listener has is "was I heard here", and
  /// a bucket covering a second of audio with one transmitted block in it is a
  /// second in which they were heard.
  final Uint8List transmitted;

  int get buckets => maxima.length;
  bool get isEmpty => maxima.isEmpty;
  bool wasSent(int bucket) =>
      bucket < transmitted.length && transmitted[bucket] != 0;
}

/// Reads a `.s16` and reduces it to per-bucket extremes.
///
/// On a background isolate: a ride is tens of megabytes, and the panel this
/// feeds has a spectrum analyser in it that must not stop while a file is
/// being scanned.
///
/// Extremes rather than an average of magnitudes. A gate that shut mid-word
/// shows up as a cliff, and averaging is exactly the operation that would
/// round a cliff off into a slope.
Future<Waveform> _scan(List<Object> args) async {
  final path = args[0] as String;
  final buckets = args[1] as int;

  final file = File(path);
  final length = await file.length();
  final total = length ~/ _bytesPerSample;
  if (total == 0) {
    return Waveform(
        Float32List(0), Float32List(0), Duration.zero, Uint8List(0));
  }

  final minima = Float32List(buckets);
  final maxima = Float32List(buckets);
  final perBucket = (total / buckets).ceil();

  final handle = await file.open();
  try {
    // A megabyte at a time. Whole-file reads are what turn a long ride into an
    // out-of-memory crash on the phone that recorded it.
    const chunkSamples = 512 * 1024;
    var index = 0;
    while (index < total) {
      final want = (total - index).clamp(0, chunkSamples);
      final bytes = await handle.read(want * _bytesPerSample);
      if (bytes.isEmpty) break;
      final pcm = bytes.buffer.asInt16List(
        bytes.offsetInBytes,
        bytes.lengthInBytes ~/ _bytesPerSample,
      );
      for (var i = 0; i < pcm.length; i++) {
        final bucket = ((index + i) ~/ perBucket).clamp(0, buckets - 1);
        final v = pcm[i] / 32768.0;
        if (v < minima[bucket]) minima[bucket] = v;
        if (v > maxima[bucket]) maxima[bucket] = v;
      }
      index += pcm.length;
    }
  } finally {
    await handle.close();
  }

  return Waveform(
    minima,
    maxima,
    Duration(milliseconds: (total * 1000 / kRecordingRate).round()),
    await _transmitted(path, buckets, total),
  );
}

/// Which buckets went on the wire, from the decision log beside the audio.
///
/// Returns an empty list when there is no log, which the painter reads as "no
/// information" rather than as "nothing was sent". Those are different, and
/// colouring a whole recording as untransmitted because its `.csv` was never
/// sent would be a confident lie.
Future<Uint8List> _transmitted(String audioPath, int buckets, int total) async {
  final log = File('${audioPath.substring(0, audioPath.length - 4)}.csv');
  if (!await log.exists()) return Uint8List(0);
  final out = Uint8List(buckets);
  final perBucket = (total / buckets).ceil();
  try {
    var block = 0;
    for (final line in await log.readAsLines()) {
      // The first line is a comment and the second is the header. Skipping by
      // content rather than by count: a plain reader that takes the comment as
      // the header is a mistake this project has already made once, offline.
      if (line.isEmpty || line.startsWith('#')) continue;
      final comma = line.indexOf(',');
      if (comma <= 0) continue;
      final first = int.tryParse(line.substring(0, comma));
      if (first == null) continue; // the header row
      final rest = line.substring(comma + 1);
      final next = rest.indexOf(',');
      if (next <= 0) continue;
      if (rest.substring(0, next) == '1') {
        // Each row is one block of `kRecordingBlock` samples.
        final start = first * kRecordingBlock;
        final a = (start ~/ perBucket).clamp(0, buckets - 1);
        final b = ((start + kRecordingBlock - 1) ~/ perBucket)
            .clamp(0, buckets - 1);
        for (var i = a; i <= b; i++) {
          out[i] = 1;
        }
      }
      block++;
    }
    if (block == 0) return Uint8List(0);
  } on Exception {
    // A log that cannot be read is no log. The waveform is still worth drawing.
    return Uint8List(0);
  }
  return out;
}

/// The scan, reachable from a test.
///
/// Exposed because the interesting half of it is reading the decision log, and
/// that needs no engine, no audio device and no isolate to exercise — only two
/// files on disk.
@visibleForTesting
Future<Waveform> scanForTest(String path, int buckets) =>
    _scan(<Object>[path, buckets]);

/// Plays a diagnostic recording back through the engine's own output.
///
/// **The engine holds no file and no cursor.** This class reads the `.s16`,
/// converts it, and pushes decoded samples across the bridge; the Rust side is
/// a queue that the output drains. That split is deliberate: a file read on
/// the audio thread is a missed deadline, and a missed deadline is a click in
/// somebody's helmet.
///
/// The playhead is derived rather than counted. What was pushed minus what is
/// still queued is what has actually reached the speaker; a timer running
/// forward from "play" drifts ahead the moment the device buffers, and the
/// drift is worst on exactly the Bluetooth routes this app is used on.
class RecordingPlayer extends ChangeNotifier {
  RecordingPlayer();

  /// Kept ahead of the speaker. Enough that a late timer tick is inaudible,
  /// short enough that a seek does not play a third of a second of the place
  /// you just left.
  static const _target = Duration(milliseconds: 350);
  static const _tick = Duration(milliseconds: 80);

  RandomAccessFile? _handle;
  Timer? _timer;
  String? _path;

  int _totalSamples = 0;
  int _cursor = 0; // Where the next read starts, in samples.
  int _pushed = 0; // Handed to the engine since the last seek.
  int _atSeek = 0; // Where that seek was, in samples.
  bool _playing = false;
  bool _finished = false;

  String? get path => _path;
  bool get playing => _playing;
  bool get hasFile => _handle != null;
  Duration get duration =>
      Duration(milliseconds: (_totalSamples * 1000 / kRecordingRate).round());

  /// Where the playhead is, in samples, as heard rather than as sent.
  int get _positionSamples {
    var queued = 0;
    try {
      queued = previewQueued();
    } catch (_) {
      // No engine: nothing is playing, so nothing is outstanding.
    }
    return (_atSeek + _pushed - queued).clamp(0, _totalSamples);
  }

  Duration get position => Duration(
    milliseconds: (_positionSamples * 1000 / kRecordingRate).round(),
  );

  double get progress =>
      _totalSamples == 0 ? 0 : _positionSamples / _totalSamples;

  /// Opens a recording without starting it.
  Future<void> open(String path) async {
    await stop();
    final file = File(path);
    if (!file.existsSync()) return;
    _handle = await file.open();
    _path = path;
    _totalSamples = await file.length() ~/ _bytesPerSample;
    _cursor = 0;
    _atSeek = 0;
    _pushed = 0;
    _finished = false;
    notifyListeners();
  }

  Future<Waveform> waveform({int buckets = 320}) {
    final path = _path;
    if (path == null) {
      return Future.value(
        Waveform(Float32List(0), Float32List(0), Duration.zero, Uint8List(0)),
      );
    }
    return compute(_scan, <Object>[path, buckets]);
  }

  void play() {
    if (_handle == null || _playing) return;
    if (_finished) seekTo(0);
    _playing = true;
    _timer = Timer.periodic(_tick, (_) => _feed());
    _feed();
    notifyListeners();
  }

  void pause() {
    if (!_playing) return;
    _playing = false;
    _timer?.cancel();
    _timer = null;
    // Where the ear got to, not where the reader got to. Anything still queued
    // was never heard and must be read again on resume.
    final heard = _positionSamples;
    _clearEngine();
    _cursor = heard;
    _atSeek = heard;
    _pushed = 0;
    notifyListeners();
  }

  /// Moves the playhead. A seek is a discard and a refill — whatever was
  /// queued belongs to a moment the listener has just decided not to hear.
  void seekTo(int samples) {
    if (_handle == null) return;
    final target = samples.clamp(0, _totalSamples);
    _clearEngine();
    _cursor = target;
    _atSeek = target;
    _pushed = 0;
    _finished = false;
    if (_playing) _feed();
    notifyListeners();
  }

  void seekToFraction(double f) => seekTo((f * _totalSamples).round());

  Future<void> stop() async {
    _playing = false;
    _timer?.cancel();
    _timer = null;
    _clearEngine();
    await _handle?.close();
    _handle = null;
    _path = null;
    _totalSamples = 0;
    _cursor = 0;
    _atSeek = 0;
    _pushed = 0;
    _finished = false;
    notifyListeners();
  }

  void _clearEngine() {
    try {
      previewClear();
    } catch (_) {
      // No engine to clear. Nothing was playing either.
    }
  }

  void _feed() {
    final handle = _handle;
    if (handle == null || !_playing) return;

    int queued;
    try {
      queued = previewQueued();
    } catch (_) {
      // The engine went away underneath us — devices closed, or it never
      // started. Stop rather than spin a timer against nothing.
      pause();
      return;
    }

    final want = (_target.inMilliseconds * kRecordingRate ~/ 1000) - queued;
    if (want > 0 && _cursor < _totalSamples) {
      final take = want.clamp(0, _totalSamples - _cursor);
      final bytes = handle..setPositionSync(_cursor * _bytesPerSample);
      final raw = bytes.readSync(take * _bytesPerSample);
      final pcm = raw.buffer.asInt16List(
        raw.offsetInBytes,
        raw.lengthInBytes ~/ _bytesPerSample,
      );
      final samples = Float32List(pcm.length);
      for (var i = 0; i < pcm.length; i++) {
        samples[i] = pcm[i] / 32768.0;
      }
      try {
        final accepted = previewPush(samples: samples);
        _cursor += accepted;
        _pushed += accepted;
      } catch (_) {
        pause();
        return;
      }
    }

    if (_cursor >= _totalSamples && queued == 0) {
      _playing = false;
      _finished = true;
      _timer?.cancel();
      _timer = null;
    }
    notifyListeners();
  }

  @override
  void dispose() {
    _timer?.cancel();
    _clearEngine();
    _handle?.close();
    super.dispose();
  }
}
