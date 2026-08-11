import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:path_provider/path_provider.dart';

/// The folder share archives are built in.
///
/// Ours alone, and that is the point twice over: it can be emptied without
/// caring what else is in it, and it can be *opened* without showing the rider
/// the rest of their temporary directory. On Windows the second one is not a
/// nicety — see [revealFile], which cannot select a file there and can only
/// open the folder it is in.
Future<Directory> shareStagingDir() async {
  final temp = await getTemporaryDirectory();
  final dir = Directory('${temp.path}${Platform.pathSeparator}mumbleway-share');
  await dir.create(recursive: true);
  return dir;
}

/// Empties the staging folder.
///
/// On the way in rather than the way out, because the archives cannot be
/// deleted on the way out: `share` returns when the sheet closes and AirDrop
/// and mail composers go on reading the file after that. A share that produced
/// four archives followed by one that produces two would otherwise send the two
/// new ones and two stale ones beside them.
Future<void> clearStaging(Directory dir) async {
  for (final f in dir.listSync().whereType<File>()) {
    try {
      f.deleteSync();
    } catch (_) {
      // Still held open by a transfer that has not finished. It will be
      // overwritten by name if this share needs that number again.
    }
  }
}

/// Shows a file to the person using the computer, in their own file manager.
///
/// The fallback for a desktop whose system has no share sheet to offer, and the
/// thing that was missing: the desktop path used to copy the file's path to the
/// clipboard and print it in a snackbar, which is an instruction rather than an
/// action. Somebody who wants to send a recording still had to open a file
/// manager, paste a path, and find the file — for a feature whose entire
/// purpose is getting the recording off the device.
///
/// Returns false rather than throwing when there is nothing to ask, because the
/// caller has a message to show and this has nothing to add to it.
Future<bool> revealFile(String path) async {
  try {
    if (Platform.isWindows) {
      // **The folder, not the file, and this is measured rather than chosen.**
      // `explorer.exe /select,<path>` is the spelling that selects a file, and
      // it cannot be written from Dart: `Process` quotes any argument
      // containing a space, and Explorer answers a quoted `/select,…` by
      // opening Documents. Four spellings were tried against a path with a
      // space in it — the bare argument, the path quoted inside it,
      // `runInShell`, and through `cmd /c` — and they opened Documents,
      // Documents, Documents and the Desktop. Passing the directory as one
      // ordinary argument is the only form that lands anywhere near the file,
      // and it lands exactly on it.
      //
      // Which is why the archives get a folder of their own: opening it shows
      // what is about to be sent and nothing else, so nothing is lost by not
      // selecting.
      await Process.start('explorer.exe', [File(path).parent.path]);
      return true;
    }
    if (Platform.isMacOS) {
      // `-R` reveals in Finder instead of opening, which for a `.zip` is the
      // difference between showing the file and unpacking it. No quoting
      // trouble here: the path is an ordinary argument.
      final r = await Process.run('open', ['-R', path]);
      return r.exitCode == 0;
    }
    if (Platform.isLinux) {
      // No portable "reveal", so the containing folder is the closest thing.
      final r = await Process.run('xdg-open', [File(path).parent.path]);
      return r.exitCode == 0;
    }
  } catch (e) {
    debugPrint('reveal failed: $e');
  }
  return false;
}
