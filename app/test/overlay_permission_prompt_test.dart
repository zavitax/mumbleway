import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mumbleway/l10n/app_localizations.dart';
import 'package:mumbleway/screens/settings_screen.dart';
import 'package:mumbleway/services/overlay.dart';
import 'package:mumbleway/state/app_state.dart';
import 'package:shared_preferences/shared_preferences.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  const overlayChannel = MethodChannel('mumbleway/overlay');

  void resetOverlayBridge() {
    OverlayBridge.kindOverride = null;
    final bridge = OverlayBridge.instance;
    bridge.onTransmit = null;
    bridge.onToggleMute = null;
    bridge.onToggleDeafen = null;
    bridge.onHangup = null;
    bridge.onDismissed = null;
    bridge.onStatus = null;
    TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
        .setMockMethodCallHandler(overlayChannel, null);
  }

  List<MethodCall> installOverlayChannel({
    required bool Function() hasPermission,
    bool Function()? requestPermission,
  }) {
    final calls = <MethodCall>[];
    TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
        .setMockMethodCallHandler(overlayChannel, (call) async {
          calls.add(call);
          return switch (call.method) {
            'hasPermission' => hasPermission(),
            'requestPermission' => (requestPermission ?? hasPermission)(),
            'show' => true,
            'hide' => true,
            'phrases' => null,
            _ => null,
          };
        });
    return calls;
  }

  Widget hostSettings(AppState state) => AppStateScope(
    state: state,
    child: MaterialApp(
      locale: const Locale('en'),
      localizationsDelegates: const [
        ...L.localizationsDelegates,
        GlobalMaterialLocalizations.delegate,
        GlobalWidgetsLocalizations.delegate,
        GlobalCupertinoLocalizations.delegate,
      ],
      supportedLocales: L.supportedLocales,
      home: const SettingsScreen(),
    ),
  );

  SavedServer sampleServer() => SavedServer(
    name: 'Club',
    host: 'example.com',
    port: 64738,
    username: 'rider',
  );

  setUp(() {
    SharedPreferences.setMockInitialValues({});
    resetOverlayBridge();
  });

  tearDown(resetOverlayBridge);

  Future<bool> granted() async => true;
  Future<bool> refused() async => false;

  group('mayAskForOverlayAtLaunch', () {
    test('launch asks for the overlay permission only once', () async {
      final prefs = await SharedPreferences.getInstance();
      expect(
        await AppState.mayAskForOverlayAtLaunch(
          prefs,
          refused,
          hasServers: true,
        ),
        isTrue,
      );
      // The rider backed out of the settings page without granting it. Every
      // later launch must leave them in the app.
      expect(
        await AppState.mayAskForOverlayAtLaunch(
          prefs,
          refused,
          hasServers: true,
        ),
        isFalse,
      );
      expect(
        await AppState.mayAskForOverlayAtLaunch(
          prefs,
          refused,
          hasServers: true,
        ),
        isFalse,
      );
    });

    test('the record of asking survives a restart', () async {
      final prefs = await SharedPreferences.getInstance();
      await AppState.mayAskForOverlayAtLaunch(prefs, refused, hasServers: true);
      final values = prefs.getKeys().map((k) => MapEntry(k, prefs.get(k)!));
      SharedPreferences.setMockInitialValues(Map.fromEntries(values));
      final again = await SharedPreferences.getInstance();
      expect(
        await AppState.mayAskForOverlayAtLaunch(
          again,
          refused,
          hasServers: true,
        ),
        isFalse,
      );
    });

    test('a granted permission always restores the window', () async {
      final prefs = await SharedPreferences.getInstance();
      await AppState.mayAskForOverlayAtLaunch(prefs, refused, hasServers: true);
      // Granted later from system settings: nothing to ask, so go ahead.
      expect(
        await AppState.mayAskForOverlayAtLaunch(
          prefs,
          granted,
          hasServers: true,
        ),
        isTrue,
      );
    });

    test('granted at first launch spends no ask', () async {
      final prefs = await SharedPreferences.getInstance();
      expect(
        await AppState.mayAskForOverlayAtLaunch(
          prefs,
          granted,
          hasServers: true,
        ),
        isTrue,
      );
      // Revoked afterwards: the one ask is still available.
      expect(
        await AppState.mayAskForOverlayAtLaunch(
          prefs,
          refused,
          hasServers: true,
        ),
        isTrue,
      );
    });

    test(
      'empty server list does not ask on cold launch and saves the ask',
      () async {
        final prefs = await SharedPreferences.getInstance();
        expect(
          await AppState.mayAskForOverlayAtLaunch(
            prefs,
            refused,
            hasServers: false,
          ),
          isFalse,
        );
        expect(prefs.getBool('mumbleway.floatingWindowAsked'), isNull);

        // Once the rider has a server, the one-time launch prompt is still
        // available, and then spent after prompting once.
        expect(
          await AppState.mayAskForOverlayAtLaunch(
            prefs,
            refused,
            hasServers: true,
          ),
          isTrue,
        );
        expect(
          await AppState.mayAskForOverlayAtLaunch(
            prefs,
            refused,
            hasServers: true,
          ),
          isFalse,
        );
      },
    );
  });

  group('_restoreOverlay', () {
    test(
      'binds callbacks and phrases even when enableOverlay is skipped',
      () async {
        OverlayBridge.kindOverride = FloatingKind.androidOverlay;
        SharedPreferences.setMockInitialValues({
          'mumbleway.floatingWindowAsked': true,
        });
        final calls = installOverlayChannel(hasPermission: () => false);
        final state = AppState()..servers.add(sampleServer());
        addTearDown(state.dispose);

        await state.restoreOverlayForTesting();

        expect(state.overlay.onTransmit, equals(state.setTransmit));
        expect(state.overlay.onToggleMute, equals(state.toggleMute));
        expect(state.overlay.onToggleDeafen, equals(state.toggleDeafen));
        expect(state.overlay.onHangup, equals(state.hangupAll));
        expect(state.overlay.onDismissed, isNotNull);
        expect(state.overlay.onStatus, isNotNull);

        final phraseCalls = calls.where((c) => c.method == 'phrases').toList();
        expect(phraseCalls, hasLength(1));
        expect(phraseCalls.single.arguments, isA<Map>());
        expect(
          calls.where((c) => c.method == 'requestPermission'),
          isEmpty,
        );
        expect(calls.where((c) => c.method == 'show'), isEmpty);

        // Verify the bound callbacks drive AppState without going through
        // enableOverlay().
        expect(state.muted, isFalse);
        state.overlay.onToggleMute!();
        expect(state.muted, isTrue);

        expect(state.deafened, isFalse);
        state.overlay.onToggleDeafen!();
        expect(state.deafened, isTrue);
      },
    );

    test(
      'skips permission prompt on cold launch with no servers, asks once '
      'after a server is saved, and binds callbacks immediately',
      () async {
        OverlayBridge.kindOverride = FloatingKind.androidOverlay;
        final calls = installOverlayChannel(hasPermission: () => false);
        final state = AppState();
        addTearDown(state.dispose);

        await state.restoreOverlayForTesting();

        // Callbacks and phrases are already wired up even on an empty launch,
        // without touching OS permission settings or spending the one-time ask.
        expect(state.overlay.onTransmit, equals(state.setTransmit));
        expect(state.overlay.onToggleMute, equals(state.toggleMute));
        expect(state.overlay.onToggleDeafen, equals(state.toggleDeafen));
        expect(state.overlay.onHangup, equals(state.hangupAll));
        expect(calls.where((c) => c.method == 'phrases'), isNotEmpty);
        expect(calls.where((c) => c.method == 'hasPermission'), isEmpty);
        expect(calls.where((c) => c.method == 'requestPermission'), isEmpty);

        final prefs = await SharedPreferences.getInstance();
        expect(prefs.getBool('mumbleway.floatingWindowAsked'), isNull);

        // After adding a server, the next launch asks once, and not again.
        state.servers.add(sampleServer());
        await state.restoreOverlayForTesting();
        expect(calls.where((c) => c.method == 'requestPermission'), hasLength(1));
        expect(prefs.getBool('mumbleway.floatingWindowAsked'), isTrue);

        await state.restoreOverlayForTesting();
        expect(calls.where((c) => c.method == 'requestPermission'), hasLength(1));
      },
    );

    test('does not bind callbacks when overlay is unsupported', () async {
      OverlayBridge.kindOverride = FloatingKind.none;
      final calls = installOverlayChannel(hasPermission: () => false);
      final state = AppState()..servers.add(sampleServer());
      addTearDown(state.dispose);

      await state.restoreOverlayForTesting();

      expect(state.overlay.onTransmit, isNull);
      expect(state.overlay.onToggleMute, isNull);
      expect(state.overlay.onToggleDeafen, isNull);
      expect(state.overlay.onHangup, isNull);
      expect(calls, isEmpty);
    });
  });

  group('wantOverlay preference and Settings switch', () {
    test(
      'disableOverlay and enableOverlay update wantOverlay, persist, and '
      'notify with no live server',
      () async {
        OverlayBridge.kindOverride = FloatingKind.androidOverlay;
        final calls = installOverlayChannel(hasPermission: () => true);
        final state = AppState();
        addTearDown(state.dispose);
        var notifications = 0;
        state.addListener(() => notifications++);

        expect(state.wantOverlay, isTrue);
        expect(state.overlayEnabled, isFalse);

        await state.disableOverlay();
        expect(state.wantOverlay, isFalse);
        expect(state.overlayEnabled, isFalse);
        expect(notifications, 1);
        expect(calls.where((c) => c.method == 'hide'), hasLength(1));

        final prefs = await SharedPreferences.getInstance();
        expect(prefs.getBool('mumbleway.floatingWindow'), isFalse);

        final error = await state.enableOverlay();
        expect(error, isNull);
        expect(state.wantOverlay, isTrue);
        expect(state.overlayEnabled, isFalse);
        expect(notifications, 2);
        expect(prefs.getBool('mumbleway.floatingWindow'), isTrue);
        expect(calls.where((c) => c.method == 'show'), isEmpty);
      },
    );

    testWidgets(
      'Settings switch binds to wantOverlay and toggles cleanly without a '
      'live server',
      (t) async {
        OverlayBridge.kindOverride = FloatingKind.androidOverlay;
        var perm = true;
        final calls = installOverlayChannel(
          hasPermission: () => perm,
          requestPermission: () => perm,
        );
        final state = AppState();
        addTearDown(state.dispose);

        t.view.physicalSize = const Size(430, 2400);
        t.view.devicePixelRatio = 1.0;
        addTearDown(t.view.resetPhysicalSize);
        addTearDown(t.view.resetDevicePixelRatio);

        await t.pumpWidget(hostSettings(state));
        for (var i = 0; i < 5; i++) {
          await t.pump(const Duration(milliseconds: 16));
        }

        final l = await L.delegate.load(const Locale('en'));
        await t.scrollUntilVisible(find.text(l.floatingWindow), 300);
        final tileFinder = find.ancestor(
          of: find.text(l.floatingWindow),
          matching: find.byType(SwitchListTile),
        );
        await t.ensureVisible(tileFinder);
        await t.pump(const Duration(milliseconds: 50));

        // Reflects wantOverlay (true) even when overlayEnabled is false.
        expect(state.overlayEnabled, isFalse);
        expect(t.widget<SwitchListTile>(tileFinder).value, isTrue);

        // Toggle OFF with no live server.
        await t.tap(tileFinder);
        for (var i = 0; i < 5; i++) {
          await t.pump(const Duration(milliseconds: 50));
        }
        expect(state.wantOverlay, isFalse);
        expect(state.overlayEnabled, isFalse);
        expect(t.widget<SwitchListTile>(tileFinder).value, isFalse);
        expect(calls.where((c) => c.method == 'hide'), hasLength(1));
        var prefs = await SharedPreferences.getInstance();
        expect(prefs.getBool('mumbleway.floatingWindow'), isFalse);

        // Toggle ON with permission granted and still no live server.
        await t.tap(tileFinder);
        for (var i = 0; i < 5; i++) {
          await t.pump(const Duration(milliseconds: 50));
        }
        expect(state.wantOverlay, isTrue);
        expect(state.overlayEnabled, isFalse);
        expect(t.widget<SwitchListTile>(tileFinder).value, isTrue);
        expect(calls.where((c) => c.method == 'show'), isEmpty);
        prefs = await SharedPreferences.getInstance();
        expect(prefs.getBool('mumbleway.floatingWindow'), isTrue);

        // Toggle OFF, then try toggling ON when permission is denied.
        await t.tap(tileFinder);
        for (var i = 0; i < 5; i++) {
          await t.pump(const Duration(milliseconds: 50));
        }
        expect(state.wantOverlay, isFalse);

        perm = false;
        await t.tap(tileFinder);
        for (var i = 0; i < 5; i++) {
          await t.pump(const Duration(milliseconds: 50));
        }
        expect(state.wantOverlay, isFalse);
        expect(t.widget<SwitchListTile>(tileFinder).value, isFalse);
        expect(
          find.text(
            'Allow "display over other apps" for MumbleWay, then try again.',
          ),
          findsOneWidget,
        );
      },
    );
  });
}
