import 'package:flutter/material.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mumbleway/l10n/app_localizations.dart';
import 'package:mumbleway/services/proxy.dart';
import 'package:mumbleway/state/app_state.dart';
import 'package:mumbleway/src/rust/api/mumbleway.dart';
import 'package:mumbleway/theme.dart';
import 'package:mumbleway/widgets/status_badge.dart';

void main() {
  group('SystemProxy parsing', () {
    test('reads a plain host:port and strips any scheme', () {
      expect(SystemProxy.stripScheme('http://127.0.0.1:10809'), '127.0.0.1:10809');
      expect(SystemProxy.stripScheme('127.0.0.1:10809'), '127.0.0.1:10809');
      expect(SystemProxy.pickFromWindowsValue('http://10.0.0.1:8080'),
          '10.0.0.1:8080');
    });

    test('prefers the https entry in a per-scheme list', () {
      // Windows stores either one value or "http=a;https=b"; this app fetches
      // over https, so that entry is the relevant one.
      const value = 'http=proxy-a:1;https=proxy-b:2;ftp=proxy-c:3';
      expect(SystemProxy.pickFromWindowsValue(value), 'proxy-b:2');
    });

    test('falls back to the http entry when there is no https one', () {
      expect(SystemProxy.pickFromWindowsValue('http=proxy-a:1;ftp=proxy-c:3'),
          'proxy-a:1');
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

  group('Localisation', () {
    /// Pumps an app in [locale] and hands back a context that resolves strings.
    Future<BuildContext> contextFor(WidgetTester tester, Locale locale) async {
      late BuildContext captured;
      await tester.pumpWidget(MaterialApp(
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
        home: Builder(builder: (c) {
          captured = c;
          return const SizedBox();
        }),
      ));
      return captured;
    }

    testWidgets('every connection status has a label in both languages',
        (tester) async {
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
    await tester.pumpWidget(wrap(
      const StatusBadge(status: ConnStatus.connected),
    ));
    expect(find.text('Connected'), findsOneWidget);
  });

  testWidgets('StatusBadge distinguishes reconnecting from connected',
      (tester) async {
    await tester.pumpWidget(wrap(
      const StatusBadge(status: ConnStatus.reconnecting, detail: 'ping timeout'),
    ));
    expect(find.text('Reconnecting'), findsOneWidget);
    expect(find.text('Connected'), findsNothing);
  });

  testWidgets('TransportChip reports UDP with its ping', (tester) async {
    await tester.pumpWidget(wrap(
      const TransportChip(transport: 'udp', pingMs: 42),
    ));
    expect(find.textContaining('UDP'), findsOneWidget);
    expect(find.textContaining('42'), findsOneWidget);
  });

  testWidgets('TransportChip reports a tunnelled TCP fallback', (tester) async {
    await tester.pumpWidget(wrap(
      const TransportChip(transport: 'tcp', pingMs: 0),
    ));
    expect(find.textContaining('TCP'), findsOneWidget);
  });
}
