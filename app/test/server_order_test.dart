import 'package:flutter_test/flutter_test.dart';
import 'package:mumbleway/state/app_state.dart';

/// Where a server lands in the list when the rider connects to it.
///
/// The ordering is a pure function precisely so it can be tested here: the
/// list lives on `AppState`, which needs an engine behind it, and none of the
/// cases below are about an engine.
void main() {
  SavedServer s(String host) =>
      SavedServer(name: host, host: host, port: 64738, username: 'rider');

  List<SavedServer> listOf(List<String> hosts) => [for (final h in hosts) s(h)];

  List<String> idsOf(List<SavedServer> list) => [for (final e in list) e.id];

  /// Connect to `id` with `live` already connected.
  List<String> connect(
    List<String> hosts,
    String host, {
    Set<String> live = const {},
  }) {
    final list = listOf(hosts);
    AppState.promoteOnConnect(
      list,
      '$host:64738',
      (id) => live.contains(id.split(':').first),
    );
    return [for (final e in idsOf(list)) e.split(':').first];
  }

  test('the server just connected to goes to the top', () {
    expect(connect(['a', 'b', 'c'], 'c'), ['c', 'a', 'b']);
  });

  test('connecting to the one already at the top changes nothing', () {
    expect(connect(['a', 'b', 'c'], 'a'), ['a', 'b', 'c']);
  });

  test('a server already connected keeps its place', () {
    // The rule this exists for. `a` is live, so joining `c` must not push `a`
    // down — the list must not reorder under a thumb reaching for a live
    // conversation.
    expect(connect(['a', 'b', 'c'], 'c', live: {'a'}), ['a', 'c', 'b']);
  });

  test('the second connected server sits beneath the first', () {
    // Two live already, so a third lands under both rather than on top.
    expect(
      connect(['a', 'b', 'c', 'd'], 'd', live: {'a', 'b'}),
      ['a', 'b', 'd', 'c'],
    );
  });

  test('only a live run at the very top is protected', () {
    // `c` is live but sits below a disconnected `a`, so it is not part of the
    // run at the top and does not hold `b` down.
    expect(connect(['a', 'c', 'b'], 'b', live: {'c'}), ['b', 'a', 'c']);
  });

  test('reconnecting a live server leaves the order alone', () {
    // It is already inside the protected run, and re-tapping it must not
    // reshuffle the run around it.
    expect(connect(['a', 'b', 'c'], 'b', live: {'a', 'b'}), ['a', 'b', 'c']);
  });

  test('a server that is not in the list is ignored', () {
    expect(connect(['a', 'b'], 'zz'), ['a', 'b']);
  });

  test('a single-entry list survives', () {
    expect(connect(['a'], 'a'), ['a']);
  });
}
