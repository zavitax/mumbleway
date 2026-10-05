import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';

import '../services/server_html.dart';
import '../services/site_links.dart';

/// Text a server wrote, with its links working.
///
/// The words are exactly the words that were shown before this existed — see
/// [parseServerText]. What changes is that an anchor is now tappable, so a
/// welcome message saying "it is written up **here**" leads somewhere instead
/// of being a sentence with a hole in it.
///
/// **The address is a server operator's to choose**, which is why only `http`,
/// `https` and `mailto` are offered at all. The destination is not written into
/// the sentence — that was asked for explicitly — but it does reach a screen
/// reader through the span's semantics, so somebody who cannot see the styling
/// is not simply told to tap an unexplained word. Opening goes through
/// [openSite], so a device with no browser says so rather than swallowing the
/// tap.
class ServerText extends StatefulWidget {
  /// A welcome message, under the rule that its text may not move.
  const ServerText(this.html, {super.key, this.style}) : _tidy = false;

  /// A channel description, under the core's old `strip_html` rules — the
  /// contents of a Qt `<style>` block dropped, entities decoded, whitespace
  /// collapsed, cut at 512 characters — and now with its links intact.
  const ServerText.description(this.html, {super.key, this.style})
      : _tidy = true;

  /// The server's text, markup and all.
  final String html;

  final TextStyle? style;

  final bool _tidy;

  @override
  State<ServerText> createState() => _ServerTextState();
}

class _ServerTextState extends State<ServerText> {
  /// **Kept and disposed, not built inline.** A `TapGestureRecognizer` holds a
  /// pointer subscription; one created in `build` leaks on every rebuild, and
  /// this sits on a pane that rebuilds whenever anything moves.
  final _recognizers = <TapGestureRecognizer>[];

  @override
  void dispose() {
    for (final r in _recognizers) {
      r.dispose();
    }
    super.dispose();
  }

  void _clear() {
    for (final r in _recognizers) {
      r.dispose();
    }
    _recognizers.clear();
  }

  @override
  Widget build(BuildContext context) {
    _clear();
    final spans = widget._tidy
        ? parseServerDescription(widget.html)
        : parseServerText(widget.html);
    final linkColour = Theme.of(context).colorScheme.primary;

    return SelectionArea(
      child: Text.rich(
        TextSpan(
          style: widget.style,
          children: [
            for (final span in spans)
              if (span.href case final href?)
                TextSpan(
                  text: span.text,
                  style: TextStyle(
                    color: linkColour,
                    decoration: TextDecoration.underline,
                    decorationColor: linkColour,
                  ),
                  // Where it goes, without putting it in the sentence.
                  semanticsLabel: '${span.text} — $href',
                  recognizer: _recognizerFor(href),
                  mouseCursor: SystemMouseCursors.click,
                )
              else
                TextSpan(text: span.text),
          ],
        ),
      ),
    );
  }

  TapGestureRecognizer _recognizerFor(Uri href) {
    final r = TapGestureRecognizer()
      ..onTap = () {
        if (!mounted) return;
        openSite(context, href);
      };
    _recognizers.add(r);
    return r;
  }
}
