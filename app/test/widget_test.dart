import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mumbleway/services/proxy.dart';
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

  Widget wrap(Widget child) => MaterialApp(
        theme: buildTheme(Brightness.dark),
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
      for (final s in ConnStatus.values) {
        expect(StatusVisual.of(s).label, isNotEmpty, reason: 'no label for $s');
      }
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
