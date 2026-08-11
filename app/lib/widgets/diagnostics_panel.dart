import 'dart:async';

import 'package:flutter/foundation.dart' show ValueListenable;
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../src/rust/api/mumbleway.dart';
import '../l10n/app_localizations.dart';
import '../services/engine_log.dart';
import '../state/app_state.dart';
import '../theme.dart';
import 'recording_toggle.dart';
import 'spectrum_view.dart';

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

  /// The whole device's CPU, as a share of everything it has.
  ///
  /// The mean of the per-core readings, which is what "the device is 40% busy"
  /// means and is bounded at 100 by construction. Summing the cores instead
  /// would give 800% on an eight-core phone, which is a true number nobody
  /// reads correctly.
  ///
  /// Kept beside [_cpu] rather than replacing it: this process's share and the
  /// device's total answer different questions, and the interesting case — a
  /// phone that is busy with something else entirely — is exactly where the
  /// two disagree.
  final _cpuTotal = _History('%');

  /// One history per core of the **device**, under the line for this process.
  ///
  /// Grown on first sight rather than declared, because the count is not known
  /// until the core answers and differs per device. Kept as a list of the same
  /// `_History` the other series use so the graph needs no special case.
  final _perCore = <_History>[];

  /// Whether the platform ever gave us per-core figures.
  ///
  /// Three states, not two, and the difference matters on screen: not asked
  /// yet, answered, and refused. A phone that refuses draws no lines, and a
  /// graph that silently has fewer lines than a rider expects is
  /// indistinguishable from a graph that is broken — so the panel says which.
  bool? _perCoreAvailable;

  /// How far the plots are through the current sample interval, 0 to 1.
  ///
  /// **One clock for all six graphs, and it is not the display's.** Each graph
  /// used to own an `AnimationController`, which is driven by a `Ticker` and so
  /// repaints once per vsync — six painters at the refresh rate, 360 paints a
  /// second on a 60 Hz screen and 720 on a 120 Hz one, for a plot that gains a
  /// new sample once a second and moves one pixel-ish between frames. Opening
  /// the panel put a visible step in GPU load.
  ///
  /// A timer at [_scrollFps] instead. The value means exactly what the
  /// controller's did, so the painter is unchanged, and the graphs all advance
  /// together because they are all fed by the same one-second [_refresh] — a
  /// per-graph clock was never buying independence, only frames.
  final _scroll = ValueNotifier<double>(1);

  /// Fast enough that a scroll of one step per second reads as motion rather
  /// than as ticking, which is all this has to be. The eye cannot follow a
  /// 46-pixel-high plot moving a fifth of a pixel per frame at 120 Hz.
  static const _scrollFps = 20;
  static const _scrollPeriod = Duration(milliseconds: 1000 ~/ _scrollFps);

  Timer? _scrollTick;

  /// Time since the sample the plots are currently walking away from.
  ///
  /// Read rather than counted: a timer that fires late — and on a phone under
  /// the load this panel exists to explain, it will — would otherwise leave the
  /// plot short of where the data says it should be, and the error would
  /// accumulate for as long as the panel stayed open.
  final _sinceSample = Stopwatch();

  @override
  void initState() {
    super.initState();
    _refresh();
    _tick = Timer.periodic(const Duration(seconds: 1), (_) => _refresh());
  }

  void _startScrolling() {
    _sinceSample
      ..reset()
      ..start();
    _scrollTick ??= Timer.periodic(_scrollPeriod, (_) {
      final t = _sinceSample.elapsedMilliseconds / 1000;
      // A `ValueNotifier` is silent when the value is unchanged, so a stalled
      // sample stops repainting instead of redrawing the same frame for ever.
      _scroll.value = t >= 1 ? 1 : t;
    });
  }

  void _stopScrolling() {
    _scrollTick?.cancel();
    _scrollTick = null;
    _sinceSample.stop();
  }

  /// Where the chain stands, for the one counter measured before it runs.
  UiChainStatus? _chain;

  /// Where a block's time goes.
  UiStageCosts? _costs;

  void _refresh() {
    if (!mounted) return;
    try {
      final now = _Snapshot.of(audioDiagnostics());
      // Free to ask for and always current, unlike the spectrum: the chain
      // publishes it every block whether anybody is reading. The input peak
      // rides on it.
      final chain = audioChainStatus();
      // Likewise kept whether or not anybody is looking, and for a stronger
      // reason: a cost only measured while a panel is open is measured under
      // different load than the one being complained about.
      final costs = audioStageCosts();
      final was = _previous;
      // Sampled outside `setState`, and kept whether or not anybody is
      // looking. The graphs answer "what was it doing when the audio broke",
      // so a history that only exists while the panel is open is a history
      // that is empty exactly when it is wanted.
      if (was != null) {
        // Rates from deltas: the counters are cumulative precisely so the
        // interval is the caller's business, and this one is a second.
        _bytesIn.add((now.bytesIn - was.bytesIn) / 1024);
        _bytesOut.add((now.bytesOut - was.bytesOut) / 1024);
        _packetsIn.add((now.voiceIn - was.voiceIn).toDouble());
        _packetsOut.add((now.voiceOut - was.voiceOut).toDouble());
      }
      _cpu.add(now.cpuPercent);
      // Latched on the first non-empty answer rather than set from each
      // sample: a device that reports cores must not have its lines vanish
      // because one reading came back short, and one that refuses must not
      // look like it is about to start.
      if (now.cpuPerCore.isNotEmpty) {
        _perCoreAvailable = true;
        while (_perCore.length < now.cpuPerCore.length) {
          _perCore.add(_History('%'));
        }
        for (var i = 0; i < _perCore.length; i++) {
          _perCore[i].add(
            i < now.cpuPerCore.length ? now.cpuPerCore[i] : 0.0,
          );
        }
        _cpuTotal.add(
          now.cpuPerCore.reduce((a, b) => a + b) / now.cpuPerCore.length,
        );
      } else {
        _perCoreAvailable ??= false;
      }
      _memory.add(now.memoryMb);
      _previous = now;

      // **Painting is the part that is gated, not sampling.** This panel is
      // never disposed — it is only slid out of sight — so without this a
      // rider with it closed rebuilt forty widgets and six graphs every second
      // for the whole ride, drawing them to a screen nobody was looking at.
      //
      // `getInheritedWidgetOfExactType` rather than `AppStateScope.of`: this
      // runs from a timer, and the second one would register a dependency
      // outside `build`.
      final open = context
              .getInheritedWidgetOfExactType<AppStateScope>()
              ?.notifier
              ?.diagnosticsOpen ??
          false;
      if (!open) {
        // Nothing is on screen to scroll. The sampling above continues, because
        // the history is the point of it; the painting does not.
        _stopScrolling();
        return;
      }
      _startScrolling();

      setState(() {
        _audio = now;
        _chain = chain;
        _costs = costs;
      });
    } catch (_) {
      // The engine is not up; nothing to report.
    }
  }

  @override
  void dispose() {
    _tick?.cancel();
    _stopScrolling();
    _scroll.dispose();
    super.dispose();
  }

  /// The core names stages by a stable id; the label is this side's business.
  ///
  /// A `switch` rather than a map lookup so a stage added in Rust and not here
  /// fails to compile rather than showing a rider `de-hiss`.
  static String _stageLabel(L l, String id) => switch (id) {
    'input' => l.diagStageInput,
    'enhancer' => l.diagStageEnhancer,
    'suppression' => l.diagStageSuppression,
    'feedback' => l.diagStageFeedback,
    'de-hiss' => l.diagStageDehiss,
    'transmit' => l.diagStageTransmit,
    'encode' => l.diagStageEncode,
    _ => id,
  };

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
                    // First, because it is the only thing here that shows the
                    // fault as it happens. The counters below say what went
                    // wrong afterwards and are read second, if at all.
                    //
                    // Built only under this `if`, and it is load-bearing rather
                    // than tidiness: asking the engine for a frame is what makes
                    // it compute one, so a widget that is never built costs
                    // nothing in the audio worker either. This panel is never
                    // disposed — only slid out of sight — so nothing else would
                    // ever stop it.
                    if (AppStateScope.of(context).diagnosticsOpen) ...[
                      const SpectrumView(),
                      // Wider than the gaps inside the analyser, so the card
                      // reads as a separate thing rather than as one more row
                      // of the readout above it.
                      const SizedBox(height: 20),
                      // Directly under the analyser, because it answers the
                      // question the analyser raises: having watched the gate
                      // shut on a word, the next thing anyone wants is that
                      // moment on disk where it can be looked at properly.
                      const RecordingToggle(),
                      const SizedBox(height: 16),
                    ],
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
                              // Before the chain, unlike everything around it.
                              // The meter beside the gain slider is measured
                              // after suppression, so it cannot show an
                              // overdriven microphone -- which is exactly the
                              // fault that hid here for an evening.
                              _Row(
                                l.diagInputPeak,
                                '${_chain?.inputPeakDb.toStringAsFixed(1) ?? "-"} dBFS',
                                bad: (_chain?.inputPeakDb ?? -120) > -0.5,
                              ),
                              if ((_chain?.inputClipped ?? BigInt.zero) >
                                  BigInt.zero)
                                _Row(
                                  l.diagInputClipped,
                                  '${_chain!.inputClipped} samples',
                                  bad: true,
                                ),
                              // All three from the chain, and all three after
                              // suppression, because they are only meaningful
                              // against each other: the level the gate sees,
                              // the floor it tracks, and the bar it opens at.
                              // Mixing a raw level into that trio would invite
                              // a comparison that means nothing -- which is
                              // why the raw number is the row above, named
                              // separately.
                              _Row(
                                l.diagMicrophoneLevel,
                                '${_chain?.levelDb.toStringAsFixed(0) ?? "-"} dBFS',
                              ),
                              _Row(
                                l.diagNoiseFloor,
                                '${_chain?.noiseFloorDb.toStringAsFixed(0) ?? "-"} dBFS',
                              ),
                              _Row(
                                l.diagOpensAt,
                                '${_chain?.activationThresholdDb.toStringAsFixed(0) ?? "-"} dBFS',
                              ),
                            ],
                          ),
                        ],
                        // Where a block's 10 ms actually goes.
                        //
                        // **The measurement that would have saved a day.** The
                        // enhancer carried the only stopwatch in the chain, so
                        // when blocks ran late it was the only stage that could
                        // be blamed — and it switched itself off for the
                        // session on that evidence. Measured alone on the phone
                        // it was reported from, the model fits the budget with
                        // room to spare. Every stage carries a clock now, and
                        // the two rows at the bottom are what stop this being
                        // read the same wrong way: `unattributed` is the part
                        // of the block no stage was timing, and a backlog that
                        // climbs is the chain losing whatever the stages say.
                        if ((_costs?.blocks ?? BigInt.zero) > BigInt.zero)
                          _Group(
                            title: l.diagBlockCost,
                            rows: [
                              for (final s in _costs!.stages)
                                _Row(
                                  _stageLabel(l, s.id),
                                  '${(s.meanUs / 1000).toStringAsFixed(2)} ms',
                                  // Amber on a stage that is on its own more
                                  // than a third of the budget: not a fault,
                                  // but the place to look first.
                                  bad: s.meanUs > _costs!.budgetUs / 3,
                                ),
                              _Row(
                                l.diagBlockUnattributed,
                                '${(_costs!.unattributedUs / 1000).toStringAsFixed(2)} ms',
                              ),
                              _Row(
                                l.diagBlockTotal,
                                '${(_costs!.blockMeanUs / 1000).toStringAsFixed(2)} ms'
                                ' / ${(_costs!.blockWorstUs / 1000).toStringAsFixed(1)}',
                                bad: _costs!.blockMeanUs > _costs!.budgetUs,
                              ),
                              _Row(
                                l.diagBlockBacklog,
                                '${_costs!.backlogMeanMs.toStringAsFixed(0)} ms'
                                ' / ${_costs!.backlogWorstMs.toStringAsFixed(0)}',
                                // One block of slack is normal; a queue that
                                // sits deep is the chain not keeping up, and it
                                // says so before anything is dropped.
                                bad: _costs!.backlogMeanMs > 20,
                              ),
                            ],
                          ),
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
                          phase: _scroll,
                          title: l.diagNetwork,
                          series: [
                            _Series('in', _bytesIn, StatusColors.connected),
                            _Series('out', _bytesOut, StatusColors.connecting),
                          ],
                        ),
                        _Graph(
                          phase: _scroll,
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
                          phase: _scroll,
                          title: 'CPU',
                          series: [
                            _Series('app', _cpu, scheme.primary),
                            // The whole device, as one number rather than one
                            // per core. Drawn as no line at all — the cores
                            // below already are that line, and a mean of them
                            // laid over them would be a shape with no
                            // information in it.
                            if (_perCoreAvailable == true)
                              _Series(
                                'device',
                                _cpuTotal,
                                scheme.onSurfaceVariant,
                                inPlot: false,
                              ),
                            // The device's cores, under the app's own line.
                            //
                            // Dimmed on purpose: eight cores against one
                            // process would otherwise be eight bright lines
                            // burying the one line this panel exists for.
                            // They are context for it, not peers of it.
                            //
                            // Lines only. See [_Series.inLegend].
                            for (var i = 0; i < _perCore.length; i++)
                              _Series(
                                '$i',
                                _perCore[i],
                                scheme.onSurfaceVariant.withValues(alpha: 0.35),
                                inLegend: false,
                              ),
                          ],
                        ),
                        // Said rather than left as an absence. Per-core times
                        // come only from the global `/proc/stat` on Linux,
                        // which is the file the Android sandbox denies us —
                        // the same denial that made the CPU figure read 0%
                        // before it was measured another way.
                        if (_perCoreAvailable == false)
                          Padding(
                            padding: const EdgeInsets.only(left: 4, bottom: 6),
                            child: Text(
                              l.diagPerCoreUnavailable,
                              style: TextStyle(
                                fontSize: 11,
                                color: scheme.onSurfaceVariant,
                              ),
                            ),
                          ),
                        _Graph(
                          phase: _scroll,
                          title: l.diagMemory,
                          series: [_Series('rss', _memory, scheme.primary)],
                        ),
                      ],
                    ),
                    const SizedBox(height: 16),
                    // Built only while the panel is genuinely on screen, and
                    // that `if` is load-bearing rather than tidiness. Asking
                    // the engine for a frame is what makes it compute one, so
                    // a widget that is never built costs nothing in the audio
                    // worker either — and this panel is never disposed, only
                    // slid out of sight, so nothing else would ever stop it.
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
    // Open at the tail, not the top. The log has usually been running for the
    // whole session by the time anybody looks at it, so starting at the oldest
    // line means the newest — the reason the panel was opened — is several
    // screens away. After this the follow rule in [_onLines] takes over.
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (_scroll.hasClients) {
        _scroll.jumpTo(_scroll.position.maxScrollExtent);
      }
    });
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
    required this.cpuPerCore,
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
    cpuPerCore: d.cpuPerCore,
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
  final List<double> cpuPerCore;
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

  /// How many samples have ever been added, which is **not** `samples.length`.
  ///
  /// The graph restarts its scroll on each new sample and needs to know one has
  /// arrived. `samples.length` looks like that signal and is one for exactly
  /// [capacity] seconds: once the window is full every `add` also drops one off
  /// the front, the length is pinned at 31 for ever, and anything watching it
  /// concludes that time has stopped. This does not stop.
  int get added => _added;
  int _added = 0;

  void add(double value) {
    samples.add(value);
    if (samples.length > capacity) samples.removeAt(0);
    _added++;
  }

  double get latest => samples.isEmpty ? 0 : samples.last;
  double get peak =>
      samples.isEmpty ? 0 : samples.reduce((a, b) => a > b ? a : b);
}

/// One line on a graph.
class _Series {
  const _Series(
    this.name,
    this.history,
    this.color, {
    this.inLegend = true,
    this.inPlot = true,
  });

  final String name;
  final _History history;
  final Color color;

  /// Whether this series gets a swatch and a number above the graph.
  ///
  /// **Off for the device's cores.** The header prints one value per series,
  /// which is fine for two and unreadable for twenty: a 20-thread machine
  /// turned the title row into a wall of numbers nobody reads individually,
  /// and it pushed the figure people *do* read off the end. The cores are
  /// worth seeing as shapes — is one pinned while the rest idle — and their
  /// individual percentages are not worth the room. The mean of them is
  /// reported instead, once.
  final bool inLegend;

  /// Whether this series is drawn on the plot.
  ///
  /// **Off for the device total**, which is the mean of lines already on the
  /// graph. Drawing it would add a shape carrying no information the cores do
  /// not already carry, through the middle of the ones that do. It exists to
  /// be read as a number.
  final bool inPlot;
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
/// One time series plot, scrolled by the panel's clock rather than its own.
///
/// **Stateless on purpose.** It used to own an `AnimationController` and
/// restart it whenever a sample arrived, which is where a fixed-capacity ring
/// and a change signal met and got it wrong: the restart compared
/// `samples.length`, which stops changing the moment the window fills, so the
/// graphs froze a second after anybody opened the panel while the numbers above
/// them carried on. There is nothing here now for that bug to live in — the
/// clock is the panel's, and it is reset by the same code that adds the sample.
class _Graph extends StatelessWidget {
  const _Graph({
    required this.title,
    required this.series,
    required this.phase,
  });

  final String title;
  final List<_Series> series;

  /// How far through the current sample interval, 0 to 1. See
  /// `_DiagnosticsPanelState._scroll`.
  final ValueListenable<double> phase;

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    final quiet = scheme.onSurfaceVariant;
    final peak = series
        .map((s) => s.history.peak)
        .fold(0.0, (a, b) => a > b ? a : b);
    final unit = series.first.history.unit;
    final decimals = series.first.history.decimals;

    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Row(
          children: [
            Expanded(
              child: Text(
                title,
                style: TextStyle(fontSize: 11, color: quiet),
                overflow: TextOverflow.ellipsis,
              ),
            ),
            for (final s in series.where((s) => s.inLegend)) ...[
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
            // Its own layer. Six of these scroll together, and without a
            // boundary each one's repaint dirties the layer they share — so
            // every graph, every counter row and every dot beside them is
            // redrawn six times over for one graph's worth of movement.
            child: RepaintBoundary(
              child: ValueListenableBuilder<double>(
                valueListenable: phase,
                // Only this builder runs on a tick. The title, the legend and
                // the peak below are outside it, so twenty times a second the
                // work is six painters and nothing else — no layout, no text
                // shaping, no rebuild of the rows around them.
                builder: (context, value, _) => CustomPaint(
                  size: Size.infinite,
                  painter: _GraphPainter(
                    series: series,
                    peak: peak,
                    phase: value,
                    grid: quiet.withValues(alpha: 0.16),
                  ),
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
      if (!s.inPlot) continue;
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
