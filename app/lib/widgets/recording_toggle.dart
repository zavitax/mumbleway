import 'dart:async';
import 'dart:io';

import 'package:flutter/foundation.dart' show compute;
import 'package:flutter/material.dart';
import 'package:path_provider/path_provider.dart';
import 'package:share_plus/share_plus.dart';

import '../l10n/app_localizations.dart';
import '../services/file_reveal.dart';
import '../services/recording_archive.dart';
import '../src/rust/api/mumbleway.dart';
import '../state/app_state.dart';
import 'error_snack.dart';
import 'recording_preview.dart';

/// Turns on recording of the microphone and of what the chain decided about it.
///
/// This exists because of a specific failure. Every measurement of the noise
/// suppression was invalidated at once by discovering that the recordings
/// behind it had come from the phone's own microphone rather than the headset's
/// — audio carries no record of what captured it, so nothing in the analysis
/// could have caught it. Recording from inside the app makes the audio the
/// chain's own input by construction, and there is nothing left to be wrong
/// about.
///
/// **Off unless a rider turns it on, and it says what it does before they do.**
/// It writes a microphone to storage; the wording is deliberately plain rather
/// than reassuring, because someone who does not want that has to be able to
/// tell from the switch alone.
class RecordingToggle extends StatefulWidget {
  const RecordingToggle({super.key});

  @override
  State<RecordingToggle> createState() => _RecordingToggleState();
}

class _RecordingToggleState extends State<RecordingToggle> {
  Timer? _tick;
  UiRecordingState _state = UiRecordingState(
    active: false,
    droppedBlocks: BigInt.zero,
    directory: '',
  );

  /// What is on disk from this session and previous ones.
  int _files = 0;
  int _bytes = 0;
  bool _busy = false;

  /// Whether the status line and the two buttons are showing.
  ///
  /// Shut to begin with. This card sits under the analyser in a bottom sheet,
  /// and while a ride is happening the analyser is the thing being watched —
  /// none of what folds away is wanted until afterwards.
  bool _expanded = false;

  @override
  void initState() {
    super.initState();
    _refresh();
    // Only while this is on screen: the widget is built under the panel's
    // `diagnosticsOpen`, so closing the panel stops the polling without any
    // code here having to notice. The recording itself carries on.
    _tick = Timer.periodic(const Duration(seconds: 1), (_) => _refresh());
  }

  @override
  void dispose() {
    _tick?.cancel();
    super.dispose();
  }

  Future<void> _refresh() async {
    if (!mounted) return;
    var now = _state;
    try {
      now = diagnosticRecordingState();
    } catch (_) {
      // The engine is not up, so there is no recording to report on. The same
      // guard the rest of this panel uses: the diagnostics are meant to explain
      // a failure, and throwing on the way to describing one is the least
      // useful thing they could do.
    }
    final dir = await _directory();
    var files = 0;
    var bytes = 0;
    if (dir.existsSync()) {
      for (final f in dir.listSync().whereType<File>()) {
        files++;
        bytes += f.lengthSync();
      }
    }
    if (!mounted) return;
    setState(() {
      _state = now;
      _files = files;
      _bytes = bytes;
    });
  }

  /// Where recordings are actually going, once a start has proved a folder can
  /// be made. Null until then, and never set to somewhere that did not work.
  Directory? _proven;

  /// Where a rider can actually get at the files afterwards, best first.
  ///
  /// The answer differs per platform and getting it wrong is silent — the
  /// recording works and the files are simply unreachable, which looks like the
  /// feature failing.
  ///
  /// * **Android**: app-specific external storage. Visible over USB and to a
  ///   file manager, and needs no storage permission, which this has no other
  ///   use for.
  /// * **iOS**: the documents directory, which the Files app shows only because
  ///   `UIFileSharingEnabled` is set in `Info.plist`. Without that key the files
  ///   exist and nobody can reach them.
  /// * **Desktop**: documents, where a person can find them without being told.
  ///
  /// Windows carries a second and a third, because Documents there is often not
  /// a folder. See [_createDirectory].
  Future<List<Directory>> _bases() async {
    final bases = <Directory>[];
    if (Platform.isAndroid) {
      final external = await getExternalStorageDirectory();
      if (external != null) bases.add(external);
    }
    bases.add(await getApplicationDocumentsDirectory());
    if (Platform.isWindows) {
      // Still somewhere a person can open without being told, and outside the
      // redirection that breaks the first choice.
      final downloads = await getDownloadsDirectory();
      if (downloads != null) bases.add(downloads);
      // Last resort. Nobody will find this without being shown it, which is
      // why it is last — but it is on a local disk and it always exists, and a
      // recording nobody can find beats a recording that was never made.
      bases.add(await getApplicationSupportDirectory());
    }
    return bases;
  }

  Directory _folder(Directory base) =>
      Directory('${base.path}${Platform.pathSeparator}mumbleway-recordings');

  /// The folder to list.
  ///
  /// Wherever the last start proved it could write, or failing that the first
  /// candidate that is actually there — because a fallback taken in an earlier
  /// session left the files somewhere this one has not been told about, and a
  /// count of zero beside a folder full of recordings is the same wrong answer
  /// the fallback exists to prevent.
  Future<Directory> _directory() async {
    final proven = _proven;
    if (proven != null) return proven;
    final bases = await _bases();
    for (final base in bases) {
      final dir = _folder(base);
      if (await dir.exists()) return dir;
    }
    return _folder(bases.first);
  }

  /// Makes the folder, and checks it is really there.
  ///
  /// **`create` can succeed and create nothing.** On Windows the Documents
  /// folder is usually redirected into OneDrive — here it is
  /// `%USERPROFILE%\OneDrive\Documents` — and while OneDrive is not running
  /// that path is a placeholder rather than a directory. `CreateDirectoryW`
  /// against it returns success, `Directory.create(recursive: true)` therefore
  /// returns normally, and the folder does not exist afterwards: the next write
  /// fails with *the system cannot find the path*, which is the error the rider
  /// sees and which points at the wrong step. Catching the exception is no
  /// help, because there is no exception.
  ///
  /// So this asks whether the folder is there rather than whether making it
  /// threw, and moves down [_bases] until one answers yes.
  Future<Directory> _createDirectory() async {
    Object? firstFailure;
    final bases = await _bases();
    for (final base in bases) {
      final dir = _folder(base);
      try {
        await dir.create(recursive: true);
        if (await dir.exists()) {
          _proven = dir;
          return dir;
        }
        firstFailure ??= FileSystemException(
          'created without error and is not there',
          dir.path,
        );
      } on FileSystemException catch (e) {
        firstFailure ??= e;
      }
    }
    // Every candidate refused. Report the first, which is the one for the place
    // the recordings were meant to go.
    throw firstFailure ??
        FileSystemException('nowhere to record to', bases.first.path);
  }

  /// Both directions go through [AppState], which owns the audio hold.
  ///
  /// Calling the engine's recorder directly would start it with the devices
  /// shut: the capture worker feeds the recorder and does not run until they
  /// are open, so the result is an empty file that looks exactly like a ride
  /// nobody spoke on. On Android it would be worse than empty — without the
  /// session the hands-free link is never made and the audio would come from
  /// the phone's own microphone, which is the confusion this feature exists to
  /// end.
  Future<void> _setRecording(bool on) async {
    final l = L.of(context);
    final state = AppStateScope.of(context);
    final messenger = ScaffoldMessenger.of(context);
    setState(() => _busy = true);
    try {
      if (on) {
        final dir = await _createDirectory();
        // Names the files after when the ride was, because the alternative is
        // a directory of identically-named chunks whose order has to be
        // guessed from timestamps the sharing may not preserve.
        final now = DateTime.now();
        final tag =
            '${now.year}${_two(now.month)}${_two(now.day)}-'
            '${_two(now.hour)}${_two(now.minute)}';
        // Opens the microphone, and reports it rather than recording silence
        // if it cannot be had.
        final error = await state.beginDiagnosticRecording(dir.path, tag);
        if (error != null) {
          showError(messenger, l.diagRecordingFailed(error));
        }
      } else {
        // Returns what storage could not keep up with. Surfaced rather than
        // swallowed: a recording with gaps is still useful, and one with gaps
        // nobody knows about is a measurement waiting to be wrong.
        final dropped = state.endDiagnosticRecording();
        if (dropped > 0) {
          messenger.showSnackBar(
            SnackBar(content: Text(l.diagRecordingDropped(dropped))),
          );
        }
      }
    } catch (e) {
      showError(messenger, l.diagRecordingFailed('$e'));
    } finally {
      if (mounted) setState(() => _busy = false);
      await _refresh();
    }
  }

  static String _two(int v) => v.toString().padLeft(2, '0');


  /// Hands the recordings over as a single archive.
  ///
  /// A `.zip` rather than the files themselves, and that is a repair rather
  /// than tidiness. **iOS types every shared item by its file extension.**
  /// `.csv` it knows — `public.comma-separated-values-text` — but `.s16` it has
  /// never heard of, so it invents a *dynamic* identifier for it
  /// (`dyn.ah62d4rv4ge81gqm0`), and a share target that accepts only declared
  /// types quietly accepts none of it. The sheet opens, a target is picked, and
  /// nothing arrives. Nothing fails and nothing logs.
  ///
  /// Android was never affected, which is what made this look like a share
  /// sheet that was simply broken: Android matches on MIME type, where
  /// `application/octet-stream` is perfectly ordinary, so the same call works
  /// there and the fault only ever appears on a phone.
  ///
  /// `public.zip-archive` is a type every target knows. It also makes the pair
  /// one item, so audio cannot arrive without the decisions recorded alongside
  /// it — they are worth little apart — and raw PCM of a mostly silent ride
  /// compresses several times over, which matters when what is being sent is
  /// the length of a ride.
  /// Opens the preview sheet.
  ///
  /// Takes no audio hold. The engine's output is already running whenever the
  /// devices are open, and a preview is mixed into it the same way a cue is —
  /// so this neither needs the microphone nor should take it.
  Future<void> _listen() async {
    final dir = await _directory();
    if (!dir.existsSync() || !mounted) return;
    await showRecordingPreview(context, dir);
    // The sheet can delete recordings, so the count and the size above it are
    // stale the moment it closes. Unconditional rather than reported back:
    // a re-count is cheap, and a flag that says nothing changed is one more
    // thing that can be wrong.
    if (mounted) await _refresh();
  }

  Future<void> _share() async {
    final l = L.of(context);
    final messenger = ScaffoldMessenger.of(context);
    final dir = await _directory();
    if (!dir.existsSync()) return;
    // Newest ride first, so what a cap leaves out is the oldest rather than
    // whatever happened to sort last. File names begin with the date, so
    // descending order is chronological without reading any of them.
    final files = dir.listSync().whereType<File>().toList()
      ..sort((a, b) => b.path.compareTo(a.path));
    if (files.isEmpty) return;

    setState(() => _busy = true);
    try {
      final staging = await shareStagingDir();
      await clearStaging(staging);

      // Off this isolate. Packing a ride's worth of audio takes seconds, and
      // doing it here would stop the meters and the spectrum in a panel whose
      // whole job is to be watched.
      final archives = await compute(packRecordings, [
        '$archiveCapBytes',
        staging.path,
        for (final f in files) f.path,
      ]);

      final shared = SharePlus.instance.share(
        ShareParams(
          files: [
            for (final a in archives) XFile(a, mimeType: 'application/zip'),
          ],
          subject: 'MumbleWay diagnostic recording',
        ),
      );

      // **The spinner ends at the handoff.** See the same comment in
      // `recording_preview.dart`: on Android this future completes only when
      // the chooser returns an activity result, and a target that takes over
      // often never returns one, so waiting on it to clear the button leaves a
      // spinner on screen for the life of the process after a share that
      // worked.
      if (mounted) setState(() => _busy = false);
      ShareResult? result;
      try {
        result = await shared;
      } on UnimplementedError {
        // A desktop with no share sheet for files. Showing the archives is the
        // next best thing, and the staging folder holds nothing else — which
        // is the whole reason it exists.
        if (!await revealFile(archives.first) && mounted) {
          showError(messenger, l.diagRecordingShareFailed('$staging'));
        }
      }
      // Deliberately not deleted here. `share` returns when the sheet closes,
      // and AirDrop and the mail composer go on reading the file after that —
      // deleting it now would truncate the transfer it was made for. The next
      // share clears it, and the system empties this directory anyway.

      if (!mounted) return;
      // Only after a target was actually chosen. A dismissed sheet must not
      // offer to delete what it did not send, and the offer is an action the
      // rider has to reach for rather than something that happens to them. A
      // revealed folder is not a send either: nothing has left the machine yet.
      if (result?.status == ShareResultStatus.success) {
        messenger.showSnackBar(
          SnackBar(
            content: Text(l.diagRecordingShared(files.length, archives.length)),
            duration: const Duration(seconds: 8),
            action: SnackBarAction(label: l.delete, onPressed: _confirmDiscard),
          ),
        );
      }
    } catch (e) {
      if (mounted) showError(messenger, l.diagRecordingShareFailed('$e'));
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }


  /// Asks first, because the files are the only copy there will ever be.
  ///
  /// A ride cannot be recorded again. The wind, the speed, the headset and the
  /// fault being chased were all particular to it, and this button sits beside
  /// the one that sends them — a mis-tap is the difference between a
  /// measurement and starting the whole exercise over.
  ///
  /// [only] narrows it to particular files — what a share just sent, so the
  /// next archive carries the rides that did not fit rather than the same ones
  /// again. Everything, when it is not given.
  Future<void> _confirmDiscard({List<String>? only}) async {
    final l = L.of(context);
    final scheme = Theme.of(context).colorScheme;
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (dialog) => AlertDialog(
        title: Text(l.diagRecordingDiscardTitle),
        content: Text(l.diagRecordingDiscardBody(only?.length ?? _files)),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(dialog, false),
            child: Text(l.cancel),
          ),
          FilledButton(
            // Coloured as the destructive one so the two actions cannot be
            // told apart only by reading them, which is not what happens at
            // the roadside with gloves on.
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
    // Dismissed by tapping outside answers null, which is a no.
    if (confirmed == true) await _discard(only: only);
  }

  Future<void> _discard({List<String>? only}) async {
    final dir = await _directory();
    if (dir.existsSync()) {
      final wanted = only?.toSet();
      // Only the files this wrote, and only while nothing is recording — the
      // switch below is disabled while active, so there is no live writer whose
      // file could be deleted out from under it.
      for (final f in dir.listSync().whereType<File>()) {
        if (wanted != null && !wanted.contains(f.path)) continue;
        try {
          f.deleteSync();
        } catch (_) {
          // A file the system still holds open; the next attempt gets it.
        }
      }
    }
    await _refresh();
  }

  @override
  Widget build(BuildContext context) {
    final l = L.of(context);
    final scheme = Theme.of(context).colorScheme;
    final megabytes = (_bytes / (1024 * 1024)).toStringAsFixed(1);
    // The switch follows the side that owns the audio hold, not the engine's
    // own flag. They agree in every ordinary case; where they cannot, the hold
    // is the one that must be given back exactly once.
    final active = AppStateScope.of(context).diagnosticRecording;
    final dropped = _state.droppedBlocks.toInt();

    return Card(
      margin: EdgeInsets.zero,
      color: scheme.surfaceContainerHigh,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          ListTile(
            leading: Icon(
              active ? Icons.fiber_manual_record : Icons.mic_none,
              color: active ? scheme.error : null,
            ),
            title: Text(l.diagRecording),
            // One line, not the paragraph that used to be here. The panel is a
            // bottom sheet on a phone and that text pushed the spectrum, the
            // chain lights and both buttons below the fold, so the switch
            // explained itself at the cost of everything it was next to.
            //
            // Kept visible even when the rest is folded away, because it is the
            // half that had to stay: this writes a microphone to storage, and
            // someone who does not want that has to be able to tell from the
            // switch alone — not after finding and opening something.
            subtitle: Text(
              active ? l.diagRecordingActive : l.diagRecordingBody,
            ),
            trailing: Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                Switch(value: active, onChanged: _busy ? null : _setRecording),
                AnimatedRotation(
                  turns: _expanded ? 0.5 : 0,
                  duration: const Duration(milliseconds: 150),
                  child: const Icon(Icons.expand_more),
                ),
              ],
            ),
            // The row opens it; the switch records. Two controls in one tile,
            // which is what an ExpansionTile does everywhere else in Material —
            // and the switch is large, adjacent and the obvious thing to reach
            // for, so the ambiguity is smaller than it reads.
            onTap: () => setState(() => _expanded = !_expanded),
          ),
          // Stacked, not side by side. These were in one Row, where the two
          // buttons take their intrinsic width and the Expanded text gets
          // whatever is left -- which in a panel this narrow, with Russian
          // labels on both buttons, was a few pixels. The status line then
          // wrapped one character per line.
          //
          // Nothing about that is fixed by a smaller font or an ellipsis: the
          // text needs the full width, so it gets its own line.
          // Folded away by default. The panel is a bottom sheet, the analyser
          // above is the thing being watched, and none of this is needed until
          // a ride is over — at which point one tap brings it back.
          if (_expanded)
            Padding(
              padding: const EdgeInsets.fromLTRB(16, 0, 16, 12),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  Text(
                    // Losses come first while they are happening: a recording
                    // with gaps is still worth having, and one whose gaps are
                    // only discovered during analysis is a measurement waiting
                    // to be wrong.
                    dropped > 0
                        ? l.diagRecordingDropped(dropped)
                        : _files == 0
                        ? l.diagRecordingNone
                        : '${l.diagRecordingStopped(_files)} · '
                              '${l.diagRecordingSize(megabytes)}',
                    style: TextStyle(
                      fontSize: 12,
                      color: dropped > 0
                          ? scheme.error
                          : scheme.onSurfaceVariant,
                    ),
                  ),
                  const SizedBox(height: 8),
                  // Driven to opposite ends of the card, deliberately. These
                  // actions are not equivalents: one sends a file and the
                  // other destroys the only copy of a ride that cannot be
                  // recorded again. Side by side they sat a thumb's width
                  // apart — in gloves, on a phone clamped to a handlebar. The
                  // distance between them is the safety feature.
                  Row(
                    children: [
                      // Both actions touch the files, so neither is offered while
                      // a writer is appending to them.
                      //
                      // Small, and the colour of what it does. A glyph is a
                      // smaller target than a word, which is the right size
                      // for the button nobody should hit by accident.
                      IconButton(
                        onPressed: _files == 0 || active || _busy
                            ? null
                            : _confirmDiscard,
                        icon: const Icon(Icons.delete_outline),
                        iconSize: 20,
                        visualDensity: VisualDensity.compact,
                        color: scheme.error,
                        tooltip: l.diagRecordingDiscard,
                      ),
                      const Spacer(),
                      // Between the two, and nearer the send button than the
                      // delete one: listening is what you do on the way to
                      // sending, and the policy asks people to do it. An
                      // instruction nobody can act on is not a safeguard.
                      IconButton(
                        onPressed: _files == 0 || active || _busy
                            ? null
                            : _listen,
                        icon: const Icon(Icons.graphic_eq),
                        tooltip: l.diagRecordingListen,
                      ),
                      const SizedBox(width: 4),
                      // The glyph alone. It is the one control on this panel
                      // whose meaning a shape carries completely, and the label
                      // beside it was the widest thing in the row. The words are
                      // still there for anyone who needs them — as the tooltip,
                      // and as what a screen reader announces.
                      IconButton.filledTonal(
                        // Also off while an archive is being packed: that takes
                        // seconds on a long ride, and a second tap would start a
                        // second pack over the same files.
                        onPressed: _files == 0 || active || _busy
                            ? null
                            : _share,
                        icon: Icon(shareIcon),
                        tooltip: l.diagRecordingShare,
                      ),
                    ],
                  ),
                ],
              ),
            ),
        ],
      ),
    );
  }
}

/// Packs the recordings into one archive, on a background isolate.
///
/// [args] is the archive to write, followed by the files to put in it. A plain
/// list of strings because it has to cross an isolate boundary, and this is the
/// shape that needs nothing said about how to do that.
///
/// Top-level because [compute] can only carry a function that is: a method
/// would drag the widget across with it.
/// Numbered from one per share, so what arrives is in an obvious order and an
/// interrupted share leaves a fixed set of names rather than one per attempt.
///
