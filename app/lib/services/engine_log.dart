import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';

import '../src/rust/api/mumbleway.dart';

/// Severity, in the order the engine numbers them.
enum LogLevel {
  trace('TRACE'),
  debug('DEBUG'),
  info('INFO'),
  warn('WARN'),
  error('ERROR');

  const LogLevel(this.label);

  final String label;

  /// The engine sends a number rather than a name, so the order here is part of
  /// the wire format and not a detail of presentation.
  static LogLevel of(int i) =>
      i >= 0 && i < LogLevel.values.length ? LogLevel.values[i] : LogLevel.info;
}

/// One line the engine wrote about itself.
@immutable
class LogLine {
  const LogLine({
    required this.seq,
    required this.at,
    required this.level,
    required this.target,
    required this.message,
  });

  factory LogLine.of(UiLogEntry e) => LogLine(
    seq: e.seq.toInt(),
    at: DateTime.fromMillisecondsSinceEpoch(e.atMs.toInt()),
    level: LogLevel.of(e.level),
    target: e.target,
    message: e.message,
  );

  final int seq;
  final DateTime at;
  final LogLevel level;
  final String target;
  final String message;

  String get clock {
    String two(int n) => n.toString().padLeft(2, '0');
    return '${two(at.hour)}:${two(at.minute)}:${two(at.second)}'
        '.${at.millisecond.toString().padLeft(3, '0')}';
  }

  @override
  String toString() => '$clock ${level.label.padRight(5)} [$target] $message';
}

/// What the engine has said about itself, for the diagnostics panel and for the
/// platform log behind it.
///
/// Two audiences, one source. On a device we are holding, the platform log —
/// Console on Apple, logcat on Android — is where this belongs, because it can
/// be read while the app misbehaves rather than afterwards. On a device we are
/// not holding, which is every device a rider actually uses, the only reachable
/// copy is the one drawn inside the app and read back to us over a message.
class EngineLog extends ChangeNotifier {
  EngineLog._();

  static final EngineLog instance = EngineLog._();

  /// Matches the ring in the engine: keeping more here would only ever hold
  /// lines whose neighbours have already gone.
  static const capacity = 1000;

  static const _channel = MethodChannel('mumbleway/log');

  final List<LogLine> _lines = [];
  final Set<int> _seen = {};

  /// True once a platform handler has refused to answer, so the fallback is not
  /// re-attempted for every line on desktop where there is no handler at all.
  bool _platformSinkMissing = false;

  List<LogLine> get lines => List.unmodifiable(_lines);
  bool get isEmpty => _lines.isEmpty;

  /// Adds lines that arrived from the engine.
  ///
  /// Deduplicated by sequence number and re-sorted, because the backfill
  /// overlaps the stream: the fetch returns everything recorded so far, some of
  /// which has already been delivered, and some of which is older than anything
  /// delivered yet.
  void add(List<UiLogEntry> entries) {
    final fresh = [
      for (final e in entries)
        if (_seen.add(e.seq.toInt())) LogLine.of(e),
    ];
    if (fresh.isEmpty) return;

    _lines.addAll(fresh);
    _lines.sort((a, b) => a.seq.compareTo(b.seq));
    if (_lines.length > capacity) {
      final excess = _lines.length - capacity;
      for (final line in _lines.take(excess)) {
        _seen.remove(line.seq);
      }
      _lines.removeRange(0, excess);
    }

    _mirror(fresh);
    notifyListeners();
  }

  /// Fetches what the engine recorded before the UI was listening.
  ///
  /// The lines that explain a failed start are all written before anything is
  /// attached to the stream, which makes them the ones most worth having.
  void backfill() {
    try {
      add(recentLogs());
    } catch (_) {
      // The engine is not up. When that is itself the problem there is nothing
      // to fetch, and failing here would replace a diagnosis with an error.
    }
  }

  Future<void> clear() async {
    try {
      clearLogs();
    } catch (_) {
      // Clearing the view is still worth doing without an engine behind it.
    }
    _lines.clear();
    _seen.clear();
    notifyListeners();
  }

  /// The whole log as text, for sharing.
  String asText() => _lines.map((l) => l.toString()).join('\n');

  /// Repeats the lines into the platform's own log.
  ///
  /// Batched into one call: a burst during a failed connect is dozens of lines,
  /// and a platform channel hop each would be paid on the UI thread.
  void _mirror(List<LogLine> fresh) {
    if (_platformSinkMissing) {
      // Desktop, where stderr is visible anyway and there is nothing to gain
      // from a channel that has no handler.
      for (final line in fresh) {
        debugPrint(line.toString());
      }
      return;
    }
    _channel
        .invokeMethod<void>('write', {
          'lines': [
            for (final line in fresh)
              {
                'level': line.level.index,
                'target': line.target,
                'message': '${line.clock} [${line.target}] ${line.message}',
              },
          ],
        })
        .catchError((Object _) {
          _platformSinkMissing = true;
          for (final line in fresh) {
            debugPrint(line.toString());
          }
        });
  }
}
