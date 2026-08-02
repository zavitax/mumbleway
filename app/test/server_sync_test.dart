import 'package:flutter_test/flutter_test.dart';
import 'package:mumbleway/state/server_sync.dart';

/// One server entry, reduced to the two fields the merge actually reads.
Map<String, dynamic> entry(String id, {int at = 0, String name = 'n'}) => {
  'localId': id,
  'name': name,
  'host': id,
  'port': 64738,
  'updatedAt': at,
};

SyncSnapshot snap(
  List<Map<String, dynamic>> servers, [
  Map<String, int> deleted = const {},
]) => SyncSnapshot(servers: servers, deleted: deleted);

List<String> idsOf(SyncSnapshot s) => [for (final e in s.servers) syncIdOf(e)];

const now = 1800000000000;

/// A moment [ago] milliseconds before [now].
///
/// Tests use real timestamps rather than 1, 2, 3 because tombstones expire:
/// small numbers are all comfortably older than the retention window, and a
/// test written with them proves only that ancient deletions get discarded.
int since(int ago) => now - ago;

void main() {
  group('merging two devices', () {
    test('keeps what either side has', () {
      final m = mergeSnapshots(
        snap([entry('a')]),
        snap([entry('b')]),
        nowMs: now,
      );
      expect(idsOf(m), ['a', 'b']);
    });

    test('the more recent edit wins, whole', () {
      final m = mergeSnapshots(
        snap([entry('a', at: since(900), name: 'old')]),
        snap([entry('a', at: since(800), name: 'new')]),
        nowMs: now,
      );
      expect(m.servers.single['name'], 'new');
    });

    test('an entry with no timestamp loses to one that has been edited', () {
      final m = mergeSnapshots(
        snap([entry('a', name: 'legacy')]),
        snap([entry('a', at: since(995), name: 'edited')]),
        nowMs: now,
      );
      expect(m.servers.single['name'], 'edited');
    });

    test('but is never deleted merely for being old', () {
      final m = mergeSnapshots(
        snap([entry('a', name: 'legacy')]),
        const SyncSnapshot(),
        nowMs: now,
      );
      expect(idsOf(m), ['a']);
    });

    test('a deletion removes the entry from the other device', () {
      final m = mergeSnapshots(
        snap([entry('a', at: since(900))]),
        snap([], {'a': since(800)}),
        nowMs: now,
      );
      expect(idsOf(m), isEmpty);
      expect(m.deleted, {'a': since(800)});
    });

    test('an edit after a deletion brings the entry back', () {
      final m = mergeSnapshots(
        snap([entry('a', at: since(700), name: 'revived')]),
        snap([], {'a': since(800)}),
        nowMs: now,
      );
      expect(m.servers.single['name'], 'revived');
      expect(m.deleted, isEmpty, reason: 'the tombstone has been overruled');
    });

    test('a tie keeps the entry rather than the deletion', () {
      final m = mergeSnapshots(
        snap([entry('a', at: since(800))]),
        snap([], {'a': since(800)}),
        nowMs: now,
      );
      expect(idsOf(m), ['a']);
    });

    test('a deletion survives a round trip so every device sees it', () {
      // The phone deletes. The laptop merges and drops the entry. What the
      // laptop then publishes has to still carry the tombstone, or the desktop
      // — which has been off all week — puts the server back for everyone.
      final phone = mergeSnapshots(
        snap([], {'a': since(800)}),
        snap([entry('a', at: since(900))]),
        nowMs: now,
      );
      final desktop = mergeSnapshots(
        snap([entry('a', at: since(900))]),
        phone,
        nowMs: now,
      );
      expect(idsOf(desktop), isEmpty);
    });

    test('tombstones expire, so they do not pile up forever', () {
      final ancient = now - tombstoneLife.inMilliseconds - 1;
      final m = mergeSnapshots(
        snap([], {'a': ancient}),
        const SyncSnapshot(),
        nowMs: now,
      );
      expect(m.deleted, isEmpty);
    });

    test('this device keeps its own order, new arrivals go last', () {
      final m = mergeSnapshots(
        snap([entry('b'), entry('a')]),
        snap([entry('a'), entry('c')]),
        nowMs: now,
      );
      expect(idsOf(m), ['b', 'a', 'c']);
    });

    test('an entry is never duplicated, however many sides have it', () {
      final m = mergeSnapshots(
        snap([entry('a', at: since(999))], {'a': since(999)}),
        snap([entry('a', at: since(998))], {'a': since(998)}),
        nowMs: now,
      );
      expect(idsOf(m).length + m.deleted.length, 1);
    });

    test('both devices reach the same answer from a dead-heat edit', () {
      // Otherwise each prefers its own copy, and the two spend the afternoon
      // overwriting one another.
      final a = entry('x', at: since(500), name: 'one');
      final b = entry('x', at: since(500), name: 'two');
      final left = mergeSnapshots(snap([a]), snap([b]), nowMs: now);
      final right = mergeSnapshots(snap([b]), snap([a]), nowMs: now);
      expect(left.servers.single['name'], right.servers.single['name']);
    });

    test('merging with nothing changes nothing', () {
      final mine = snap(
        [entry('a', at: since(999)), entry('b', at: since(998))],
        {'c': since(997)},
      );
      final m = mergeSnapshots(mine, const SyncSnapshot(), nowMs: now);
      expect(sameSnapshot(m, mine), isTrue);
    });

    test('settles after one exchange rather than ringing back and forth', () {
      final phone = snap([entry('a', at: since(999))]);
      final laptop = snap([entry('b', at: since(998))]);
      final first = mergeSnapshots(phone, laptop, nowMs: now);
      final second = mergeSnapshots(laptop, first, nowMs: now);
      expect(sameSnapshot(first, second), isTrue);
      expect(
        sameSnapshot(mergeSnapshots(first, second, nowMs: now), first),
        isTrue,
      );
    });
  });

  group('payloads', () {
    test('survive a round trip', () {
      final s = snap([entry('a', at: since(993))], {'b': since(991)});
      final back = SyncSnapshot.decode(s.encode())!;
      expect(sameSnapshot(back, s), isTrue);
    });

    test('nonsense from a future version is ignored, not thrown', () {
      expect(SyncSnapshot.decode('not json'), isNull);
      expect(SyncSnapshot.decode('[1,2,3]'), isNull);
      expect(SyncSnapshot.decode(''), isNull);
      expect(SyncSnapshot.decode(null), isNull);
      // A payload of the right shape with the wrong contents parses to
      // nothing rather than to garbage entries.
      expect(
        SyncSnapshot.decode('{"servers":[1,2],"deleted":{"a":"x"}}')!.isEmpty,
        isTrue,
      );
    });

    test('an entry from before local ids falls back to host and port', () {
      expect(syncIdOf({'host': 'h', 'port': 64738}), 'h:64738');
    });
  });

  group('sameSnapshot', () {
    test('ignores order', () {
      expect(
        sameSnapshot(
          snap([entry('a'), entry('b')]),
          snap([entry('b'), entry('a')]),
        ),
        isTrue,
      );
    });

    test('notices a changed field', () {
      expect(
        sameSnapshot(
          snap([entry('a', name: 'x')]),
          snap([entry('a', name: 'y')]),
        ),
        isFalse,
      );
    });

    test('notices a differing tombstone', () {
      expect(
        sameSnapshot(snap([], {'a': since(999)}), snap([], {'a': since(998)})),
        isFalse,
      );
    });
  });
}
