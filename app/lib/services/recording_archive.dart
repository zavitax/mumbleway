import 'dart:io';

import 'package:archive/archive_io.dart';

/// Packing diagnostic recordings into archives fit to send.
///
/// Lives here rather than beside either button because both of them need it:
/// the card shares every ride at once, and the listen sheet shares the one
/// ride somebody has just played. They have to agree on the format, the
/// ceiling and the naming — two copies of a zip writer would drift, and the
/// one that drifted would produce an archive the intake tool silently could
/// not read.
///
/// Everything here is top-level so it can run under [compute]: the packing
/// takes seconds on a ride's worth of audio, and doing it on the UI isolate
/// would stop the meters and the spectrum in the panel it was started from.

/// How large any one archive is allowed to get.
///
/// Not arbitrary, and not the phone's limit — it is the smallest ceiling on
/// the way to somewhere useful. Telegram refuses to hand a bot a file over
/// 20 MB, which is the tightest of the transports these are actually sent
/// over, and mail gateways are not far behind. Everything still goes; it goes
/// as several files.
///
/// Under rather than at the limit, because "20 MB" is not always 20 x 2^20 on
/// the far side.
const archiveCapBytes = 18 * 1024 * 1024;

/// The prefix every archive this app produces is named with.
///
/// Also what a stale-archive sweep matches on, so it must stay a prefix of
/// the names [packRecordings] writes.
const archiveStem = 'mumbleway-recordings';

/// The suffixes a recorder segment writes, beyond the `.s16` itself.
///
/// **One list, because there used to be two and one of them was wrong.** The
/// card shared everything in the directory and so carried the motion track by
/// accident; the listen sheet built its own list of `[audio, '$stem.csv']` and
/// silently dropped it. A rider sent a ride to debug the tap gesture and it
/// arrived with no motion in it at all — the one track the question needed, and
/// nothing on either end said it was missing.
///
/// A new track goes here and both share paths pick it up.
const rideSuffixes = ['.csv', '.motion.csv'];

/// The ride a recorder file belongs to, as the segment stem both share paths
/// and the intake tool group by.
///
/// **Not `lastIndexOf('.')`.** `20261009-1054-000.motion.csv` has two
/// extensions, so the naive stem is `…-000.motion` — a different ride from
/// `…-000`, which is how the motion track came to be a "ride" of its own that
/// the cap could put in a *different archive* from the audio it describes.
/// That breaks the one guarantee [packRecordings] makes about not splitting a
/// ride, and it breaks it for the track that is useless without the other two.
/// **Longest suffix first, or `.motion.csv` is matched as `.csv`** and the stem
/// comes back as `…-000.motion` — which is the very fault this function exists
/// to prevent, arrived at from the other direction. Sorted rather than relying
/// on the order of [rideSuffixes], so adding a track cannot reintroduce it.
String rideStem(String fileName) {
  final longestFirst = [...rideSuffixes]
    ..sort((a, b) => b.length.compareTo(a.length));
  for (final suffix in longestFirst) {
    if (fileName.endsWith(suffix)) {
      return fileName.substring(0, fileName.length - suffix.length);
    }
  }
  final dot = fileName.lastIndexOf('.');
  return dot < 0 ? fileName : fileName.substring(0, dot);
}

/// Every file of the ride whose audio is at [audioPath], the ones that exist.
///
/// Used by the single-ride share so it cannot disagree with the bulk one about
/// what a ride is made of.
List<String> rideFiles(String audioPath, {bool Function(String)? exists}) {
  final present = exists ?? (p) => File(p).existsSync();
  final stem = rideStem(audioPath);
  return [
    for (final p in [audioPath, for (final s in rideSuffixes) '$stem$s'])
      if (present(p)) p,
  ];
}

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
List<String> packRecordings(List<String> args) {
  final cap = int.parse(args.first);
  final into = args[1];
  final sep = Platform.pathSeparator;

  final rides = <String, List<String>>{};
  for (final path in args.skip(2)) {
    (rides[rideStem(path.split(sep).last)] ??= []).add(path);
  }

  String pathFor(int i) => '$into$sep$archiveStem-$i.zip';

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
