import 'package:flutter/material.dart';

import '../l10n/app_localizations.dart';
import '../state/app_state.dart';

/// Language switcher for the title bar.
///
/// Shows the language that is *active*, not the one a tap would switch to. The
/// button is on every screen, so most of the time it is being read rather than
/// pressed, and a control that answers "what am I looking at" is more use than
/// one that answers "what happens if I press this" — which the tooltip covers
/// anyway.
///
/// The flags are images rather than emoji because Windows ships no colour
/// flag glyphs: the emoji fall back to two letters in a box there, which is
/// exactly the thing a flag is meant to avoid.
class LanguageButton extends StatelessWidget {
  const LanguageButton({super.key});

  static const _flags = {
    'en': 'assets/flags/us.png',
    'ru': 'assets/flags/ru.png',
  };
  static const _codes = {'en': 'EN', 'ru': 'RU'};
  static const _names = {'en': 'English', 'ru': 'Русский'};

  @override
  Widget build(BuildContext context) {
    final state = AppStateScope.of(context);

    final current = state.locale?.languageCode ??
        Localizations.localeOf(context).languageCode;
    final locales = AppState.supportedLocales;
    final index = locales.indexWhere((l) => l.languageCode == current);
    final next =
        locales[(index < 0 ? 0 : index + 1) % locales.length].languageCode;

    final flag = _flags[current];
    return Tooltip(
      message: '${L.of(context).language}: ${_names[current] ?? current}'
          '\n${L.of(context).switchToLanguage(_names[next] ?? next)}',
      child: InkWell(
        onTap: state.cycleLocale,
        borderRadius: BorderRadius.circular(8),
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 8),
          child: Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              if (flag != null)
                ClipRRect(
                  borderRadius: BorderRadius.circular(2),
                  child: Image.asset(
                    flag,
                    height: 14,
                    // Flags are wide and thin; letting width follow keeps the
                    // proportions right rather than squashing them to a box.
                    fit: BoxFit.contain,
                    filterQuality: FilterQuality.medium,
                  ),
                ),
              const SizedBox(width: 6),
              Text(
                _codes[current] ?? current.toUpperCase(),
                style: const TextStyle(
                  fontSize: 13,
                  fontWeight: FontWeight.w700,
                  letterSpacing: 0.4,
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }
}
