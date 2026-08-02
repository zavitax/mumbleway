import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_localizations/flutter_localizations.dart';

import 'l10n/app_localizations.dart';
import 'src/rust/frb_generated.dart';
import 'state/app_state.dart';
import 'screens/home_screen.dart';
import 'theme.dart';

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();
  await RustLib.init();

  // Riders use this on a handlebar mount; locking to portrait keeps the
  // push-to-talk button in a predictable place.
  await SystemChrome.setPreferredOrientations([
    DeviceOrientation.portraitUp,
    DeviceOrientation.portraitDown,
  ]);

  runApp(const MumbleWayApp());
}

class MumbleWayApp extends StatefulWidget {
  const MumbleWayApp({super.key});

  @override
  State<MumbleWayApp> createState() => _MumbleWayAppState();
}

class _MumbleWayAppState extends State<MumbleWayApp> {
  final AppState _state = AppState();

  @override
  void initState() {
    super.initState();
    _state.start();
  }

  @override
  void dispose() {
    _state.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return AppStateScope(
      state: _state,
      // Rebuilds when the language changes, so the switch is instant rather
      // than needing a restart.
      child: ListenableBuilder(
        listenable: _state,
        builder: (context, _) => MaterialApp(
          title: 'MumbleWay',
          debugShowCheckedModeBanner: false,
          theme: buildTheme(Brightness.light),
          darkTheme: buildTheme(Brightness.dark),
          // Dark by default: most riding comms happen with the phone in a
          // mount, and a bright screen at night is a hazard.
          themeMode: ThemeMode.dark,
          locale: _state.locale,
          supportedLocales: AppState.supportedLocales,
          localizationsDelegates: const [
            L.delegate,
            GlobalMaterialLocalizations.delegate,
            GlobalWidgetsLocalizations.delegate,
            GlobalCupertinoLocalizations.delegate,
          ],
          home: const HomeScreen(),
        ),
      ),
    );
  }
}
