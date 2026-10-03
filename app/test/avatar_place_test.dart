import 'dart:typed_data';

import 'package:flutter/gestures.dart';
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
/// rider had to go two screens deep to see. It is now the overflow menu's first
/// entry and it is drawn as a picture — large and round — because the picture
/// is the control and pressing it is how it is changed.
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

  Future<void> openMenu(WidgetTester t) async {
    await t.tap(find.byIcon(Icons.more_vert));
    await t.pump(const Duration(milliseconds: 400));
  }

  testWidgets('the menu leads with the picture, half its width in a quarter of '
      'air', (t) async {
    final state = ready();
    await t.pumpWidget(host(state, const HomeScreen()));
    await t.pump(const Duration(milliseconds: 50));
    await openMenu(t);

    expect(find.byType(AvatarMenuTile), findsOneWidget);
    final entry = t.getRect(find.byType(AvatarMenuTile));
    final face = t.getRect(find.byType(MyAvatar));
    expect(face.width, closeTo(entry.width / 2, 1));
    expect(face.left - entry.left, closeTo(entry.width / 4, 1));
    expect(face.top - entry.top, closeTo(entry.width / 4, 1));
    expect(entry.bottom - face.bottom, closeTo(entry.width / 4, 1));

    // Nothing to take down: the mark is not a picture the rider put there.
    expect(find.text('Remove picture'), findsNothing);
  });

  testWidgets('what pressing it does is said over the picture, on hover', (
    t,
  ) async {
    final state = ready();
    await t.pumpWidget(host(state, const HomeScreen()));
    await t.pump(const Duration(milliseconds: 50));
    await openMenu(t);

    // Nothing beside the picture until a pointer is over it: a label standing
    // there all the time would make the entry read as a setting with a
    // thumbnail rather than as the picture itself.
    expect(find.text('Change your picture'), findsNothing);

    final mouse = await t.createGesture(kind: PointerDeviceKind.mouse);
    await mouse.addPointer(location: Offset.zero);
    addTearDown(mouse.removePointer);
    await t.pumpAndSettle();
    await mouse.moveTo(t.getCenter(find.byType(MyAvatar)));
    await t.pumpAndSettle();

    expect(find.text('Change your picture'), findsOneWidget);
  });

  testWidgets('with a picture, taking it down becomes possible', (t) async {
    final state = ready()..myAvatar = Uint8List.fromList(picture());
    await t.pumpWidget(host(state, const HomeScreen()));
    await t.pump(const Duration(milliseconds: 50));
    await openMenu(t);

    expect(find.byType(AvatarMenuTile), findsOneWidget);
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

    expect(find.byType(MyAvatar), findsNothing);
    expect(find.text('Your picture'), findsNothing);
  });
}
