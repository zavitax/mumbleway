import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mumbleway/theme.dart';
import 'package:mumbleway/widgets/error_snack.dart';

/// A notice is not a failure, and has to be legible.
///
/// Both halves of this were wrong at once when it was written: the toast took
/// the saturated amber as a *fill* and never set a text colour, so the theme's
/// own dark text landed on it and the message could not be read at all.
void main() {
  Future<void> show(
    WidgetTester t,
    void Function(ScaffoldMessengerState) fire,
  ) async {
    await t.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: Builder(
            builder: (context) => ElevatedButton(
              onPressed: () => fire(ScaffoldMessenger.of(context)),
              child: const Text('go'),
            ),
          ),
        ),
      ),
    );
    await t.tap(find.text('go'));
    await t.pump();
  }

  testWidgets('a notice is amber on dark, and says so explicitly', (t) async {
    await show(t, (m) => showNotice(m, 'This channel will not carry your voice.'));

    final bar = t.widget<SnackBar>(find.byType(SnackBar));
    expect(bar.backgroundColor, StatusColors.noticeBackground);

    final text = t.widget<Text>(find.text('This channel will not carry your voice.'));
    expect(
      text.style?.color,
      StatusColors.noticeForeground,
      reason: 'left unset it inherits the theme, which is what made it '
          'unreadable on the amber',
    );
    // The same amber the panel on the card uses, so the two read as one
    // message rather than two.
    expect(StatusColors.noticeForeground, StatusColors.reconnecting);
  });

  testWidgets('and a failure still gets the failure colours', (t) async {
    await show(t, (m) => showError(m, 'The server refused you.'));

    final bar = t.widget<SnackBar>(find.byType(SnackBar));
    expect(bar.backgroundColor, StatusColors.errorBackground);
    expect(
      t.widget<Text>(find.text('The server refused you.')).style?.color,
      StatusColors.errorForeground,
    );
  });
}
