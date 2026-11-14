import 'dart:async';

import 'package:connectivity_plus/connectivity_plus.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mumbleway/services/network_watch.dart';

void main() {
  const none = [ConnectivityResult.none];
  const wifi = [ConnectivityResult.wifi];
  const mobile = [ConnectivityResult.mobile];
  const settle = Duration(milliseconds: 150);

  /// Feeds [reports] spaced [gap] apart, waits for anything pending to settle,
  /// and returns how often the engine was told.
  Future<int> run(
    List<List<ConnectivityResult>> reports, {
    Duration gap = const Duration(milliseconds: 300),
  }) async {
    var told = 0;
    final ctl = StreamController<List<ConnectivityResult>>();
    final watch = NetworkWatch(
      onAvailable: () => told++,
      changes: ctl.stream,
      settle: settle,
    )..start();
    for (final r in reports) {
      ctl.add(r);
      await Future<void>.delayed(gap);
    }
    await Future<void>.delayed(settle * 3);
    watch.dispose();
    await ctl.close();
    return told;
  }

  test('the starting state is not news', () async {
    expect(await run([wifi]), 0);
  });

  test('coming back from no network is reported', () async {
    expect(await run([wifi, none, wifi]), 1);
    expect(await run([none, mobile]), 1, reason: 'started in a tunnel');
  });

  test('a handover between networks is reported', () async {
    expect(await run([wifi, mobile]), 1);
  });

  test('the same network announced again is not', () async {
    expect(await run([wifi, wifi, wifi]), 0);
  });

  test('a link still settling is reported once', () async {
    final told = await run([
      none,
      wifi,
      mobile,
      wifi,
      mobile,
    ], gap: const Duration(milliseconds: 5));
    expect(told, 1);
  });

  test('losing the network before it settles cancels the report', () async {
    final told = await run([
      none,
      wifi,
      none,
    ], gap: const Duration(milliseconds: 5));
    expect(told, 0);
  });
}
