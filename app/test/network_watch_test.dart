import 'dart:async';

import 'package:connectivity_plus/connectivity_plus.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mumbleway/services/network_watch.dart';

void main() {
  const none = [ConnectivityResult.none];
  const wifi = [ConnectivityResult.wifi];
  const mobile = [ConnectivityResult.mobile];
  const ethernet = [ConnectivityResult.ethernet];
  const bluetooth = [ConnectivityResult.bluetooth];
  const other = [ConnectivityResult.other];
  const settle = Duration(milliseconds: 150);

  /// Feeds [reports] spaced [gap] apart, waits for anything pending to settle,
  /// and returns how often the engine was told.
  Future<int> run(
    List<List<ConnectivityResult>> reports, {
    Duration gap = const Duration(milliseconds: 300),
    Future<List<ConnectivityResult>> Function()? check,
  }) async {
    var told = 0;
    final ctl = StreamController<List<ConnectivityResult>>();
    final watch = NetworkWatch(
      onAvailable: () => told++,
      changes: ctl.stream,
      check: check,
      settle: settle,
    )..start();
    if (check != null) {
      await Future<void>.delayed(Duration.zero);
    }
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

  test('toggling bluetooth or other non-routing links is ignored', () async {
    const wifiBt = [ConnectivityResult.wifi, ConnectivityResult.bluetooth];
    const wifiOther = [ConnectivityResult.wifi, ConnectivityResult.other];
    expect(await run([wifi, wifiBt, wifi, wifiOther, wifi]), 0);
    expect(await run([none, bluetooth, other, none]), 0);
  });

  test('routing interfaces vpn and ethernet are tracked', () async {
    const wifiVpn = [ConnectivityResult.wifi, ConnectivityResult.vpn];
    expect(await run([wifi, wifiVpn]), 1);
    expect(await run([ethernet, wifi]), 1);
  });

  test('seeded initial state reports the first real transition', () async {
    // Platform stream only emits on changes, so seeding via check() ensures
    // the very first stream event (wifi -> mobile, or none -> mobile) fires.
    expect(await run([mobile], check: () async => wifi), 1);
    expect(await run([mobile], check: () async => none), 1);
    // Redundant replay of the starting state or bluetooth toggle stays silent.
    expect(await run([wifi], check: () async => wifi), 0);
    expect(
      await run([
        [ConnectivityResult.wifi, ConnectivityResult.bluetooth],
      ], check: () async => wifi),
      0,
    );
  });

  test('failing initial check falls back to seeding from the stream', () async {
    expect(
      await run(
        [wifi, mobile],
        check: () async => throw StateError('missing plugin'),
      ),
      1,
    );
    expect(
      await run(
        [wifi],
        check: () async => throw StateError('missing plugin'),
      ),
      0,
    );
  });

  test('late initial check does not overwrite newer stream state', () async {
    var told = 0;
    final ctl = StreamController<List<ConnectivityResult>>();
    final probe = Completer<List<ConnectivityResult>>();
    final watch = NetworkWatch(
      onAvailable: () => told++,
      changes: ctl.stream,
      check: () => probe.future,
      settle: settle,
    )..start();

    // Stream emits wifi before check() completes; late check(none) must not
    // overwrite _last to empty and cause a duplicate wifi report.
    ctl.add(wifi);
    await Future<void>.delayed(Duration.zero);
    probe.complete(none);
    await Future<void>.delayed(Duration.zero);
    ctl.add(wifi);
    await Future<void>.delayed(settle * 3);
    expect(told, 0);

    watch.dispose();
    await ctl.close();
  });
}
