import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mumbleway/services/server_html.dart';
import 'package:mumbleway/widgets/server_text.dart';

/// Links in a server's own text.
///
/// A welcome message saying "it is written up **here**, with pictures" had
/// nothing to tap: the pane deleted every tag with one regular expression, so
/// the anchor's address went out with the anchor and the sentence was left with
/// a hole in it. What this has to prove is that links now work **and that not a
/// character of the visible text moved**, because that was the condition.
void main() {
  /// Exactly what the pane did before any of this existed.
  String oldBehaviour(String html) =>
      html.replaceAll(RegExp(r'<[^>]*>'), ' ').trim();

  group('the words do not move', () {
    const samples = [
      'Plain text with no markup at all.',
      'Настраиваем push-to-talk ( <a href="https://example.test/ptt">тут '
          'написано как это сделать, с картинками</a> ) или выбираем в меню.',
      '<b>Welcome</b> to the <i>server</i>.<br/>Rules below.',
      'Mixed <a href="http://a.test">one</a> and <a href="http://b.test">two</a>.',
      'Entities stay as they are: &amp; and &quot;',
      'A half tag survives: safe <b',
      '   leading and trailing   ',
      '<p>only tags</p>',
      '',
    ];

    for (final html in samples) {
      test('"${html.length > 34 ? '${html.substring(0, 34)}…' : html}"', () {
        expect(
          serverTextPlain(html),
          oldBehaviour(html),
          reason: 'the visible text changed, and it was asked not to',
        );
      });
    }
  });

  group('links', () {
    test('an anchor becomes one, and carries its address', () {
      final spans = parseServerText(
        'see <a href="https://example.test/ptt">here</a> for how',
      );
      final link = spans.singleWhere((s) => s.href != null);
      expect(link.text, 'here');
      expect(link.href, Uri.parse('https://example.test/ptt'));
    });

    test('the text around it stays ordinary', () {
      final spans = parseServerText('see <a href="http://a.test">here</a> now');
      expect(spans.where((s) => s.href != null).length, 1);
      expect(serverTextPlain('see <a href="http://a.test">here</a> now'),
          'see  here  now');
    });

    test('an address with an escaped ampersand is unescaped', () {
      // Only in the href. The *text* keeps its entities, which is the rule
      // this feature was given.
      final spans = parseServerText(
        '<a href="https://e.test/?a=1&amp;b=2">x</a>',
      );
      expect(spans.single.href, Uri.parse('https://e.test/?a=1&b=2'));
    });

    test('single quotes and bare attributes are read too', () {
      expect(
        parseServerText("<a href='http://a.test'>x</a>").single.href,
        Uri.parse('http://a.test'),
      );
      expect(
        parseServerText('<a href=http://b.test>x</a>').single.href,
        Uri.parse('http://b.test'),
      );
    });

    test('mailto is offered', () {
      expect(
        parseServerText('<a href="mailto:a@b.test">write</a>').single.href,
        Uri.parse('mailto:a@b.test'),
      );
    });
  });

  group('what a server is not allowed to hand a rider', () {
    // **The welcome message is arbitrary text from whoever runs the server.**
    // An allow-list, so a scheme nobody thought about is inert rather than
    // live. Each of these stays ordinary text, exactly as it was before links
    // worked at all.
    const refused = [
      'javascript:alert(1)',
      'data:text/html,<script>x</script>',
      'file:///etc/passwd',
      'mumble://evil.test:64738/',
      'mumble-proxy://evil.test:1080/',
    ];

    for (final href in refused) {
      test('"${href.split(':').first}:" is not a link', () {
        final html = '<a href="$href">tap me</a>';
        expect(parseServerText(html).every((s) => s.href == null), isTrue,
            reason: href);
        // Against the old behaviour rather than a guess: an address carrying a
        // `>` truncates its own tag, and did so before this too. The contract
        // is "the same text", not "the text I would have written".
        expect(serverTextPlain(html), oldBehaviour(html));
      });
    }

    test('an anchor with no address is not a link', () {
      expect(parseServerText('<a>bare</a>').single.href, isNull);
    });

    test('a relative address is not a link', () {
      // Relative to what? There is no page this text came from.
      expect(parseServerText('<a href="/ptt">x</a>').single.href, isNull);
    });
  });

  group('a channel description keeps the text the core used to produce', () {
    // The core stripped these itself and sent plain text, which threw every
    // address away before anything could draw it. It now sends the markup and
    // this does the stripping — so these are `strip_html`'s own cases, ported,
    // and they are what proves no description changed on screen.
    const cases = {
      '<p>back in <b>ten</b></p>': 'back in ten',
      'On the A9<br/>heading north': 'On the A9 heading north',
      'Tom &amp; Jerry &lt;3': 'Tom & Jerry <3',
      '&quot;quoted&quot;': '"quoted"',
      'R&D on tyres': 'R&D on tyres',
      'fish & chips': 'fish & chips',
      'safe <b': 'safe',
      // Qt's editor writes one of these into every description it saves, so
      // without dropping its contents a channel reads as a stylesheet.
      '<style>p, li { white-space: pre-wrap; }</style>Sunday run':
          'Sunday run',
      '<script>alert(1)</script>Hello': 'Hello',
      '  spaced   out  ': 'spaced out',
    };

    cases.forEach((html, want) {
      test('"$html"', () {
        expect(serverDescriptionPlain(html), want);
      });
    });

    test("and it is still cut at the core's limit", () {
      final long = 'x' * 900;
      expect(serverDescriptionPlain(long).length, kServerTextMaxChars);
    });
  });

  group('links in a channel description', () {
    test('an anchor survives the stripping', () {
      final spans = parseServerDescription(
        '<p>Read <a href="https://e.test/how">the guide</a> first.</p>',
      );
      final link = spans.singleWhere((s) => s.href != null);
      expect(link.text, 'the guide');
      expect(link.href, Uri.parse('https://e.test/how'));
      expect(
        serverDescriptionPlain(
          '<p>Read <a href="https://e.test/how">the guide</a> first.</p>',
        ),
        'Read the guide first.',
      );
    });

    test('a link inside a Qt style block is not resurrected', () {
      // The contents of an opaque element are not text, and an anchor written
      // inside one is not an offer to the rider.
      final spans = parseServerDescription(
        '<style><a href="https://evil.test">x</a></style>ok',
      );
      expect(spans.every((s) => s.href == null), isTrue);
      expect(serverDescriptionPlain(
        '<style><a href="https://evil.test">x</a></style>ok',
      ), 'ok');
    });

    test('the same schemes are refused here', () {
      expect(
        parseServerDescription('<a href="javascript:x">tap</a>')
            .every((s) => s.href == null),
        isTrue,
      );
    });
  });

  testWidgets('the widget draws the words and leaks no recognizer', (
    tester,
  ) async {
    await tester.pumpWidget(
      const MaterialApp(
        home: Scaffold(
          body: ServerText(
            'see <a href="https://example.test">here</a> for how',
          ),
        ),
      ),
    );
    await tester.pump();

    expect(find.textContaining('here'), findsOneWidget);
    // Disposing with a live recognizer attached is what would throw.
    await tester.pumpWidget(const MaterialApp(home: Scaffold(body: SizedBox())));
    await tester.pump();
  });
}
