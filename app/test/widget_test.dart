import 'dart:convert';
import 'dart:io' show File, Platform;
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mumbleway/services/audio_session.dart';
import 'package:mumbleway/widgets/app_bar_title.dart';
import 'package:mumbleway/l10n/app_localizations.dart';
import 'package:mumbleway/services/button_controller.dart';
import 'package:mumbleway/services/overlay.dart';
import 'package:mumbleway/services/proxy.dart';
import 'package:mumbleway/state/app_state.dart';
import 'package:mumbleway/src/rust/api/mumbleway.dart';
import 'package:mumbleway/theme.dart';
import 'package:mumbleway/widgets/ptt_button.dart';
import 'package:mumbleway/widgets/voice_meter.dart';
import 'package:mumbleway/widgets/status_badge.dart';

void main() {
  group('SystemProxy parsing', () {
    test('reads a plain host:port and strips any scheme', () {
      expect(
        SystemProxy.stripScheme('http://127.0.0.1:10809'),
        '127.0.0.1:10809',
      );
      expect(SystemProxy.stripScheme('127.0.0.1:10809'), '127.0.0.1:10809');
      expect(
        SystemProxy.pickFromWindowsValue('http://10.0.0.1:8080'),
        '10.0.0.1:8080',
      );
    });

    test('prefers the https entry in a per-scheme list', () {
      // Windows stores either one value or "http=a;https=b"; this app fetches
      // over https, so that entry is the relevant one.
      const value = 'http=proxy-a:1;https=proxy-b:2;ftp=proxy-c:3';
      expect(SystemProxy.pickFromWindowsValue(value), 'proxy-b:2');
    });

    test('falls back to the http entry when there is no https one', () {
      expect(
        SystemProxy.pickFromWindowsValue('http=proxy-a:1;ftp=proxy-c:3'),
        'proxy-a:1',
      );
    });

    test('splits bypass lists on both separators', () {
      final b = SystemProxy.splitBypass('localhost;127.*, 10.*;<local>');
      expect(b, ['localhost', '127.*', '10.*', '<local>']);
      expect(SystemProxy.splitBypass(null), isEmpty);
      expect(SystemProxy.splitBypass('   '), isEmpty);
    });

    test('bypass matching handles wildcards and the <local> shorthand', () {
      const bypass = ['localhost', '10.*', '*.internal', '<local>'];
      expect(SystemProxy.hostBypasses('localhost', bypass), isTrue);
      expect(SystemProxy.hostBypasses('10.0.0.5', bypass), isTrue);
      expect(SystemProxy.hostBypasses('build.internal', bypass), isTrue);
      // <local> means any name without a dot.
      expect(SystemProxy.hostBypasses('buildserver', bypass), isTrue);

      expect(SystemProxy.hostBypasses('publist.mumble.info', bypass), isFalse);
      expect(SystemProxy.hostBypasses('example.com', bypass), isFalse);
    });
  });

  // Widgets under test resolve localised strings, so the harness has to supply
  // the delegates just as the real app does.
  Widget wrap(Widget child) => MaterialApp(
    theme: buildTheme(Brightness.dark),
    supportedLocales: AppState.supportedLocales,
    localizationsDelegates: const [
      L.delegate,
      GlobalMaterialLocalizations.delegate,
      GlobalWidgetsLocalizations.delegate,
      // MaterialApp needs a Cupertino delegate covering every supported
      // locale, even on Android and desktop.
      GlobalCupertinoLocalizations.delegate,
    ],
    home: Scaffold(body: Center(child: child)),
  );

  group('StatusVisual', () {
    test('connected, error and reconnecting never share a colour', () {
      // A rider reads state by colour first, so these three must stay distinct.
      final connected = StatusVisual.of(ConnStatus.connected).color;
      final failed = StatusVisual.of(ConnStatus.failed).color;
      final reconnecting = StatusVisual.of(ConnStatus.reconnecting).color;
      expect(connected, isNot(failed));
      expect(connected, isNot(reconnecting));
      expect(failed, isNot(reconnecting));
    });

    test('covers every ConnStatus value', () {
      // Colour and icon are context-free; the label is localised and covered
      // by the Localisation group below.
      for (final s in ConnStatus.values) {
        expect(StatusVisual.of(s).icon, isNotNull, reason: 'no icon for $s');
      }
    });
  });

  group('ButtonController', () {
    late ButtonController c;
    late List<bool> transmits;
    late int mutes;

    setUp(() {
      c = ButtonController.instance;
      c.setBindings([]);
      transmits = [];
      mutes = 0;
      c.onTransmit = transmits.add;
      c.onToggleMute = () => mutes++;
    });

    test('media key ids cannot collide with keyboard keys', () {
      // A remote's "play/pause" and some ordinary key must never map to the
      // same binding, or one would silently trigger the other.
      //
      // Flutter allocates logical key ids in two ranges: Unicode characters up
      // to 0x10FFFF, and everything else from 0x0100000000 upwards. The media
      // ids are placed in the gap between them, which is where nothing else
      // lives.
      const unicodeMax = 0x10FFFF;
      const flutterPlaneStart = 0x0100000000;

      for (final code in [24, 79, 85, 87, 88, 126, 127]) {
        final id = ButtonController.mediaKeyId(code);
        expect(id, greaterThan(unicodeMax), reason: 'code $code');
        expect(id, lessThan(flutterPlaneStart), reason: 'code $code');
      }

      expect(
        ButtonController.mediaKeyId(85),
        isNot(LogicalKeyboardKey.mediaPlayPause.keyId),
      );
      expect(
        ButtonController.mediaKeyId(85),
        isNot(LogicalKeyboardKey.space.keyId),
      );

      // Distinct codes stay distinct.
      expect(
        ButtonController.mediaKeyId(85),
        isNot(ButtonController.mediaKeyId(87)),
      );
    });

    test('hold-to-talk keys on press and unkeys on release', () {
      final id = ButtonController.mediaKeyId(79); // headset hook
      c.addBinding(ButtonBinding(keyId: id, action: ButtonAction.pushToTalk));

      c.handleMediaButton(79, true);
      c.handleMediaButton(79, false);
      expect(transmits, [true, false]);
    });

    test('toggle mode alternates and ignores the release', () {
      // Some remotes only ever send a click, never a release; toggle mode is
      // what makes those usable.
      final id = ButtonController.mediaKeyId(85);
      c.addBinding(
        ButtonBinding(keyId: id, action: ButtonAction.toggleTransmit),
      );

      c.handleMediaButton(85, true);
      c.handleMediaButton(85, false);
      c.handleMediaButton(85, true);
      c.handleMediaButton(85, false);
      expect(transmits, [true, false]);
    });

    test('unbound buttons do nothing', () {
      c.handleMediaButton(87, true);
      expect(transmits, isEmpty);
      expect(mutes, 0);
    });

    test('rebinding a key replaces the previous action', () {
      // Otherwise one press would fire two things at once.
      final id = ButtonController.mediaKeyId(85);
      c.addBinding(ButtonBinding(keyId: id, action: ButtonAction.pushToTalk));
      c.addBinding(ButtonBinding(keyId: id, action: ButtonAction.toggleMute));

      expect(c.bindings.where((b) => b.keyId == id).length, 1);
      c.handleMediaButton(85, true);
      expect(transmits, isEmpty);
      expect(mutes, 1);
    });

    test('bindings survive a round trip through JSON', () {
      final original = ButtonBinding(
        keyId: ButtonController.mediaKeyId(85),
        action: ButtonAction.toggleTransmit,
        label: 'Play / pause',
      );
      final back = ButtonBinding.fromJson(original.toJson())!;
      expect(back.keyId, original.keyId);
      expect(back.action, original.action);
      expect(back.label, original.label);
      expect(back.displayName, 'Play / pause');
    });

    test('malformed stored bindings are discarded rather than crashing', () {
      expect(ButtonBinding.fromJson({'keyId': 'nope', 'action': 0}), isNull);
      expect(ButtonBinding.fromJson({'keyId': 1, 'action': 999}), isNull);
      expect(ButtonBinding.fromJson({}), isNull);
    });
  });

  group('Localisation', () {
    /// Pumps an app in [locale] and hands back a context that resolves strings.
    Future<BuildContext> contextFor(WidgetTester tester, Locale locale) async {
      late BuildContext captured;
      await tester.pumpWidget(
        MaterialApp(
          locale: locale,
          supportedLocales: AppState.supportedLocales,
          localizationsDelegates: const [
            L.delegate,
            GlobalMaterialLocalizations.delegate,
            GlobalWidgetsLocalizations.delegate,
            // MaterialApp needs a Cupertino delegate covering every supported
            // locale, even on Android and desktop.
            GlobalCupertinoLocalizations.delegate,
          ],
          home: Builder(
            builder: (c) {
              captured = c;
              return const SizedBox();
            },
          ),
        ),
      );
      return captured;
    }

    testWidgets('every connection status has a label in both languages', (
      tester,
    ) async {
      for (final locale in AppState.supportedLocales) {
        final ctx = await contextFor(tester, locale);
        for (final s in ConnStatus.values) {
          expect(
            StatusVisual.labelOf(ctx, s),
            isNotEmpty,
            reason: 'no label for $s in ${locale.languageCode}',
          );
        }
      }
    });

    testWidgets('Russian actually differs from English', (tester) async {
      // Guards against a catalogue silently falling back to English, which
      // would look translated while being nothing of the sort.
      final en = L.of(await contextFor(tester, const Locale('en')));
      final ru = L.of(await contextFor(tester, const Locale('ru')));

      expect(ru.statusConnected, isNot(en.statusConnected));
      expect(ru.pttHoldToTalk, isNot(en.pttHoldToTalk));
      expect(ru.addServer, isNot(en.addServer));
      expect(ru.kick, isNot(en.kick));
    });

    testWidgets('placeholders survive translation', (tester) async {
      final ru = L.of(await contextFor(tester, const Locale('ru')));
      expect(ru.talkingOnMany(3), contains('3'));
      expect(ru.maxServersNote(2), contains('2'));
      expect(ru.reconnectingIn(5, 2), allOf(contains('5'), contains('2')));
      expect(ru.kickTitle('Alice'), contains('Alice'));
    });
  });

  testWidgets('StatusBadge shows the state label', (tester) async {
    await tester.pumpWidget(
      wrap(const StatusBadge(status: ConnStatus.connected)),
    );
    expect(find.text('Connected'), findsOneWidget);
  });

  testWidgets('StatusBadge distinguishes reconnecting from connected', (
    tester,
  ) async {
    await tester.pumpWidget(
      wrap(
        const StatusBadge(
          status: ConnStatus.reconnecting,
          detail: 'ping timeout',
        ),
      ),
    );
    expect(find.text('Reconnecting'), findsOneWidget);
    expect(find.text('Connected'), findsNothing);
  });

  testWidgets('TransportChip reports UDP with its ping', (tester) async {
    await tester.pumpWidget(
      wrap(const TransportChip(transport: 'udp', pingMs: 42)),
    );
    expect(find.textContaining('UDP'), findsOneWidget);
    expect(find.textContaining('42'), findsOneWidget);
  });

  testWidgets('TransportChip reports a tunnelled TCP fallback', (tester) async {
    await tester.pumpWidget(
      wrap(const TransportChip(transport: 'tcp', pingMs: 0)),
    );
    expect(find.textContaining('TCP'), findsOneWidget);
  });

  test('who is speaking comes from the audio, not the roster', () {
    // The server never reports it. Reading it off UiUser.talking left every
    // participant showing "silent" for as long as the roster went unchanged,
    // which is most of the time.
    final rt = ServerRuntime();
    expect(rt.isSpeaking(7), isFalse, reason: 'nothing heard yet');

    rt.speakerLevels[7] = -20;
    expect(rt.isSpeaking(7), isTrue);
    expect(rt.speakerLevels[7], -20);

    // Falls back to silent as the level decays.
    rt.speakerLevels[7] = -90;
    expect(rt.isSpeaking(7), isFalse);
  });

  test('a meter falls to silence when its speaker stops being reported', () {
    // A speaker who stops talking is reaped from the mixer and simply drops
    // out of the reports. Without decay their meter freezes at whatever it
    // last showed, so a full bar sits beside someone who went quiet long ago.
    final rt = ServerRuntime();
    rt.noteSpeakerLevel(3, -18);
    expect(rt.isSpeaking(3), isTrue);

    var previous = rt.speakerLevels[3]!;
    for (var i = 0; i < 40 && rt.speakerLevels.containsKey(3); i++) {
      rt.decayUnreported(const <int>{});
      final now = rt.speakerLevels[3];
      if (now != null) {
        expect(now, lessThan(previous), reason: 'must keep falling');
        previous = now;
      }
    }
    expect(rt.isSpeaking(3), isFalse, reason: 'never reached silence');
  });

  test('a meter rises at once but falls gradually', () {
    // Rising slowly would clip the start of every word; falling instantly
    // would make the meter flicker between syllables.
    final rt = ServerRuntime();
    rt.noteSpeakerLevel(1, -20);
    expect(rt.speakerLevels[1], -20, reason: 'a rise is immediate');

    rt.noteSpeakerLevel(1, -90);
    expect(rt.speakerLevels[1], greaterThan(-90.0), reason: 'a fall is paced');
    expect(rt.speakerLevels[1], lessThan(-20.0));
  });

  test('the meter scale is shared and clamped', () {
    // Every surface that draws a level, a threshold or a noise floor goes
    // through this, so a drift here would silently misreport the margin
    // between the markers rather than look wrong.
    expect(OverlayBridge.meterFraction(0), 1.0);
    expect(OverlayBridge.meterFraction(VoiceMeter.floorDb), 0.0);
    expect(
      OverlayBridge.meterFraction(VoiceMeter.floorDb / 2),
      closeTo(0.5, 1e-9),
    );
    // Silence is reported as -120 dBFS, well below the floor.
    expect(OverlayBridge.meterFraction(-120), 0.0);
    expect(OverlayBridge.meterFraction(12), 1.0);
    expect(OverlayBridge.meterFraction(double.nan), 0.0);
    expect(OverlayBridge.meterFraction(double.negativeInfinity), 0.0);

    // The floating window draws its own meter natively; it must use the very
    // same scale, or the same voice reads differently in two places at once.
    expect(OverlayBridge.meterFraction(-25), VoiceMeter.fractionFor(-25));
  });

  testWidgets('the on-air light flashes rather than sitting steady', (
    tester,
  ) async {
    await tester.pumpWidget(wrap(const OnAirIndicator()));
    // Scoped to the indicator: the surrounding MaterialApp has fades of its
    // own, so an unscoped finder matches more than one.
    final fade = find.descendant(
      of: find.byType(OnAirIndicator),
      matching: find.byType(FadeTransition),
    );
    double opacityNow() => tester.widget<FadeTransition>(fade).opacity.value;

    final first = opacityNow();
    await tester.pump(const Duration(milliseconds: 350));
    expect(opacityNow(), isNot(closeTo(first, 0.05)));

    // Repeating, not a one-shot fade that leaves it dark for the rest of the
    // transmission.
    await tester.pump(const Duration(milliseconds: 350));
    await tester.pump(const Duration(milliseconds: 350));
    expect(opacityNow(), greaterThan(0.25));
  });

  test('only Picture in Picture lacks a deafen control', () {
    // The system owns the buttons there and offers exactly three, which go to
    // talk, mute and hang up. Deafen is the one that gives way, being a
    // comfort setting rather than a control the call needs.
    expect(
      OverlayBridge.hasDeafenFor(FloatingKind.iosPictureInPicture),
      isFalse,
    );
    expect(OverlayBridge.hasDeafenFor(FloatingKind.androidOverlay), isTrue);
    expect(OverlayBridge.hasDeafenFor(FloatingKind.macosPanel), isTrue);
    expect(OverlayBridge.hasDeafenFor(FloatingKind.none), isFalse);
  });

  group('audio session', () {
    // The bug this guards: iOS reports zero input channels until the session
    // is configured, and the engine then fails inside CoreAudio with wording
    // that names the symptom and not the cause.
    test('a refused microphone is not usable', () {
      const s = AudioSessionState(
        granted: false,
        inputChannels: 2,
        sampleRate: 48000,
      );
      expect(s.usable, isFalse);
    });

    test('granted but silent hardware is not usable either', () {
      const s = AudioSessionState(
        granted: true,
        inputChannels: 0,
        sampleRate: 48000,
      );
      expect(s.usable, isFalse);
    });

    test('platforms without a session are never blocked by this', () {
      // -1 rather than 0: nothing was asked, so nothing was refused, and the
      // startup check must not read that as an absent microphone.
      expect(AudioSessionState.notNeeded.usable, isTrue);
      expect(AudioSessionState.notNeeded.inputChannels, isNot(0));
    });

    test('it is only iOS that needs one', () {
      expect(
        AudioSessionBridge.instance.isNeeded,
        Platform.isIOS,
        reason: 'macOS, Android and Windows have no session to configure',
      );
    });
  });

  group('app bar title', () {
    /// Renders the bar itself at [width], which is what the title measures
    /// against. Squeezing it with actions inside a full-width bar does not
    /// work: the test surface is 800 wide and the title keeps plenty of room.
    Future<void> pumpAt(WidgetTester tester, double width) async {
      await tester.pumpWidget(
        MaterialApp(
          home: Center(
            child: SizedBox(
              width: width,
              child: AppBar(title: const AppBarTitle('MumbleWay')),
            ),
          ),
        ),
      );
    }

    testWidgets('shows the name when there is room for all of it', (
      tester,
    ) async {
      await pumpAt(tester, 380);
      expect(find.text('MumbleWay'), findsOneWidget);
    });

    testWidgets('drops the name rather than truncating it', (tester) async {
      // A cut-off product name reads as a rendering fault. The icon alone
      // reads as a decision.
      await pumpAt(tester, 40);
      expect(find.text('MumbleWay'), findsNothing);
    });

    testWidgets('but a screen reader is still told what the app is', (
      tester,
    ) async {
      final handle = tester.ensureSemantics();
      await pumpAt(tester, 40);
      expect(find.bySemanticsLabel('MumbleWay'), findsOneWidget);
      handle.dispose();
    });
  });

  group('translation coverage', () {
    // The gaps found on a real phone were of two kinds: keys translated in the
    // .arb files but never used by the widget, and strings that were never
    // keys at all. This catches neither directly — but it does catch the third
    // kind, a key added to English and forgotten in Russian, which is how the
    // first kind starts.
    test('every string exists in both languages, and differs', () async {
      final en =
          jsonDecode(await File('lib/l10n/app_en.arb').readAsString())
              as Map<String, dynamic>;
      final ru =
          jsonDecode(await File('lib/l10n/app_ru.arb').readAsString())
              as Map<String, dynamic>;

      bool isString(String k) => !k.startsWith('@');
      final enKeys = en.keys.where(isString).toSet();
      final ruKeys = ru.keys.where(isString).toSet();

      expect(enKeys.difference(ruKeys), isEmpty, reason: 'missing in Russian');
      expect(ruKeys.difference(enKeys), isEmpty, reason: 'stale in Russian');

      // A handful are legitimately identical: the product name, and hints that
      // are format examples rather than prose.
      const sameOnPurpose = {
        'appTitle',
        'proxyHostPortHint',
        'serverAddressHint',
      };
      final untranslated = [
        for (final k in enKeys)
          if (!sameOnPurpose.contains(k) &&
              en[k] == ru[k] &&
              (en[k] as String).length > 3)
            k,
      ];
      expect(untranslated, isEmpty, reason: 'English left in the Russian file');
    });
  });
}
