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
    return Directory(
      '${base.path}${Platform.pathSeparator}mumbleway-recordings',
    );
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

  /// The glyph each platform draws for this.
  ///
  /// Android has its own: three nodes joined by two lines. Everywhere else —
  /// iOS, macOS and Windows alike — it is a box with an arrow leaving the top,
  /// which is what `ios_share` is despite the name. Using the wrong one is not
  /// wrong so much as foreign: it is a button people find by its shape rather
  /// than by reading it.
  IconData get _shareIcon => Platform.isAndroid ? Icons.share : Icons.ios_share;

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
    // Newest ride first, so what a cap leaves out is the oldest rather than
    // whatever happened to sort last. File names begin with the date, so
    // descending order is chronological without reading any of them.
    final files = dir.listSync().whereType<File>().toList()
      ..sort((a, b) => b.path.compareTo(a.path));
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
      // Cleared on the way in rather than on the way out, because they cannot
      // be deleted on the way out — see below. A share that produced four
      // archives followed by one that produces two would otherwise send the
      // two new ones and the two stale ones beside them.
      for (final f in temp.listSync().whereType<File>()) {
        // The stem rather than the numbered prefix, so the single
        // `mumbleway-recordings.zip` that earlier builds left here is cleared
        // too. It will never be shared again and nothing else would remove it.
        if (f.path
            .split(Platform.pathSeparator)
            .last
            .startsWith(_archiveStem)) {
          try {
            f.deleteSync();
          } catch (_) {
            // Still held open by a transfer that has not finished. It will be
            // overwritten by name if this share needs that number again.
          }
        }
      }

      // Off this isolate. Packing a ride's worth of audio takes seconds, and
      // doing it here would stop the meters and the spectrum in a panel whose
      // whole job is to be watched.
      final archives = await compute(_packRecordings, [
        '$_archiveCapBytes',
        temp.path,
        for (final f in files) f.path,
      ]);

      final result = await SharePlus.instance.share(
        ShareParams(
          files: [
            for (final a in archives) XFile(a, mimeType: 'application/zip'),
          ],
          subject: 'MumbleWay diagnostic recording',
        ),
      );
      // Deliberately not deleted here. `share` returns when the sheet closes,
      // and AirDrop and the mail composer go on reading the file after that —
      // deleting it now would truncate the transfer it was made for. The next
      // share clears it, and the system empties this directory anyway.

      if (!mounted) return;
      // Only after a target was actually chosen. A dismissed sheet must not
      // offer to delete what it did not send, and the offer is an action the
      // rider has to reach for rather than something that happens to them.
      if (result.status == ShareResultStatus.success) {
        messenger.showSnackBar(
          SnackBar(
            content: Text(l.diagRecordingShared(files.length, archives.length)),
            duration: const Duration(seconds: 8),
            action: SnackBarAction(label: l.delete, onPressed: _confirmDiscard),
          ),
        );
      }
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

  /// How large any one archive is allowed to get.
  ///
  /// Not arbitrary, and not the phone's limit — it is the smallest ceiling on
  /// the way to somewhere useful. Telegram refuses to hand a bot a file over
  /// 20 MB, which is the tightest of the transports these are actually sent
  /// over, and mail gateways are not far behind. Everything still goes; it
  /// goes as several files.
  ///
  /// Under rather than at the limit, because "20 MB" is not always 20 × 2^20
  /// on the far side.
  static const _archiveCapBytes = 18 * 1024 * 1024;

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
                            : _confirmDiscard,
                        child: Text(l.diagRecordingDiscard),
                      ),
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
                        icon: Icon(_shareIcon),
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
/// Top-level because the packing runs on another isolate, which cannot reach a
/// member of the widget that asked for it.
const _archiveStem = 'mumbleway-recordings';

/// [args] is the size ceiling, then the directory to write into, then the files
/// to pack, newest ride first. Returns the archives written.
///
/// **As many archives as it takes, none of them over the ceiling.** A single
/// capped archive would mean the oldest rides silently never leave the phone,
/// and the ones that have been sitting there longest are exactly the ones most
/// likely to hold the fault being chased.
///
/// Rides are kept whole and never straddle two archives. A `.s16` without the
/// `.csv` that describes it is not half a ride, it is a recording nobody can
/// say anything about.
///
/// It packs, measures, and starts a new archive when one would go over.
/// Compression on PCM varies with how much of the ride was speech — between
/// about five and twenty times on real recordings — so no ratio guessed in
/// advance is close enough to size a cap by. Measuring costs a repack;
/// guessing costs an archive that silently cannot be sent.
///
/// A single ride larger than the ceiling goes in an archive of its own and
/// exceeds it, because the alternative is a recording that can never be sent
/// at all and nothing on screen saying so.
List<String> _packRecordings(List<String> args) {
  final cap = int.parse(args.first);
  final into = args[1];
  final sep = Platform.pathSeparator;

  final rides = <String, List<String>>{};
  for (final path in args.skip(2)) {
    final name = path.split(sep).last;
    final dot = name.lastIndexOf('.');
    (rides[dot < 0 ? name : name.substring(0, dot)] ??= []).add(path);
  }

  String pathFor(int i) => '$into$sep$_archiveStem-$i.zip';

  void write(String out, List<String> stems) {
    final encoder = ZipFileEncoder();
    encoder.create(out);
    for (final stem in stems) {
      for (final path in rides[stem]!) {
        // Names inside the archive are the file names alone, which is what
        // `addFileSync` defaults to. The full path would carry the device's
        // own directory layout to whoever receives it.
        encoder.addFileSync(File(path));
      }
    }
    encoder.closeSync();
  }

  final archives = <String>[];
  var current = <String>[];
  var index = 1;

  for (final stem in rides.keys) {
    final trial = [...current, stem];
    write(pathFor(index), trial);
    if (File(pathFor(index)).lengthSync() > cap && current.isNotEmpty) {
      // Over. Put back what fit, close it, and begin the next one on this ride.
      write(pathFor(index), current);
      archives.add(pathFor(index));
      index += 1;
      current = [stem];
      write(pathFor(index), current);
    } else {
      current = trial;
    }
  }
  if (current.isNotEmpty) archives.add(pathFor(index));
  return archives;
}
