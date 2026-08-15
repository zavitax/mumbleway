import 'dart:io';
import 'dart:math' as math;

import 'package:flutter/foundation.dart' show compute;
import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:share_plus/share_plus.dart';

import '../l10n/app_localizations.dart';
import '../services/file_reveal.dart';
import '../services/recording_archive.dart';
import '../services/recording_player.dart';
import '../state/app_state.dart';
import '../theme.dart';
import 'error_snack.dart';

/// The glyph each platform draws for sharing.
///
/// Android has its own: three nodes joined by two lines. Everywhere else —
/// iOS, macOS and Windows alike — it is a box with an arrow leaving the top,
/// which is what `ios_share` is despite the name. Using the wrong one is not
/// wrong so much as foreign: it is a button people find by its shape rather
/// than by reading it.
///
/// Defined once, here, and used by both the card and this sheet.
IconData get shareIcon => Platform.isAndroid ? Icons.share : Icons.ios_share;

/// Plays a diagnostic recording back, with a waveform and a playhead.
///
/// **A sheet rather than a section.** The privacy policy asks people to listen
/// to a recording before sending it, and an instruction nobody can act on is
/// not a safeguard. But the panel it belongs to is a bottom sheet on a phone
/// that already has an analyser, two rows of lights and four counter groups in
/// it — so this costs nothing at all until somebody asks for it, and goes away
/// again afterwards.
Future<void> showRecordingPreview(BuildContext context, Directory dir) {
  // Read here rather than inside the sheet. A modal route is a child of the
  // Navigator, not of the widget that opened it, so the scope this sits under
  // is not necessarily in the sheet's own ancestry.
  final state = AppStateScope.of(context);
  final width = MediaQuery.of(context).size.width;
  return showModalBottomSheet<void>(
    context: context,
    isScrollControlled: true,
    useSafeArea: true,
    showDragHandle: true,
    // **Material caps a bottom sheet at 640px and that is a phone's width.**
    // The cap is right on a tablet, where a sheet spanning a 1024px screen
    // would be a wall; it is wrong on a desktop window, where it left this one
    // as a narrow column in the middle of an empty screen with a waveform
    // squeezed into it. A waveform is the one thing here that is *made* of
    // horizontal room -- every extra pixel is more of the recording visible at
    // once -- so the sheet takes at least four fifths of what there is.
    //
    // The floor keeps small windows out of it: below 800px logical, four
    // fifths would be narrower than Material's own cap, so the cap wins. On a
    // phone both terms come out at the screen width and nothing changes.
    constraints: BoxConstraints(
      maxWidth: math.max(width * 0.8, math.min(width, 640)),
    ),
    builder: (_) => _PreviewSheet(dir: dir, state: state),
  );
}

class _PreviewSheet extends StatefulWidget {
  const _PreviewSheet({required this.dir, required this.state});
  final Directory dir;
  final AppState state;

  @override
  State<_PreviewSheet> createState() => _PreviewSheetState();
}

class _PreviewSheetState extends State<_PreviewSheet> {
  final _player = RecordingPlayer();
  List<File> _files = const [];
  String? _selected;
  Waveform? _wave;
  bool _loading = false;
  bool _sharing = false;
  bool _holdingAudio = false;
  Future<void>? _pendingLoad;

  @override
  void initState() {
    super.initState();
    // Give the devices back the moment playback stops.
    _player.addListener(_releaseWhenIdle);
    _files = _scan();
    if (_files.isNotEmpty) _load(_files.first.path);
  }

  /// Audio only. The `.csv` beside each one is the decision log and there is
  /// nothing to listen to in it. Newest first — the names begin with the date,
  /// so descending order is chronological without reading any of them.
  List<File> _scan() =>
      widget.dir
          .listSync()
          .whereType<File>()
          .where((f) => f.path.toLowerCase().endsWith('.s16'))
          .toList()
        ..sort((a, b) => b.path.compareTo(a.path));

  static String _stem(String path) {
    final name = path.split(Platform.pathSeparator).last;
    return name.substring(0, name.lastIndexOf('.'));
  }

  @override
  void dispose() {
    _player.removeListener(_releaseWhenIdle);
    _player.dispose();
    _releaseAudio();
    super.dispose();
  }

  /// Hands the devices back as soon as nothing is playing.
  ///
  /// The hold used to last as long as the sheet was open, on the reasoning
  /// that play/pause should not open and shut the devices repeatedly. That is
  /// true and it is not worth what it costs: **iOS lights the recording
  /// indicator for the whole time**, so a rider who played a clip and stopped
  /// was left looking at a system-wide light saying the microphone was live
  /// while nothing was happening. On a device where the indicator is the
  /// user's only evidence about the microphone, it has to mean what it says.
  ///
  /// Reaching for the devices again on the next play costs a fraction of a
  /// second and is invisible; a recording light that is lit for no reason is
  /// not.
  void _releaseWhenIdle() {
    if (!_player.playing) _releaseAudio();
  }

  void _releaseAudio() {
    if (!_holdingAudio) return;
    _holdingAudio = false;
    widget.state.releaseAudio();
  }

  /// Opens the devices before the first play, and keeps them open.
  ///
  /// **Playback goes through the engine**, mixed into its output beside the
  /// cues — which means it needs an output callback running to drain the
  /// queue, and there is no such callback unless something is holding the
  /// devices open. A call or a recording does that; a rider who opens the
  /// panel to listen to yesterday's ride does not. Pressing play then moved
  /// nothing and made no sound, because the samples were being pushed into a
  /// queue nobody was reading.
  ///
  /// Taken on the first play rather than when the sheet opens, so looking at a
  /// waveform does not switch the microphone on. Released when the sheet
  /// closes rather than on pause, so play/pause does not open and shut the
  /// devices repeatedly.
  Future<bool> _ensureAudio() async {
    if (_holdingAudio) return true;
    final error = await widget.state.holdAudio();
    if (error != null) {
      if (mounted) showError(ScaffoldMessenger.of(context), error);
      return false;
    }
    _holdingAudio = true;
    return true;
  }

  Future<void> _load(String path) {
    setState(() {
      _selected = path;
      _wave = null;
      _loading = true;
    });
    // Held so a delete can wait for it. Opening the file is real I/O, and a
    // delete that arrives while it is still in flight would unlink a file this
    // is about to take a handle on — see [_delete].
    return _pendingLoad = _open(path);
  }

  Future<void> _open(String path) async {
    await _player.open(path);
    final wave = await _player.waveform();
    if (!mounted) return;
    setState(() {
      _wave = wave;
      _loading = false;
    });
  }

  /// Sends the one ride on screen, rather than everything on the device.
  ///
  /// The card's button is all-or-nothing, which is right when the answer is
  /// "here is everything I have" and wrong once somebody has listened through
  /// four rides and found the one with the fault in it. Sending that one is
  /// both smaller and more use to whoever receives it.
  ///
  /// Still a `.zip`, for a ride of one. **iOS cannot share a `.s16` at all** —
  /// it has no declared type for the extension, so it invents a dynamic one
  /// (`dyn.…`) and every target that filters on declared types silently
  /// accepts none of it: the sheet opens, a target is picked, nothing arrives.
  /// The archive also keeps the audio and its decision log together, which is
  /// the same reason the card zips.
  Future<void> _share() async {
    final path = _selected;
    if (path == null) return;
    final l = L.of(context);
    final messenger = ScaffoldMessenger.of(context);
    final stem = path.substring(0, path.lastIndexOf('.'));
    final files = [
      for (final p in [path, '$stem.csv'])
        if (File(p).existsSync()) p,
    ];

    setState(() => _sharing = true);
    try {
      final staging = await shareStagingDir();
      await clearStaging(staging);
      final archives = await compute(packRecordings, [
        '$archiveCapBytes',
        staging.path,
        ...files,
      ]);
      final shared = SharePlus.instance.share(
        ShareParams(
          files: [
            for (final a in archives) XFile(a, mimeType: 'application/zip'),
          ],
          subject: 'MumbleWay diagnostic recording ${_stem(path)}',
        ),
      );

      // **The spinner ends here, not when that future does.**
      //
      // It covers the packing above, which is this app's work and takes
      // seconds. What follows is another app's: on Android `share` completes
      // only when the chooser returns an activity result, and a target that
      // takes over — Telegram, mail — often never returns one. The rider sends
      // the file, comes back through recents, and no result is ever delivered.
      // The future stays pending for the life of the process and the button
      // stays a spinner, having successfully shared the recording.
      //
      // Awaited below all the same, so a real failure still reaches the
      // snackbar; it simply no longer holds the button hostage.
      if (mounted) setState(() => _sharing = false);
      try {
        await shared;
      } on UnimplementedError {
        // A desktop with no share sheet for files — Linux, or a Windows older
        // than the one that brought the system share UI. Showing the archive
        // is the next best thing and is what somebody came here to do anyway.
        if (!await revealFile(archives.first) && mounted) {
          showError(messenger, l.diagRecordingShareFailed('$staging'));
        }
      }
      // Not deleted afterwards, for the reason the card records: `share`
      // returns when the sheet closes, and AirDrop and the mail composer go on
      // reading the file after that.
    } catch (e) {
      if (mounted) {
        showError(messenger, l.diagRecordingShareFailed('$e'));
      }
    } finally {
      if (mounted) setState(() => _sharing = false);
    }
  }

  /// Asks before removing one ride, for the same reason the card asks before
  /// removing all of them: it cannot be recorded again.
  ///
  /// Here it matters more rather than less. Someone deletes from *this* sheet
  /// having just listened to what is in the file, which is exactly when a
  /// recording gets deleted on purpose — and exactly when the wrong one is
  /// deleted by a thumb, since the button sits beside a transport control that
  /// is pressed repeatedly.
  Future<void> _confirmDelete() async {
    final path = _selected;
    if (path == null) return;
    final l = L.of(context);
    final scheme = Theme.of(context).colorScheme;
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (dialog) => AlertDialog(
        title: Text(l.diagPreviewDeleteTitle),
        // Named, because the sheet holds several and the one on screen is the
        // one being destroyed. "Delete this recording?" over a list is a
        // question about whichever one the reader last looked at.
        content: Text(l.diagPreviewDeleteBody(_stem(path))),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(dialog, false),
            child: Text(l.cancel),
          ),
          FilledButton(
            style: FilledButton.styleFrom(
              backgroundColor: scheme.error,
              foregroundColor: scheme.onError,
            ),
            onPressed: () => Navigator.pop(dialog, true),
            child: Text(l.delete),
          ),
        ],
      ),
    );
    if (confirmed == true) await _delete(path);
  }

  Future<void> _delete(String path) async {
    // Wait for any open still in flight before closing anything.
    //
    // This is not defensive. `_load` is started from `initState` and not
    // awaited, so on a slow read the sheet is on screen and usable while the
    // file is still being opened — and the delete button is one tap away.
    // Deleting then found `_handle` still null, closed nothing, and the open
    // took its handle immediately afterwards on a file being unlinked.
    await _pendingLoad;

    // Now the handle can actually be closed. Windows refuses to unlink an open
    // file outright; a POSIX unlink succeeds and keeps the bytes alive behind
    // the closed name, so the space is not returned until the sheet is
    // disposed — which looks like a delete that freed nothing. `stop()` closes
    // it and clears what was queued in the engine, so nothing carries on
    // playing out of a file that is gone.
    await _player.stop();
    _releaseAudio();

    // Audio first, and the log only once the audio has actually gone.
    //
    // The pair is the unit: audio without its decision log is a recording
    // nobody can say anything about. Deleting them in the other order, or
    // blindly, produced exactly the state the rule forbids — a refused unlink
    // on the `.s16` left the ride audible and unreadable, and the failure was
    // swallowed, so nothing on screen said so.
    final stem = path.substring(0, path.lastIndexOf('.'));
    var gone = false;
    try {
      final audio = File(path);
      if (audio.existsSync()) audio.deleteSync();
      gone = !audio.existsSync();
    } catch (_) {
      gone = false;
    }

    if (!gone) {
      if (mounted) {
        showError(
          ScaffoldMessenger.of(context),
          L.of(context).diagPreviewDeleteFailed,
        );
      }
      return;
    }

    try {
      final log = File('$stem.csv');
      if (log.existsSync()) log.deleteSync();
    } catch (_) {
      // A log without its audio is harmless — it is numbers describing a ride
      // that no longer exists here, and the card's delete-all clears it.
    }

    if (!mounted) return;
    final was = _files.indexWhere((f) => f.path == path);
    final remaining = _scan();
    setState(() {
      _files = remaining;
      _selected = null;
      _wave = null;
    });
    if (remaining.isEmpty) return;
    // Whatever took its place in the list, rather than jumping back to the
    // newest: deleting three in a row should walk down the list, not bounce to
    // the top between each one.
    await _load(remaining[was.clamp(0, remaining.length - 1)].path);
  }

  /// `m:ss.mmm`.
  ///
  /// Milliseconds because this is a diagnostic panel: the thing being looked
  /// for is a gate closing mid-word, which happens inside a second and cannot
  /// be pointed at with a clock that only counts them. Tabular figures below
  /// keep it from twitching as the digits change.
  static String _clock(Duration d) {
    final m = d.inMinutes;
    final s = d.inSeconds % 60;
    final ms = d.inMilliseconds % 1000;
    return '$m:${s.toString().padLeft(2, '0')}'
        '.${ms.toString().padLeft(3, '0')}';
  }

  /// `m:ss`, for the length.
  ///
  /// The two halves of the readout are read for different reasons, so they are
  /// not shown to the same precision. The position is a pointer at a moment
  /// and needs the milliseconds; the length is context and never changes. On a
  /// narrow phone the four characters this gives back are the difference
  /// between the row fitting another control and not.
  static String _length(Duration d) =>
      '${d.inMinutes}:${(d.inSeconds % 60).toString().padLeft(2, '0')}';

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
            // A close button, and not only the drag handle above it.
            //
            // A sheet closes by being swiped down or by the back gesture, and
            // neither is on screen — the handle is a grip, not a control that
            // says what it does. The diagnostics panel this sheet is opened
            // from has an explicit ✕ in its header, so the same glyph in the
            // same corner means the same thing in both places, which is the
            // whole argument for putting it here rather than a nicer one.
            Row(
              children: [
                Expanded(
                  child: Text(
                    l.diagPreviewTitle,
                    style: Theme.of(context).textTheme.titleMedium,
                  ),
                ),
                IconButton(
                  tooltip: l.diagClose,
                  icon: const Icon(Icons.close, size: 20),
                  // `maybePop`, so this goes through whatever the route would
                  // consult on a back gesture rather than round it. Closing by
                  // the button and closing by swiping down should be the same
                  // act — the hold on the devices is given back in `dispose`
                  // either way, so neither can leave the microphone open.
                  onPressed: () => Navigator.maybePop(context),
                ),
              ],
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
                    return ChoiceChip(
                      label: Text(
                        _stem(path),
                        style: const TextStyle(fontSize: 12),
                      ),
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
                    // Why there is no green, when there is none.
                    //
                    // **A waveform with nothing green in it reads as a fault
                    // in the drawing**, and it took a device, a decision log
                    // and an afternoon to establish that it was not one: the
                    // ride had `speaking` at 64.9% and `transmitting` at
                    // exactly zero, because the chain had been told not to
                    // send. Nothing on screen said so. This is one line and it
                    // replaces that whole investigation.
                    if (!_loading && _player.nothingSent != NothingSent.some)
                      Padding(
                        padding: const EdgeInsets.only(top: 6),
                        child: Row(
                          crossAxisAlignment: CrossAxisAlignment.start,
                          children: [
                            Icon(
                              Icons.info_outline,
                              size: 14,
                              color: scheme.onSurfaceVariant,
                            ),
                            const SizedBox(width: 6),
                            Expanded(
                              child: Text(
                                switch (_player.nothingSent) {
                                  NothingSent.muted => l.diagPreviewNoneMuted,
                                  NothingSent.pushToTalk =>
                                    l.diagPreviewNonePushToTalk,
                                  _ => l.diagPreviewNoneUnexplained,
                                },
                                style: TextStyle(
                                  fontSize: 11,
                                  color: scheme.onSurfaceVariant,
                                ),
                              ),
                            ),
                          ],
                        ),
                      ),
                    const SizedBox(height: 6),
                    Row(
                      children: [
                        IconButton.filled(
                          onPressed: _loading
                              ? null
                              : () async {
                                  if (_player.playing) {
                                    _player.pause();
                                    return;
                                  }
                                  if (await _ensureAudio()) _player.play();
                                },
                          icon: Icon(
                            _player.playing ? Icons.pause : Icons.play_arrow,
                          ),
                          tooltip: _player.playing
                              ? l.diagPreviewPause
                              : l.diagPreviewPlay,
                        ),
                        const SizedBox(width: 4),
                        // Beside play, because it modifies what play does.
                        //
                        // **Green, and the same green as the waveform above
                        // it.** That is not decoration: the bars this plays
                        // are exactly the bars drawn in that colour, so the
                        // button says which parts of the picture it means
                        // without a word of explanation. Outlined when it is
                        // off, filled when it is on, so the state reads at a
                        // glance on a phone held at arm's length.
                        IconButton(
                          onPressed: _player.canSkipSilence
                              ? () =>
                                    _player.setSpeechOnly(!_player.speechOnly)
                              : null,
                          icon: Icon(
                            _player.speechOnly
                                ? Icons.record_voice_over
                                : Icons.record_voice_over_outlined,
                          ),
                          iconSize: 20,
                          color: _player.speechOnly
                              ? StatusColors.connected
                              : null,
                          tooltip: !_player.canSkipSilence
                              ? l.diagPreviewSentOnlyNone
                              : _player.speechOnly
                              ? l.diagPreviewSentOnlyOff
                              : l.diagPreviewSentOnly,
                        ),
                        // And what it sounded like, beside which parts went.
                        //
                        // Two separate questions, deliberately: the green
                        // control answers *which stretches* from the decision
                        // log — what the chain decided on the day — and this
                        // one answers *what they sounded like*, by running the
                        // audio through a chain now. Turn both on and what is
                        // left is what the far end got.
                        //
                        // Amber rather than green, because it is not a claim
                        // about the wire. Green means "this went out"; this
                        // means "this is the treatment it had".
                        IconButton(
                          onPressed: () =>
                              _player.setThroughChain(!_player.throughChain),
                          icon: Icon(
                            _player.throughChain
                                ? Icons.graphic_eq
                                : Icons.graphic_eq_outlined,
                          ),
                          iconSize: 20,
                          color: _player.throughChain
                              ? StatusColors.connecting
                              : null,
                          tooltip: _player.throughChain
                              ? l.diagPreviewChainOff
                              : l.diagPreviewChain,
                        ),
                        // Delete beside the transport control, share at the far
                        // end. That is the card's rule and the reason is the
                        // same: the button that destroys the only copy and the
                        // button that sends it are kept as far apart as the row
                        // allows, so a thumb cannot mean one and hit the other.
                        // Of the two, only delete asks first — it is the one
                        // that cannot be taken back.
                        //
                        // Not gated on `_loading`. That flag means the waveform
                        // is still being scanned, which is a fact about the
                        // picture; the file is known either way, and the delete
                        // stops the player before it touches it.
                        IconButton(
                          onPressed: _confirmDelete,
                          icon: const Icon(Icons.delete_outline),
                          iconSize: 20,
                          color: scheme.error,
                          tooltip: l.diagPreviewDelete,
                        ),
                        // In the gap between the two groups, so it is
                        // genuinely centred rather than drifting with the play
                        // button — and it keeps delete and share as far apart
                        // as the row allows, which is the rule above.
                        Expanded(
                          child: Center(
                            child: Text(
                              '${_clock(_player.position)} / '
                              '${_length(_player.duration)}',
                              style: TextStyle(
                                fontFeatures: const [
                                  FontFeature.tabularFigures()
                                ],
                                color: scheme.onSurfaceVariant,
                              ),
                            ),
                          ),
                        ),
                        IconButton.filledTonal(
                          onPressed: _sharing ? null : _share,
                          icon: _sharing
                              ? const SizedBox(
                                  width: 18,
                                  height: 18,
                                  child: CircularProgressIndicator(
                                    strokeWidth: 2,
                                  ),
                                )
                              : Icon(shareIcon),
                          tooltip: l.diagPreviewShare,
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
class _Scrubber extends StatefulWidget {
  const _Scrubber({
    required this.wave,
    required this.progress,
    required this.onSeek,
  });

  final Waveform wave;
  final double progress;
  final ValueChanged<double> onSeek;

  @override
  State<_Scrubber> createState() => _ScrubberState();
}

class _ScrubberState extends State<_Scrubber> {
  /// 1 shows the whole recording; 64 shows a sixty-fourth of it.
  double _zoom = 1;

  /// Fraction of the recording at the left edge.
  double _left = 0;

  double _zoomAtGestureStart = 1;

  static const double _maxZoom = 64;

  double get _span => 1 / _zoom;

  void _clamp() {
    _zoom = _zoom.clamp(1.0, _maxZoom);
    _left = _left.clamp(0.0, (1 - _span).clamp(0.0, 1.0));
  }

  /// Keeps the playhead on screen as it moves.
  ///
  /// Only when the playhead itself moved. Doing it on every rebuild would drag
  /// the view back under a listener who had just scrolled somewhere to look at
  /// it, which is the opposite of what zoom is for.
  @override
  void didUpdateWidget(_Scrubber old) {
    super.didUpdateWidget(old);
    if (old.progress != widget.progress) _follow();
  }

  void _follow() {
    if (_zoom <= 1) return;
    final p = widget.progress;
    if (p >= _left && p <= _left + _span) return;
    setState(() {
      _left = p - _span / 2;
      _clamp();
    });
  }

  /// Zooms about a point, keeping whatever is under it under it.
  void _zoomBy(double factor, double focalFraction) {
    setState(() {
      final before = _left + focalFraction * _span;
      _zoom = (_zoom * factor).clamp(1.0, _maxZoom);
      _left = before - focalFraction * _span;
      _clamp();
    });
  }

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    return LayoutBuilder(
      builder: (context, box) {
        final w = box.maxWidth;
        double fractionAt(double dx) =>
            (_left + (dx / w).clamp(0.0, 1.0) * _span).clamp(0.0, 1.0);
        void seek(Offset local) => widget.onSeek(fractionAt(local.dx));

        return Listener(
          // Desktop. A wheel with a modifier held is the zoom gesture Windows
          // and Linux users already have everywhere; a trackpad pinch arrives
          // as its own event on macOS and needs no modifier.
          onPointerSignal: (event) {
            if (event is PointerScrollEvent) {
              final keys = HardwareKeyboard.instance;
              if (keys.isControlPressed || keys.isMetaPressed) {
                _zoomBy(event.scrollDelta.dy > 0 ? 0.85 : 1.18,
                    (event.localPosition.dx / w).clamp(0.0, 1.0));
              } else if (_zoom > 1) {
                // Unmodified, it pans, which is the only thing left for it to
                // mean once the view is narrower than the recording.
                setState(() {
                  _left += event.scrollDelta.dy / w * _span;
                  _clamp();
                });
              }
            } else if (event is PointerScaleEvent) {
              _zoomBy(event.scale,
                  (event.localPosition.dx / w).clamp(0.0, 1.0));
            }
          },
          child: GestureDetector(
            behavior: HitTestBehavior.opaque,
            onTapDown: (d) => seek(d.localPosition),
            // Scale rather than a drag recogniser, because the two conflict and
            // scale can do both: one finger is a scrub, two are a pinch.
            onScaleStart: (d) {
              _zoomAtGestureStart = _zoom;
              if (d.pointerCount < 2) seek(d.localFocalPoint);
            },
            onScaleUpdate: (d) {
              if (d.pointerCount >= 2) {
                setState(() {
                  final f = (d.localFocalPoint.dx / w).clamp(0.0, 1.0);
                  final before = _left + f * _span;
                  _zoom = (_zoomAtGestureStart * d.scale).clamp(1.0, _maxZoom);
                  _left = before - f * _span - d.focalPointDelta.dx / w * _span;
                  _clamp();
                });
              } else {
                seek(d.localFocalPoint);
              }
            },
            child: CustomPaint(
              painter: _WavePainter(
                wave: widget.wave,
                progress: widget.progress,
                left: _left,
                span: _span,
                // Layer one is the microphone, and it is deliberately the
                // palest thing here: it is context for the two above it rather
                // than the answer to anything. Grey rather than the accent it
                // used to be, so that green means one thing on this drawing.
                played: scheme.onSurfaceVariant,
                unplayed: scheme.outlineVariant,
                sent: StatusColors.connected,
                head: scheme.error,
                // Layer two, darker than layer one. Where the chain removed
                // nothing the two coincide and read as one band; where it
                // removed a lot, the pale raw trace stands out around a dark
                // core, and that gap is the suppressor's work made visible.
                processedColour: scheme.onSurface,
                processedAhead: scheme.outline,
              ),
              size: Size.infinite,
            ),
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
    required this.left,
    required this.span,
    required this.played,
    required this.unplayed,
    required this.sent,
    required this.head,
    required this.processedColour,
    required this.processedAhead,
  });

  final Waveform wave;
  final double progress, left, span;
  final Color played, unplayed, sent, head;

  /// The middle layer: what survived processing, before the gate.
  ///
  /// Darker than the raw trace under it rather than lighter, so that where the
  /// chain removed nothing the two coincide and read as one solid band, and
  /// where it removed a lot the pale raw trace stands out around a dark core.
  final Color processedColour, processedAhead;

  @override
  void paint(Canvas canvas, Size size) {
    if (wave.isEmpty) return;
    final mid = size.height / 2;
    double xOf(double fraction) => (fraction - left) / span * size.width;
    final headX = xOf(progress);

    // A hairline at zero, so a stretch the gate closed reads as silence rather
    // than as a gap in the drawing.
    canvas.drawLine(
      Offset(0, mid),
      Offset(size.width, mid),
      Paint()
        ..color = unplayed
        ..strokeWidth = 1,
    );

    // Only the buckets in view, so zooming in costs less work rather than more.
    final first = (left * wave.buckets).floor().clamp(0, wave.buckets - 1);
    final last = ((left + span) * wave.buckets).ceil().clamp(0, wave.buckets);
    final step = size.width / (wave.buckets * span);
    final paint = Paint()..strokeWidth = step > 1.6 ? step - 0.8 : step;

    // **Three layers, drawn back to front, because the interesting thing is
    // the distance between them.** What the microphone heard, what the chain
    // left of it, and which of that the chain would have sent. Any one alone
    // answers "how loud was I"; the three together answer "what did the
    // suppressor and the gate actually do to me", which is the question a
    // diagnostic recording exists for.
    //
    // The two upper layers are envelopes rather than waveforms — see
    // `Waveform.processed`. The recorder keeps one audio stream, so their
    // height comes from the level the chain measured per 10 ms block. They are
    // drawn symmetrically about the centre line for that reason: an envelope
    // has a magnitude and no sign, and drawing one as if it had a waveform's
    // asymmetry would be inventing detail that was never recorded.
    for (var layer = 0; layer < 3; layer++) {
      for (var i = first; i < last; i++) {
        final x = xOf(i / wave.buckets) + step / 2;
        final ahead = x > headX;
        double top, bottom;

        if (layer == 0) {
          // 1. The microphone, as recorded. The only true waveform here, so it
          // keeps its asymmetry.
          paint.color = (ahead ? unplayed : played).withValues(alpha: 0.5);
          top = mid - (wave.maxima[i] * mid).clamp(0.5, mid);
          bottom = mid - (wave.minima[i] * mid).clamp(-mid, -0.5);
        } else {
          final amp = wave.processedAt(i);
          // No log, no layer. A flat line here would claim the chain removed
          // everything, which is a much stronger statement than "not recorded".
          if (amp == null) continue;
          if (layer == 2 && !wave.wouldSendAt(i)) continue;
          paint.color = layer == 1
              ? (ahead ? processedAhead : processedColour).withValues(alpha: 0.5)
              : (ahead ? sent.withValues(alpha: 0.4) : sent.withValues(alpha: 0.75));
          final h = (amp * mid).clamp(0.5, mid);
          top = mid - h;
          bottom = mid + h;
        }
        canvas.drawLine(Offset(x, top), Offset(x, bottom), paint);
      }
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
      old.progress != progress ||
      old.left != left ||
      old.span != span ||
      !identical(old.wave, wave);
}
