import 'package:flutter_test/flutter_test.dart';
import 'package:mumbleway/l10n/app_localizations.dart';
import 'package:mumbleway/src/rust/api/mumbleway.dart';
import 'package:mumbleway/state/app_state.dart';
import 'package:flutter/material.dart' show Locale;

/// A server enforces its bandwidth allowance by dropping voice packets and
/// telling nobody — and the packet is counted as received before it is dropped,
/// so the per-rider loss figures show a clean link while the words go missing.
///
/// The arithmetic lives in the core and is tested there. What is pinned here is
/// the half a rider actually meets: being told, once, when somebody else's
/// setting has reduced their audio, and not being told when it has not.
void main() {
  Future<List<String>> collect(AppState state, void Function() act) async {
    final seen = <String>[];
    final sub = state.serverSuggestions.listen(seen.add);
    act();
    await Future<void>.delayed(Duration.zero);
    await sub.cancel();
    return seen;
  }

  AppEvent cap({
    String server = 's',
    int capBps = 32000,
    int bitrateBps = 17280,
    bool capped = true,
    bool belowFloor = false,
  }) => AppEvent.bandwidth(
    serverId: server,
    capBps: capBps,
    bitrateBps: bitrateBps,
    capped: capped,
    belowFloor: belowFloor,
  );

  test('an allowance we fit inside says nothing', () async {
    // Murmur's default is 72 kbit/s and this app fits inside it. A notice
    // here would fire on every ordinary server.
    final state = AppState();
    addTearDown(state.dispose);

    final seen = await collect(
      state,
      () => state.onEvent(
        cap(capBps: 72000, bitrateBps: 24000, capped: false),
      ),
    );

    expect(seen, isEmpty);
    expect(state.audioCapped, isFalse);
    expect(state.audioBitrateBps, 24000);
  });

  test('a reduced bitrate is reported once, with the figure', () async {
    final state = AppState();
    addTearDown(state.dispose);

    final first = await collect(state, () => state.onEvent(cap()));
    final again = await collect(state, () => state.onEvent(cap()));

    expect(first, ['bitrate']);
    expect(again, isEmpty, reason: 'reconnects are ordinary; nagging is not');
    expect(state.audioCapped, isTrue);
    expect(state.audioBitrateBps, 17280);
    expect(state.runtimeFor('s').bandwidthCapBps, 32000);
  });

  test('an allowance too low for voice says something stronger', () async {
    // Nothing the app can do fixes this: the per-packet toll is fixed by the
    // frame length. The rider is going to be partly inaudible and should know.
    final state = AppState();
    addTearDown(state.dispose);

    final seen = await collect(
      state,
      () => state.onEvent(cap(capBps: 16000, bitrateBps: 8000, belowFloor: true)),
    );

    expect(seen, ['bitrate-floor']);
    expect(state.audioBelowFloor, isTrue);
  });

  test('each server gets to say it once', () async {
    final state = AppState();
    addTearDown(state.dispose);

    final seen = await collect(state, () {
      state
        ..onEvent(cap(server: 'a'))
        ..onEvent(cap(server: 'b'));
    });

    expect(seen, ['bitrate', 'bitrate']);
  });

  group('what the diagnostics row says', () {
    late L en;
    late L ru;

    setUpAll(() async {
      en = await L.delegate.load(const Locale('en'));
      ru = await L.delegate.load(const Locale('ru'));
    });

    test('the limit is named only when it is doing something', () {
      for (final l in [en, ru]) {
        // Uncapped: the rate, and nothing to blame.
        expect(l.diagKbps(24), contains('24'));
        // Capped: both numbers, because the gap between them is the point.
        final capped = l.diagKbpsCapped(17, 32);
        expect(capped, contains('17'));
        expect(capped, contains('32'));
        expect(capped, isNot(l.diagKbps(17)));
      }
    });

    test('the notice carries the rate that is now in use', () {
      expect(en.serverCapsBitrate(17), contains('17'));
      expect(ru.serverCapsBitrate(17), contains('17'));
      expect(en.serverCapTooLow, isNot(ru.serverCapTooLow));
    });
  });
}
