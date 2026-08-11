import 'dart:io';
import 'dart:ui' as ui;

import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:image/image.dart' as img;
import 'package:mumbleway/l10n/app_localizations.dart';
import 'package:mumbleway/screens/add_server_screen.dart';
import 'package:mumbleway/screens/home_screen.dart';
import 'package:mumbleway/screens/settings_screen.dart';
import 'package:mumbleway/src/rust/api/mumbleway.dart';
import 'package:mumbleway/state/app_state.dart';
import 'package:mumbleway/theme.dart';
import 'package:path_provider_platform_interface/path_provider_platform_interface.dart';
import 'package:plugin_platform_interface/plugin_platform_interface.dart';

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
/// # The path provider has to be replaced, not mocked
///
/// `HomeScreen` builds the diagnostics panel, which builds the recording
/// toggle, which asks `path_provider` where a rider's recordings would go. On
/// Windows that plugin is Dart-and-FFI rather than a method channel, so the
/// usual trick of mocking the channel does not intercept it -- it simply never
/// answers under the test binding, and the first pump waits until the run dies
/// on its ten-minute timeout.
///
/// So the platform implementation itself is swapped out below.
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
    PathProviderPlatform.instance = _ScratchPaths(
      Directory.systemTemp.createTempSync('mw-shots').path,
    );
    await _loadFonts();
  });

  /// Every size a store asks for. Landscape entries get the desktop layout for
  /// free -- the app is responsive, so the same tree answers both.
  // The pixel ratio is not decoration. The app is responsive, and it chooses
  // its layout from *logical* width -- so a 1080-wide phone at ratio 1 looks
  // 1080 points wide and gets the two-pane tablet layout, which is not what
  // anybody's phone shows. At ratio 3 it is 360 points wide and gets the phone
  // layout, which is the thing being photographed.
  const shots = <_Shot>[
    // Apple wants one set per device class it is listed on.
    _Shot('app-store', 'iphone-6.9', 1320, 2868, 3, everyScene: true),
    _Shot('app-store', 'iphone-6.5', 1242, 2688, 3),
    _Shot('app-store', 'ipad-12.9', 2048, 2732, 2),
    _Shot('mac-app-store', 'mac', 2880, 1800, 2),
    // A small laptop, and the size worth having because it is the one the
    // desktop layout is tightest in -- if the panel fits here it fits
    // everywhere above.
    //
    // 1280x800 rather than the 1366x768 this used to be. **Apple accepts only
    // 16:10 for Mac**: 1280x800, 1440x900, 2560x1600, 2880x1800 and nothing
    // else. 1366x768 is 16:9, so the settings, addServer and diagnostics Mac
    // shots were unuploadable for as long as they existed -- the store refuses
    // them at upload, after the form is filled in, and only home-mac-2880x1800
    // ever had a valid size. The Microsoft entry below keeps 1366x768 because
    // that is Microsoft's own floor and Microsoft accepts it.
    _Shot('mac-app-store', 'mac-small', 1280, 800, 1, everyScene: true),
    // Play takes anything between 320 and 3840 on the short side, 16:9-ish.
    _Shot('google-play', 'phone', 1080, 2400, 3),
    _Shot('google-play', 'tablet-10', 1600, 2560, 2),
    // The Microsoft Store's own floor, and the size asked for above it.
    _Shot('microsoft-store', 'desktop-small', 1366, 768, 1, everyScene: true),
    _Shot('microsoft-store', 'desktop', 3840, 2160, 2),
  ];

  // Both languages, because a Russian store page with English screenshots
  // reads as a half-finished translation -- and because Russian is longer than
  // English almost everywhere, so it is also the pass that shows whether the
  // interface still fits. The diagnostic panel's status line was wrapping a
  // letter at a time in Russian and nowhere else.
  for (final locale in AppState.supportedLocales) {
  for (final scene in _Scene.values) {
  for (final shot in shots) {
    // The home screen is what a store leads with, so it gets every size. The
    // other two are supporting shots and only need one phone and one desktop;
    // seven of each would be padding a listing rather than filling it.
    if (scene != _Scene.home && !shot.everyScene) continue;

    testWidgets(
        '${locale.languageCode} ${scene.name} ${shot.store} ${shot.name} '
        '${shot.width}x${shot.height}', (
      tester,
    ) async {
      // physicalSize is the file's pixel size; the ratio decides how many
      // logical points that is, and therefore which layout the app picks.
      tester.view.physicalSize = Size(shot.width.toDouble(), shot.height.toDouble());
      tester.view.devicePixelRatio = shot.ratio.toDouble();
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);

      final state = _connectedState();
      if (scene == _Scene.diagnostics) state.diagnosticsOpen = true;

      final key = GlobalKey();
      await tester.pumpWidget(
        RepaintBoundary(
          key: key,
          child: _app(state, locale, scene),
        ),
      );
      // Fixed pumps rather than pumpAndSettle, which never returns here: the
      // on-air light blinks for as long as audio is going out and the meters
      // run off a ticker, both on purpose, so there is no settled state to
      // wait for. Enough frames to let the entry animations finish.
      for (var i = 0; i < 6; i++) {
        await tester.pump(const Duration(milliseconds: 120));
      }

      final file = File(
        '${root.path}/${shot.store}/screenshots/${locale.languageCode}/'
        '${scene.name}-${shot.name}-${shot.width}x${shot.height}.png',
      );

      // Inside runAsync, and this is the whole reason the harness hung.
      // A widget test runs in a zone where timers and futures are faked, but
      // toImage and toByteData wait on the real rasterizer -- so awaited under
      // fake async they deadlock, silently, until the ten-minute timeout. Only
      // runAsync gives them a live event loop to complete on.
      int? width;
      int? height;
      await tester.runAsync(() async {
        await _capture(key, file, shot.ratio.toDouble());
        // A file that decodes to the wrong size is the failure worth catching:
        // it uploads, and the store refuses it after the form is filled in.
        final decoded = await decodeImageFromList(file.readAsBytesSync());
        width = decoded.width;
        height = decoded.height;
      });

      expect(file.existsSync(), isTrue);
      expect(width, shot.width);
      expect(height, shot.height);
      // Colour type 6 is truecolour-with-alpha. Apple refuses that at upload,
      // and every Apple screenshot in this repository carried it until the
      // encoder above was changed -- invisibly, because the channel was fully
      // opaque. Assert what was written, not what was drawn.
      expect(
        _pngColourType(file),
        2,
        reason: 'a store screenshot must carry no alpha channel',
      );

      // `markAudioActiveForTesting` starts the relief poll timer, and the
      // binding asserts no timer is pending once the tree is disposed. That
      // check runs at the end of the body, *before* any `addTearDown`, so
      // registering the dispose there is too late -- it has to be here.
      state.dispose();
    });
  }
  }
  }
}

/// Which part of the app a shot is of.
enum _Scene {
  /// The call itself: roster, channel, connection. What a store leads with.
  home,

  /// Where a rider changes the things this app exists to let them change --
  /// noise profile, transmit mode, audio devices.
  settings,

  /// Adding a server, which is the first thing anyone does and the step a
  /// listing most needs to show is simple. Rendered empty, as a new user meets
  /// it, rather than pre-filled.
  addServer,

  /// The analyser and the chain status.
  ///
  /// The spectrum comes out **empty** here, and honestly so: the bands are
  /// computed by the audio worker, and there is no engine behind a test. This
  /// shows the panel and the per-stage status, not a live signal. A screenshot
  /// with a moving spectrum has to be taken on a device.
  diagnostics,
}

/// Answers every "where do files go" question with one scratch directory.
///
/// `MockPlatformInterfaceMixin` rather than a plain subclass: `PlatformInterface`
/// refuses an instance that does not carry its private token, and that check is
/// the whole point of the base class -- this mixin is the sanctioned way past it
/// for a test.
class _ScratchPaths extends Fake
    with MockPlatformInterfaceMixin
    implements PathProviderPlatform {
  _ScratchPaths(this.root);
  final String root;

  @override
  Future<String?> getTemporaryPath() async => root;
  @override
  Future<String?> getApplicationSupportPath() async => root;
  @override
  Future<String?> getApplicationDocumentsPath() async => root;
  @override
  Future<String?> getApplicationCachePath() async => root;
  @override
  Future<String?> getDownloadsPath() async => root;
  @override
  Future<String?> getLibraryPath() async => root;
  @override
  Future<String?> getExternalStoragePath() async => root;
  @override
  Future<List<String>?> getExternalStoragePaths({StorageDirectory? type}) async => [root];
  @override
  Future<List<String>?> getExternalCachePaths() async => [root];
}

class _Shot {
  const _Shot(
    this.store,
    this.name,
    this.width,
    this.height,
    this.ratio, {
    this.everyScene = false,
  });
  final String store;
  final String name;
  final int width;
  final int height;

  /// Device pixel ratio, which decides the logical size and so the layout.
  final int ratio;

  /// Whether the supporting scenes are shot at this size as well as the home
  /// screen. One phone and one small desktop is enough for those.
  final bool everyScene;
}

Widget _app(AppState state, Locale locale, _Scene scene) => AppStateScope(
  state: state,
  child: MaterialApp(
    debugShowCheckedModeBanner: false,
    locale: locale,
    theme: buildTheme(Brightness.dark),
    supportedLocales: AppState.supportedLocales,
    localizationsDelegates: const [
      L.delegate,
      GlobalMaterialLocalizations.delegate,
      GlobalWidgetsLocalizations.delegate,
      GlobalCupertinoLocalizations.delegate,
    ],
    // These are pushed routes in the app; here each is the home, so no
    // navigation has to be driven to reach it. The widgets are the same ones.
    home: switch (scene) {
      _Scene.settings => const SettingsScreen(),
      _Scene.addServer => const AddServerScreen(),
      _ => const HomeScreen(),
    },
  ),
);

/// A channel with people in it, one of them talking.
///
/// Names are drawn from nowhere in particular and no real rider. A screenshot
/// showing a real person's handle is that person's business, not ours.
AppState _connectedState() {
  final state = AppState()
    // Every screen is gated on this; without it the whole app is a spinner and
    // the screenshot is a picture of one.
    ..markReadyForTesting()
    // Without this the talk panel draws `_MicIdleNotice` instead of the meter,
    // because that panel keys on `audioActive` rather than on the connection.
    // The result was a screenshot that contradicted itself: a card reading
    // **Connected** directly above the sentence "The microphone meter appears
    // here once you connect to a server." Both halves were behaving correctly
    // -- there is no engine behind a widget test, so the microphone really was
    // shut -- which is exactly why it survived. The hook to say otherwise has
    // existed since the probe-spinner work and this harness never called it.
    ..markAudioActiveForTesting();
  final server = SavedServer(
    name: 'Sunday Run',
    host: 'mumble.example.org',
    port: 64738,
    username: 'Ilya',
    defaultChannel: 'On the road',
  );
  state.servers.add(server);

  final runtime = state.runtimeFor(server.id);
  // Without a probe the card's reachability line is a spinner reading
  // "Checking…", which in a store screenshot is a picture of the app thinking
  // rather than of the app working. Same reasoning as `markReadyForTesting`
  // above, which exists because the toolbar had the same problem.
  runtime.probe = UiServerStatus(
    serverId: server.id,
    reachable: true,
    pingMs: 34,
    users: 4,
    maxUsers: 100,
    version: '1.4.287',
  );
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

/// Writes the boundary to a PNG **with no alpha channel**.
///
/// Apple refuses a screenshot that carries transparency -- "Images cannot
/// include alpha channels or transparencies" -- and it refuses it at upload,
/// after the listing form is filled in, rather than at review. Flutter's own
/// PNG encoder (`ImageByteFormat.png`) always writes RGBA, so every Apple
/// screenshot this harness produced carried a channel the store rejects. The
/// pixels underneath were fully opaque, which is why nothing ever looked
/// wrong and why nothing caught it.
///
/// So the raw RGBA is re-encoded to three channels. That is a channel drop,
/// not a composite: there is nothing to flatten against.
Future<void> _capture(GlobalKey key, File file, double ratio) async {
  final boundary = key.currentContext!.findRenderObject()! as RenderRepaintBoundary;
  final image = await boundary.toImage(pixelRatio: ratio);
  final data = await image.toByteData(format: ui.ImageByteFormat.rawRgba);
  final rgba = img.Image.fromBytes(
    width: image.width,
    height: image.height,
    bytes: data!.buffer,
    numChannels: 4,
  );
  file.parent.createSync(recursive: true);
  file.writeAsBytesSync(img.encodePng(rgba.convert(numChannels: 3)));
  image.dispose();
}

/// The PNG colour type, read out of IHDR.
///
/// 2 is truecolour, 6 is truecolour with alpha. Read from the bytes rather
/// than by decoding, because the whole point is to check what was *written*.
/// Layout: 8-byte signature, then the IHDR chunk -- length, type, width,
/// height, bit depth, and colour type at offset 25.
int _pngColourType(File file) => file.readAsBytesSync()[25];

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
    // Without this every icon in the interface draws as an empty square, which
    // is not subtly wrong -- the mute, talk and connection glyphs are most of
    // what a screenshot of this app is showing.
    await family('MaterialIcons', ['$dir/MaterialIcons-Regular.otf']);
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
