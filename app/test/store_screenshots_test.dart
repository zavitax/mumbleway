import 'dart:io';
import 'dart:ui' as ui;

import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mumbleway/l10n/app_localizations.dart';
import 'package:mumbleway/screens/home_screen.dart';
import 'package:mumbleway/src/rust/api/mumbleway.dart';
import 'package:mumbleway/state/app_state.dart';
import 'package:mumbleway/theme.dart';

/// Renders the real interface at each store's required size.
///
/// **These are the shipping widgets, not a mock-up.** Every pixel comes from
/// the same `HomeScreen` and `ServerCard` the app builds at runtime; only the
/// roster is sample data, which is what a screenshot of a voice app has to be
/// unless five people are asked to stand by while it is taken.
///
/// What that buys is a screenshot that cannot quietly stop matching the
/// product. Redraw them after a UI change and the change is in them; a
/// hand-composed image in a design tool would not be, and nobody would notice
/// until a reviewer did.
///
/// Skipped by a normal `flutter test`, because it writes files and takes
/// seconds rather than milliseconds:
///
/// ```
/// $env:MUMBLEWAY_SHOTS_OUT = "brand/store"
/// flutter test test/store_screenshots_test.dart
/// ```
///
/// # Known not to work yet
///
/// It hangs. `HomeScreen` builds the diagnostics panel, which builds
/// [RecordingToggle], which asks `path_provider` where to write recordings --
/// and that call never returns under the test binding, so the first pump waits
/// for ever and the run dies on the ten-minute timeout.
///
/// Fixing it means giving the test a fake `PathProviderPlatform` rather than
/// letting the real one run, since `path_provider_windows` is Dart-and-FFI and
/// mocking the method channel does not intercept it. Left here, honestly
/// broken, because the harness itself is right and the remaining work is one
/// substitution -- not because it is finished.
///
/// # Fonts have to be loaded by hand
///
/// `flutter test` ships no fonts, and text renders as boxes without them --
/// which produces a screenshot that is technically the real widget tree and
/// useless to everybody. Roboto comes out of the Flutter SDK's own cache
/// because that is what the app actually draws with: the theme sets no
/// `fontFamily`, so body text is the platform default. Exo 2 is the wordmark's
/// face and is bundled by the app itself.
void main() {
  final out = Platform.environment['MUMBLEWAY_SHOTS_OUT'];
  if (out == null) {
    test('store screenshots', () {}, skip: 'set MUMBLEWAY_SHOTS_OUT to write them');
    return;
  }
  final root = Directory(out);

  setUpAll(() async {
    TestWidgetsFlutterBinding.ensureInitialized();
    await _loadFonts();
  });

  /// Every size a store asks for. Landscape entries get the desktop layout for
  /// free -- the app is responsive, so the same tree answers both.
  const shots = <_Shot>[
    // Apple wants one set per device class it is listed on.
    _Shot('app-store', 'iphone-6.9', 1320, 2868),
    _Shot('app-store', 'iphone-6.5', 1242, 2688),
    _Shot('app-store', 'ipad-12.9', 2048, 2732),
    _Shot('mac-app-store', 'mac', 2880, 1800),
    // Play takes anything between 320 and 3840 on the short side, 16:9-ish.
    _Shot('google-play', 'phone', 1080, 2400),
    _Shot('google-play', 'tablet-10', 1600, 2560),
    // Asked for explicitly; comfortably above the 1366x768 floor.
    _Shot('microsoft-store', 'desktop', 3840, 2160),
  ];

  for (final shot in shots) {
    testWidgets('${shot.store} ${shot.name} ${shot.width}x${shot.height}', (
      tester,
    ) async {
      // devicePixelRatio 1 so the surface size *is* the pixel size, and the
      // file comes out at exactly what the store asked for rather than at some
      // multiple of it.
      tester.view.physicalSize = Size(shot.width.toDouble(), shot.height.toDouble());
      tester.view.devicePixelRatio = 1.0;
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);

      final key = GlobalKey();
      await tester.pumpWidget(
        RepaintBoundary(key: key, child: _app(_connectedState())),
      );
      // Fixed pumps rather than pumpAndSettle, which never returns here: the
      // on-air light blinks for as long as audio is going out and the meters
      // run off a ticker, both on purpose, so there is no settled state to
      // wait for. Enough frames to let the entry animations finish.
      for (var i = 0; i < 6; i++) {
        await tester.pump(const Duration(milliseconds: 120));
      }

      final file = File(
        '${root.path}/${shot.store}/screenshots/${shot.name}-${shot.width}x${shot.height}.png',
      );
      await _capture(key, file);

      expect(file.existsSync(), isTrue);
      // A file that decodes to the wrong size is the failure worth catching:
      // it uploads, and the store refuses it after the form is filled in.
      final decoded = await decodeImageFromList(file.readAsBytesSync());
      expect(decoded.width, shot.width);
      expect(decoded.height, shot.height);
    });
  }
}

class _Shot {
  const _Shot(this.store, this.name, this.width, this.height);
  final String store;
  final String name;
  final int width;
  final int height;
}

Widget _app(AppState state) => AppStateScope(
  state: state,
  child: MaterialApp(
    debugShowCheckedModeBanner: false,
    theme: buildTheme(Brightness.dark),
    supportedLocales: AppState.supportedLocales,
    localizationsDelegates: const [
      L.delegate,
      GlobalMaterialLocalizations.delegate,
      GlobalWidgetsLocalizations.delegate,
      GlobalCupertinoLocalizations.delegate,
    ],
    home: const HomeScreen(),
  ),
);

/// A channel with people in it, one of them talking.
///
/// Names are drawn from nowhere in particular and no real rider. A screenshot
/// showing a real person's handle is that person's business, not ours.
AppState _connectedState() {
  final state = AppState();
  final server = SavedServer(
    name: 'Sunday Run',
    host: 'mumble.example.org',
    port: 64738,
    username: 'Ilya',
    defaultChannel: 'On the road',
  );
  state.servers.add(server);

  final runtime = state.runtimeFor(server.id);
  runtime.status = ConnStatus.connected;
  runtime.selfSession = 1;
  runtime.transport = 'udp';
  runtime.udpPingMs = 34;
  runtime.tcpPingMs = 41;
  runtime.channels = const [
    UiChannel(id: 0, name: 'Root', description: '', userCount: 0, maxUsers: 0),
    UiChannel(
      id: 1,
      name: 'On the road',
      parent: 0,
      description: '',
      userCount: 4,
      maxUsers: 0,
    ),
    UiChannel(
      id: 2,
      name: 'Coffee stop',
      parent: 0,
      description: '',
      userCount: 1,
      maxUsers: 0,
    ),
  ];
  runtime.users = const [
    UiUser(
      session: 1,
      name: 'Ilya',
      channelId: 1,
      talking: false,
      muted: false,
      deafened: false,
      localMute: false,
      status: 'silent',
    ),
    UiUser(
      session: 2,
      name: 'Marek',
      channelId: 1,
      talking: true,
      muted: false,
      deafened: false,
      localMute: false,
      status: 'talking',
    ),
    UiUser(
      session: 3,
      name: 'Dani',
      channelId: 1,
      talking: false,
      muted: false,
      deafened: false,
      localMute: false,
      status: 'silent',
    ),
    UiUser(
      session: 4,
      name: 'Bea',
      channelId: 1,
      talking: false,
      muted: true,
      deafened: false,
      localMute: false,
      status: 'muted',
    ),
  ];
  return state;
}

Future<void> _capture(GlobalKey key, File file) async {
  final boundary = key.currentContext!.findRenderObject()! as RenderRepaintBoundary;
  final image = await boundary.toImage(pixelRatio: 1.0);
  final data = await image.toByteData(format: ui.ImageByteFormat.png);
  file.parent.createSync(recursive: true);
  file.writeAsBytesSync(data!.buffer.asUint8List());
  image.dispose();
}

/// Loads the faces the app actually draws with.
///
/// Read off disk rather than through `rootBundle`, which in a test resolves
/// against an asset manifest the test runner has not built.
Future<void> _loadFonts() async {
  Future<void> family(String name, Iterable<String> paths) async {
    final loader = FontLoader(name);
    var any = false;
    for (final path in paths) {
      final file = File(path);
      if (!file.existsSync()) continue;
      any = true;
      loader.addFont(
        Future.value(ByteData.view(file.readAsBytesSync().buffer)),
      );
    }
    if (any) await loader.load();
  }

  await family('Exo2', const ['assets/fonts/Exo2-Variable.ttf']);

  // Roboto, from the SDK that is building this. Located rather than hard-coded
  // so the path survives a Flutter installed somewhere else; if it cannot be
  // found the text falls back to boxes, which is loud rather than subtle.
  final sdk = _flutterRoot();
  if (sdk != null) {
    final dir = '$sdk/bin/cache/artifacts/material_fonts';
    await family('Roboto', [
      '$dir/roboto-regular.ttf',
      '$dir/roboto-medium.ttf',
      '$dir/roboto-bold.ttf',
    ]);
  }
}

String? _flutterRoot() {
  // `which flutter` in Dart: the executable running this test lives inside the
  // SDK, so walk up from it rather than searching PATH.
  var dir = File(Platform.resolvedExecutable).parent;
  for (var i = 0; i < 6; i++) {
    if (Directory('${dir.path}/bin/cache/artifacts/material_fonts').existsSync()) {
      return dir.path;
    }
    dir = dir.parent;
  }
  final env = Platform.environment['FLUTTER_ROOT'];
  return env;
}
