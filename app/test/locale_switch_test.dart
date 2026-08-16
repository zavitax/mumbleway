import 'package:flutter/material.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mumbleway/l10n/app_localizations.dart';
import 'package:mumbleway/screens/settings_screen.dart';
import 'package:mumbleway/state/app_state.dart';

/// Changing the language has to change every string on the screen.
///
/// **This is a regression test for a caching optimisation that broke it.**
/// `Watch` was written to hold on to the subtree it built while its selected
/// value was unchanged, returning the same widget instance so the framework
/// could skip it. That is a large saving and it has one sharp edge: a builder
/// that reads `L.of(context)` inside itself captures the strings of the moment
/// it last ran, and a cached subtree never runs again. Six settings tiles did
/// exactly that and kept their old language.
///
/// Asserting on the *English* disappearing rather than on the Russian
/// appearing: it needs no non-ASCII in this file, and it fails for the right
/// reason. A test looking for the Russian would also pass if the tile vanished.
void main() {
  Widget host(AppState state, Locale locale) => AppStateScope(
    state: state,
    child: MaterialApp(
      locale: locale,
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

  testWidgets('every string on the settings screen follows the language', (
    t,
  ) async {
    final state = AppState();
    addTearDown(state.dispose);
    t.view.physicalSize = const Size(430, 2400);
    t.view.devicePixelRatio = 1.0;
    addTearDown(t.view.resetPhysicalSize);
    addTearDown(t.view.resetDevicePixelRatio);

    await t.pumpWidget(host(state, const Locale('en')));
    for (var i = 0; i < 5; i++) {
      await t.pump(const Duration(milliseconds: 16));
    }

    // Every English string the screen is currently showing, taken from the
    // tree rather than listed here — a list would go stale as the screen grows
    // and would quietly stop covering the tile that broke.
    final english = t
        .widgetList<Text>(find.byType(Text))
        .map((w) => w.data)
        .whereType<String>()
        .where((s) => s.trim().isNotEmpty)
        // Numbers, units and the version footer are the same in both.
        .where((s) => RegExp('[A-Za-z]').hasMatch(s))
        .toSet();
    expect(english, isNotEmpty, reason: 'the screen rendered no text at all');

    await t.pumpWidget(host(state, const Locale('ru')));
    for (var i = 0; i < 5; i++) {
      await t.pump(const Duration(milliseconds: 16));
    }

    final stillEnglish = t
        .widgetList<Text>(find.byType(Text))
        .map((w) => w.data)
        .whereType<String>()
        .where(english.contains)
        // Deliberately the same in both languages, and not string-table
        // entries at all:
        //
        //  - the app's own name and the protocol's;
        //  - a reading with its unit, which is `+0 dB` in either language;
        //  - whatever the engine said went wrong. That one is a raw exception
        //    surfaced where the meter would be, so it is untranslated by
        //    nature — it appears here only because a widget test has no Rust
        //    library behind it. Worth its own fix one day; it is not this one.
        .where(
          (s) => !RegExp(
            r'^(MumbleWay|Mumble|Opus|AEC|RNNoise)$'
            r'|^[+\-0-9.,\s]*(dB|ms|kHz|Hz|%)$'
            r'|^Bad state:',
          ).hasMatch(s.trim()),
        )
        .toSet();

    expect(
      stillEnglish,
      isEmpty,
      reason:
          'these strings did not follow the language change:\n'
          '  ${stillEnglish.join('\n  ')}',
    );
  });
}
