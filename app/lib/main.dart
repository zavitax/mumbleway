import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_localizations/flutter_localizations.dart';

import 'l10n/app_localizations.dart';
import 'screens/add_server_screen.dart';
import 'services/deep_links.dart';
import 'services/server_proxy.dart';
import 'src/rust/api/mumbleway.dart';
import 'services/qr_intake.dart';
import 'src/rust/frb_generated.dart';
import 'state/app_state.dart';
import 'screens/home_screen.dart';
import 'theme.dart';
import 'widgets/refusal_listener.dart';
import 'widgets/remote_mute_listener.dart';

/// `args` carries a link on Windows, and is empty everywhere else.
///
/// **Desktop has no URL scheme plumbing**, and the method channel the phones
/// use is simply unimplemented there. What Windows does instead is start the
/// app with the URL as an argument — so an invitation or a shared proxy opened
/// from a browser arrives here, and nowhere else.
/// A link this app was launched with, on a platform that passes one as an
/// argument rather than through a channel.
String? _launchLink;

Future<void> main(List<String> args) async {
  _launchLink = args
      .map((a) => a.trim())
      .where(
        (a) =>
            a.toLowerCase().startsWith('mumble:') ||
            a.toLowerCase().startsWith('mumble-proxy:') ||
            a.toLowerCase().startsWith('https://zavitax.github.io/mumbleway/join'),
      )
      .firstOrNull;
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

class _MumbleWayAppState extends State<MumbleWayApp>
    with WidgetsBindingObserver {
  final AppState _state = AppState();

  /// Lets a `mumble://` link open a screen from outside the widget tree.
  ///
  /// A link arrives from the platform, not from a tap on anything, so there is
  /// no context to push from at the moment it lands.
  final GlobalKey<NavigatorState> _navigator = GlobalKey<NavigatorState>();

  StreamSubscription<String>? _linkSub;

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
    _state.start();
    _linkSub = DeepLinks.instance.links.listen(_openLink);
    unawaited(_startLinks());
  }

  /// Hangs up when the app is being taken away.
  ///
  /// `detached` means the process is on its way out with the interface already
  /// gone. Mumble has no goodbye message — a client leaves by closing its
  /// socket — so this is the last chance to close one deliberately rather than
  /// leave it to whenever the operating system gets round to reaping the
  /// process, which on a phone is long after the rider believes they have left.
  ///
  /// Deliberately not `paused` or `inactive`. Those are a rider glancing at
  /// their map, or a notification sliding down, and dropping a call for either
  /// would be far worse than the fault this fixes.
  ///
  /// Not to be relied on alone. Android does not promise to deliver `detached`
  /// before it tears the engine down, and the connections outlive the isolate
  /// because they are held by the Rust core in the process; OverlayService
  /// closes that gap from the platform side. This is the polite path, taken
  /// when there is still time to be polite.
  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    if (state == AppLifecycleState.detached) {
      unawaited(_state.disconnectAll());
    }
  }

  Future<void> _startLinks() async {
    final initial = await DeepLinks.instance.start() ?? _launchLink;
    if (initial != null) _openLink(initial);
  }

  /// Opens the add-server form on whatever the link describes.
  ///
  /// A draft, never a saved entry, and never a connection: a link can be
  /// planted anywhere a rider might tap — a web page, a message from a
  /// stranger, a code stuck to a lamp post — and an app that joined a voice
  /// server because a link said so would be handing over a live microphone on
  /// somebody else's say-so. The details are shown and the rider decides.
  Future<void> _openLink(String url) async {
    final navigator = await _readyNavigator();
    if (navigator == null) return;

    // A proxy on its own, which looks like a server link and is not one. Asked
    // about rather than adopted: every connection set to follow the app's
    // proxy would go through a machine somebody else controls, which is a
    // bigger thing to agree to than adding a server.
    if (await parseProxyLink(text: url) case final proxy?) {
      await _offerProxy(navigator, proxy);
      return;
    }

    final result = await QrReader.fromText(
      url,
      await _state.suggestedUsername(),
    );
    if (result case QrInvitation(:final server)) {
      await navigator.push(
        MaterialPageRoute(builder: (_) => AddServerScreen(prefill: server)),
      );
    }
    // Anything else came from a link this app should not have been handed in
    // the first place. There is nobody to apologise to and nothing to fix.
  }

  /// Asks before a shared proxy becomes the one everything goes through.
  Future<void> _offerProxy(NavigatorState navigator, ServerProxy proxy) async {
    final context = navigator.context;
    final l = L.of(context);
    final address = '${proxy.host}:${proxy.port}';
    final accepted = await showDialog<bool>(
      context: context,
      builder: (c) => AlertDialog(
        title: Text(l.proxyLinkTitle),
        content: Text(l.proxyLinkBody(address), style: const TextStyle(fontSize: 13)),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(c, false),
            child: Text(l.cancel),
          ),
          FilledButton(
            onPressed: () => Navigator.pop(c, true),
            child: Text(l.proxyLinkUse),
          ),
        ],
      ),
    );
    if (accepted != true) return;
    await ServerProxies.instance.setDefault(
      mode: DefaultProxyMode.custom,
      proxy: proxy,
    );
    await _state.reregisterIdleServers();
    if (!navigator.mounted) return;
    ScaffoldMessenger.maybeOf(navigator.context)?.showSnackBar(
      SnackBar(content: Text(l.proxyLinkAdded)),
    );
  }

  /// The navigator, once there is one.
  ///
  /// A link that launched the app is asked for in `initState`, which can beat
  /// the first frame — and a link handled before the navigator is mounted is a
  /// link silently dropped, which on a cold start is *every* link, the one
  /// case that matters most. Waits a few frames rather than assuming either
  /// order.
  Future<NavigatorState?> _readyNavigator() async {
    for (var attempt = 0; attempt < 20; attempt++) {
      final navigator = _navigator.currentState;
      if (navigator != null) return navigator;
      if (!mounted) return null;
      await WidgetsBinding.instance.endOfFrame;
    }
    return _navigator.currentState;
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    _linkSub?.cancel();
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
          navigatorKey: _navigator,
          // Inside the MaterialApp, so there is a ScaffoldMessenger to show a
          // snackbar in, and above the home screen so a refusal still reaches
          // the user after they have navigated somewhere else. The same for
          // another rider turning the microphone off or on.
          builder: (context, child) => RemoteMuteListener(
            child: RefusalListener(child: child ?? const SizedBox.shrink()),
          ),
          home: const HomeScreen(),
        ),
      ),
    );
  }
}
