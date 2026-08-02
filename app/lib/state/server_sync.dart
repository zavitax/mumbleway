/// Reconciling two devices' server lists.
///
/// Kept free of Flutter, the Rust bridge and the platform channels so the
/// rules below can be tested directly. Everything here works on the same JSON
/// the list is already persisted as, which means sync never needs its own
/// parallel model that can drift from the real one.
library;

import 'dart:convert';

/// How long a deletion is remembered.
///
/// Tombstones are the only reason a delete on one device survives contact with
/// a phone that has been in a drawer since before it happened: without one,
/// the drawer phone still has the entry, sees it missing from the cloud, and
/// helpfully puts it back. They expire because they would otherwise accumulate
/// forever, and a phone offline for three months has bigger problems.
const Duration tombstoneLife = Duration(days: 90);

/// One device's view of the list, as it travels between devices.
class SyncSnapshot {
  const SyncSnapshot({this.servers = const [], this.deleted = const {}});

  /// Server entries, in the order they should be shown.
  final List<Map<String, dynamic>> servers;

  /// `localId` to the moment it was deleted, in milliseconds since the epoch.
  final Map<String, int> deleted;

  bool get isEmpty => servers.isEmpty && deleted.isEmpty;

  Map<String, dynamic> toJson() => {
    'v': 1,
    'servers': servers,
    'deleted': deleted,
  };

  String encode() => jsonEncode(toJson());

  static SyncSnapshot fromJson(Map<String, dynamic> j) => SyncSnapshot(
    servers: [
      for (final s in (j['servers'] as List? ?? const []))
        if (s is Map) Map<String, dynamic>.from(s),
    ],
    deleted: {
      for (final e in (j['deleted'] as Map? ?? const {}).entries)
        if (e.value is num) '${e.key}': (e.value as num).toInt(),
    },
  );

  /// Parses a payload, returning null rather than throwing.
  ///
  /// The input arrives from another device running a version of the app this
  /// one knows nothing about, so it is treated as untrusted: a payload that
  /// cannot be read leaves the local list alone, which is the same outcome as
  /// having never synced at all.
  static SyncSnapshot? decode(String? raw) {
    if (raw == null || raw.isEmpty) return null;
    try {
      final j = jsonDecode(raw);
      if (j is! Map<String, dynamic>) return null;
      return fromJson(j);
    } catch (_) {
      return null;
    }
  }
}

/// The identity of an entry across devices.
String syncIdOf(Map<String, dynamic> s) =>
    (s['localId'] as String?) ?? '${s['host']}:${s['port']}';

/// When an entry was last changed, or 0 if it predates sync.
///
/// Zero is deliberately the oldest possible time: an entry saved before this
/// feature existed loses every conflict against one that has been edited since,
/// which is the right way round. It is never grounds for deletion, though —
/// only a tombstone deletes.
int syncStampOf(Map<String, dynamic> s) {
  final v = s['updatedAt'];
  return v is num ? v.toInt() : 0;
}

/// Merges the cloud's copy into this device's copy.
///
/// The rules, in the order they matter:
///
/// * An entry either side has and neither has deleted survives. Union, not
///   intersection — a server added on the phone must reach the laptop, and
///   "the laptop has never heard of it" is not evidence of anything.
/// * When both sides have the same entry, the one edited more recently wins
///   whole. Not field by field: a half-merged entry could combine one device's
///   host with another's username and produce a server that never existed.
/// * A deletion beats an edit only if it happened later. Deleting on the phone
///   while editing on the laptop leaves the entry alive, because the edit is
///   the more recent statement of intent.
/// * Exact ties keep the entry. Two events in the same millisecond means the
///   clocks are indistinguishable, and of the two ways to be wrong, an entry
///   that comes back can be deleted again in a second, while one that should
///   not have gone takes its password with it.
///
/// Order is [mine]'s, with entries seen only in [theirs] appended. Each device
/// keeps the arrangement its owner made rather than having a remote list
/// reshuffle it, and new arrivals land at the end where they are noticed.
///
/// Clocks are trusted only as far as they have to be. A device set badly wrong
/// can win conflicts it should lose, but it cannot delete anything it did not
/// delete, and every entry it has ever seen still survives somewhere.
SyncSnapshot mergeSnapshots(
  SyncSnapshot mine,
  SyncSnapshot theirs, {
  required int nowMs,
}) {
  final byIdMine = {for (final s in mine.servers) syncIdOf(s): s};
  final byIdTheirs = {for (final s in theirs.servers) syncIdOf(s): s};

  final cutoff = nowMs - tombstoneLife.inMilliseconds;
  final deletions = <String, int>{};
  for (final source in [mine.deleted, theirs.deleted]) {
    for (final e in source.entries) {
      if (e.value < cutoff) continue;
      final existing = deletions[e.key];
      if (existing == null || e.value > existing) deletions[e.key] = e.value;
    }
  }

  final servers = <Map<String, dynamic>>[];
  final kept = <String>{};
  final tombstones = <String, int>{};

  void consider(String id) {
    if (!kept.add(id)) return;
    final winner = _pick(byIdMine[id], byIdTheirs[id]);
    final deletedAt = deletions[id];
    if (winner == null) {
      // Nothing but a tombstone: carry it, so a device that still holds the
      // entry learns of the deletion the next time it reads.
      if (deletedAt != null) tombstones[id] = deletedAt;
      return;
    }
    if (deletedAt != null && deletedAt > syncStampOf(winner)) {
      tombstones[id] = deletedAt;
      return;
    }
    servers.add(winner);
  }

  for (final s in mine.servers) {
    consider(syncIdOf(s));
  }
  for (final s in theirs.servers) {
    consider(syncIdOf(s));
  }
  for (final id in deletions.keys) {
    consider(id);
  }

  return SyncSnapshot(servers: servers, deleted: tombstones);
}

/// The more recently edited of two versions of one entry.
///
/// Equal timestamps are broken by content rather than by which argument came
/// first, so both devices reach the same answer instead of each preferring its
/// own copy and rewriting the other's forever.
Map<String, dynamic>? _pick(Map<String, dynamic>? a, Map<String, dynamic>? b) {
  if (a == null) return b;
  if (b == null) return a;
  final sa = syncStampOf(a);
  final sb = syncStampOf(b);
  if (sa != sb) return sa > sb ? a : b;
  return jsonEncode(a).compareTo(jsonEncode(b)) <= 0 ? a : b;
}

/// Whether two snapshots say the same thing, ignoring order.
///
/// Used to decide whether a merge is worth uploading. Without it two devices
/// that merely disagree about ordering would take turns rewriting the cloud
/// copy, each waking the other to do the same, forever.
bool sameSnapshot(SyncSnapshot a, SyncSnapshot b) {
  if (a.servers.length != b.servers.length) return false;
  if (a.deleted.length != b.deleted.length) return false;
  for (final e in a.deleted.entries) {
    if (b.deleted[e.key] != e.value) return false;
  }
  final left = a.servers.map(jsonEncode).toList()..sort();
  final right = b.servers.map(jsonEncode).toList()..sort();
  for (var i = 0; i < left.length; i++) {
    if (left[i] != right[i]) return false;
  }
  return true;
}
