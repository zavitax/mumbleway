import 'dart:async';
import 'dart:io';
import 'dart:math' as math;

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

/// Why a recording has nothing green in it, when it has nothing green in it.
///
/// **Added because a waveform with no green reads as a broken drawing.** An
/// Android ride came back with `speaking` at 64.9% and `transmitting` at
/// exactly zero: nothing was wrong with the colour, the chain had been told
/// not to send. Only two settings do that, and until they were logged the file
/// could not say which — so the panel could only show grey and leave the
/// reader to guess at a fault that was not there.
enum NothingSent {
  /// Something went out. There is nothing to explain.
  some,

  /// The microphone was muted for most of it.
  muted,

  /// Push to talk, and the button was never pressed.
  pushToTalk,

  /// The log does not say. Either it predates the two columns that would
  /// answer it, or the gate simply never opened — which is itself a finding.
  unexplained,
}

/// The transmit decision for every 10 ms block, and why there were none.
class Decisions {
  const Decisions(this.sent, this.reason);

  /// Empty when there is no log, and every caller must read that as "no
  /// information" rather than as "nothing was sent". Those are different: it
  /// would colour a whole recording as untransmitted, and refuse to play a
  /// note of it in speech-only mode, on the strength of a `.csv` that was
  /// simply never written.
  final Uint8List sent;
  final NothingSent reason;

  static final none = Decisions(Uint8List(0), NothingSent.some);

  bool get anySent => sent.any((f) => f != 0);
}

/// Reads the decision log beside a recording.
///
/// **One reader serves the green on the waveform, the speech-only transport
/// and the note under it**, so what a listener sees, what they hear and what
/// they are told cannot drift apart. Three parsers of one file, agreeing
/// today, is a thing that stops being true quietly.
Future<Decisions> _decisions(String audioPath) async {
  final log = File('${audioPath.substring(0, audioPath.length - 4)}.csv');
  if (!await log.exists()) return Decisions.none;

  final sent = <int>[];
  var muted = 0, pushToTalk = 0;
  // Found by name, not by position. Two columns were added after recordings
  // were already on people's phones, and a reader that counts commas would
  // give every one of those older files a different meaning.
  var sentAt = -1, modeAt = -1, mutedAt = -1;

  try {
    for (final line in await log.readAsLines()) {
      // The first line is a comment and the second is the header. Skipping by
      // content rather than by count: a plain reader that takes the comment as
      // the header is a mistake this project has already made once, offline.
      if (line.isEmpty || line.startsWith('#')) continue;
      final parts = line.split(',');
      if (parts.isEmpty) continue;
      if (int.tryParse(parts.first) == null) {
        sentAt = parts.indexOf('transmitting');
        modeAt = parts.indexOf('mode');
        mutedAt = parts.indexOf('muted');
        continue;
      }
      if (sentAt < 0 || sentAt >= parts.length) continue;
      // The row's position in the file, not what its first column says.
      //
      // **The two disagree, and it shipped.** A recording rotates to a new
      // pair of files every 16 MB, and the writer's block counter ran on
      // across the rotation: the second file's log opened at block 17,477
      // while its own audio opened at sample zero. Every row then pointed past
      // the end of its own recording and was clamped to the last bucket, so
      // the tail of a long ride drew with no green at all — and the tail is
      // what the listen sheet opens first, being the newest name. The waveform
      // said "none of this was sent" about a ride that was, which is the one
      // thing this colour must never do.
      //
      // The writer is fixed as well. Counting rows is what repairs the
      // recordings already sitting on people's phones, and rows are one per
      // block in file order by construction, so it is also less to trust.
      sent.add(parts[sentAt] == '1' ? 1 : 0);
      if (mutedAt >= 0 && mutedAt < parts.length && parts[mutedAt] == '1') {
        muted++;
      }
      // 1 is push to talk, from `TransmitMode`'s declaration order.
      if (modeAt >= 0 && modeAt < parts.length && parts[modeAt] == '1') {
        pushToTalk++;
      }
    }
  } on Exception {
    // A log that cannot be read is no log. The waveform is still worth drawing.
    return Decisions.none;
  }

  if (sent.isEmpty) return Decisions.none;
  final flags = Uint8List.fromList(sent);
  if (flags.any((f) => f != 0)) return Decisions(flags, NothingSent.some);
  // Most of it, not all: a rider who unmutes for the last two seconds and
  // still sends nothing was muted for the recording in every sense that
  // matters to somebody looking at it.
  final half = sent.length ~/ 2;
  if (muted > half) return Decisions(flags, NothingSent.muted);
  if (pushToTalk > half) return Decisions(flags, NothingSent.pushToTalk);
  return Decisions(flags, NothingSent.unexplained);
}

/// Which buckets went on the wire, for drawing.
Future<Uint8List> _transmitted(String audioPath, int buckets, int total) async {
  final flags = (await _decisions(audioPath)).sent;
  if (flags.isEmpty) return Uint8List(0);
  final out = Uint8List(buckets);
  final perBucket = (total / buckets).ceil();
  for (var block = 0; block < flags.length; block++) {
    if (flags[block] == 0) continue;
    final start = block * kRecordingBlock;
    final a = (start ~/ perBucket).clamp(0, buckets - 1);
    final b = ((start + kRecordingBlock - 1) ~/ perBucket)
        .clamp(0, buckets - 1);
    for (var i = a; i <= b; i++) {
      out[i] = 1;
    }
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

/// The per-block decisions, reachable from a test.
@visibleForTesting
Future<Decisions> decisionsForTest(String path) => _decisions(path);

/// A stretch of the file handed to the engine in one push.
///
/// Kept so the playhead can be turned back into a position in the file. In
/// ordinary playback there is only ever one of these and it grows; in
/// speech-only mode there is one per run the gate let through, because the
/// count of samples pushed is no longer the distance travelled.
class _Chunk {
  _Chunk(this.start, this.length);
  final int start;
  int length;
}

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

  /// Half a hop of raised cosine either side of a splice.
  ///
  /// Cutting from one stretch of a recording to another joins two unrelated
  /// waveforms, and the step between them is a click. That click is an
  /// artefact of the cut and not something anyone on the far end would ever
  /// have heard, so removing it is the honest rendering — but it is kept
  /// short deliberately: a longer fade would soften the abrupt onsets that are
  /// the whole reason for listening this way.
  static const _rampSamples = 240; // 5 ms

  RandomAccessFile? _handle;
  Timer? _timer;
  String? _path;

  int _totalSamples = 0;
  int _cursor = 0; // Where the next read starts, in samples.
  int _pushed = 0; // Handed to the engine since the last seek.
  int _atSeek = 0; // Where that seek was, in samples.
  bool _playing = false;
  bool _finished = false;

  /// The transmit decision per block, or empty when the ride has no log.
  Uint8List _blocks = Uint8List(0);
  bool _anySent = false;
  bool _speechOnly = false;
  NothingSent _nothingSent = NothingSent.some;

  /// What has been handed to the engine and not yet heard, in file order.
  final List<_Chunk> _chunks = [];

  String? get path => _path;
  bool get playing => _playing;
  bool get hasFile => _handle != null;
  Duration get duration =>
      Duration(milliseconds: (_totalSamples * 1000 / kRecordingRate).round());

  /// Whether skipping to what was transmitted is a question this recording can
  /// answer: it needs a decision log, and the log needs something in it.
  ///
  /// A ride where nothing was ever transmitted is a real outcome and not a
  /// fault, but "play only the transmitted parts" of it is silence, so the
  /// control says so by being unavailable rather than by playing nothing.
  bool get canSkipSilence => _anySent;

  /// Why nothing went out, when nothing did. [NothingSent.some] otherwise.
  NothingSent get nothingSent => _nothingSent;

  /// Whether playback is skipping everything the gate rejected.
  bool get speechOnly => _speechOnly;

  /// Where the playhead is, in samples, as heard rather than as sent.
  ///
  /// What was pushed minus what is still queued is what has reached the
  /// speaker. That count is the distance travelled through the *file* only
  /// while playback is contiguous; in speech-only mode the transport skips, so
  /// it is walked back through the stretches actually pushed to arrive at a
  /// position the waveform and the clock can both use.
  int get _positionSamples {
    var queued = 0;
    try {
      queued = previewQueued();
    } catch (_) {
      // No engine: nothing is playing, so nothing is outstanding.
    }
    if (_chunks.isEmpty) return _atSeek.clamp(0, _totalSamples);
    var heard = _pushed - queued;
    var at = _chunks.first.start;
    for (final c in _chunks) {
      if (heard <= 0) {
        at = c.start;
        break;
      }
      if (heard < c.length) {
        at = c.start + heard;
        break;
      }
      heard -= c.length;
      at = c.start + c.length;
    }
    return at.clamp(0, _totalSamples);
  }

  Duration get position => Duration(
    milliseconds: (_positionSamples * 1000 / kRecordingRate).round(),
  );

  double get progress =>
      _totalSamples == 0 ? 0 : _positionSamples / _totalSamples;

  /// Opens a recording without starting it.
  ///
  /// The decision log is read here, on an isolate, rather than when the
  /// speech-only control is first pressed: a long ride's log is half a
  /// megabyte, the sheet already shows a spinner across this, and a transport
  /// control that has to think before it does anything reads as a broken one.
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
    final decisions = await compute(_decisions, path);
    _blocks = decisions.sent;
    _nothingSent = decisions.reason;
    _anySent = decisions.anySent;
    if (!_anySent) _speechOnly = false;
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
    _restartFrom(_positionSamples);
    notifyListeners();
  }

  /// Moves the playhead. A seek is a discard and a refill — whatever was
  /// queued belongs to a moment the listener has just decided not to hear.
  void seekTo(int samples) {
    if (_handle == null) return;
    _restartFrom(samples.clamp(0, _totalSamples));
    _finished = false;
    if (_playing) _feed();
    notifyListeners();
  }

  /// Plays only what the gate let through, or all of it.
  ///
  /// **This is what the panel exists for.** Judging a gate by ear otherwise
  /// means two clients, two devices and a rider trying to make sense of their
  /// own voice coming back at them; here the same question is one button, on
  /// audio that has the decision beside it.
  ///
  /// It re-seeks, for the reason [pause] does: what is queued was read under
  /// the mode that was in force at the time, and carrying it over would play
  /// out a stretch the listener has just asked not to hear.
  void setSpeechOnly(bool value) {
    if (_speechOnly == value || (value && !_anySent)) return;
    _speechOnly = value;
    if (_handle != null) {
      _restartFrom(_positionSamples);
      if (_playing) _feed();
    }
    notifyListeners();
  }

  /// Throws away what was queued and reads again from [samples].
  void _restartFrom(int samples) {
    _clearEngine();
    _chunks.clear();
    _cursor = samples;
    _atSeek = samples;
    _pushed = 0;
  }

  void seekToFraction(double f) => seekTo((f * _totalSamples).round());

  Future<void> stop() async {
    _playing = false;
    _timer?.cancel();
    _timer = null;
    _restartFrom(0);
    await _handle?.close();
    _handle = null;
    _path = null;
    _totalSamples = 0;
    _blocks = Uint8List(0);
    _anySent = false;
    _nothingSent = NothingSent.some;
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

    _forget(_pushed - queued);

    // The engine's queue caps at 500 ms and the target is 350, so a push of
    // `want` is always taken whole. That is what lets the fades below be
    // applied before the push rather than reconciled against what it accepted.
    var want = (_target.inMilliseconds * kRecordingRate ~/ 1000) - queued;

    // A loop, because one pass covers one unbroken run of transmitted audio
    // and a run can be a single block. Ordinary playback goes round once.
    while (want > 0 && _cursor < _totalSamples) {
      final from = _nextAudible(_cursor);
      if (from >= _totalSamples) {
        _cursor = _totalSamples;
        break;
      }
      final spliced = from != _cursor;
      _cursor = from;
      final runEnd = _audibleEnd(_cursor);
      final take = want < runEnd - _cursor ? want : runEnd - _cursor;
      if (take <= 0) break;

      handle.setPositionSync(_cursor * _bytesPerSample);
      final raw = handle.readSync(take * _bytesPerSample);
      final pcm = raw.buffer.asInt16List(
        raw.offsetInBytes,
        raw.lengthInBytes ~/ _bytesPerSample,
      );
      if (pcm.isEmpty) break;
      final samples = Float32List(pcm.length);
      for (var i = 0; i < pcm.length; i++) {
        samples[i] = pcm[i] / 32768.0;
      }

      // Only at a join, and only where there is really a join: the last run of
      // a recording ends at its end, which is not a splice and needs no fade.
      if (spliced) _rampIn(samples);
      if (_cursor + pcm.length >= runEnd && runEnd < _totalSamples) {
        _rampOut(samples);
      }

      final int accepted;
      try {
        accepted = previewPush(samples: samples);
      } catch (_) {
        pause();
        return;
      }
      if (accepted <= 0) break;
      _note(_cursor, accepted);
      _cursor += accepted;
      _pushed += accepted;
      want -= accepted;
      if (accepted < pcm.length) break; // engine full; the rest waits
    }

    if (_cursor >= _totalSamples && queued == 0) {
      _playing = false;
      _finished = true;
      _timer?.cancel();
      _timer = null;
    }
    notifyListeners();
  }

  /// The next sample worth playing at or after [from].
  ///
  /// Everything, unless speech-only is on. Past the end of the decision log it
  /// is everything too: a log shorter than its audio is missing information,
  /// and skipping the tail of a ride on the strength of rows that were never
  /// written would be silent and wrong.
  int _nextAudible(int from) {
    if (!_speechOnly || _blocks.isEmpty) return from;
    var block = from ~/ kRecordingBlock;
    while (block < _blocks.length && _blocks[block] == 0) {
      block++;
    }
    final start = block * kRecordingBlock;
    return start > from ? start : from;
  }

  /// Where the run containing [from] stops being worth playing.
  int _audibleEnd(int from) {
    if (!_speechOnly || _blocks.isEmpty) return _totalSamples;
    var block = from ~/ kRecordingBlock;
    if (block >= _blocks.length) return _totalSamples;
    while (block < _blocks.length && _blocks[block] != 0) {
      block++;
    }
    // Off the end of the log is unknown, not silent — play to the end.
    if (block >= _blocks.length) return _totalSamples;
    final end = block * kRecordingBlock;
    return end < _totalSamples ? end : _totalSamples;
  }

  /// Records a stretch handed to the engine, merging when it continues the
  /// last one — which is every push in ordinary playback.
  void _note(int start, int length) {
    if (_chunks.isNotEmpty) {
      final last = _chunks.last;
      if (last.start + last.length == start) {
        last.length += length;
        return;
      }
    }
    _chunks.add(_Chunk(start, length));
  }

  /// Drops stretches already heard, so a long ride does not accumulate one
  /// entry per gap for the whole of it. The last is always kept: it is what
  /// the position is measured from once everything pushed has been played.
  void _forget(int heard) {
    while (_chunks.length > 1 && heard >= _chunks.first.length) {
      heard -= _chunks.first.length;
      _pushed -= _chunks.first.length;
      _chunks.removeAt(0);
    }
  }

  static void _rampIn(Float32List s) {
    final n = _rampSamples < s.length ? _rampSamples : s.length;
    for (var i = 0; i < n; i++) {
      s[i] *= 0.5 - 0.5 * math.cos(math.pi * i / n);
    }
  }

  static void _rampOut(Float32List s) {
    final n = _rampSamples < s.length ? _rampSamples : s.length;
    final base = s.length - n;
    for (var i = 0; i < n; i++) {
      s[base + i] *= 0.5 + 0.5 * math.cos(math.pi * i / n);
    }
  }

  /// The skipping arithmetic, without a file or an engine.
  ///
  /// Worth reaching in for: it is the whole of the feature, it is pure, and
  /// the alternative is asserting it by ear on a device.
  @visibleForTesting
  void loadForTest(Uint8List blocks, int totalSamples) {
    _blocks = blocks;
    _anySent = blocks.any((b) => b != 0);
    _totalSamples = totalSamples;
  }

  @visibleForTesting
  int nextAudibleForTest(int from) => _nextAudible(from);

  @visibleForTesting
  int audibleEndForTest(int from) => _audibleEnd(from);

  @override
  void dispose() {
    _timer?.cancel();
    _clearEngine();
    _handle?.close();
    super.dispose();
  }
}
