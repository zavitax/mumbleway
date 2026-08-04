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

  // Which way up the app may be is the platform's business, not ours.
  //
  // This used to lock every device to portrait, for a handlebar mount and a
  // talk button in a predictable place. That reasoning holds for a phone
  // clamped to a bar and for nothing else: it also locked every iPad, every
  // tablet and every phone being used off the bike, and it overruled the
  // per-device answers the platforms already carry. iPhone's Info.plist offers
  // portrait and both landscapes but not upside-down — an inverted phone would
  // put the talk button where the rider's hand is not — while iPad offers all
  // four and Android carries no lock at all.
  //
  // An empty list is what hands the decision back to those three, rather than
  // this app answering for all of them with the narrowest option. The button
  // stays predictable a better way: see the talk panel, which keeps it in the
  // same corner whichever way the screen turns.
  await SystemChrome.setPreferredOrientations(const []);

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
