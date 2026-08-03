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

  group('the round trip through the cloud', () {
    // An entry that has been through iCloud has had its password lifted out
    // and put back, which moves that key to the end of the map. JSON keeps
    // insertion order, so the two encodings differ while the data does not.
    Map<String, dynamic> roundTripped(Map<String, dynamic> e) {
      final copy = Map<String, dynamic>.of(e);
      final password = copy.remove('password');
      return {...copy, 'password': password};
    }

    test('does not change what an entry is', () {
      final local = {...entry('a', at: since(500)), 'password': 'pw'};
      expect(
        sameSnapshot(snap([local]), snap([roundTripped(local)])),
        isTrue,
        reason: 'key order is not a difference in the data',
      );
    });

    test('key order does not decide a tie', () {
      // The bug this exists for, in its exact shape. Learning a certificate on
      // connect edited the entry without moving its timestamp, so it tied with
      // the copy in the cloud, and the tie fell to raw JSON — where the
      // round-tripped side sorts first whatever it contains. The stale null
      // won, the entry looked changed on every sync, the live session was
      // rebuilt each time, and the client hammered the server about once a
      // second until it began refusing connections.
      //
      // The same timestamp on both sides is the point: that is what left the
      // layout deciding instead of the data.
      const at = 500;
      final mine = {
        ...entry('a', at: since(at)),
        'password': 'pw',
        'certFingerprint': 'AB:CD',
      };
      final stale = roundTripped({
        ...entry('a', at: since(at)),
        'password': 'pw',
        'certFingerprint': null,
      });

      final m = mergeSnapshots(snap([mine]), snap([stale]), nowMs: now);
      expect(
        m.servers.single['certFingerprint'],
        'AB:CD',
        reason: 'content decides, not which side has been through the cloud',
      );
    });

    test('and both devices reach that answer, whichever side asks', () {
      // Otherwise each prefers its own copy and they rewrite one another
      // indefinitely.
      const at = 500;
      final withFingerprint = {
        ...entry('a', at: since(at)),
        'certFingerprint': 'AB:CD',
      };
      final without = roundTripped({
        ...entry('a', at: since(at)),
        'certFingerprint': null,
      });
      expect(
        mergeSnapshots(
          snap([withFingerprint]),
          snap([without]),
          nowMs: now,
        ).servers.single['certFingerprint'],
        mergeSnapshots(
          snap([without]),
          snap([withFingerprint]),
          nowMs: now,
        ).servers.single['certFingerprint'],
      );
    });

    test('merging is idempotent once both sides agree', () {
      // Whatever the merge settles on has to stay settled. A merge that keeps
      // producing something new is what turned a sync into a retry loop.
      final mine = {...entry('a', at: since(500)), 'password': 'pw'};
      final theirs = roundTripped(mine);
      final first = mergeSnapshots(snap([mine]), snap([theirs]), nowMs: now);
      final second = mergeSnapshots(first, snap([theirs]), nowMs: now);
      final third = mergeSnapshots(second, first, nowMs: now);
      expect(sameSnapshot(second, first), isTrue);
      expect(sameSnapshot(third, first), isTrue);
    });

    test('the payload is byte-identical whatever the key order', () {
      // So two devices holding the same list stop rewriting each other's copy,
      // each write waking the other to do the same.
      final mine = {...entry('a', at: since(500)), 'password': 'pw'};
      expect(snap([mine]).encode(), snap([roundTripped(mine)]).encode());
    });
  });

  group('settings', () {
    Map<String, dynamic> at(Object? v, int ago) => {'v': v, 'at': since(ago)};

    test('the more recent change to each setting wins', () {
      final merged = mergeSettings(
        {'reverb': at(true, 100), 'noise': at(1, 900)},
        {'reverb': at(false, 900), 'noise': at(3, 100)},
      );
      expect(merged['reverb']['v'], isTrue, reason: 'ours was newer');
      expect(merged['noise']['v'], 3, reason: 'theirs was newer');
    });

    test('two devices changing different settings both keep theirs', () {
      // The case a single timestamp for the whole block would throw away.
      final merged = mergeSettings(
        {'reverb': at(false, 100), 'micMode': at(0, 5000)},
        {'reverb': at(true, 5000), 'micMode': at(2, 100)},
      );
      expect(merged['reverb']['v'], isFalse);
      expect(merged['micMode']['v'], 2);
    });

    test('a setting only one side has heard of survives', () {
      // Which is what makes an older build meeting a newer one harmless.
      final merged = mergeSettings(
        {'brandNew': at(7, 100)},
        {'old': at(1, 100)},
      );
      expect(merged['brandNew']['v'], 7);
      expect(merged['old']['v'], 1);
    });

    test('a dead heat keeps ours, so a device does not flap', () {
      final merged = mergeSettings(
        {'reverb': at(true, 500)},
        {'reverb': at(false, 500)},
      );
      expect(merged['reverb']['v'], isTrue);
    });

    test('settings travel in the payload and survive the trip', () {
      final s = SyncSnapshot(settings: {'reverb': at(true, 100)});
      final back = SyncSnapshot.decode(s.encode())!;
      expect(back.settings['reverb']['v'], isTrue);
      expect(sameSnapshot(back, s), isTrue);
    });

    test('a differing setting counts as a difference worth publishing', () {
      expect(
        sameSnapshot(
          SyncSnapshot(settings: {'reverb': at(true, 100)}),
          SyncSnapshot(settings: {'reverb': at(false, 100)}),
        ),
        isFalse,
      );
    });
  });
}
