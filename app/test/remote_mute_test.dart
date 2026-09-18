import 'package:flutter/material.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mumbleway/l10n/app_localizations.dart';
import 'package:mumbleway/services/remote_mute_notice.dart';
import 'package:mumbleway/src/rust/api/mumbleway.dart';
import 'package:mumbleway/state/app_state.dart';
import 'package:mumbleway/widgets/remote_mute_listener.dart';
import 'package:shared_preferences/shared_preferences.dart';

/// Another MumbleWay rider turning this rider's microphone off or on.
///
/// The core decides and does it, and plays the cue. What is pinned here is the
/// half that could quietly come apart from it: the interface believing the
/// microphone is in the state the core put it in. A mute button that reads
/// "on" over a microphone the core has closed is a rider who presses it to
/// mute and opens it instead.
void main() {
  setUp(() => SharedPreferences.setMockInitialValues({}));

  group('the interface follows the change', () {
    test('a remote mute leaves the app muted', () {
      final state = AppState();
      addTearDown(state.dispose);
      expect(state.muted, isFalse);

      state.onEvent(
        const AppEvent.remoteMuted(serverId: 's', muted: true, by: 'Anna'),
      );

      expect(state.muted, isTrue);
    });

    test('a remote unmute leaves it unmuted, so the button undoes it', () {
      final state = AppState();
      addTearDown(state.dispose);
      state.toggleMute();
      expect(state.muted, isTrue);

      state.onEvent(
        const AppEvent.remoteMuted(serverId: 's', muted: false, by: 'Anna'),
      );

      expect(state.muted, isFalse);
      // And the rider's own button now does the obvious thing.
      state.toggleMute();
      expect(state.muted, isTrue);
    });

    test('each change is announced, in order', () async {
      final state = AppState();
      addTearDown(state.dispose);
      final seen = <RemoteMuteNotice>[];
      final sub = state.remoteMuteNotices.listen(seen.add);
      addTearDown(sub.cancel);

      state
        ..onEvent(
          const AppEvent.remoteMuted(serverId: 's', muted: true, by: 'Anna'),
        )
        ..onEvent(
          const AppEvent.remoteMuted(serverId: 's', muted: false, by: 'Boris'),
        );
      await Future<void>.delayed(Duration.zero);

      expect([for (final n in seen) (n.muted, n.by)], [
        (true, 'Anna'),
        (false, 'Boris'),
      ]);
    });

    test('coming back online while muted does not throw', () {
      // Re-asserting the mute on a fresh session reaches for the engine, which
      // a test does not have. It must be best-effort, not a crash in the
      // middle of handling a status change.
      final state = AppState();
      addTearDown(state.dispose);
      state.toggleMute();
      state.onEvent(
        AppEvent.status(
          StatusUpdate(
            serverId: 's',
            status: ConnStatus.connected,
            detail: '',
            attempt: 0,
            retryInMs: BigInt.zero,
          ),
        ),
      );
      expect(state.runtimeFor('s').isLive, isTrue);
      expect(state.muted, isTrue);
    });
  });

  group('allowing other riders to unmute', () {
    test('is on for a fresh install', () {
      final state = AppState();
      addTearDown(state.dispose);
      expect(state.allowRemoteUnmute, isTrue);
    });

    test('turning it off is remembered', () async {
      final state = AppState();
      addTearDown(state.dispose);
      await state.setAllowRemoteUnmuteEnabled(value: false);

      expect(state.allowRemoteUnmute, isFalse);
      final prefs = await SharedPreferences.getInstance();
      expect(prefs.getBool('mumbleway.allowRemoteUnmute'), isFalse);
    });
  });

  group('what the rider is told', () {
    late L en;
    late L ru;

    setUpAll(() async {
      en = await L.delegate.load(const Locale('en'));
      ru = await L.delegate.load(const Locale('ru'));
    });

    test('names who did it, either way, in both languages', () {
      for (final l in [en, ru]) {
        for (final muted in [true, false]) {
          final text = RemoteMuteNotice(
            serverId: 's',
            muted: muted,
            by: 'Anna',
          ).describe(l);
          expect(text, contains('Anna'));
        }
      }
    });

    test('off and on do not read the same', () {
      for (final l in [en, ru]) {
        const off = RemoteMuteNotice(serverId: 's', muted: true, by: 'A');
        const on = RemoteMuteNotice(serverId: 's', muted: false, by: 'A');
        expect(off.describe(l), isNot(on.describe(l)));
      }
    });

    testWidgets('the notice reaches the screen', (tester) async {
      final state = AppState();
      addTearDown(state.dispose);
      await tester.pumpWidget(
        AppStateScope(
          state: state,
          child: MaterialApp(
            localizationsDelegates: const [
              ...L.localizationsDelegates,
              GlobalMaterialLocalizations.delegate,
              GlobalWidgetsLocalizations.delegate,
            ],
            supportedLocales: L.supportedLocales,
            builder: (context, child) => RemoteMuteListener(child: child!),
            home: const Scaffold(body: SizedBox.shrink()),
          ),
        ),
      );

      state.onEvent(
        const AppEvent.remoteMuted(serverId: 's', muted: false, by: 'Anna'),
      );
      await tester.pump();
      await tester.pump(const Duration(milliseconds: 300));

      expect(find.text(en.remoteUnmutedYou('Anna')), findsOneWidget);
    });
  });
}
