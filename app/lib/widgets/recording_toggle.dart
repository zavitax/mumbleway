import 'dart:async';
import 'dart:io';

import 'package:archive/archive_io.dart';
import 'package:flutter/foundation.dart' show compute;
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:path_provider/path_provider.dart';
import 'package:share_plus/share_plus.dart';

import '../l10n/app_localizations.dart';
import '../src/rust/api/mumbleway.dart';
import '../state/app_state.dart';

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

  /// Where a rider can actually get at the files afterwards.
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
  Future<Directory> _directory() async {
    Directory? base;
    if (Platform.isAndroid) {
      base = await getExternalStorageDirectory();
    }
    base ??= await getApplicationDocumentsDirectory();
    return Directory('${base.path}${Platform.pathSeparator}mumbleway-recordings');
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
        final dir = await _directory();
        await dir.create(recursive: true);
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
          messenger.showSnackBar(
            SnackBar(content: Text(l.diagRecordingFailed(error))),
          );
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
      messenger.showSnackBar(
        SnackBar(content: Text(l.diagRecordingFailed('$e'))),
      );
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
  Future<void> _share() async {
    final l = L.of(context);
    final messenger = ScaffoldMessenger.of(context);
    final dir = await _directory();
    if (!dir.existsSync()) return;
    final files = dir.listSync().whereType<File>().toList()
      ..sort((a, b) => a.path.compareTo(b.path));
    if (files.isEmpty) return;

    if (!Platform.isAndroid && !Platform.isIOS) {
      // No share sheet worth the name on desktop, and a path that can be
      // pasted is more use than one that has to be retyped.
      await Clipboard.setData(ClipboardData(text: dir.path));
      if (!mounted) return;
      messenger.showSnackBar(SnackBar(content: Text(dir.path)));
      return;
    }

    setState(() => _busy = true);
    try {
      final temp = await getTemporaryDirectory();
      final archive = '${temp.path}${Platform.pathSeparator}$_archiveName';
      // Cleared on the way in rather than on the way out, because it cannot be
      // deleted on the way out — see below.
      final previous = File(archive);
      if (previous.existsSync()) previous.deleteSync();

      // Off this isolate. Packing a ride's worth of audio takes seconds, and
      // doing it here would stop the meters and the spectrum in a panel whose
      // whole job is to be watched.
      await compute(_packRecordings, [archive, for (final f in files) f.path]);

      await SharePlus.instance.share(
        ShareParams(
          files: [XFile(archive, mimeType: 'application/zip')],
          subject: 'MumbleWay diagnostic recording',
        ),
      );
      // Deliberately not deleted here. `share` returns when the sheet closes,
      // and AirDrop and the mail composer go on reading the file after that —
      // deleting it now would truncate the transfer it was made for. The next
      // share clears it, and the system empties this directory anyway.
    } catch (e) {
      if (mounted) {
        messenger.showSnackBar(
          SnackBar(content: Text(l.diagRecordingShareFailed('$e'))),
        );
      }
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  /// One fixed name, so a share that was interrupted leaves one file behind
  /// rather than one per attempt.
  static const _archiveName = 'mumbleway-recordings.zip';

  Future<void> _discard() async {
    final dir = await _directory();
    if (dir.existsSync()) {
      // Only the files this wrote, and only while nothing is recording — the
      // switch below is disabled while active, so there is no live writer whose
      // file could be deleted out from under it.
      for (final f in dir.listSync().whereType<File>()) {
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
          SwitchListTile(
            secondary: Icon(
              active ? Icons.fiber_manual_record : Icons.mic_none,
              color: active ? scheme.error : null,
            ),
            title: Text(l.diagRecording),
            subtitle: Text(
              active ? l.diagRecordingActive : l.diagRecordingBody,
            ),
            isThreeLine: true,
            value: active,
            onChanged: _busy ? null : _setRecording,
          ),
          // Stacked, not side by side. These were in one Row, where the two
          // buttons take their intrinsic width and the Expanded text gets
          // whatever is left -- which in a panel this narrow, with Russian
          // labels on both buttons, was a few pixels. The status line then
          // wrapped one character per line.
          //
          // Nothing about that is fixed by a smaller font or an ellipsis: the
          // text needs the full width, so it gets its own line.
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
                    color: dropped > 0 ? scheme.error : scheme.onSurfaceVariant,
                  ),
                ),
                const SizedBox(height: 8),
                // Wrap rather than Row, so that two long labels on a narrow
                // phone go onto separate lines instead of overflowing.
                Wrap(
                  alignment: WrapAlignment.end,
                  spacing: 8,
                  runSpacing: 4,
                  children: [
                    // Both actions touch the files, so neither is offered while
                    // a writer is appending to them.
                    TextButton(
                      onPressed: _files == 0 || active || _busy
                          ? null
                          : _discard,
                      child: Text(l.diagRecordingDiscard),
                    ),
                    FilledButton.tonal(
                      // Also off while an archive is being packed: that takes
                      // seconds on a long ride, and a second tap would start a
                      // second pack over the same file.
                      onPressed: _files == 0 || active || _busy ? null : _share,
                      child: Text(l.diagRecordingShare),
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
void _packRecordings(List<String> args) {
  final encoder = ZipFileEncoder();
  encoder.create(args.first);
  for (final path in args.skip(1)) {
    // Names inside the archive are the file names alone, which is what
    // `addFileSync` defaults to. The full path would carry the device's own
    // directory layout to whoever receives it.
    encoder.addFileSync(File(path));
  }
  encoder.closeSync();
}
