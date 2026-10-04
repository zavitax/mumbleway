import 'package:flutter/material.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mumbleway/l10n/app_localizations.dart';
import 'package:mumbleway/src/rust/api/mumbleway.dart';
import 'package:mumbleway/state/app_state.dart';
import 'package:mumbleway/widgets/channel_panel.dart';
import 'package:mumbleway/widgets/registered_users_dialog.dart';

/// The administrative half: making channels, handing out priority, giving
/// somebody an account and taking it away.
///
/// All of it is permission-gated, and the protocol work is verified against a
/// real server in `core/tests/live_server.rs`. What is pinned here is the
/// gating — offering an action that cannot work wastes a tap at a junction,
/// and hiding one that can is worse.
UiRights rights({
  bool known = true,
  bool write = false,
  bool makeChannel = false,
  bool registerOthers = false,
  bool muteDeafen = false,
}) => UiRights(
  known: known,
  speak: true,
  muteDeafen: muteDeafen,
  moveUsers: false,
  text: true,
  whisper: true,
  makeChannel: makeChannel,
  write: write,
  kick: false,
  ban: false,
  registerOthers: registerOthers,
  selfRegister: false,
);

UiUser rider({bool priority = false}) => UiUser(
  session: 7,
  name: 'Anna',
  channelId: 1,
  talking: false,
  muted: false,
  deafened: false,
  localMute: false,
  mutedYou: false,
  userId: null,
  selfMuted: false,
  selfDeafened: false,
  status: 'silent',
  comment: '',
  prioritySpeaker: priority,
  suppressed: false,
);

const _channel = UiChannel(
  id: 1,
  name: 'Garage',
  description: '',
  userCount: 1,
  maxUsers: 0,
);

void main() {
  /// Dialogs bring their own bounded box; a tree needs scrolling around it.
  /// Wrapping a dialog in a scroll view instead gives its list unbounded
  /// height, which lays out nothing and reads as a missing row.
  Widget hostDialog(AppState state, Widget child) => AppStateScope(
    state: state,
    child: MaterialApp(
      localizationsDelegates: const [
        ...L.localizationsDelegates,
        GlobalMaterialLocalizations.delegate,
        GlobalWidgetsLocalizations.delegate,
      ],
      supportedLocales: L.supportedLocales,
      home: Scaffold(body: child),
    ),
  );

  Widget host(AppState state, Widget child) => AppStateScope(
    state: state,
    child: MaterialApp(
      localizationsDelegates: const [
        ...L.localizationsDelegates,
        GlobalMaterialLocalizations.delegate,
        GlobalWidgetsLocalizations.delegate,
      ],
      supportedLocales: L.supportedLocales,
      home: Scaffold(body: SingleChildScrollView(child: child)),
    ),
  );

  AppState stateWith(UiRights has) {
    final state = AppState();
    addTearDown(state.dispose);
    state.runtimes['srv'] = ServerRuntime()
      ..status = ConnStatus.connected
      ..rights = has;
    return state;
  }

  group('managing channels', () {
    testWidgets('no menu at all where nothing is allowed', (tester) async {
      // Unlike the participant menu, which always has local actions in it,
      // a channel menu of entirely greyed entries says nothing worth saying.
      final state = stateWith(rights());
      await tester.pumpWidget(
        host(
          state,
          const ChannelTree(
            serverId: 'srv',
            channels: [_channel],
            currentChannelId: null,
            defaultChannelName: null,
          ),
        ),
      );
      await tester.pump(const Duration(milliseconds: 50));

      expect(find.byIcon(Icons.more_horiz), findsNothing);
    });

    testWidgets('a rider who may make channels gets the menu', (tester) async {
      final state = stateWith(rights(makeChannel: true));
      await tester.pumpWidget(
        host(
          state,
          const ChannelTree(
            serverId: 'srv',
            channels: [_channel],
            currentChannelId: null,
            defaultChannelName: null,
          ),
        ),
      );
      await tester.pump(const Duration(milliseconds: 50));
      await tester.tap(find.byIcon(Icons.more_horiz));
      await tester.pump(const Duration(milliseconds: 400));

      final l = await L.delegate.load(const Locale('en'));
      PopupMenuItem<String> item(String text) => tester.widget(
        find
            .ancestor(
              of: find.text(text),
              matching: find.byType(PopupMenuItem<String>),
            )
            .first,
      );

      expect(item(l.channelAdd).enabled, isTrue);
      expect(
        item(l.channelRename).enabled,
        isFalse,
        reason: 'making a channel under this one is not permission to rename it',
      );
      expect(item(l.channelRemove).enabled, isFalse);
    });
  });

  group('the registered users', () {
    testWidgets('an empty list says so', (tester) async {
      final state = stateWith(rights(registerOthers: true));
      await tester.pumpWidget(
        hostDialog(state, const RegisteredUsersDialog(serverId: 'srv')),
      );
      await tester.pump(const Duration(milliseconds: 50));

      final l = await L.delegate.load(const Locale('en'));
      expect(find.text(l.registeredNobody), findsOneWidget);
    });

    testWidgets('each account is listed with a way to remove it', (
      tester,
    ) async {
      final state = stateWith(rights(registerOthers: true));
      state.runtimeFor('srv').registered = const [
        UiRegisteredUser(userId: 3, name: 'Anna', lastSeen: '2026-09-30'),
        UiRegisteredUser(userId: 4, name: 'Boris', lastSeen: ''),
      ];

      await tester.pumpWidget(
        hostDialog(state, const RegisteredUsersDialog(serverId: 'srv')),
      );
      await tester.pump(const Duration(milliseconds: 50));

      final l = await L.delegate.load(const Locale('en'));
      expect(find.text('Anna'), findsOneWidget);
      expect(find.text('Boris'), findsOneWidget);
      expect(find.text(l.registeredUnregister), findsNWidgets(2));
      expect(find.textContaining('2026-09-30'), findsOneWidget);
    });

    test('a list arrives under the server it came from', () {
      final state = AppState();
      addTearDown(state.dispose);

      state.onEvent(
        const AppEvent.registered(
          serverId: 'a',
          users: [UiRegisteredUser(userId: 3, name: 'Anna', lastSeen: '')],
        ),
      );

      expect(state.runtimeFor('a').registered, hasLength(1));
      expect(state.runtimeFor('b').registered, isEmpty);
    });
  });

  group('acting on a rider', () {
    testWidgets('priority speaker reads as grant or revoke, not a state', (
      tester,
    ) async {
      final l = await L.delegate.load(const Locale('en'));

      Future<String> labelFor({required bool priority}) async {
        final state = stateWith(rights(muteDeafen: true));
        await tester.pumpWidget(const SizedBox.shrink());
        await tester.pump();
        await tester.pumpWidget(
          host(
            state,
            ChannelUserList(
              serverId: 'srv',
              users: [rider(priority: priority)],
            ),
          ),
        );
        await tester.pump(const Duration(milliseconds: 50));
        await tester.tap(find.byIcon(Icons.more_vert));
        await tester.pump(const Duration(milliseconds: 400));
        return find.text(l.priorityGrant).evaluate().isNotEmpty
            ? l.priorityGrant
            : l.priorityRevoke;
      }

      expect(await labelFor(priority: false), l.priorityGrant);
      expect(await labelFor(priority: true), l.priorityRevoke);
    });

    test('details arrive against the rider they are about', () {
      final state = AppState();
      addTearDown(state.dispose);

      state.onEvent(
        const AppEvent.userDetails(
          serverId: 'a',
          details: UiUserDetails(
            session: 7,
            release: 'Mumble 1.5.735',
            os: 'Linux',
            osVersion: '',
            address: '10.0.0.5',
            strongCertificate: false,
            onlineSecs: 600,
            idleSecs: 30,
          ),
        ),
      );

      expect(state.runtimeFor('a').userDetails[7]?.release, 'Mumble 1.5.735');
      expect(state.runtimeFor('a').userDetails[8], isNull);
    });
  });
}
