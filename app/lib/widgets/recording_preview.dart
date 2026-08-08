import 'dart:io';

import 'package:flutter/material.dart';

import '../l10n/app_localizations.dart';
import '../services/recording_player.dart';

/// Plays a diagnostic recording back, with a waveform and a playhead.
///
/// **A sheet rather than a section.** The privacy policy asks people to listen
/// to a recording before sending it, and an instruction nobody can act on is
/// not a safeguard. But the panel it belongs to is a bottom sheet on a phone
/// that already has an analyser, two rows of lights and four counter groups in
/// it — so this costs nothing at all until somebody asks for it, and goes away
/// again afterwards.
Future<void> showRecordingPreview(BuildContext context, Directory dir) {
  return showModalBottomSheet<void>(
    context: context,
    isScrollControlled: true,
    useSafeArea: true,
    showDragHandle: true,
    builder: (_) => _PreviewSheet(dir: dir),
  );
}

class _PreviewSheet extends StatefulWidget {
  const _PreviewSheet({required this.dir});
  final Directory dir;

  @override
  State<_PreviewSheet> createState() => _PreviewSheetState();
}

class _PreviewSheetState extends State<_PreviewSheet> {
  final _player = RecordingPlayer();
  List<File> _files = const [];
  String? _selected;
  Waveform? _wave;
  bool _loading = false;

  @override
  void initState() {
    super.initState();
    // Audio only. The `.csv` beside each one is the decision log and there is
    // nothing to listen to in it.
    _files =
        widget.dir
            .listSync()
            .whereType<File>()
            .where((f) => f.path.toLowerCase().endsWith('.s16'))
            .toList()
          ..sort((a, b) => b.path.compareTo(a.path));
    if (_files.isNotEmpty) _load(_files.first.path);
  }

  @override
  void dispose() {
    _player.dispose();
    super.dispose();
  }

  Future<void> _load(String path) async {
    setState(() {
      _selected = path;
      _wave = null;
      _loading = true;
    });
    await _player.open(path);
    final wave = await _player.waveform();
    if (!mounted) return;
    setState(() {
      _wave = wave;
      _loading = false;
    });
  }

  static String _clock(Duration d) {
    final m = d.inMinutes;
    final s = d.inSeconds % 60;
    return '$m:${s.toString().padLeft(2, '0')}';
  }

  @override
  Widget build(BuildContext context) {
    final l = L.of(context);
    final scheme = Theme.of(context).colorScheme;

    if (_files.isEmpty) {
      return Padding(
        padding: const EdgeInsets.fromLTRB(20, 0, 20, 32),
        child: Text(l.diagRecordingNone),
      );
    }

    return SafeArea(
      child: Padding(
        padding: const EdgeInsets.fromLTRB(16, 0, 16, 20),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Text(
              l.diagPreviewTitle,
              style: Theme.of(context).textTheme.titleMedium,
            ),
            const SizedBox(height: 4),
            Text(
              l.diagPreviewBody,
              style: TextStyle(fontSize: 12, color: scheme.onSurfaceVariant),
            ),
            const SizedBox(height: 14),

            // Which ride. Named after when it was, so the newest is first and
            // the list reads as a history rather than as file names.
            if (_files.length > 1)
              SizedBox(
                height: 36,
                child: ListView.separated(
                  scrollDirection: Axis.horizontal,
                  itemCount: _files.length,
                  separatorBuilder: (_, _) => const SizedBox(width: 8),
                  itemBuilder: (context, i) {
                    final path = _files[i].path;
                    final name = path.split(Platform.pathSeparator).last;
                    final stem = name.substring(0, name.lastIndexOf('.'));
                    return ChoiceChip(
                      label: Text(stem, style: const TextStyle(fontSize: 12)),
                      selected: _selected == path,
                      onSelected: (_) => _load(path),
                    );
                  },
                ),
              ),
            if (_files.length > 1) const SizedBox(height: 14),

            ListenableBuilder(
              listenable: _player,
              builder: (context, _) {
                final wave = _wave;
                return Column(
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  children: [
                    SizedBox(
                      height: 96,
                      child: _loading || wave == null
                          ? Center(
                              child: SizedBox(
                                width: 22,
                                height: 22,
                                child: CircularProgressIndicator(
                                  strokeWidth: 2,
                                  color: scheme.outline,
                                ),
                              ),
                            )
                          : _Scrubber(
                              wave: wave,
                              progress: _player.progress,
                              onSeek: _player.seekToFraction,
                            ),
                    ),
                    const SizedBox(height: 6),
                    Row(
                      children: [
                        IconButton.filled(
                          onPressed: _loading
                              ? null
                              : () => _player.playing
                                    ? _player.pause()
                                    : _player.play(),
                          icon: Icon(
                            _player.playing ? Icons.pause : Icons.play_arrow,
                          ),
                          tooltip: _player.playing
                              ? l.diagPreviewPause
                              : l.diagPreviewPlay,
                        ),
                        const SizedBox(width: 12),
                        Text(
                          '${_clock(_player.position)} / '
                          '${_clock(_player.duration)}',
                          style: TextStyle(
                            fontFeatures: const [FontFeature.tabularFigures()],
                            color: scheme.onSurfaceVariant,
                          ),
                        ),
                      ],
                    ),
                  ],
                );
              },
            ),
          ],
        ),
      ),
    );
  }
}

/// The waveform, the playhead, and the whole of the seeking.
///
/// One gesture rather than a slider under a picture: the picture *is* the
/// control, so a tap goes there and a drag scrubs, which is what everybody
/// already expects of a waveform.
class _Scrubber extends StatelessWidget {
  const _Scrubber({
    required this.wave,
    required this.progress,
    required this.onSeek,
  });

  final Waveform wave;
  final double progress;
  final ValueChanged<double> onSeek;

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    return LayoutBuilder(
      builder: (context, box) {
        void seek(Offset local) =>
            onSeek((local.dx / box.maxWidth).clamp(0.0, 1.0));
        return GestureDetector(
          behavior: HitTestBehavior.opaque,
          onTapDown: (d) => seek(d.localPosition),
          onHorizontalDragStart: (d) => seek(d.localPosition),
          onHorizontalDragUpdate: (d) => seek(d.localPosition),
          child: CustomPaint(
            painter: _WavePainter(
              wave: wave,
              progress: progress,
              played: scheme.primary,
              unplayed: scheme.outlineVariant,
              head: scheme.error,
            ),
            size: Size.infinite,
          ),
        );
      },
    );
  }
}

class _WavePainter extends CustomPainter {
  _WavePainter({
    required this.wave,
    required this.progress,
    required this.played,
    required this.unplayed,
    required this.head,
  });

  final Waveform wave;
  final double progress;
  final Color played, unplayed, head;

  @override
  void paint(Canvas canvas, Size size) {
    if (wave.isEmpty) return;
    final mid = size.height / 2;
    final step = size.width / wave.buckets;
    final headX = size.width * progress;

    // A hairline at zero, so a stretch the gate closed reads as silence rather
    // than as a gap in the drawing.
    canvas.drawLine(
      Offset(0, mid),
      Offset(size.width, mid),
      Paint()
        ..color = unplayed
        ..strokeWidth = 1,
    );

    final paint = Paint()..strokeWidth = step > 1.6 ? step - 0.8 : step;
    for (var i = 0; i < wave.buckets; i++) {
      final x = i * step + step / 2;
      paint.color = x <= headX ? played : unplayed;
      // Always at least a hairline: a bucket of pure silence should still show
      // that the recording continues through it.
      final top = mid - (wave.maxima[i] * mid).clamp(0.5, mid);
      final bottom = mid - (wave.minima[i] * mid).clamp(-mid, -0.5);
      canvas.drawLine(Offset(x, top), Offset(x, bottom), paint);
    }

    canvas.drawLine(
      Offset(headX, 0),
      Offset(headX, size.height),
      Paint()
        ..color = head
        ..strokeWidth = 2,
    );
  }

  @override
  bool shouldRepaint(_WavePainter old) =>
      old.progress != progress || !identical(old.wave, wave);
}
