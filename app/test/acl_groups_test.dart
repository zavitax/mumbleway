import 'dart:typed_data';

import 'package:flutter/material.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mumbleway/l10n/app_localizations.dart';
import 'package:mumbleway/screens/channel_acl_screen.dart';
import 'package:mumbleway/src/rust/api/mumbleway.dart';
import 'package:mumbleway/state/app_state.dart';

/// Editing a channel's groups.
///
/// **A group's membership is three lists, not one.** Mumble keeps what this
/// channel *adds*, what it *removes* from whatever the parent passed down, and
/// the inherited members themselves — so taking an inherited member out is not
/// deleting them from a list, it is naming them in `remove`. An editor that
/// treated it as one list would either lose the distinction on save or quietly
/// hand the member back the next time the parent changed.
///
/// These tests read the draft the screen is holding rather than what reaches
/// the server: `saveAcl` goes through the bridge, which a widget test has no
/// engine for. What they pin is the shape of the edit.
void main() {
  Widget host(AppState state, UiChannel channel) => AppStateScope(
    state: state,
    child: MaterialApp(
      localizationsDelegates: const [
        ...L.localizationsDelegates,
        GlobalMaterialLocalizations.delegate,
        GlobalWidgetsLocalizations.delegate,
        GlobalCupertinoLocalizations.delegate,
      ],
      supportedLocales: L.supportedLocales,
      home: ChannelAclScreen(serverId: 'srv', channel: channel),
    ),
  );

  const channel = UiChannel(
    id: 3,
    parent: 0,
    name: 'Clubhouse',
    description: '',
    userCount: 0,
    maxUsers: 0,
  );

  UiAclGroup group({
    String name = 'vip',
    bool inherited = false,
    List<int> add = const [],
    List<int> remove = const [],
    List<int> inheritedMembers = const [],
  }) => UiAclGroup(
    name: name,
    inherited: inherited,
    inherit: true,
    inheritable: true,
    add: Uint32List.fromList(add),
    remove: Uint32List.fromList(remove),
    inheritedMembers: Uint32List.fromList(inheritedMembers),
  );

  /// A server that has already answered with this access list.
  AppState ready(UiChannelAcl acl) {
    final state = AppState();
    addTearDown(state.dispose);
    final rt = ServerRuntime()
      ..status = ConnStatus.connected
      ..acls[acl.channelId] = acl
      ..registered = const [
        UiRegisteredUser(userId: 7, name: 'Anna', lastSeen: ''),
        UiRegisteredUser(userId: 9, name: 'Boris', lastSeen: ''),
      ];
    rt.userNames.addAll({7: 'Anna', 9: 'Boris'});
    state.runtimes['srv'] = rt;
    return state;
  }

  UiChannelAcl aclWith(List<UiAclGroup> groups) => UiChannelAcl(
    channelId: 3,
    inheritAcls: true,
    groups: groups,
    rules: const [],
  );

  /// The groups the screen is currently holding, edits included.
  List<UiAclGroup> drafted(WidgetTester t) {
    final screen = t.state(find.byType(ChannelAclScreen));
    // ignore: avoid_dynamic_calls
    final draft = (screen as dynamic).edits as UiChannelAcl;
    return draft.groups;
  }

  testWidgets('a member is added by name, and lands in the add list', (
    t,
  ) async {
    final state = ready(aclWith([group()]));
    await t.pumpWidget(host(state, channel));
    await t.pump(const Duration(milliseconds: 50));

    final l = await L.delegate.load(const Locale('en'));
    expect(find.text(l.aclGroupNobody), findsOneWidget);

    await t.tap(find.text(l.aclGroupAddMember));
    await t.pumpAndSettle();
    await t.tap(find.text('Boris'));
    await t.pumpAndSettle();

    expect(drafted(t).single.add, [9]);
    expect(find.widgetWithText(Chip, 'Boris'), findsOneWidget);
  });

  testWidgets('taking out an inherited member names them in remove', (
    t,
  ) async {
    // Not deleted from the list: the parent still has them, and this channel
    // is recording that it does not want them. Delete them from `add` instead
    // and the next ACL from above puts them back.
    final state = ready(
      aclWith([group(inheritedMembers: const [7])]),
    );
    await t.pumpWidget(host(state, channel));
    await t.pump(const Duration(milliseconds: 50));

    await t.tap(find.byIcon(Icons.close));
    await t.pump(const Duration(milliseconds: 50));

    final after = drafted(t).single;
    expect(after.remove, [7], reason: 'excluded here');
    expect(after.add, isEmpty);
    expect(after.inheritedMembers, [7], reason: 'the parent still has them');
  });

  testWidgets('and putting them back clears the exclusion', (t) async {
    final state = ready(
      aclWith([group(inheritedMembers: const [7], remove: const [7])]),
    );
    await t.pumpWidget(host(state, channel));
    await t.pump(const Duration(milliseconds: 50));

    await t.tap(find.byIcon(Icons.undo));
    await t.pump(const Duration(milliseconds: 50));

    expect(drafted(t).single.remove, isEmpty);
  });

  testWidgets('a group inherited from above cannot be edited here', (t) async {
    // It belongs to the channel that defines it, and the server drops a
    // change to it from anything written here — so offering the controls
    // would be offering an edit that silently does nothing.
    final l = await L.delegate.load(const Locale('en'));
    final state = ready(
      aclWith([group(inherited: true, inheritedMembers: const [7])]),
    );
    await t.pumpWidget(host(state, channel));
    await t.pump(const Duration(milliseconds: 50));

    expect(find.text(l.aclInherited), findsOneWidget);
    expect(find.text(l.aclGroupAddMember), findsNothing);
    expect(find.byIcon(Icons.close), findsNothing);
  });

  testWidgets('a new group starts empty and takes the parent\'s members', (
    t,
  ) async {
    final l = await L.delegate.load(const Locale('en'));
    final state = ready(aclWith(const []));
    await t.pumpWidget(host(state, channel));
    await t.pump(const Duration(milliseconds: 50));

    // The button sits under the groups, so on a short test screen it starts
    // below the fold and a tap would land on whatever is there instead.
    await t.ensureVisible(find.text(l.aclGroupNew));
    await t.pumpAndSettle();
    await t.tap(find.text(l.aclGroupNew));
    await t.pumpAndSettle();
    await t.enterText(find.byType(TextField), 'rideboss');
    await t.pump();
    await t.tap(find.text(l.aclGroupCreate));
    await t.pumpAndSettle();

    final made = drafted(t).single;
    expect(made.name, 'rideboss');
    expect(made.add, isEmpty);
    expect(made.inherit, isTrue);
    expect(made.inheritable, isTrue);
  });

  testWidgets('and a name already taken is refused', (t) async {
    // Two groups of one name are one group on the server, and which of them
    // survives is anybody's guess.
    final l = await L.delegate.load(const Locale('en'));
    final state = ready(aclWith([group(name: 'vip')]));
    await t.pumpWidget(host(state, channel));
    await t.pump(const Duration(milliseconds: 50));

    // The button sits under the groups, so on a short test screen it starts
    // below the fold and a tap would land on whatever is there instead.
    await t.ensureVisible(find.text(l.aclGroupNew));
    await t.pumpAndSettle();
    await t.tap(find.text(l.aclGroupNew));
    await t.pumpAndSettle();
    await t.enterText(find.byType(TextField), 'VIP');
    await t.pump();

    expect(find.text(l.aclGroupExists), findsOneWidget);
    expect(
      t.widget<FilledButton>(find.widgetWithText(FilledButton, l.aclGroupCreate))
          .onPressed,
      isNull,
    );
  });
}
