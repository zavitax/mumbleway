import 'dart:convert';

import 'package:flutter_test/flutter_test.dart';
import 'package:mumbleway/src/rust/api/mumbleway.dart';
import 'package:mumbleway/state/app_state.dart';
import 'package:shared_preferences/shared_preferences.dart';

/// A note lives with the server entry, not with the session.
///
/// **A Mumble server keeps a comment only for registered users.** For everybody
/// else it is gone the moment the link drops, and on a bike the link drops. So
/// the copy that matters is the saved one, and the test that matters is that it
/// survives the things that end a session: a reconnect, and a restart.
SavedServer server({String note = '', String id = 'srv'}) => SavedServer(
  name: 'Clubhouse',
  host: 'mumble.example',
  port: 64738,
  username: 'rider',
  localId: id,
  note: note,
);

void main() {
  setUp(() => SharedPreferences.setMockInitialValues({}));

  group('the entry carries it', () {
    test('a note survives a round trip through storage', () {
      final saved = SavedServer.fromJson(
        jsonDecode(jsonEncode(server(note: 'On the A9').toJson()))
            as Map<String, dynamic>,
      );
      expect(saved.note, 'On the A9');
    });

    test('an entry saved before notes existed reads as having none', () {
      // Every rider's existing server list is exactly this shape.
      final old = {
        'localId': 'srv',
        'name': 'Clubhouse',
        'host': 'mumble.example',
        'port': 64738,
        'username': 'rider',
      };
      expect(SavedServer.fromJson(old).note, '');
    });

    test('an empty note is a value, not "leave it alone"', () {
      // Clearing a note is how a rider takes it down. If copyWith treated
      // empty as unchanged, the note could be set but never removed.
      final cleared = server(note: 'On the A9').copyWith(note: '');
      expect(cleared.note, '');
    });

    test('changing the note does not rebuild the connection', () {
      // A note is not part of how the session is built, and tearing the call
      // down to change a line of text would be absurd mid-ride.
      final before = server(note: 'one');
      expect(before.sameConnection(before.copyWith(note: 'two')), isTrue);
    });
  });

  _lastChannelTests();

  group('setting one', () {
    test('is remembered against that server, and stamped as an edit', () async {
      final state = AppState();
      addTearDown(state.dispose);
      state.servers.add(server());

      await state.setNoteOn('srv', '  On the A9  ');

      final saved = state.servers.single;
      expect(saved.note, 'On the A9', reason: 'and trimmed');
      expect(
        saved.updatedAt,
        greaterThan(0),
        reason: 'a deliberate edit should win against an older copy on sync',
      );
    });

    test('works while disconnected, because there is somewhere to put it',
        () async {
      // The session is where a note is *shown*; the entry is where it lives.
      final state = AppState();
      addTearDown(state.dispose);
      state.servers.add(server());

      final error = await state.setNoteOn('srv', 'Back in ten');

      expect(error, isNull);
      expect(state.servers.single.note, 'Back in ten');
    });

    test('clearing it is kept too', () async {
      final state = AppState();
      addTearDown(state.dispose);
      state.servers.add(server(note: 'On the A9'));

      await state.setNoteOn('srv', '');

      expect(state.servers.single.note, '');
    });

    test('each server keeps its own', () async {
      final state = AppState();
      addTearDown(state.dispose);
      state.servers
        ..add(server(id: 'a'))
        ..add(server(id: 'b'));

      await state.setNoteOn('a', 'Riding');

      expect(state.servers.firstWhere((s) => s.id == 'a').note, 'Riding');
      expect(
        state.servers.firstWhere((s) => s.id == 'b').note,
        '',
        reason: '"back in ten" is true on this ride, not on the club server',
      );
    });
  });
}

/// Where a rider was, so a reconnect puts them back.
///
/// A dropped link on a bike is ordinary; coming back into the root channel
/// while the group carries on talking in theirs is the same as still being
/// dropped, only quieter.
void _lastChannelTests() {
  group('the last channel used', () {
    test('survives storage', () {
      final saved = SavedServer.fromJson(
        jsonDecode(jsonEncode(server().copyWith(lastChannel: 'Garage').toJson()))
            as Map<String, dynamic>,
      );
      expect(saved.lastChannel, 'Garage');
    });

    test('an entry from before this existed simply has none', () {
      expect(SavedServer.fromJson({'localId': 'srv'}).lastChannel, isNull);
    });

    test('is not part of how the session is built', () {
      // Moving channel must not tear the connection down and rebuild it.
      final before = server();
      expect(
        before.sameConnection(before.copyWith(lastChannel: 'Garage')),
        isTrue,
      );
    });

    test('is remembered when the roster says we moved', () {
      final state = AppState();
      addTearDown(state.dispose);
      state.servers.add(server());
      final rt = state.runtimeFor('srv')
        ..selfSession = 7
        ..channels = const [
          UiChannel(
            id: 3,
            name: 'Garage',
            description: '',
            userCount: 1,
            maxUsers: 0,
          ),
        ];

      state.onEvent(
        AppEvent.users(
          serverId: 'srv',
          users: [
            UiUser(
              session: 7,
              name: 'rider',
              channelId: 3,
              talking: false,
              muted: false,
              deafened: false,
              localMute: false,
              selfMuted: false,
              selfDeafened: false,
              status: 'silent',
              comment: '',
              prioritySpeaker: false,
              suppressed: false,
            ),
          ],
        ),
      );

      expect(rt.currentChannel?.name, 'Garage');
      expect(state.servers.single.lastChannel, 'Garage');
    });

    test('moving channel is not an edit that wins a sync contest', () {
      // `updatedAt` decides which device's copy of an entry survives. A rider
      // hopping channels has not changed the server, and stamping it would
      // have this device win conflicts it took no part in.
      final state = AppState();
      addTearDown(state.dispose);
      state.servers.add(server());
      final before = state.servers.single.updatedAt;
      final rt = state.runtimeFor('srv')
        ..selfSession = 7
        ..channels = const [
          UiChannel(
            id: 3,
            name: 'Garage',
            description: '',
            userCount: 1,
            maxUsers: 0,
          ),
        ];
      state.onEvent(
        AppEvent.users(
          serverId: 'srv',
          users: [
            UiUser(
              session: 7,
              name: 'rider',
              channelId: 3,
              talking: false,
              muted: false,
              deafened: false,
              localMute: false,
              selfMuted: false,
              selfDeafened: false,
              status: 'silent',
              comment: '',
              prioritySpeaker: false,
              suppressed: false,
            ),
          ],
        ),
      );
      expect(rt.currentChannel?.name, 'Garage');
      expect(state.servers.single.updatedAt, before);
    });
  });
}
