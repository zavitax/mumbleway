import 'package:flutter/material.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mumbleway/l10n/app_localizations.dart';
import 'package:mumbleway/src/rust/api/mumbleway.dart';
import 'package:mumbleway/widgets/connection_quality.dart';

/// The server measures every rider's connection and will tell any client in the
/// same channel. What is pinned here is the reading of those numbers, because
/// the numbers themselves are never wrong and the reading easily is: a rider
/// glances at this once, at speed, and either goes looking for the person who
/// is breaking up or does not.
UiQuality q({
  double ping = 30,
  bool udp = true,
  double up = 0,
  double down = 0,
  int window = 60,
  int idle = 0,
}) => UiQuality(
  pingMs: ping,
  udp: udp,
  lossUp: up,
  lossDown: down,
  windowSecs: window,
  idleSecs: idle,
);

void main() {
  group('grading a connection', () {
    test('a healthy link says nothing', () {
      expect(gradeFor(q(ping: 28, up: 0.001, down: 0)), LinkGrade.good);
    });

    test('loss decides it before ping does', () {
      // Fast and lossy is the connection that breaks up. A rider reading
      // "30 ms" while hearing half the words would stop trusting this.
      expect(gradeFor(q(ping: 30, up: 0.12)), LinkGrade.poor);
    });

    test('the worse direction wins', () {
      // Losing what the server sends them is the same fault pointed the other
      // way: they answer questions nobody asked.
      expect(gradeFor(q(up: 0, down: 0.12)), LinkGrade.poor);
      expect(gradeFor(q(up: 0.12, down: 0)), LinkGrade.poor);
    });

    test('a long round trip alone is worth flagging', () {
      // Nothing is lost; people simply talk over each other.
      expect(gradeFor(q(ping: 140)), LinkGrade.fair);
      expect(gradeFor(q(ping: 420)), LinkGrade.poor);
    });

    test('the ordinary odd packet is not a fault', () {
      expect(gradeFor(q(up: 0.01)), LinkGrade.good);
      expect(gradeFor(q(up: 0.03)), LinkGrade.fair);
    });
  });

  group('what the tooltip says', () {
    late L en;
    late L ru;

    setUpAll(() async {
      en = await L.delegate.load(const Locale('en'));
      ru = await L.delegate.load(const Locale('ru'));
    });

    test('names both directions and the window, in both languages', () {
      for (final l in [en, ru]) {
        final text = describeQuality(l, q(ping: 42, up: 0.05, down: 0.01));
        expect(text, contains('42'));
        expect(text, contains('5%'));
        expect(text, contains('1%'));
        expect(text.split('\n').length, 4);
      }
    });

    test('a tunnelled rider is told apart from a direct one', () {
      final direct = describeQuality(en, q(ping: 42));
      final tunnelled = describeQuality(en, q(ping: 42, udp: false));
      expect(direct, isNot(tunnelled));
      expect(tunnelled, contains('TCP'));
    });

    test('losing a little is never reported as losing nothing', () {
      // 0.4% rounds to zero, and "0% lost" beside audible dropouts is worse
      // than no figure at all.
      expect(describeQuality(en, q(up: 0.004)), contains(en.qualityLossUp(1)));
      // A direction that really lost nothing still says zero, which is the
      // half of this that makes the rounding above honest.
      expect(describeQuality(en, q(up: 0.004)), contains(en.qualityLossDown(0)));
    });

    test('before the rolling window fills, it says so', () {
      expect(
        describeQuality(en, q(window: 0)),
        contains(en.qualitySinceConnect),
      );
    });
  });

  testWidgets('the bars are quiet when all is well and loud when it is not', (
    tester,
  ) async {
    Future<Color?> colourFor(UiQuality quality) async {
      await tester.pumpWidget(
        MaterialApp(
          localizationsDelegates: const [
            ...L.localizationsDelegates,
            GlobalMaterialLocalizations.delegate,
            GlobalWidgetsLocalizations.delegate,
          ],
          supportedLocales: L.supportedLocales,
          home: Scaffold(body: ConnectionQualityBars(quality: quality)),
        ),
      );
      return tester.widget<Icon>(find.byType(Icon)).color;
    }

    final healthy = await colourFor(q());
    final broken = await colourFor(q(up: 0.2));
    expect(healthy!.a, lessThan(0.5), reason: 'a channel of healthy riders has no colour in it');
    expect(broken!.a, 1.0, reason: 'the one who is breaking up is the thing the eye finds');
    expect(healthy.toARGB32(), isNot(broken.toARGB32()));
  });
}
