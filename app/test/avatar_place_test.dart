import 'dart:typed_data';

import 'package:flutter/material.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:image/image.dart' as img;
import 'package:mumbleway/l10n/app_localizations.dart';
import 'package:mumbleway/screens/home_screen.dart';
import 'package:mumbleway/screens/settings_screen.dart';
import 'package:mumbleway/state/app_state.dart';
import 'package:mumbleway/widgets/my_avatar.dart';
import 'package:shared_preferences/shared_preferences.dart';

/// Where the rider's picture is, and what stands in for one they have not
/// chosen.
///
/// It used to be a row on the settings page, between a switch and a
/// fingerprint: the one control there whose value was a picture, and the one a
/// rider had to go two screens deep to see. It is now in the overflow menu of
/// the main window, where it is the picture itself and tapping it changes it.
void main() {
  setUp(() => SharedPreferences.setMockInitialValues({}));

  Widget host(AppState state, Widget screen) => AppStateScope(
    state: state,
    child: MaterialApp(
      localizationsDelegates: const [
        ...L.localizationsDelegates,
        GlobalMaterialLocalizations.delegate,
        GlobalWidgetsLocalizations.delegate,
        GlobalCupertinoLocalizations.delegate,
      ],
      supportedLocales: L.supportedLocales,
      home: screen,
    ),
  );

  /// A real picture, small enough to go in a menu.
  List<int> picture() {
    final image = img.Image(width: 24, height: 24);
    img.fill(image, color: img.ColorRgb8(10, 120, 200));
    return img.encodePng(image);
  }

  AppState ready() {
    final state = AppState();
    addTearDown(state.dispose);
    // Without this the home screen is a spinner and nothing else.
    state.markReadyForTesting();
    return state;
  }

  testWidgets('the three dots offer the picture, and the mark stands in for '
      'one not chosen', (t) async {
    final state = ready();
    await t.pumpWidget(host(state, const HomeScreen()));
    await t.pump(const Duration(milliseconds: 50));

    await t.tap(find.byIcon(Icons.more_vert));
    await t.pump(const Duration(milliseconds: 400));

    expect(find.text('Your picture'), findsOneWidget);
    expect(find.byType(MyAvatar), findsOneWidget);
    // Nothing to take down: the mark is not a picture the rider put there.
    expect(find.text('Remove picture'), findsNothing);
  });

  testWidgets('with a picture, taking it down becomes possible', (t) async {
    final state = ready()..myAvatar = Uint8List.fromList(picture());
    await t.pumpWidget(host(state, const HomeScreen()));
    await t.pump(const Duration(milliseconds: 50));

    await t.tap(find.byIcon(Icons.more_vert));
    await t.pump(const Duration(milliseconds: 400));

    expect(find.text('Your picture'), findsOneWidget);
    expect(find.text('Remove picture'), findsOneWidget);
  });

  testWidgets('the settings page does not carry it any more', (t) async {
    final state = ready();
    t.view.physicalSize = const Size(430, 1400);
    t.view.devicePixelRatio = 1.0;
    addTearDown(t.view.resetPhysicalSize);
    addTearDown(t.view.resetDevicePixelRatio);

    await t.pumpWidget(host(state, const SettingsScreen()));
    await t.pump(const Duration(milliseconds: 50));

    // The identity section is still there; the picture is not.
    expect(find.byType(MyAvatar), findsNothing);
    expect(find.text('Your picture'), findsNothing);
  });
}
