import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../src/rust/api/mumbleway.dart';
import '../l10n/app_localizations.dart';
import '../services/engine_log.dart';
import '../state/app_state.dart';
import '../theme.dart';

/// Live diagnostics, shown over the bottom of whatever is on screen.
///
/// These numbers only earn their place when something is wrong, and by then
/// the difference between "the network is dropping audio" and "this device
/// cannot keep up" is the whole question — they sound identical. Kept out of
/// settings because settings are for decisions, and none of this is one.
class DiagnosticsPanel extends StatefulWidget {
  const DiagnosticsPanel({super.key, required this.onClose});

  final VoidCallback onClose;

  @override
  State<DiagnosticsPanel> createState() => _DiagnosticsPanelState();
}

class _DiagnosticsPanelState extends State<DiagnosticsPanel> {
  Timer? _tick;
  _Snapshot? _audio;
  _Snapshot? _previous;

  // One sample a second, so a step is a second and the window is its width in
  // seconds without any arithmetic on the reader's part.
  final _bytesIn = _History('kB/s', decimals: 1);
  final _bytesOut = _History('kB/s', decimals: 1);
  final _packetsIn = _History('/s');
  final _packetsOut = _History('/s');
  final _cpu = _History('%');
  final _memory = _History('MB');

  @override
  void initState() {
    super.initState();
    _refresh();
    _tick = Timer.periodic(const Duration(seconds: 1), (_) => _refresh());
  }

  void _refresh() {
    if (!mounted) return;
    try {
      final now = _Snapshot.of(audioDiagnostics());
      final was = _previous;
      setState(() {
        // Rates from deltas: the counters are cumulative precisely so the
        // interval is the caller's business, and this one is a second.
        if (was != null) {
          _bytesIn.add((now.bytesIn - was.bytesIn) / 1024);
          _bytesOut.add((now.bytesOut - was.bytesOut) / 1024);
          _packetsIn.add((now.voiceIn - was.voiceIn).toDouble());
          _packetsOut.add((now.voiceOut - was.voiceOut).toDouble());
        }
        _cpu.add(now.cpuPercent);
        _memory.add(now.memoryMb);
        _previous = now;
        _audio = now;
      });
    } catch (_) {
      // The engine is not up; nothing to report.
    }
  }

  @override
  void dispose() {
    _tick?.cancel();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final state = AppStateScope.of(context);
    final l = L.of(context);
    final scheme = Theme.of(context).colorScheme;
    final audio = _audio;

    return GestureDetector(
      // Swiping down is the gesture anything covering the bottom of a screen
      // is expected to answer, and it beats hunting for the button that opened
      // it. Velocity rather than distance, so a flick works as well as a drag.
      onVerticalDragEnd: (details) {
        if ((details.primaryVelocity ?? 0) > 120) widget.onClose();
      },
      child: Material(
        elevation: 12,
        color: scheme.surfaceContainerHighest,
        borderRadius: const BorderRadius.vertical(top: Radius.circular(18)),
        child: SafeArea(
          top: false,
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              // Grab handle: says "this can be dragged" without a label.
              Container(
                width: 38,
                height: 4,
                margin: const EdgeInsets.symmetric(vertical: 10),
                decoration: BoxDecoration(
                  color: scheme.onSurfaceVariant.withValues(alpha: 0.4),
                  borderRadius: BorderRadius.circular(2),
                ),
              ),
              Padding(
                padding: const EdgeInsets.fromLTRB(18, 0, 8, 4),
                child: Row(
                  children: [
                    const Icon(Icons.monitor_heart_outlined, size: 18),
                    const SizedBox(width: 8),
                    Expanded(
                      child: Text(
                        l.diagnostics,
                        style: const TextStyle(
                          fontWeight: FontWeight.w700,
                          fontSize: 15,
                        ),
                      ),
                    ),
                    TextButton(
                      onPressed: () {
                        resetAudioGlitches();
                        _refresh();
                      },
                      child: Text(l.diagReset),
                    ),
                    IconButton(
                      tooltip: l.diagClose,
                      icon: const Icon(Icons.close, size: 20),
                      onPressed: widget.onClose,
                    ),
                  ],
                ),
              ),
              Flexible(
                child: ListView(
                  shrinkWrap: true,
                  padding: const EdgeInsets.fromLTRB(18, 0, 18, 12),
                  children: [
                    // Counter groups follow the graphs' layout rule, so the
                    // panel reflows as one thing rather than half of it going
                    // wide while the other half stays in a column.
                    _ResponsiveGrid(
                      minWidth: 260,
                      children: [
                        if (audio != null) ...[
                          _Group(
                            title: l.diagIncomingAudio,
                            rows: [
                              // Real against invented is the one comparison
                              // that separates a bad link from a bug in the
                              // buffer: a little invented audio is normal, a
                              // lot of it while real audio stalls is not.
                              _Row(l.diagDecoded, '${audio.incomingRealMs} ms'),
                              _Row(
                                l.diagInvented,
                                '${audio.incomingInventedMs} ms',
                                bad:
                                    audio.incomingInventedMs >
                                    audio.incomingRealMs ~/ 4,
                              ),
                              _Row(l.diagGapsConcealed, '${audio.lostPackets}'),
                              _Row(
                                l.diagJitterBuffer,
                                '${audio.jitterBufferMs} ms',
                              ),
                              _Row(l.diagSpeakersTracked, '${audio.speakers}'),
                            ],
                          ),
                          _Group(
                            title: l.diagThisDevice,
                            rows: [
                              _Row(
                                l.diagPlaybackGaps,
                                '${audio.playbackGapMs} ms',
                                bad: audio.playbackGapMs > 0,
                              ),
                              _Row(
                                l.diagMicrophoneDropped,
                                '${audio.captureDroppedMs} ms',
                                bad: audio.captureDroppedMs > 0,
                              ),
                              _Row(
                                l.diagMicrophoneLevel,
                                '${state.inputLevelDb.toStringAsFixed(0)} dBFS',
                              ),
                              _Row(
                                l.diagNoiseFloor,
                                '${state.noiseFloorDb.toStringAsFixed(0)} dBFS',
                              ),
                              _Row(
                                l.diagOpensAt,
                                '${state.activationThresholdDb.toStringAsFixed(0)} dBFS',
                              ),
                            ],
                          ),
                        ],
                        for (final server in state.servers)
                          if (state.runtimeFor(server.id).isLive)
                            _Group(
                              title: server.name.isEmpty
                                  ? server.host
                                  : server.name,
                              rows: _serverRows(l, state.runtimeFor(server.id)),
                            ),
                      ],
                    ),
                    const SizedBox(height: 16),
                    Text(
                      'LAST 30 SECONDS',
                      style: TextStyle(
                        fontSize: 10,
                        fontWeight: FontWeight.w700,
                        letterSpacing: 0.8,
                        color: scheme.onSurfaceVariant,
                      ),
                    ),
                    const SizedBox(height: 8),
                    // Last, because a graph is what you study once a number
                    // above has told you where to look.
                    _ResponsiveGrid(
                      minWidth: 250,
                      children: [
                        _Graph(
                          title: l.diagNetwork,
                          series: [
                            _Series('in', _bytesIn, StatusColors.connected),
                            _Series('out', _bytesOut, StatusColors.connecting),
                          ],
                        ),
                        _Graph(
                          title: l.diagVoicePackets,
                          series: [
                            _Series('in', _packetsIn, StatusColors.connected),
                            _Series(
                              'out',
                              _packetsOut,
                              StatusColors.connecting,
                            ),
                          ],
                        ),
                        _Graph(
                          title: 'CPU',
                          series: [_Series('cpu', _cpu, scheme.primary)],
                        ),
                        _Graph(
                          title: l.diagMemory,
                          series: [_Series('rss', _memory, scheme.primary)],
                        ),
                      ],
                    ),
                    const SizedBox(height: 16),
                    // Last of all: the graphs say when something went wrong,
                    // and this says what the engine thought it was doing at
                    // the time.
                    const _LogView(),
                    const SizedBox(height: 8),
                  ],
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }

  static List<_Row> _serverRows(L l, ServerRuntime rt) => [
    // Which path voice is taking is the first thing to know about a call that
    // sounds wrong: a tunnelled connection behaves quite differently from a
    // direct one, and the fallback is silent.
    _Row(
      l.diagVoicePath,
      rt.transport == 'udp' ? l.diagUdpDirect : l.diagTcpTunnelled,
      bad: rt.transport != 'udp',
    ),
    _Row(
      l.diagPing,
      '${(rt.transport == 'udp' ? rt.udpPingMs : rt.tcpPingMs).round()} ms',
    ),
    _Row(l.diagInChannel, rt.currentChannel?.name ?? '—'),
    _Row(l.diagParticipants, '${rt.channelPeers.length}'),
    if (rt.attempt > 0)
      _Row(l.diagReconnectAttempts, '${rt.attempt}', bad: true),
  ];
}

/// What the engine has said about itself, newest last.
///
/// The numbers above say that something went wrong and roughly when; this says
/// what the engine was doing at the time, which is the part no counter can
/// carry. Deliberately verbatim rather than summarised — the whole value of it
/// is that a rider can read it back to us unedited.
class _LogView extends StatefulWidget {
  const _LogView();

  @override
  State<_LogView> createState() => _LogViewState();
}

class _LogViewState extends State<_LogView> {
  final _log = EngineLog.instance;
  final _scroll = ScrollController();

  /// Hides the chatter. Warnings and errors are what a rider is asked to look
  /// for, and on a phone screen twenty routine lines will bury one of them.
  bool _problemsOnly = false;

  @override
  void initState() {
    super.initState();
    _log.addListener(_onLines);
  }

  void _onLines() {
    if (!mounted) return;
    setState(() {});
    // Follow the tail, but only from the tail: scrolling back to read
    // something and being yanked to the bottom by the next line makes the log
    // unusable precisely when it is being used.
    if (!_scroll.hasClients) return;
    if (_scroll.position.pixels >= _scroll.position.maxScrollExtent - 24) {
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (_scroll.hasClients) {
          _scroll.jumpTo(_scroll.position.maxScrollExtent);
        }
      });
    }
  }

  @override
  void dispose() {
    _log.removeListener(_onLines);
    _scroll.dispose();
    super.dispose();
  }

  Color _colour(LogLevel level, ColorScheme scheme) => switch (level) {
    LogLevel.error => StatusColors.failed,
    LogLevel.warn => StatusColors.connecting,
    LogLevel.trace || LogLevel.debug => scheme.onSurfaceVariant,
    LogLevel.info => scheme.onSurface,
  };

  @override
  Widget build(BuildContext context) {
    final l = L.of(context);
    final scheme = Theme.of(context).colorScheme;
    final quiet = scheme.onSurfaceVariant;
    final lines = _problemsOnly
        ? _log.lines.where((e) => e.level.index >= LogLevel.warn.index).toList()
        : _log.lines;

    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Row(
          children: [
            Expanded(
              child: Text(
                l.diagLog.toUpperCase(),
                style: TextStyle(
                  fontSize: 10,
                  fontWeight: FontWeight.w700,
                  letterSpacing: 0.8,
                  color: quiet,
                ),
              ),
            ),
            TextButton(
              onPressed: () => setState(() => _problemsOnly = !_problemsOnly),
              child: Text(_problemsOnly ? l.diagLogAll : l.diagLogProblems),
            ),
            IconButton(
              tooltip: l.diagLogCopy,
              icon: const Icon(Icons.copy_all_outlined, size: 18),
              onPressed: lines.isEmpty
                  ? null
                  : () {
                      Clipboard.setData(ClipboardData(text: _log.asText()));
                      ScaffoldMessenger.of(
                        context,
                      ).showSnackBar(SnackBar(content: Text(l.diagLogCopied)));
                    },
            ),
            IconButton(
              tooltip: l.diagLogClear,
              icon: const Icon(Icons.delete_sweep_outlined, size: 18),
              onPressed: _log.isEmpty ? null : () => _log.clear(),
            ),
          ],
        ),
        const SizedBox(height: 4),
        Container(
          height: 190,
          width: double.infinity,
          padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 8),
          decoration: BoxDecoration(
            color: scheme.surface,
            borderRadius: BorderRadius.circular(10),
            border: Border.all(color: quiet.withValues(alpha: 0.18)),
          ),
          child: lines.isEmpty
              ? Center(
                  child: Text(
                    _problemsOnly ? l.diagLogNoProblems : l.diagLogEmpty,
                    style: TextStyle(fontSize: 12, color: quiet),
                  ),
                )
              // Horizontally scrollable rather than wrapped: these lines are
              // read by scanning down the timestamps, and a wrapped line breaks
              // that column apart.
              : Scrollbar(
                  controller: _scroll,
                  child: ListView.builder(
                    controller: _scroll,
                    itemCount: lines.length,
                    itemBuilder: (context, i) {
                      final line = lines[i];
                      return Padding(
                        padding: const EdgeInsets.symmetric(vertical: 1),
                        child: SelectableText.rich(
                          TextSpan(
                            children: [
                              TextSpan(
                                text: '${line.clock} ',
                                style: TextStyle(color: quiet),
                              ),
                              TextSpan(
                                text: '${line.target} ',
                                style: TextStyle(
                                  color: quiet,
                                  fontWeight: FontWeight.w700,
                                ),
                              ),
                              TextSpan(
                                text: line.message,
                                style: TextStyle(
                                  color: _colour(line.level, scheme),
                                  fontWeight: line.level.index >= 3
                                      ? FontWeight.w700
                                      : FontWeight.w400,
                                ),
                              ),
                            ],
                          ),
                          style: const TextStyle(
                            fontSize: 11,
                            height: 1.35,
                            fontFamily: 'monospace',
                            fontFamilyFallback: ['Menlo', 'Consolas'],
                          ),
                        ),
                      );
                    },
                  ),
                ),
        ),
      ],
    );
  }
}

/// The bridge hands `u64` across as `BigInt`; converting once here keeps the
/// arithmetic and formatting below readable.
class _Snapshot {
  const _Snapshot({
    required this.playbackGapMs,
    required this.captureDroppedMs,
    required this.incomingRealMs,
    required this.incomingInventedMs,
    required this.lostPackets,
    required this.jitterBufferMs,
    required this.speakers,
    required this.bytesIn,
    required this.bytesOut,
    required this.voiceIn,
    required this.voiceOut,
    required this.cpuPercent,
    required this.memoryMb,
  });

  factory _Snapshot.of(UiDiagnostics d) => _Snapshot(
    playbackGapMs: d.playbackGapMs.toInt(),
    captureDroppedMs: d.captureDroppedMs.toInt(),
    incomingRealMs: d.incomingRealMs.toInt(),
    incomingInventedMs: d.incomingInventedMs.toInt(),
    lostPackets: d.lostPackets.toInt(),
    jitterBufferMs: d.jitterBufferMs.toInt(),
    speakers: d.speakers,
    bytesIn: d.bytesIn.toInt(),
    bytesOut: d.bytesOut.toInt(),
    voiceIn: d.voicePacketsIn.toInt(),
    voiceOut: d.voicePacketsOut.toInt(),
    cpuPercent: d.cpuPercent,
    memoryMb: d.memoryMb,
  );

  final int playbackGapMs;
  final int captureDroppedMs;
  final int incomingRealMs;
  final int incomingInventedMs;
  final int lostPackets;
  final int jitterBufferMs;
  final int speakers;
  final int bytesIn;
  final int bytesOut;
  final int voiceIn;
  final int voiceOut;
  final double cpuPercent;
  final double memoryMb;
}

/// A fixed-length window of recent samples.
///
/// Thirty seconds is the useful span: long enough to show a stutter that has
/// already passed, short enough that the current moment still dominates the
/// shape. Older samples fall off the front rather than being averaged in,
/// because an average hides exactly the spikes this exists to reveal.
class _History {
  _History(this.unit, {this.decimals = 0});

  /// Thirty seconds shown, plus one sample held beyond the left edge so the
  /// line scrolls out of frame rather than vanishing at it.
  static const window = 30;
  static const capacity = window + 1;

  final String unit;
  final int decimals;
  final List<double> samples = [];

  void add(double value) {
    samples.add(value);
    if (samples.length > capacity) samples.removeAt(0);
  }

  double get latest => samples.isEmpty ? 0 : samples.last;
  double get peak =>
      samples.isEmpty ? 0 : samples.reduce((a, b) => a > b ? a : b);
}

/// One line on a graph.
class _Series {
  const _Series(this.name, this.history, this.color);

  final String name;
  final _History history;
  final Color color;
}

/// Thirty seconds of a measurement, drawn as lines over time.
///
/// A line rather than a number: the question these answer is never "what is it
/// now" but "what was it doing when the audio broke", and a value that has
/// already passed is invisible to a readout. Shape carries that — a spike, a
/// step, a slow climb — where a digit cannot.
///
/// Related quantities share axes. In against out is the comparison worth
/// making, and on two separate graphs the reader has to make it themselves.
class _Graph extends StatefulWidget {
  const _Graph({required this.title, required this.series});

  final String title;
  final List<_Series> series;

  @override
  State<_Graph> createState() => _GraphState();
}

class _GraphState extends State<_Graph> with SingleTickerProviderStateMixin {
  late final AnimationController _phase = AnimationController(
    vsync: this,
    // One sample interval: the plot travels exactly one step in the time the
    // next sample takes to arrive, so it scrolls continuously instead of
    // jumping once a second.
    duration: const Duration(seconds: 1),
  )..forward();

  int _seen = 0;

  @override
  void didUpdateWidget(_Graph old) {
    super.didUpdateWidget(old);
    final now = widget.series.first.history.samples.length;
    // Restarting on a new sample rather than looping freely keeps the motion
    // married to the data: a late sample stalls the scroll instead of letting
    // it run ahead and then snap back.
    if (now != _seen) {
      _seen = now;
      _phase.forward(from: 0);
    }
  }

  @override
  void dispose() {
    _phase.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    final quiet = scheme.onSurfaceVariant;
    final peak = widget.series
        .map((s) => s.history.peak)
        .fold(0.0, (a, b) => a > b ? a : b);
    final unit = widget.series.first.history.unit;
    final decimals = widget.series.first.history.decimals;

    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Row(
          children: [
            Expanded(
              child: Text(
                widget.title,
                style: TextStyle(fontSize: 11, color: quiet),
                overflow: TextOverflow.ellipsis,
              ),
            ),
            for (final s in widget.series) ...[
              const SizedBox(width: 6),
              Container(
                width: 7,
                height: 7,
                decoration: BoxDecoration(
                  color: s.color,
                  borderRadius: BorderRadius.circular(2),
                ),
              ),
              const SizedBox(width: 3),
              Text(
                s.history.latest.toStringAsFixed(decimals),
                style: const TextStyle(
                  fontSize: 11,
                  fontWeight: FontWeight.w700,
                  fontFeatures: [FontFeature.tabularFigures()],
                ),
              ),
            ],
            const SizedBox(width: 4),
            Text(unit, style: TextStyle(fontSize: 10, color: quiet)),
          ],
        ),
        const SizedBox(height: 4),
        SizedBox(
          height: 46,
          child: ClipRect(
            child: AnimatedBuilder(
              animation: _phase,
              builder: (context, _) => CustomPaint(
                size: Size.infinite,
                painter: _GraphPainter(
                  series: widget.series,
                  peak: peak,
                  phase: _phase.value,
                  grid: quiet.withValues(alpha: 0.16),
                ),
              ),
            ),
          ),
        ),
        Row(
          children: [
            Text('30s ago', style: TextStyle(fontSize: 9, color: quiet)),
            const Spacer(),
            // The scale is the peak of the window, so it has to be stated:
            // otherwise the same shape means something different minute to
            // minute and the graph quietly lies.
            Text(
              'peak ${peak.toStringAsFixed(decimals)} $unit',
              style: TextStyle(fontSize: 9, color: quiet),
            ),
          ],
        ),
      ],
    );
  }
}

class _GraphPainter extends CustomPainter {
  _GraphPainter({
    required this.series,
    required this.peak,
    required this.phase,
    required this.grid,
  });

  final List<_Series> series;
  final double peak;

  /// How far through the current sample interval, 0 to 1. The whole plot
  /// slides one step left across that span.
  final double phase;
  final Color grid;

  @override
  void paint(Canvas canvas, Size size) {
    canvas.drawLine(
      Offset(0, size.height),
      Offset(size.width, size.height),
      Paint()
        ..color = grid
        ..strokeWidth = 1,
    );
    canvas.drawLine(
      Offset(0, size.height / 2),
      Offset(size.width, size.height / 2),
      Paint()
        ..color = grid.withValues(alpha: 0.5)
        ..strokeWidth = 1,
    );

    // One shared scale, so the two lines on a graph can be compared by eye.
    final scale = peak <= 0 ? 0.0 : 1 / peak;
    final step = size.width / _History.window;

    for (final s in series) {
      final samples = s.history.samples;
      if (samples.isEmpty) continue;

      // The newest sample starts at the right edge and walks left as the
      // interval elapses, everything earlier one step behind it. Pinning it to
      // the edge instead is what makes a plot jump once a second.
      Offset pointAt(int i) => Offset(
        size.width - (samples.length - 1 - i) * step - phase * step,
        size.height - (samples[i] * scale) * (size.height - 3),
      );

      final path = Path()..moveTo(pointAt(0).dx, pointAt(0).dy);
      for (var i = 1; i < samples.length; i++) {
        final p = pointAt(i);
        path.lineTo(p.dx, p.dy);
      }

      final area = Path.from(path)
        ..lineTo(pointAt(samples.length - 1).dx, size.height)
        ..lineTo(pointAt(0).dx, size.height)
        ..close();
      canvas.drawPath(area, Paint()..color = s.color.withValues(alpha: 0.14));
      canvas.drawPath(
        path,
        Paint()
          ..color = s.color
          ..strokeWidth = 1.6
          ..style = PaintingStyle.stroke
          ..strokeJoin = StrokeJoin.round,
      );
    }
  }

  @override
  bool shouldRepaint(_GraphPainter old) =>
      old.phase != phase || old.peak != peak;
}

/// Lays panels out across the width available, in even rows.
///
/// Used for the counter groups and the graphs alike, so the panel reflows as
/// one thing. A time series in particular needs horizontal room to be legible
/// — squeezed narrow, thirty seconds of detail becomes a smudge — so these sit
/// side by side while each can still be read, and stack as the window narrows
/// rather than all shrinking together.
class _ResponsiveGrid extends StatelessWidget {
  const _ResponsiveGrid({required this.children, required this.minWidth});

  final List<Widget> children;

  /// Narrower than this and the contents stop being readable.
  final double minWidth;

  @override
  Widget build(BuildContext context) {
    if (children.isEmpty) return const SizedBox.shrink();
    return LayoutBuilder(
      builder: (context, constraints) {
        final fits = (constraints.maxWidth / minWidth).floor().clamp(
          1,
          children.length,
        );

        // Even rows rather than as many as will fit. Four items across three
        // columns leaves a lone one on the second row, which reads as an
        // afterthought and wastes the width it was given; two and two is the
        // same information with none of that.
        final rows = (children.length / fits).ceil();
        final columns = (children.length / rows).ceil();

        const gap = 14.0;
        final width = (constraints.maxWidth - gap * (columns - 1)) / columns;
        return Wrap(
          spacing: gap,
          runSpacing: 12,
          children: [
            for (final child in children) SizedBox(width: width, child: child),
          ],
        );
      },
    );
  }
}

class _Group extends StatelessWidget {
  const _Group({required this.title, required this.rows});

  final String title;
  final List<_Row> rows;

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        const SizedBox(height: 8),
        Text(
          title.toUpperCase(),
          style: TextStyle(
            fontSize: 10,
            fontWeight: FontWeight.w700,
            letterSpacing: 0.8,
            color: Theme.of(context).colorScheme.onSurfaceVariant,
          ),
        ),
        const SizedBox(height: 4),
        ...rows,
      ],
    );
  }
}

class _Row extends StatelessWidget {
  const _Row(this.label, this.value, {this.bad = false});

  final String label;
  final String value;

  /// Highlights a number that means something is going wrong, so the panel can
  /// be read at a glance rather than compared line by line.
  final bool bad;

  @override
  Widget build(BuildContext context) {
    final quiet = Theme.of(context).colorScheme.onSurfaceVariant;
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 2),
      child: Row(
        children: [
          Expanded(
            child: Text(label, style: TextStyle(fontSize: 12, color: quiet)),
          ),
          Text(
            value,
            style: TextStyle(
              fontSize: 12,
              fontWeight: FontWeight.w600,
              color: bad ? StatusColors.connecting : null,
              fontFeatures: const [FontFeature.tabularFigures()],
            ),
          ),
        ],
      ),
    );
  }
}
