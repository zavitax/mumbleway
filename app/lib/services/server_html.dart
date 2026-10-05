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
/// How much of a description is worth drawing, matching the core's own cap.
const int kServerTextMaxChars = 512;

/// Turns a channel description into runs, keeping links as links.
///
/// **The text this produces is what the core's `strip_html` produced**, which
/// is what was on screen before links existed: tags dropped with a space only
/// where one is needed, the contents of `<script>` and `<style>` dropped
/// outright — Qt's editor writes a `<style>` block into every description it
/// saves, so without that a channel reads `p, li { white-space: pre-wrap; }` —
/// the six entities decoded, runs of whitespace collapsed, and the whole thing
/// cut at [kServerTextMaxChars].
///
/// Separate from [parseServerText], which is the welcome message's older and
/// cruder rule and is kept exactly because that text was asked not to move.
List<ServerTextSpan> parseServerDescription(String html) {
  final out = <ServerTextSpan>[];
  final buffer = StringBuffer();
  Uri? open;
  String? skipping;
  var entity = StringBuffer();
  var inEntity = false;

  void flush() {
    if (buffer.isEmpty) return;
    out.add(ServerTextSpan(buffer.toString(), href: open));
    buffer.clear();
  }

  // Whether the last character emitted anywhere was not whitespace, which is
  // what decides if a tag needs to leave a space behind it.
  bool endsInWord() {
    final text = buffer.toString();
    if (text.isNotEmpty) return !_isSpace(text[text.length - 1]);
    for (final span in out.reversed) {
      if (span.text.isEmpty) continue;
      return !_isSpace(span.text[span.text.length - 1]);
    }
    return false;
  }

  void emitEntity() {
    final name = entity.toString();
    buffer.write(switch (name) {
      'lt' => '<',
      'gt' => '>',
      'amp' => '&',
      'quot' => '"',
      'apos' || '#39' => "'",
      'nbsp' => ' ',
      // Not an entity. "R&D;" is not markup and should not lose characters.
      _ => '&$name;',
    });
    entity = StringBuffer();
  }

  var i = 0;
  while (i < html.length) {
    final ch = html[i];

    if (inEntity) {
      if (ch == ';') {
        inEntity = false;
        emitEntity();
        i++;
        continue;
      }
      entity.write(ch);
      // Entities are short. Anything longer was an ampersand in ordinary text.
      if (entity.length > 8) {
        inEntity = false;
        buffer.write('&$entity');
        entity = StringBuffer();
      }
      i++;
      continue;
    }

    if (ch == '<') {
      final gt = html.indexOf('>', i);
      if (gt < 0) {
        // An unclosed tag takes the rest of the string with it, which is the
        // safe direction: showing the inside of one would be showing markup.
        break;
      }
      final tag = html.substring(i + 1, gt);
      final name = _tagName(tag).replaceFirst('/', '');
      final closing = _tagName(tag).startsWith('/');

      if (skipping != null) {
        if (closing && name == skipping) skipping = null;
        i = gt + 1;
        continue;
      }
      if (name == 'script' || name == 'style') {
        skipping = name;
        i = gt + 1;
        continue;
      }

      // A break in the markup is a break in the sentence.
      final needsSpace = endsInWord();
      if (name == 'a' && !closing) {
        if (needsSpace) buffer.write(' ');
        flush();
        open = _safeHref(_attribute(tag, 'href'));
      } else if (name == 'a' && closing) {
        flush();
        open = null;
        if (needsSpace) buffer.write(' ');
      } else if (needsSpace) {
        buffer.write(' ');
      }
      i = gt + 1;
      continue;
    }

    if (skipping != null) {
      i++;
      continue;
    }

    if (ch == '&') {
      inEntity = true;
      entity = StringBuffer();
      i++;
      continue;
    }

    buffer.write(ch);
    i++;
  }

  if (inEntity) buffer.write('&$entity');
  flush();

  return _truncate(_collapse(out), kServerTextMaxChars);
}

bool _isSpace(String c) => c.trim().isEmpty;

/// Collapses runs of whitespace across span boundaries and trims the ends,
/// which is what `split_whitespace().join(" ")` does to one string.
List<ServerTextSpan> _collapse(List<ServerTextSpan> spans) {
  final texts = <StringBuffer>[];
  final hrefs = <Uri?>[];
  var pendingSpace = false;
  var started = false;

  for (final span in spans) {
    final buffer = StringBuffer();
    for (final c in span.text.split('')) {
      if (_isSpace(c)) {
        if (started) pendingSpace = true;
        continue;
      }
      if (pendingSpace) {
        pendingSpace = false;
        // **Onto whichever side is not a link.** A space that drifts into one
        // gets underlined with it and read out as part of its text, so it goes
        // to the plain run next to it — before the link if the run before is
        // plain, after it otherwise.
        final nextIsLink = span.href != null;
        final previousIsPlain = texts.isNotEmpty && hrefs.last == null;
        if (buffer.isEmpty && nextIsLink && previousIsPlain) {
          texts.last.write(' ');
        } else {
          buffer.write(' ');
        }
      }
      buffer.write(c);
      started = true;
    }
    if (buffer.isNotEmpty) {
      texts.add(buffer);
      hrefs.add(span.href);
    }
  }

  return [
    for (var i = 0; i < texts.length; i++)
      ServerTextSpan(texts[i].toString(), href: hrefs[i]),
  ];
}

/// Cuts the run at [max] characters in total.
List<ServerTextSpan> _truncate(List<ServerTextSpan> spans, int max) {
  final out = <ServerTextSpan>[];
  var used = 0;
  for (final span in spans) {
    if (used >= max) break;
    final room = max - used;
    if (span.text.length <= room) {
      out.add(span);
      used += span.text.length;
    } else {
      out.add(ServerTextSpan(span.text.substring(0, room), href: span.href));
      used = max;
    }
  }
  return out;
}

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

/// The plain text [parseServerDescription] shows, for tests and for callers
/// that want no links.
String serverDescriptionPlain(String html) =>
    parseServerDescription(html).map((s) => s.text).join();

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
