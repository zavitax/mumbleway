import 'package:flutter/material.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mumbleway/l10n/app_localizations.dart';
import 'package:mumbleway/screens/channel_acl_screen.dart';
import 'package:mumbleway/services/channel_permissions.dart';
import 'package:mumbleway/src/rust/api/mumbleway.dart';
import 'package:mumbleway/state/app_state.dart';

/// Channel permissions, where the dangerous mistake is not a failed edit but a
/// successful one that changes something nobody touched: the list is written
/// whole, so a rule dropped or a bit flipped on the way through is a permission
/// revoked by accident.
const _channel = UiChannel(
  id: 1,
  name: 'Garage',
  description: '',
  userCount: 0,
  maxUsers: 0,
);

UiAclRule rule({
  String? group = 'all',
  int? userId,
  int grant = 0,
  int deny = 0,
  bool inherited = false,
}) => UiAclRule(
  applyHere: true,
  applySubs: true,
  inherited: inherited,
  userId: userId,
  group: group,
  grant: grant,
  deny: deny,
);

void main() {
  group('a permission has three states, not two', () {
    test('silence is not denial', () {
      // A rule that does not mention a permission leaves it to another rule or
      // to the parent channel. A checkbox would turn that into a denial.
      expect(
        standOf(0, 0, ChannelPermission.speak),
        PermissionStand.unset,
      );
      expect(
        standOf(ChannelPermission.speak.bit, 0, ChannelPermission.speak),
        PermissionStand.granted,
      );
      expect(
        standOf(0, ChannelPermission.speak.bit, ChannelPermission.speak),
        PermissionStand.denied,
      );
    });

    test('tapping cycles through all three', () {
      var stand = PermissionStand.unset;
      stand = nextStand(stand);
      expect(stand, PermissionStand.granted);
      stand = nextStand(stand);
      expect(stand, PermissionStand.denied);
      stand = nextStand(stand);
      expect(stand, PermissionStand.unset);
    });

    test('setting one state clears the other, and touches nothing else', () {
      // Granted and denied are separate masks, and a permission in both is a
      // rule that contradicts itself.
      const speak = ChannelPermission.speak;
      const enter = ChannelPermission.enter;
      var (grant, deny) = (speak.bit | enter.bit, 0);

      (grant, deny) = applyStand(grant, deny, speak, PermissionStand.denied);
      expect(grant & speak.bit, 0, reason: 'no longer granted');
      expect(deny & speak.bit, speak.bit);
      expect(grant & enter.bit, enter.bit, reason: 'the other one is untouched');

      (grant, deny) = applyStand(grant, deny, speak, PermissionStand.unset);
      expect(grant & speak.bit, 0);
      expect(deny & speak.bit, 0);
    });
  });

  group('what a rule says, in words', () {
    late L en;

    setUpAll(() async => en = await L.delegate.load(const Locale('en')));

    test('a rule that says nothing says so', () {
      expect(describeRule(en, 0, 0), en.aclSaysNothing);
    });

    test('both halves are named', () {
      final text = describeRule(
        en,
        ChannelPermission.speak.bit,
        ChannelPermission.whisper.bit,
      );
      expect(text, contains(en.permSpeak));
      expect(text, contains(en.permWhisper));
      expect(text, contains(en.permSpeak));
    });
  });

  testWidgets('inherited rules are shown and cannot be edited', (tester) async {
    // Half the reason a permission behaves unexpectedly is a rule set in a
    // parent channel, so hiding them would hide the answer; and the server
    // drops them from anything written back, so offering to edit one would be
    // offering an edit that evaporates.
    final state = AppState();
    addTearDown(state.dispose);
    state.runtimes['srv'] = ServerRuntime()..status = ConnStatus.connected;
    state.runtimeFor('srv').acls[1] = UiChannelAcl(
      channelId: 1,
      inheritAcls: true,
      groups: const [],
      rules: [
        rule(group: 'all', grant: ChannelPermission.speak.bit, inherited: true),
        rule(group: 'admin', grant: ChannelPermission.write.bit),
      ],
    );

    await tester.pumpWidget(
      AppStateScope(
        state: state,
        child: MaterialApp(
          localizationsDelegates: const [
            ...L.localizationsDelegates,
            GlobalMaterialLocalizations.delegate,
            GlobalWidgetsLocalizations.delegate,
          ],
          supportedLocales: L.supportedLocales,
          home: const ChannelAclScreen(serverId: 'srv', channel: _channel),
        ),
      ),
    );
    await tester.pump(const Duration(milliseconds: 100));

    final l = await L.delegate.load(const Locale('en'));
    // Both rules are on screen.
    expect(find.textContaining(l.aclEverybody), findsOneWidget);
    expect(find.textContaining('admin'), findsOneWidget);
    // The inherited one says where it came from and offers no delete.
    expect(find.text(l.aclInherited), findsOneWidget);
    expect(find.byIcon(Icons.delete_outline), findsOneWidget);
  });

  test('an access list is kept per channel, per server', () {
    final state = AppState();
    addTearDown(state.dispose);

    state.onEvent(
      AppEvent.acl(
        serverId: 'a',
        acl: UiChannelAcl(
          channelId: 1,
          inheritAcls: true,
          groups: const [],
          rules: [rule()],
        ),
      ),
    );

    expect(state.runtimeFor('a').acls[1], isNotNull);
    expect(state.runtimeFor('a').acls[2], isNull);
    expect(state.runtimeFor('b').acls[1], isNull);
  });

  test('names found for ids are remembered', () {
    // A rule names people by number, and a number is not something to show.
    final state = AppState();
    addTearDown(state.dispose);

    state.onEvent(
      const AppEvent.userNames(
        serverId: 'a',
        names: [UiUserName(userId: 3, name: 'Anna')],
      ),
    );

    expect(state.runtimeFor('a').userNames[3], 'Anna');
  });
}
