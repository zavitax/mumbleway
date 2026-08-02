import 'package:flutter/material.dart';

import '../state/app_state.dart';

/// One-tap language switcher for the title bar.
///
/// A flag rather than a menu: there are two languages, so a toggle is faster
/// than opening a list, and the flag says which one you would switch *to*
/// without needing to read anything.
///
/// The flags are emoji rather than images so they need no assets and scale with
/// the text size.
class LanguageButton extends StatelessWidget {
  const LanguageButton({super.key});

  static const _flags = {'en': '🇬🇧', 'ru': '🇷🇺'};
  static const _names = {'en': 'English', 'ru': 'Русский'};

  @override
  Widget build(BuildContext context) {
    final state = AppStateScope.of(context);

    final current = state.locale?.languageCode ??
        Localizations.localeOf(context).languageCode;
    final locales = AppState.supportedLocales;
    final index = locales.indexWhere((l) => l.languageCode == current);
    final next = locales[(index < 0 ? 0 : index + 1) % locales.length]
        .languageCode;

    return IconButton(
      // Shows the language you would switch to, so a tap is predictable.
      tooltip: 'Switch to ${_names[next] ?? next}',
      onPressed: state.cycleLocale,
      icon: Text(
        _flags[next] ?? next.toUpperCase(),
        style: const TextStyle(fontSize: 20),
      ),
    );
  }
}
