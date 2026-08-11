import 'package:flutter/material.dart';

/// Status colours are used consistently everywhere, so a rider can read the
/// state from a glance at colour alone without parsing text.
class StatusColors {
  static const connected = Color(0xFF2ECC71);
  static const connecting = Color(0xFFF1C40F);
  static const reconnecting = Color(0xFFE67E22);
  static const failed = Color(0xFFE74C3C);
  static const idle = Color(0xFF7F8C8D);
  static const talking = Color(0xFF3498DB);

  /// The pair a failure is written in: [connecting]'s yellow on a deep red.
  ///
  /// Fixed rather than taken from the scheme, and for the same reason the six
  /// above are: this is read at a glance, through a visor, by somebody who has
  /// just been told no. Material's own choice is `inverseSurface`, which on a
  /// dark theme is a white card — the loudest thing on the screen and the one
  /// colour that says nothing about what happened.
  ///
  /// The red is darker than [failed] because the yellow has to survive on it.
  /// `#F1C40F` on `#E74C3C` is about 2:1 and unreadable; on this it is 5.3:1.
  static const errorBackground = Color(0xFF8C1D18);
  static const errorForeground = connecting;
}

ThemeData buildTheme(Brightness brightness) {
  final scheme = ColorScheme.fromSeed(
    seedColor: const Color(0xFF3498DB),
    brightness: brightness,
  );

  return ThemeData(
    useMaterial3: true,
    colorScheme: scheme,
    // Generous defaults throughout: this gets operated with gloves on.
    filledButtonTheme: FilledButtonThemeData(
      style: FilledButton.styleFrom(
        minimumSize: const Size(0, 52),
        shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(14)),
        textStyle: const TextStyle(fontSize: 16, fontWeight: FontWeight.w600),
      ),
    ),
    outlinedButtonTheme: OutlinedButtonThemeData(
      style: OutlinedButton.styleFrom(
        minimumSize: const Size(0, 52),
        shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(14)),
      ),
    ),
    cardTheme: CardThemeData(
      clipBehavior: Clip.antiAlias,
      shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(18)),
      margin: const EdgeInsets.symmetric(horizontal: 16, vertical: 8),
    ),
    inputDecorationTheme: InputDecorationTheme(
      filled: true,
      border: OutlineInputBorder(
        borderRadius: BorderRadius.circular(14),
        borderSide: BorderSide.none,
      ),
      contentPadding: const EdgeInsets.symmetric(horizontal: 16, vertical: 18),
    ),
    listTileTheme: const ListTileThemeData(
      contentPadding: EdgeInsets.symmetric(horizontal: 20, vertical: 4),
    ),
  );
}
