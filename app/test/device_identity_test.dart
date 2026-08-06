import 'dart:math';

import 'package:flutter_test/flutter_test.dart';
import 'package:mumbleway/services/device_identity.dart';

/// The sanitiser is where every platform's answer is judged, and the platforms
/// disagree wildly about what they will tell an app. These are the shapes each
/// one actually returns.
void main() {
  group('sanitize', () {
    test('a macOS full name becomes one dashed word', () {
      // NSFullUserName() — a real name with a space in it, which Mumble will
      // not accept as typed.
      expect(DeviceIdentity.sanitize('Ilya Melamed'), 'Ilya-Melamed');
    });

    test('a possessive device name keeps the person, not the hardware', () {
      // UIDevice.name before iOS 16, and Settings.Global.DEVICE_NAME on plenty
      // of Android phones.
      expect(DeviceIdentity.sanitize("Ilya's iPhone"), 'Ilya');
      expect(DeviceIdentity.sanitize('Ilya’s iPad'), 'Ilya');
      expect(DeviceIdentity.sanitize('iPhone von Anna'), 'Anna');
      expect(DeviceIdentity.sanitize('iPhone de Marie'), 'Marie');
    });

    test('what a device says when it knows nothing is not an answer', () {
      // The important case. iOS 16 gives every phone the same string, and
      // accepting it would put every rider in the group on the same name —
      // which is the exact failure that dropping the username from shared
      // codes was meant to end.
      for (final generic in [
        'iPhone',
        'iPad',
        'Android',
        'localhost',
        'Owner',
        'user',
        'root',
      ]) {
        expect(
          DeviceIdentity.sanitize(generic),
          isNull,
          reason: '"$generic" identifies nobody',
        );
      }
    });

    test('nothing usable comes back as nothing', () {
      expect(DeviceIdentity.sanitize(null), isNull);
      expect(DeviceIdentity.sanitize(''), isNull);
      expect(DeviceIdentity.sanitize('   '), isNull);
      // A single letter is not a name anyone will recognise on a roster.
      expect(DeviceIdentity.sanitize('A'), isNull);
      // Nothing survives the character filter, so there is nothing left.
      expect(DeviceIdentity.sanitize('Илья'), isNull);
      expect(DeviceIdentity.sanitize('!!! ???'), isNull);
    });

    test('the result is always a name a Mumble server will accept', () {
      // Mumble's default policy is [-=\w\[\]\{\}\(\)@\|\.]+ — this app sticks
      // to the safe middle of it: letters, digits, underscore, dot, dash.
      final allowed = RegExp(r'^[A-Za-z0-9_.\-]+$');
      for (final raw in [
        "Ilya's iPhone",
        'Ilya Melamed',
        'Anna-Maria O\'Brien',
        'user@example.com',
        'Ilya   Melamed',
        'Björn Müller',
      ]) {
        final name = DeviceIdentity.sanitize(raw);
        if (name == null) continue;
        expect(
          allowed.hasMatch(name),
          isTrue,
          reason: '"$raw" produced "$name", which a server would reject',
        );
        expect(name.length, lessThanOrEqualTo(32));
      }
    });

    test('a long name is shortened rather than thrown away', () {
      final long = 'Wolfeschlegelsteinhausenbergerdorff Alexandrovich';
      final name = DeviceIdentity.sanitize(long)!;
      expect(name.length, 32);
      // Never left ending on a separator, which reads as a truncation bug.
      expect(name.endsWith('-'), isFalse);
      expect(name.endsWith('.'), isFalse);
    });

    test('runs of separators collapse instead of stacking up', () {
      expect(DeviceIdentity.sanitize('  Ilya   ~*~   Melamed  '), 'Ilya-Melamed');
      expect(DeviceIdentity.sanitize('--Ilya--'), 'Ilya');
    });
  });

  group('randomName', () {
    test('is two real words with a dash between them', () {
      final name = DeviceIdentity.randomName();
      expect(name, matches(RegExp(r'^[a-z]+-[a-z]+$')));
      // And it must survive its own sanitiser, or the fallback would be a
      // name the server rejects.
      expect(DeviceIdentity.sanitize(name), name);
    });

    test('picks from both lists rather than repeating one word', () {
      DeviceIdentity.randomOverride = Random(1);
      addTearDown(() => DeviceIdentity.randomOverride = null);
      final names = List.generate(40, (_) => DeviceIdentity.randomName());
      expect(names.toSet().length, greaterThan(20));
    });
  });

  group('suggest', () {
    tearDown(() => DeviceIdentity.platformOverride = null);

    test('uses what the platform said when it is usable', () async {
      DeviceIdentity.platformOverride = () async => "Ilya's iPhone";
      expect(await DeviceIdentity.instance.suggest(), 'Ilya');
    });

    test('falls through to two words when the platform is unhelpful', () async {
      for (final answer in [null, '', 'iPhone']) {
        DeviceIdentity.platformOverride = () async => answer;
        final name = await DeviceIdentity.instance.suggest();
        expect(name, matches(RegExp(r'^[a-z]+-[a-z]+$')));
      }
    });

    test('a platform that throws still yields a name', () async {
      // The whole point: a rider is never left with an empty username field
      // because something on the platform side misbehaved.
      DeviceIdentity.platformOverride = () async => throw StateError('no');
      expect(
        await DeviceIdentity.instance.suggest(),
        matches(RegExp(r'^[a-z]+-[a-z]+$')),
      );
    });
  });
}
