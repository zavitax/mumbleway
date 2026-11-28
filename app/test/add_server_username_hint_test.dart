import 'dart:math';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mumbleway/l10n/app_localizations.dart';
import 'package:mumbleway/screens/add_server_screen.dart';
import 'package:mumbleway/services/device_identity.dart';
import 'package:mumbleway/state/app_state.dart';
import 'package:shared_preferences/shared_preferences.dart';

/// When the device gives no usable name, the add-server form fills a generated
/// word pair and says so beneath the field so the rider knows to replace it.
///
/// Once they type their own name — or when they open an already-saved server
/// to edit it — that notice stays out of the way.
void main() {
  setUp(() => SharedPreferences.setMockInitialValues({}));

  Widget host(AppState state, Widget screen) => AppStateScope(
    state: state,
    child: MaterialApp(
      localizationsDelegates: L.localizationsDelegates,
      supportedLocales: L.supportedLocales,
      home: screen,
    ),
  );

  SavedServer serverWithUsername(String username) => SavedServer(
    name: 'Clubhouse',
    host: '10.0.0.5',
    port: 64738,
    username: username,
  );

  testWidgets(
    'shows hint for a generated username when adding, hides it once changed',
    (t) async {
      DeviceIdentity.platformOverride = () async => 'User';
      addTearDown(() => DeviceIdentity.platformOverride = null);
      DeviceIdentity.randomOverride = Random(0);
      addTearDown(() => DeviceIdentity.randomOverride = null);

      final state = AppState();
      addTearDown(state.dispose);

      await t.pumpWidget(host(state, const AddServerScreen()));
      await t.pumpAndSettle();

      final l = L.of(t.element(find.byType(AddServerScreen)));
      final usernameField = find.byType(TextFormField).at(3);
      final controller = t.widget<TextFormField>(usernameField).controller!;
      expect(DeviceIdentity.isGenerated(controller.text), isTrue);
      expect(find.text(l.usernameGenerated), findsOneWidget);

      await t.ensureVisible(usernameField);
      await t.enterText(usernameField, 'Ilya');
      await t.pump();
      expect(find.text(l.usernameGenerated), findsNothing);
    },
  );

  testWidgets('hides hint when the device offers a usable name', (t) async {
    DeviceIdentity.platformOverride = () async => "Ilya's iPhone";
    addTearDown(() => DeviceIdentity.platformOverride = null);

    final state = AppState();
    addTearDown(state.dispose);

    await t.pumpWidget(host(state, const AddServerScreen()));
    await t.pumpAndSettle();

    final l = L.of(t.element(find.byType(AddServerScreen)));
    final controller = t
        .widget<TextFormField>(find.byType(TextFormField).at(3))
        .controller!;
    expect(controller.text, 'Ilya');
    expect(find.text(l.usernameGenerated), findsNothing);
  });

  testWidgets('never shows hint when editing an existing saved server', (
    t,
  ) async {
    final saved = serverWithUsername('amber-otter');
    final state = AppState();
    addTearDown(state.dispose);
    state.servers.add(saved);

    await t.pumpWidget(host(state, AddServerScreen(existing: saved)));
    await t.pumpAndSettle();

    final l = L.of(t.element(find.byType(AddServerScreen)));
    expect(find.text(l.usernameGenerated), findsNothing);

    final usernameField = find.byType(TextFormField).at(3);
    await t.ensureVisible(usernameField);
    await t.enterText(usernameField, 'marble-fern');
    await t.pump();
    expect(find.text(l.usernameGenerated), findsNothing);
  });

  testWidgets('shows hint for a prefill holding our made-up name', (t) async {
    // The invitation carried no name, so it was filled with the one this
    // device made up earlier.
    SharedPreferences.setMockInitialValues({
      'mumbleway.suggestedName': 'amber-otter',
      'mumbleway.suggestedNameGenerated': true,
    });
    final state = AppState();
    addTearDown(state.dispose);

    await t.pumpWidget(
      host(state, AddServerScreen(prefill: serverWithUsername('amber-otter'))),
    );
    await t.pumpAndSettle();

    final l = L.of(t.element(find.byType(AddServerScreen)));
    expect(find.text(l.usernameGenerated), findsOneWidget);

    final usernameField = find.byType(TextFormField).at(3);
    await t.ensureVisible(usernameField);
    await t.enterText(usernameField, 'Alice');
    await t.pump();
    expect(find.text(l.usernameGenerated), findsNothing);
  });

  testWidgets('no hint for a dictionary-pair prefill we did not make up', (
    t,
  ) async {
    final state = AppState();
    addTearDown(state.dispose);

    await t.pumpWidget(
      host(state, AddServerScreen(prefill: serverWithUsername('amber-otter'))),
    );
    await t.pumpAndSettle();

    final l = L.of(t.element(find.byType(AddServerScreen)));
    expect(DeviceIdentity.isGenerated('amber-otter'), isTrue);
    expect(find.text(l.usernameGenerated), findsNothing);
  });

  testWidgets('no hint for a typed dictionary pair or a real name like one', (
    t,
  ) async {
    // The device's real name happens to be a word pair from the list.
    DeviceIdentity.platformOverride = () async => 'marble-fern';
    addTearDown(() => DeviceIdentity.platformOverride = null);

    final state = AppState();
    addTearDown(state.dispose);

    await t.pumpWidget(host(state, const AddServerScreen()));
    await t.pumpAndSettle();

    final l = L.of(t.element(find.byType(AddServerScreen)));
    final usernameField = find.byType(TextFormField).at(3);
    final controller = t.widget<TextFormField>(usernameField).controller!;
    expect(controller.text, 'marble-fern');
    expect(find.text(l.usernameGenerated), findsNothing);

    await t.ensureVisible(usernameField);
    await t.enterText(usernameField, 'amber-otter');
    await t.pump();
    expect(find.text(l.usernameGenerated), findsNothing);
  });
}
