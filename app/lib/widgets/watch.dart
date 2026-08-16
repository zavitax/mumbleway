import 'package:flutter/widgets.dart';

import '../state/app_state.dart';

/// One value of the app state, and the only part of a screen that follows it.
///
/// [AppStateScope] is an `InheritedNotifier`, so reading it subscribes to
/// *every* change: a call in a screen's own `build` rebuilds that screen's
/// whole subtree each time anything at all moves, and the audio engine moves
/// something twice a second for the whole of a ride.
///
/// Two things are wrong with subscribing at the top, and only wrapping fixes
/// the first:
///
///  1. **Scope** — the whole screen rebuilds for one control.
///  2. **Relevance** — even wrapped, a control rebuilds on notifications that
///     changed nothing it displays.
///
/// So this takes a [select] as well as a [builder]. While the selected value
/// compares equal, the previously built subtree is returned *by identity*,
/// which makes `Element.updateChild` skip it without descending — the cost is
/// one comparison, not one rebuild.
///
/// Measured on the settings screen: one notification rebuilt 875 widgets;
/// wrapping the four radio groups took it to 345, and selecting each group's
/// own value took it to 109 — 13.2 ms to 4.2 ms.
///
/// [select] must return something with a meaningful `==` — an enum, a number, a
/// bool, a string, or a record of those. Returning an object that compares by
/// identity means the cache never hits, which merely wastes the wrapper.
/// Returning one that compares equal while its contents differ means the screen
/// goes stale, which is worse and is why this takes a selector rather than
/// diffing the built widget or guessing.
///
/// **Keep `L.of(context)` and `Theme.of(context)` in the enclosing build, not
/// inside [builder].** Those subscribe the parent, so a language or theme
/// change rebuilds it, which makes a new [Watch] and drops the cache. Read them
/// inside and a translated string can survive a language change.
class Watch<T> extends StatefulWidget {
  const Watch(this.select, this.builder, {super.key});

  /// The value this part of the screen actually shows.
  final T Function(AppState state) select;

  /// Called only when [select] returns something different, or the parent
  /// rebuilds.
  final Widget Function(BuildContext context, AppState state) builder;

  @override
  State<Watch<T>> createState() => _WatchState<T>();
}

class _WatchState<T> extends State<Watch<T>> {
  T? _value;
  Widget? _child;

  @override
  void didUpdateWidget(Watch<T> old) {
    super.didUpdateWidget(old);
    // The parent rebuilt, so [Watch.builder] is a new closure over whatever
    // made it rebuild — the locale, most often. Nothing here can tell what it
    // captured, so the safe move is to build again.
    _child = null;
  }

  @override
  Widget build(BuildContext context) {
    final state = AppStateScope.of(context);
    final value = widget.select(state);
    if (_child == null || value != _value) {
      _value = value;
      _child = widget.builder(context, state);
    }
    return _child!;
  }
}
