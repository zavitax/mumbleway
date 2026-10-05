/// The scraps of HTML a Mumble server puts in its text, turned into something
/// that can be drawn.
///
/// Servers routinely put markup in the welcome message — Mumble's own client
/// renders it, so operators write it. This client never drew any of it: the
/// welcome pane deleted every tag with a regular expression and showed what was
/// left, which reads fine right up to the moment the text says "click **here**"
/// and there is nothing to click, because the address went out with the tag.
library;

/// One run of server text, and where it points if it points anywhere.
class ServerTextSpan {
  const ServerTextSpan(this.text, {this.href});

  final String text;

  /// The link's destination, or `null` for ordinary text.
  ///
  /// Only ever `http`, `https` or `mailto` — see [_safeHref].
  final Uri? href;

  @override
  String toString() => href == null ? 'text($text)' : 'link($href, $text)';
}

/// Splits server text into runs, keeping links as links.
///
/// **The visible text is exactly what it was before.** Every tag becomes a
/// single space and the whole run is trimmed at the ends, which is precisely
/// what the regular expression this replaces did, down to leaving an
/// unterminated tag standing as ordinary text. Nothing moves on screen; the
/// only difference is that the words inside an anchor now carry its address.
///
/// Entities are deliberately **not** decoded in the visible text, for the same
/// reason: `&amp;` renders as `&amp;` today, and changing that would move text
/// nobody asked to move. They are decoded inside an `href`, which is not text
/// anybody reads.
List<ServerTextSpan> parseServerText(String html) {
  final out = <ServerTextSpan>[];
  final buffer = StringBuffer();
  Uri? open;

  void flush() {
    if (buffer.isEmpty) return;
    out.add(ServerTextSpan(buffer.toString(), href: open));
    buffer.clear();
  }

  var i = 0;
  while (i < html.length) {
    final lt = html.indexOf('<', i);
    if (lt < 0) {
      buffer.write(html.substring(i));
      break;
    }
    buffer.write(html.substring(i, lt));

    final gt = html.indexOf('>', lt);
    if (gt < 0) {
      // Half a tag. The regular expression this replaces needed a closing `>`
      // to match, so it left the remainder standing as text — and so does this.
      buffer.write(html.substring(lt));
      break;
    }

    final tag = html.substring(lt + 1, gt);
    switch (_tagName(tag)) {
      case 'a':
        // The space a tag becomes belongs outside the link, not inside it.
        flush();
        buffer.write(' ');
        flush();
        open = _safeHref(_attribute(tag, 'href'));
      case '/a':
        flush();
        open = null;
        buffer.write(' ');
        flush();
      default:
        buffer.write(' ');
    }
    i = gt + 1;
  }
  flush();

  return _trimEnds(out);
}

/// The plain text [parseServerText] shows, for callers that want no links.
String serverTextPlain(String html) =>
    parseServerText(html).map((s) => s.text).join();

/// Lower-cased tag name, including the slash of a closing tag.
String _tagName(String tag) {
  final trimmed = tag.trim();
  if (trimmed.isEmpty) return '';
  // From index 1, so a leading `/` stays part of the name and `</a>` is told
  // from `<a>`.
  final end = trimmed.indexOf(RegExp(r'[\s/>]'), 1);
  final name = end < 0 ? trimmed : trimmed.substring(0, end);
  return name.toLowerCase();
}

/// One attribute out of a tag's text: double quoted, single quoted, or bare.
String? _attribute(String tag, String want) {
  final re = RegExp(
    '$want\\s*=\\s*("([^"]*)"|\'([^\']*)\'|([^\\s>]+))',
    caseSensitive: false,
  );
  final m = re.firstMatch(tag);
  if (m == null) return null;
  return m.group(2) ?? m.group(3) ?? m.group(4);
}

/// A destination worth offering, or `null`.
///
/// **A welcome message is arbitrary text from whoever runs the server**, so the
/// schemes are an allow-list rather than a block-list. `javascript:` and
/// `data:` are the obvious ones to keep out; `file:` would hand a server a way
/// to point at the rider's own disk, and `mumble:` would let it add servers by
/// a tap on prose. Anything else stays ordinary text, which is what all of it
/// was before this existed.
Uri? _safeHref(String? raw) {
  if (raw == null) return null;
  final decoded = _decodeEntities(raw.trim());
  if (decoded.isEmpty) return null;
  final url = Uri.tryParse(decoded);
  if (url == null || !url.hasScheme) return null;
  return switch (url.scheme.toLowerCase()) {
    'http' || 'https' || 'mailto' => url,
    _ => null,
  };
}

/// The five entities an attribute may carry. Not a general decoder: this runs
/// on an address, not on prose.
String _decodeEntities(String s) => s
    .replaceAll('&amp;', '&')
    .replaceAll('&lt;', '<')
    .replaceAll('&gt;', '>')
    .replaceAll('&quot;', '"')
    .replaceAll('&#39;', "'");

/// Trims the ends of the whole run, as `.trim()` did on the single string.
List<ServerTextSpan> _trimEnds(List<ServerTextSpan> spans) {
  final out = [...spans];
  while (out.isNotEmpty) {
    final left = out.first.text.trimLeft();
    if (left.isEmpty) {
      out.removeAt(0);
      continue;
    }
    out[0] = ServerTextSpan(left, href: out.first.href);
    break;
  }
  while (out.isNotEmpty) {
    final right = out.last.text.trimRight();
    if (right.isEmpty) {
      out.removeLast();
      continue;
    }
    out[out.length - 1] = ServerTextSpan(right, href: out.last.href);
    break;
  }
  return out;
}
