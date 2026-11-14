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
/// What counts as "came back" is deliberately broad: any change to a set that
/// is not empty and not just `none`. Going from Wi-Fi to mobile data is the
/// common case on a bike, and the old socket rarely survives it either.
///
/// Reports are debounced, because a link that is still settling announces
/// itself several times in a second, and each report is an immediate attempt.
class NetworkWatch {
  NetworkWatch({
    required this.onAvailable,
    this._changes,
    this.settle = const Duration(milliseconds: 800),
  });

  /// Called once the network has come back and stayed for [settle].
  final void Function() onAvailable;

  /// How long a new network has to hold before it is reported.
  final Duration settle;

  final Stream<List<ConnectivityResult>>? _changes;
  StreamSubscription<List<ConnectivityResult>>? _sub;
  Timer? _pending;
  Set<ConnectivityResult>? _last;

  /// Starts listening. Safe to call once; the first report only records the
  /// starting state, since the engine has just been told everything anyway.
  void start() {
    if (_sub != null) return;
    final changes = _changes ?? Connectivity().onConnectivityChanged;
    _sub = changes.listen(
      _on,
      onError: (Object _) {
        // A platform without the plugin simply never reports; reconnects then
        // fall back to the backoff alone, which is how it worked before.
      },
    );
  }

  void _on(List<ConnectivityResult> results) {
    final now = results.toSet();
    final previous = _last;
    _last = now;
    final online = now.any((r) => r != ConnectivityResult.none);
    if (!online) {
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

  static bool _same(Set<ConnectivityResult> a, Set<ConnectivityResult> b) =>
      a.length == b.length && a.containsAll(b);

  void dispose() {
    _pending?.cancel();
    _pending = null;
    _sub?.cancel();
    _sub = null;
  }
}
