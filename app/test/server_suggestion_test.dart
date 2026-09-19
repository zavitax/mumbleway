import 'package:flutter_test/flutter_test.dart';
import 'package:mumbleway/src/rust/api/mumbleway.dart';
import 'package:mumbleway/state/app_state.dart';

/// A server administrator can ask riders to use push-to-talk or positional
/// audio. It is a suggestion the server neither enforces nor checks, so the
/// question is only ever *when is it worth interrupting somebody*.
void main() {
  Future<List<String>> collect(
    AppState state,
    void Function() act,
  ) async {
    final seen = <String>[];
    final sub = state.serverSuggestions.listen(seen.add);
    act();
    await Future<void>.delayed(Duration.zero);
    await sub.cancel();
    return seen;
  }

  test('a rider already using push to talk is not told to', () async {
    final state = AppState();
    addTearDown(state.dispose);
    state.micMode = MicMode.pushToTalk;

    final seen = await collect(
      state,
      () => state.onEvent(
        const AppEvent.serverSuggests(serverId: 's', pushToTalk: true),
      ),
    );

    expect(seen, isEmpty, reason: 'they are already doing it');
  });

  test('a rider on voice activation is offered the switch', () async {
    final state = AppState();
    addTearDown(state.dispose);
    state.micMode = MicMode.voiceActivity;

    final seen = await collect(
      state,
      () => state.onEvent(
        const AppEvent.serverSuggests(serverId: 's', pushToTalk: true),
      ),
    );

    expect(seen, ['ptt']);
  });

  test('the same server does not ask twice', () async {
    // Reconnects are ordinary on a bike. A notice that comes back every time
    // is one people learn to swipe away without reading.
    final state = AppState();
    addTearDown(state.dispose);
    state.micMode = MicMode.voiceActivity;

    final first = await collect(
      state,
      () => state.onEvent(
        const AppEvent.serverSuggests(serverId: 's', pushToTalk: true),
      ),
    );
    final second = await collect(
      state,
      () => state.onEvent(
        const AppEvent.serverSuggests(serverId: 's', pushToTalk: true),
      ),
    );

    expect(first, ['ptt']);
    expect(second, isEmpty);
  });

  test('a different server gets its own say', () async {
    final state = AppState();
    addTearDown(state.dispose);
    state.micMode = MicMode.voiceActivity;

    final seen = await collect(state, () {
      state
        ..onEvent(const AppEvent.serverSuggests(serverId: 'a', pushToTalk: true))
        ..onEvent(
          const AppEvent.serverSuggests(serverId: 'b', pushToTalk: true),
        );
    });

    expect(seen, ['ptt', 'ptt']);
  });

  test('positional audio is mentioned once, and asks for nothing', () async {
    final state = AppState();
    addTearDown(state.dispose);

    final seen = await collect(
      state,
      () => state.onEvent(
        const AppEvent.serverSuggests(serverId: 's', positional: true),
      ),
    );

    expect(seen, ['positional']);
  });

  test('a suggestion of nothing in particular says nothing', () async {
    final state = AppState();
    addTearDown(state.dispose);

    final seen = await collect(
      state,
      () => state.onEvent(
        const AppEvent.serverSuggests(
          serverId: 's',
          pushToTalk: false,
          positional: false,
        ),
      ),
    );

    expect(seen, isEmpty);
  });
}
