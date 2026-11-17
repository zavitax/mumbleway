import 'dart:async';

import 'package:connectivity_plus/connectivity_plus.dart';

/// Tells the engine when the device's network comes back.
///
/// A session that loses its server waits out a backoff that grows with each
/// failed attempt, up to tens of seconds. A rider riding out of a tunnel has
/// a working link again long before that backoff ends, so the operating
/// system's word that a network appeared is passed straight to the engine,
/// which resets the backoff and tries at once (`network_available`).
///
/// What counts as "came back" is filtered to routing interfaces only (Wi-Fi,
/// mobile, ethernet, VPN), ignoring `bluetooth` and `other` so toggling a
/// Bluetooth headset never triggers a reconnect. Going from Wi-Fi to mobile
/// data is the common case on a bike, and the old socket rarely survives it
/// either.
///
/// Reports are debounced, because a link that is still settling announces
/// itself several times in a second, and each report is an immediate attempt.
class NetworkWatch {
  NetworkWatch({
    required this.onAvailable,
    this._changes,
    this._check,
    this.settle = const Duration(milliseconds: 800),
  });

  /// Called once the network has come back and stayed for [settle].
  final void Function() onAvailable;

  /// How long a new network has to hold before it is reported.
  final Duration settle;

  final Stream<List<ConnectivityResult>>? _changes;
  final Future<List<ConnectivityResult>> Function()? _check;
  StreamSubscription<List<ConnectivityResult>>? _sub;
  Timer? _pending;
  Set<ConnectivityResult>? _last;

  static const Set<ConnectivityResult> _routing = {
    ConnectivityResult.wifi,
    ConnectivityResult.mobile,
    ConnectivityResult.ethernet,
    ConnectivityResult.vpn,
  };

  /// Starts listening. Safe to call once; seeds the starting state without
  /// firing [onAvailable] so the first real transition after startup is
  /// never swallowed.
  void start() {
    if (_sub != null) return;
    final changesOverride = _changes;
    final Stream<List<ConnectivityResult>> changes;
    final Future<List<ConnectivityResult>> Function()? check;
    if (changesOverride != null) {
      changes = changesOverride;
      check = _check;
    } else {
      final connectivity = Connectivity();
      changes = connectivity.onConnectivityChanged;
      check = _check ?? connectivity.checkConnectivity;
    }
    final sub = changes.listen(
      _on,
      onError: (Object _) {
        // A platform without the plugin simply never reports; reconnects then
        // fall back to the backoff alone, which is how it worked before.
      },
    );
    _sub = sub;
    if (check != null) {
      unawaited(_seed(check, sub));
    }
  }

  Future<void> _seed(
    Future<List<ConnectivityResult>> Function() check,
    StreamSubscription<List<ConnectivityResult>> sub,
  ) async {
    try {
      final initial = await check();
      if (_sub != sub || _last != null) return;
      _last = _routingOf(initial);
    } catch (_) {
      // A platform or test binding without the plugin falls back to seeding
      // from the first stream report instead.
    }
  }

  void _on(List<ConnectivityResult> results) {
    final now = _routingOf(results);
    final previous = _last;
    _last = now;
    if (now.isEmpty) {
      // Lost it: a report still waiting to settle is now wrong.
      _pending?.cancel();
      _pending = null;
      return;
    }
    if (previous == null || _same(previous, now)) return;
    _pending?.cancel();
    _pending = Timer(settle, () {
      _pending = null;
      onAvailable();
    });
  }

  static Set<ConnectivityResult> _routingOf(
    List<ConnectivityResult> results,
  ) => results.where(_routing.contains).toSet();

  static bool _same(Set<ConnectivityResult> a, Set<ConnectivityResult> b) =>
      a.length == b.length && a.containsAll(b);

  void dispose() {
    _pending?.cancel();
    _pending = null;
    _sub?.cancel();
    _sub = null;
    _last = null;
  }
}
