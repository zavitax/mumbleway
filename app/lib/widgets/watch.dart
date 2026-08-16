import 'package:flutter/widgets.dart';

import '../state/app_state.dart';

/// One value of the app state, and the only part of a screen that follows it.
///
/// [AppStateScope] is an `InheritedNotifier`, so *reading* it subscribes to
/// every change: a call in a screen's own `build` rebuilds that screen's whole
/// subtree each time anything at all moves, and the audio engine moves
/// something twice a second for the whole of a ride.
///
/// Two things are wrong with subscribing at the top, and only wrapping fixes
/// the first:
///
///  1. **Scope** — the whole screen rebuilds for one control.
///  2. **Relevance** — even wrapped, a control rebuilds on notifications that
///     changed nothing it displays.
///
/// So this takes a [select] as well as a [builder], and does not subscribe
/// through the inherited widget at all: it reads the state without depending on
/// it and listens to it directly, so an unrelated notification does not reach
/// this element in the first place. When [select] returns something equal,
/// nothing is marked dirty and nothing rebuilds.
///
/// Measured on the settings screen: one notification rebuilt 875 widgets;
/// wrapping the four radio groups took it to 345, and selecting each group's
/// own value took it to 109 — 13.2 ms to 4.2 ms.
///
/// **An earlier version cached the built subtree instead, and shipped a bug.**
/// It subscribed normally, then returned the previous widget by identity while
/// the selected value was unchanged, so `Element.updateChild` would skip it.
/// That works until a builder reads something *other* than the app state —
/// `L.of(context)` most of all. Such a builder captures the strings of the
/// moment it last ran, the cached subtree never runs again, and changing the
/// language leaves six settings tiles in the old one. The rule "keep `L.of` in
/// the enclosing build" was written down and was still too easy to break, so
/// the cache is gone: the builder now runs on every rebuild, and a rebuild
/// happens for a locale change and not for a level meter.
///
/// [select] must return something with a meaningful `==` — an enum, a number, a
/// bool, a string, or a record of those. Returning an object that compares by
/// identity means it rebuilds every time, which merely wastes the wrapper.
/// Returning one that compares equal while its contents differ means the
/// control goes stale, which is worse and is why this takes a selector rather
/// than diffing the built widget or guessing.
class Watch<T> extends StatefulWidget {
  const Watch(this.select, this.builder, {super.key});

  /// Everything [builder] reads from the app state, and nothing else.
  final T Function(AppState state) select;

  final Widget Function(BuildContext context, AppState state) builder;

  @override
  State<Watch<T>> createState() => _WatchState<T>();
}

/// One value of a [Listenable], and the only part of a tree that follows it.
///
/// The same idea as [Watch], for a notifier held in hand rather than reached
/// through an inherited widget — and for the same reason. `ListenableBuilder`
/// rebuilds its whole subtree on every notification, which is right when the
/// subtree *is* the thing that changed and wasteful when it is not.
///
/// Measured on the listen sheet: `RecordingPlayer` fires every 80 ms while a
/// recording plays, and one `ListenableBuilder` around the waveform *and* the
/// transport controls rebuilt 204 widgets a tick — six `IconButton`s with their
/// tooltips, ink and gesture machinery, none of which move with the playhead.
///
/// Unlike [Watch] this does not cache a subtree: when [select] returns the same
/// value nothing is rebuilt at all, because nothing is marked dirty. Return a
/// record to follow several values at once — records compare by their fields,
/// so `(a.playing, a.speechOnly)` is a complete key and a cheap one.
///
/// The same warning as [Watch]: a [select] that misses a value the [builder]
/// reads shows a stale control, which is worse than the rebuild it saved.
class WhenChanged<T> extends StatefulWidget {
  const WhenChanged({
    required this.listenable,
    required this.select,
    required this.builder,
    super.key,
  });

  final Listenable listenable;

  /// Everything [builder] reads from [listenable], and nothing else.
  final T Function() select;

  final WidgetBuilder builder;

  @override
  State<WhenChanged<T>> createState() => _WhenChangedState<T>();
}

class _WhenChangedState<T> extends State<WhenChanged<T>> {
  late T _value = widget.select();

  @override
  void initState() {
    super.initState();
    widget.listenable.addListener(_changed);
  }

  @override
  void didUpdateWidget(WhenChanged<T> old) {
    super.didUpdateWidget(old);
    if (old.listenable != widget.listenable) {
      old.listenable.removeListener(_changed);
      widget.listenable.addListener(_changed);
    }
    // The parent rebuilt, so [WhenChanged.select] is a new closure and may be
    // reading something else entirely.
    _value = widget.select();
  }

  @override
  void dispose() {
    widget.listenable.removeListener(_changed);
    super.dispose();
  }

  void _changed() {
    final next = widget.select();
    if (next == _value) return;
    setState(() => _value = next);
  }

  @override
  Widget build(BuildContext context) => widget.builder(context);
}

class _WatchState<T> extends State<Watch<T>> {
  AppState? _state;
  T? _value;

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    // `read`, not `of`: depending on the scope is the thing being avoided.
    // Resolved here rather than in `initState` because an inherited widget is
    // not reachable that early, and re-resolved because this runs again if the
    // scope above is ever replaced.
    final next = AppStateScope.read(context);
    if (identical(next, _state)) return;
    _state?.removeListener(_changed);
    _state = next..addListener(_changed);
    _value = widget.select(next);
  }

  @override
  void didUpdateWidget(Watch<T> old) {
    super.didUpdateWidget(old);
    // A new [Watch.select] may be reading something else entirely.
    if (_state case final state?) _value = widget.select(state);
  }

  @override
  void dispose() {
    _state?.removeListener(_changed);
    super.dispose();
  }

  void _changed() {
    final state = _state;
    if (state == null) return;
    final next = widget.select(state);
    if (next == _value) return;
    // Not `mounted`-guarded by luck: the listener is removed in `dispose`.
    setState(() => _value = next);
  }

  @override
  Widget build(BuildContext context) => widget.builder(context, _state!);
}
