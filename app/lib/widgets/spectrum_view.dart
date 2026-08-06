import 'package:flutter/material.dart';
import 'package:flutter/scheduler.dart';

import '../l10n/app_localizations.dart';
import '../src/rust/api/mumbleway.dart';
import '../theme.dart';

/// Live view of what the capture chain is doing to the rider's voice.
///
/// Three spectra taken from the same block — the microphone, what the noise
/// gate was about to judge, and what reached the encoder — over a row of dots
/// saying where each stage of the chain stands.
///
/// **Mount this only while it is on screen.** Asking the engine for a frame is
/// what makes it compute one; the ask expires half a second later, so a widget
/// that is not built costs nothing at all, in the core as well as here. The
/// diagnostics panel is never disposed — it is only slid off screen — so this
/// has to be created and destroyed by an `if`, not merely hidden.
///
/// Driven by a [Ticker] rather than a [Timer] on purpose: Flutter mutes tickers
/// when the app goes to the background, so backgrounding stops the polling, the
/// engine's arming lapses, and the transforms stop — with no code of its own to
/// get that wrong.
class SpectrumView extends StatefulWidget {
  const SpectrumView({super.key});

  @override
  State<SpectrumView> createState() => _SpectrumViewState();
}

class _SpectrumViewState extends State<SpectrumView>
    with SingleTickerProviderStateMixin {
  Ticker? _ticker;
  UiSpectrum? _spectrum;
  UiChainStatus? _chain;

  /// The last frame counter seen. A u64 in the core, so a BigInt here.
  BigInt? _lastSeq;
  int _sinceNewFrame = 0;

  /// Roughly 20 Hz. Faster buys nothing: the core smooths the bands at 33 Hz
  /// and an eye cannot follow either.
  static const _pollEvery = 3;
  int _tick = 0;

  @override
  void initState() {
    super.initState();
    _ticker = createTicker(_onTick)..start();
  }

  @override
  void dispose() {
    _ticker?.dispose();
    super.dispose();
  }

  void _onTick(Duration _) {
    if (++_tick % _pollEvery != 0) return;
    UiSpectrum? spectrum;
    UiChainStatus? chain;
    try {
      // This call is the ask. Not calling it is how the engine finds out to
      // stop.
      spectrum = audioSpectrum();
      chain = audioChainStatus();
    } catch (_) {
      // The engine is not up. Nothing to draw, and nothing to complain about.
      spectrum = null;
      chain = null;
    }

    // A frame counter that has stopped moving means the worker has stopped —
    // the devices are shut, most likely — which looks exactly like silence if
    // the last frame is left on screen.
    if (spectrum != null && spectrum.seq == _lastSeq) {
      _sinceNewFrame++;
    } else {
      _sinceNewFrame = 0;
      _lastSeq = spectrum?.seq;
    }

    if (!mounted) return;
    setState(() {
      _spectrum = spectrum;
      _chain = chain;
    });
  }

  /// True when frames have stopped arriving for about a second.
  bool get _stalled => _sinceNewFrame > 20;

  @override
  Widget build(BuildContext context) {
    final l = L.of(context);
    final spectrum = _spectrum;
    final scheme = Theme.of(context).colorScheme;

    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(
          l.diagSpectrum,
          style: TextStyle(
            fontSize: 10,
            fontWeight: FontWeight.w700,
            letterSpacing: 0.8,
            color: scheme.onSurfaceVariant,
          ),
        ),
        const SizedBox(height: 8),
        _Legend(transmitting: spectrum?.transmitting ?? false),
        const SizedBox(height: 6),
        SizedBox(
          height: 120,
          child: (spectrum == null || _stalled)
              ? _Idle(message: _stalled ? l.diagSpectrumStalled : l.diagSpectrumWaiting)
              : CustomPaint(
                  size: Size.infinite,
                  painter: _SpectrumPainter(
                    spectrum: spectrum,
                    grid: scheme.onSurfaceVariant.withValues(alpha: 0.16),
                    raw: scheme.onSurfaceVariant.withValues(alpha: 0.55),
                    preGate: scheme.primary,
                  ),
                ),
        ),
        const SizedBox(height: 10),
        if (_chain != null) _ChainDots(status: _chain!),
      ],
    );
  }
}

/// Says which trace is which, and what the sent trace's colour means.
class _Legend extends StatelessWidget {
  const _Legend({required this.transmitting});

  final bool transmitting;

  @override
  Widget build(BuildContext context) {
    final l = L.of(context);
    final scheme = Theme.of(context).colorScheme;
    return Wrap(
      spacing: 14,
      runSpacing: 4,
      children: [
        _key(scheme.onSurfaceVariant.withValues(alpha: 0.55), l.diagTraceRaw, context),
        _key(scheme.primary, l.diagTracePreGate, context),
        _key(
          _SpectrumPainter.sentColour(transmitting),
          transmitting ? l.diagTraceSentLive : l.diagTraceSentIdle,
          context,
        ),
      ],
    );
  }

  Widget _key(Color colour, String label, BuildContext context) => Row(
    mainAxisSize: MainAxisSize.min,
    children: [
      Container(width: 8, height: 8, color: colour),
      const SizedBox(width: 5),
      Text(
        label,
        style: TextStyle(
          fontSize: 10,
          color: Theme.of(context).colorScheme.onSurfaceVariant,
        ),
      ),
    ],
  );
}

/// Shown instead of a frozen picture when there is nothing live to draw.
class _Idle extends StatelessWidget {
  const _Idle({required this.message});

  final String message;

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    return DecoratedBox(
      decoration: BoxDecoration(
        border: Border.all(color: scheme.onSurfaceVariant.withValues(alpha: 0.16)),
        borderRadius: BorderRadius.circular(4),
      ),
      child: Center(
        child: Text(
          message,
          style: TextStyle(fontSize: 11, color: scheme.onSurfaceVariant),
        ),
      ),
    );
  }
}

class _SpectrumPainter extends CustomPainter {
  const _SpectrumPainter({
    required this.spectrum,
    required this.grid,
    required this.raw,
    required this.preGate,
  });

  final UiSpectrum spectrum;
  final Color grid;
  final Color raw;
  final Color preGate;

  /// The sent trace's colour, which is the one piece of state this display
  /// carries that the shapes cannot.
  ///
  /// A flat sent trace means the same shape whether the gate shut or nobody
  /// spoke, and those are opposite diagnoses — so live is a pale green and idle
  /// is grey, and the difference is visible at a glance without reading a
  /// label.
  static Color sentColour(bool transmitting) => transmitting
      ? const Color(0xFF9BE8B4)
      : const Color(0xFF8A93A0);

  @override
  void paint(Canvas canvas, Size size) {
    if (spectrum.centresHz.isEmpty) return;
    final floor = spectrum.floorDb;

    double y(double db) {
      final t = ((db - floor) / (0 - floor)).clamp(0.0, 1.0);
      return size.height - t * size.height;
    }

    final lines = Paint()
      ..color = grid
      ..strokeWidth = 1;
    // Every 20 dB, so the eye has a scale without the grid competing with data.
    for (var db = floor; db <= 0; db += 20) {
      final yy = y(db);
      canvas.drawLine(Offset(0, yy), Offset(size.width, yy), lines);
    }

    final bands = spectrum.centresHz.length;
    final slot = size.width / bands;

    // Sent first as filled bars, so it reads as the body of the display and the
    // two thin lines read as what it was made from.
    final bars = Paint()..color = sentColour(spectrum.transmitting);
    for (var i = 0; i < bands; i++) {
      final top = y(spectrum.sentDb[i]);
      if (top >= size.height) continue;
      canvas.drawRect(
        Rect.fromLTRB(i * slot + 0.5, top, (i + 1) * slot - 0.5, size.height),
        bars,
      );
    }

    void trace(List<double> db, Color colour) {
      final path = Path();
      for (var i = 0; i < bands; i++) {
        final x = i * slot + slot / 2;
        final yy = y(db[i]);
        if (i == 0) {
          path.moveTo(x, yy);
        } else {
          path.lineTo(x, yy);
        }
      }
      canvas.drawPath(
        path,
        Paint()
          ..color = colour
          ..style = PaintingStyle.stroke
          ..strokeWidth = 1.4
          ..strokeJoin = StrokeJoin.round,
      );
    }

    trace(spectrum.rawDb, raw);
    trace(spectrum.preGateDb, preGate);
  }

  @override
  bool shouldRepaint(_SpectrumPainter old) => old.spectrum.seq != spectrum.seq;
}

/// The processing chain as a row of dots, in the order audio passes through it.
class _ChainDots extends StatelessWidget {
  const _ChainDots({required this.status});

  final UiChainStatus status;

  @override
  Widget build(BuildContext context) {
    final l = L.of(context);
    return Wrap(
      spacing: 12,
      runSpacing: 8,
      children: [
        for (final stage in status.stages)
          _Dot(label: _label(l, stage), state: stage.state, value: stage.value),
      ],
    );
  }

  /// Composed here rather than sent from Rust: everything else on this panel is
  /// translated, and a message built in the engine would be the one string a
  /// translator could not reach.
  String _label(L l, UiStage stage) => switch (stage.id) {
    'aec' => l.diagStageEcho,
    'rnnoise' => l.diagStageSuppressor,
    'vad' => l.diagStageVoice,
    'gate' => l.diagStageGate,
    'agc' => l.diagStageLevel,
    'dehiss' => l.diagStageHiss,
    'feedback' => l.diagStageFeedback,
    'transmit' => l.diagStageTransmit,
    _ => stage.id,
  };
}

class _Dot extends StatelessWidget {
  const _Dot({required this.label, required this.state, required this.value});

  final String label;
  final StageState state;
  final double value;

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    // The app's own status vocabulary, so a dot here means what a dot means
    // everywhere else in the interface.
    final colour = switch (state) {
      StageState.good => StatusColors.connected,
      StageState.warn => StatusColors.connecting,
      StageState.bad => StatusColors.failed,
      StageState.off => scheme.onSurfaceVariant.withValues(alpha: 0.35),
    };

    return Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        Container(
          width: 8,
          height: 8,
          decoration: BoxDecoration(color: colour, shape: BoxShape.circle),
        ),
        const SizedBox(width: 5),
        Text(
          label,
          style: TextStyle(fontSize: 11, color: scheme.onSurfaceVariant),
        ),
      ],
    );
  }
}
