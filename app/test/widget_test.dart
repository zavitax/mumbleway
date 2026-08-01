import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mumbleway/src/rust/api/mumbleway.dart';
import 'package:mumbleway/theme.dart';
import 'package:mumbleway/widgets/status_badge.dart';

void main() {
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
