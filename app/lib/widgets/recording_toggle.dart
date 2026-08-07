import 'dart:async';
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:path_provider/path_provider.dart';
import 'package:share_plus/share_plus.dart';

import '../l10n/app_localizations.dart';
import '../src/rust/api/mumbleway.dart';

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
    final now = diagnosticRecordingState();
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

  Future<void> _setRecording(bool on) async {
    final l = L.of(context);
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
        startDiagnosticRecording(directory: dir.path, tag: tag);
      } else {
        // Returns what storage could not keep up with. Surfaced rather than
        // swallowed: a recording with gaps is still useful, and one with gaps
        // nobody knows about is a measurement waiting to be wrong.
        final dropped = stopDiagnosticRecording();
        if (dropped > BigInt.zero) {
          messenger.showSnackBar(
            SnackBar(content: Text(l.diagRecordingDropped(dropped.toInt()))),
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

  Future<void> _share() async {
    final dir = await _directory();
    if (!dir.existsSync()) return;
    final files = dir.listSync().whereType<File>().toList()
      ..sort((a, b) => a.path.compareTo(b.path));
    if (files.isEmpty) return;

    if (Platform.isAndroid || Platform.isIOS) {
      await SharePlus.instance.share(
        ShareParams(
          files: files.map((f) => XFile(f.path)).toList(),
          subject: 'MumbleWay diagnostic recording',
        ),
      );
    } else {
      // No share sheet worth the name on desktop, and a path that can be
      // pasted is more use than one that has to be retyped.
      await Clipboard.setData(ClipboardData(text: dir.path));
      if (!mounted) return;
      ScaffoldMessenger.of(
        context,
      ).showSnackBar(SnackBar(content: Text(dir.path)));
    }
  }

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

    return Card(
      margin: EdgeInsets.zero,
      color: scheme.surfaceContainerHigh,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          SwitchListTile(
            secondary: Icon(
              _state.active ? Icons.fiber_manual_record : Icons.mic_none,
              color: _state.active ? scheme.error : null,
            ),
            title: Text(l.diagRecording),
            subtitle: Text(l.diagRecordingBody),
            isThreeLine: true,
            value: _state.active,
            onChanged: _busy ? null : _setRecording,
          ),
          Padding(
            padding: const EdgeInsets.fromLTRB(16, 0, 16, 12),
            child: Row(
              children: [
                Expanded(
                  child: Text(
                    _files == 0
                        ? l.diagRecordingNone
                        : '${l.diagRecordingStopped(_files)} · '
                              '${l.diagRecordingSize(megabytes)}',
                    style: TextStyle(
                      fontSize: 12,
                      color: scheme.onSurfaceVariant,
                    ),
                  ),
                ),
                // Both actions touch the files, so neither is offered while a
                // writer is appending to them.
                TextButton(
                  onPressed: _files == 0 || _state.active ? null : _discard,
                  child: Text(l.diagRecordingDiscard),
                ),
                FilledButton.tonal(
                  onPressed: _files == 0 || _state.active ? null : _share,
                  child: Text(l.diagRecordingShare),
                ),
              ],
            ),
          ),
        ],
      ),
    );
  }
}
