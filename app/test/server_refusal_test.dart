import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mumbleway/l10n/app_localizations.dart';
import 'package:mumbleway/services/server_refusal.dart';

/// A refusal is the only account the user gets of why an action did nothing,
/// so what it says matters more than most strings in the app.
void main() {
  late L en;
  late L ru;

  setUpAll(() async {
    en = await L.delegate.load(const Locale('en'));
    ru = await L.delegate.load(const Locale('ru'));
  });

  group('what the server refused', () {
    test("the server's own words win, because an admin meant them", () {
      const r = ServerRefusal(
        serverId: 's',
        reason: 'Only moderators may mute on this server',
        kind: 1,
      );
      expect(r.describe(en), 'Only moderators may mute on this server');
      // And they are not translated: they did not come from us.
      expect(r.describe(ru), 'Only moderators may mute on this server');
    });

    test('an empty reason falls back to the type, translated', () {
      const r = ServerRefusal(serverId: 's', reason: '', kind: 1);
      expect(r.describe(en), en.denyPermission);
      expect(r.describe(ru), ru.denyPermission);
      expect(r.describe(en), isNot(r.describe(ru)),
          reason: 'the Russian must actually be Russian');
    });

    test('whitespace is not words', () {
      const r = ServerRefusal(serverId: 's', reason: '   \n ', kind: 9);
      expect(r.describe(en), en.denyChannelFull);
    });

    test('every deny type Mumble defines says something specific', () {
      final seen = <String>{};
      for (var kind = 0; kind <= 13; kind++) {
        final text = ServerRefusal(serverId: 's', reason: '', kind: kind)
            .describe(en);
        expect(text, isNotEmpty);
        seen.add(text);
      }
      // Not one message reused for all fourteen: that would be no better than
      // the placeholder this replaced.
      expect(seen.length, greaterThan(8));
    });

    test('an unknown type still says permission was refused', () {
      // Mumble adds deny types over time. Falling silent on one would be worse
      // than being vague about it.
      const r = ServerRefusal(serverId: 's', reason: '', kind: 999);
      expect(r.describe(en), en.denyPermission);
    });

    test('the snackbar names the server as the one refusing', () {
      expect(en.serverRefused('nope'), contains('nope'));
      expect(en.serverRefused('nope').toLowerCase(), contains('server'));
    });
  });
}
