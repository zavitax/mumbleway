import 'package:flutter/material.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mumbleway/l10n/app_localizations.dart';
import 'package:mumbleway/screens/add_server_screen.dart';
import 'package:mumbleway/state/app_state.dart';
import 'package:shared_preferences/shared_preferences.dart';

/// What a saved server keeps when the rider edits it.
///
/// **An entry carries more than this form shows.** Access tokens, the rider's
/// note, the channel to come back to after a dropped link: none of them has a
/// field, and the save built a brand new [SavedServer] out of the five fields
/// that do. Everything else went, silently and permanently — a rider who
/// corrected a typo in the server's name came back unable to enter the channel
/// their token was for, with nothing on screen having mentioned it.
///
/// The fix is a default rather than a list: the draft is the saved entry with
/// the form's five fields changed, so a field added to [SavedServer] tomorrow
/// survives an edit without anybody remembering this screen exists.
void main() {
  setUp(() => SharedPreferences.setMockInitialValues({}));

  Widget host(AppState state, SavedServer existing) => AppStateScope(
    state: state,
    child: MaterialApp(
      localizationsDelegates: const [
        ...L.localizationsDelegates,
        GlobalMaterialLocalizations.delegate,
        GlobalWidgetsLocalizations.delegate,
        GlobalCupertinoLocalizations.delegate,
      ],
      supportedLocales: L.supportedLocales,
      home: AddServerScreen(existing: existing),
    ),
  );

  final saved = SavedServer(
    name: 'Clubhouse',
    host: '10.0.0.5',
    port: 64739,
    username: 'Anna',
    password: 'hunter2',
    certFingerprint: 'ab:cd',
    defaultChannel: 'Garage',
    lastChannel: 'Clubhouse',
    accessTokens: const ['vip', 'rideboss'],
    note: 'Wednesday ride, 7pm',
  );

  testWidgets('editing the name keeps the tokens, the note and the way back', (
    t,
  ) async {
    final state = AppState();
    addTearDown(state.dispose);
    state.servers.add(saved);

    await t.pumpWidget(host(state, saved));
    await t.pump(const Duration(milliseconds: 50));

    await t.enterText(find.byType(TextFormField).first, 'The Clubhouse');
    await t.tap(find.text(L.of(t.element(find.byType(AddServerScreen))).saveChanges));
    await t.pumpAndSettle();

    final after = state.servers.single;
    expect(after.name, 'The Clubhouse');
    expect(after.accessTokens, ['vip', 'rideboss']);
    expect(after.note, 'Wednesday ride, 7pm');
    expect(after.lastChannel, 'Clubhouse');
    expect(after.defaultChannel, 'Garage');
    expect(after.certFingerprint, 'ab:cd');
    expect(after.password, 'hunter2');
  });

  testWidgets('and clearing the password still clears it', (t) async {
    // The one field where "empty" is a value rather than "unchanged", which is
    // why keeping everything by default needs saying out loud for this one.
    final state = AppState();
    addTearDown(state.dispose);
    state.servers.add(saved);

    await t.pumpWidget(host(state, saved));
    await t.pump(const Duration(milliseconds: 50));

    await t.enterText(find.byType(TextFormField).at(4), '');
    await t.tap(find.text(L.of(t.element(find.byType(AddServerScreen))).saveChanges));
    await t.pumpAndSettle();

    expect(state.servers.single.password, isNull);
    expect(state.servers.single.accessTokens, ['vip', 'rideboss']);
  });
}
