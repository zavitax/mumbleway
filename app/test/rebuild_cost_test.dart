import 'package:flutter/material.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mumbleway/l10n/app_localizations.dart';
import 'package:mumbleway/screens/home_screen.dart';
import 'package:mumbleway/screens/settings_screen.dart';
import 'package:mumbleway/state/app_state.dart';

// How much of each screen is rebuilt when the state object says something
// changed, and what that costs.
//
// **`AppStateScope` is an `InheritedNotifier`, so `of(context)` subscribes.**
// Every widget that reads it rebuilds on every `notifyListeners()`, of which
// there are dozens of callers — and one of them is a two-second poll that runs
// for the whole of every ride. That is not free and it is not visible in the
// code: a widget opts into the cost by reading a value, and nothing marks the
// difference between reading one that changes and one that does not.
//
// **Nothing here asserts.** These are printed figures, not thresholds: the
// timings are CPU-only — a widget test has no raster thread — so the absolute
// numbers mean nothing and only the comparison between two runs means anything.
// A bound would either be loose enough to pass through a real regression or
// tight enough to fail on a slow machine.
//
// Where it stood when this was last run, per notification. The widget counts
// are exact and repeatable; the times vary by half again between runs on the
// same machine, so read them as "about a third of what it was" rather than as
// figures:
//
// | Screen   | Widgets rebuilt | Before  | After   |
// |----------|-----------------|---------|---------|
// | home     | 395 -> 224      | ~16 ms  | ~9.5 ms |
// | settings | 875 -> 109      | ~13 ms  | ~3 ms   |
//
// What still rebuilds on each is what genuinely shows live audio: the talk
// panel and mic notice on home, the device list and level meter on settings.

/// Flutter's own widgets, by the names they print. Not exhaustive and does not
/// need to be — it only has to leave this app's widgets legible in the census.
final _framework = RegExp(
  r'^_?(Animated|Default|Raw|Focus|Actions?|Semantics|Gesture|Ink|Material|'
  r'Text|Icon|Padding|Align|Center|Column|Row|Stack|Sized|Constrained|Container|'
  r'Listener|Mouse|Tap|Baseline|Flex|Expanded|Safe|Media|Directionality|'
  r'Builder|Selection|Shortcuts|Theme|Tooltip|Visibility|Opacity|Positioned|'
  r'ListTile|RadioListTile|Radio|Switch|Slider|Checkbox|Divider|Scroll|'
  r'Repaint|Notification|Inherited|Merge|Exclude|Block|KeyedSubtree|'
  r'ValueListenable|ListenableBuilder|Table|Wrap|Transform|ClipR|Decorated)',
);

void main() {
  Widget host(AppState state, Widget screen) => AppStateScope(
    state: state,
    child: MaterialApp(
      localizationsDelegates: const [
        ...L.localizationsDelegates,
        GlobalMaterialLocalizations.delegate,
        GlobalWidgetsLocalizations.delegate,
        GlobalCupertinoLocalizations.delegate,
      ],
      supportedLocales: L.supportedLocales,
      home: screen,
    ),
  );

  /// How long a screen takes to produce frames, with and without the state
  /// object saying something changed.
  ///
  /// **Counting dirty elements was tried first and measures nothing.** An
  /// `InheritedNotifier` marks only itself when it fires; its dependents are
  /// told during the rebuild that follows, so a count taken before the next
  /// frame reports one widget however many are subscribed.
  ///
  /// Time is what the question was really about. It is CPU-only here — there
  /// is no raster thread in a widget test — so the absolute figures mean
  /// nothing and the comparison between two of them means quite a lot.
  Future<(int, int, int)> frameCost(WidgetTester t, Widget screen) async {
    final state = AppState();
    addTearDown(state.dispose);
    // **Without this every screen here is a spinner.** `HomeScreen.build`
    // returns a `CircularProgressIndicator` and nothing else until `ready`, so
    // the first figures this harness produced were 186 widgets of loading
    // indicator, reported as "home is fine" — which they had not measured.
    state.markReadyForTesting();
    t.view.physicalSize = const Size(430, 950);
    t.view.devicePixelRatio = 1.0;
    addTearDown(t.view.resetPhysicalSize);
    addTearDown(t.view.resetDevicePixelRatio);

    await t.pumpWidget(host(state, screen));
    // Frames rather than `pumpAndSettle`: both screens animate for as long as
    // they are on show, so settling never happens and waiting for it times the
    // test out.
    for (var i = 0; i < 5; i++) {
      await t.pump(const Duration(milliseconds: 16));
    }

    const frames = 60;
    final idle = Stopwatch()..start();
    for (var i = 0; i < frames; i++) {
      await t.pump(const Duration(milliseconds: 16));
    }
    idle.stop();

    final busy = Stopwatch()..start();
    for (var i = 0; i < frames; i++) {
      state.notifyListeners();
      await t.pump(const Duration(milliseconds: 16));
    }
    busy.stop();

    return (
      t.allElements.length,
      idle.elapsedMilliseconds,
      busy.elapsedMilliseconds,
    );
  }

  /// Which widgets rebuild when the state object says something changed, and
  /// how many of each.
  ///
  /// `debugPrintRebuildDirtyWidgets` names every widget the framework rebuilds,
  /// which is the census the timing above cannot give: a screen that costs
  /// 10 ms a notification says nothing about *which* of its widgets are
  /// responsible, and the answer was three times somewhere other than where it
  /// looked — a lifecycle method holding a subscription nobody could see from
  /// `build`, a set of radio groups rebuilding for values they do not show, and
  /// a panel rebuilt not because it subscribed but because its parent handed it
  /// a fresh callback.
  Future<Map<String, int>> rebuildCensus(WidgetTester t, Widget screen) async {
    final state = AppState();
    addTearDown(state.dispose);
    // **Without this every screen here is a spinner.** `HomeScreen.build`
    // returns a `CircularProgressIndicator` and nothing else until `ready`, so
    // the first figures this harness produced were 186 widgets of loading
    // indicator, reported as "home is fine" — which they had not measured.
    state.markReadyForTesting();
    await t.pumpWidget(host(state, screen));
    for (var i = 0; i < 5; i++) {
      await t.pump(const Duration(milliseconds: 16));
    }

    final counts = <String, int>{};
    final previous = debugPrint;
    debugPrint = (String? message, {int? wrapWidth}) {
      if (message == null) return;
      // The framework's own format: "Rebuilt Foo" / "Rebuilt Foo dirty".
      final name = message.split(' ').skip(1).firstOrNull;
      if (name != null) counts[name] = (counts[name] ?? 0) + 1;
    };
    debugPrintRebuildDirtyWidgets = true;
    state.notifyListeners();
    await t.pump(const Duration(milliseconds: 16));
    debugPrintRebuildDirtyWidgets = false;
    debugPrint = previous;
    return counts;
  }

  testWidgets('what one notification rebuilds', (t) async {
    t.view.physicalSize = const Size(430, 950);
    t.view.devicePixelRatio = 1.0;
    addTearDown(t.view.resetPhysicalSize);
    addTearDown(t.view.resetDevicePixelRatio);

    for (final (name, screen) in <(String, Widget)>[
      ('home', const HomeScreen()),
      ('settings', const SettingsScreen()),
    ]) {
      final counts = await rebuildCensus(t, screen);
      final total = counts.values.fold(0, (a, b) => a + b);
      // This app's own widgets, not the framework's. A `RadioListTile` brings
      // twenty widgets of gesture, focus and text machinery with it, so a
      // census sorted by count is a list of Flutter internals and says nothing
      // about which part of *this* screen asked for them.
      final mine =
          counts.entries
              .where((e) => RegExp(r'^_?[A-Z]').hasMatch(e.key))
              .where((e) => !_framework.hasMatch(e.key))
              .toList()
            ..sort((a, b) => b.value.compareTo(a.value));
      // ignore: avoid_print
      print(
        'REBUILT $name: $total widgets — '
        '${mine.map((e) => '${e.key}×${e.value}').join(', ')}',
      );
    }
  });

  /// What having the diagnostics panel open costs, per frame.
  ///
  /// The panel is **always mounted** — `home_screen.dart` only slides it
  /// off-screen — so this is not open-versus-absent but open-versus-idle: its
  /// one-second sampler runs either way, and only the drawing is supposed to
  /// stop when it is closed.
  ///
  /// The fake clock advances with `pump`, so 60 frames of 16 ms run the real
  /// timers: one full refresh at 1 Hz and about nineteen scroll steps at 20 Hz.
  /// That is the steady state a rider actually has on screen.
  testWidgets('what the diagnostics panel costs while it is open', (t) async {
    t.view.physicalSize = const Size(430, 950);
    t.view.devicePixelRatio = 1.0;
    addTearDown(t.view.resetPhysicalSize);
    addTearDown(t.view.resetDevicePixelRatio);

    Future<(int, int)> run({required bool open}) async {
      final state = AppState();
      addTearDown(state.dispose);
      // **Without this every screen here is a spinner.** `HomeScreen.build`
      // returns a `CircularProgressIndicator` and nothing else until `ready`, so
      // the first figures this harness produced were 186 widgets of loading
      // indicator, reported as "home is fine" — which they had not measured.
      state.markReadyForTesting();
      if (open) state.toggleDiagnostics();
      await t.pumpWidget(host(state, const HomeScreen()));
      for (var i = 0; i < 5; i++) {
        await t.pump(const Duration(milliseconds: 16));
      }
      final w = t.allElements.length;
      final clock = Stopwatch()..start();
      for (var i = 0; i < 60; i++) {
        await t.pump(const Duration(milliseconds: 16));
      }
      clock.stop();
      return (w, clock.elapsedMilliseconds);
    }

    final (closedWidgets, closed) = await run(open: false);
    final (openWidgets, opened) = await run(open: true);
    // ignore: avoid_print
    print(
      'PANEL closed: $closedWidgets widgets, 60 frames ${closed}ms — '
      'open: $openWidgets widgets, 60 frames ${opened}ms',
    );
  });

  testWidgets('what a frame costs on each screen', (t) async {
    for (final (name, screen) in <(String, Widget)>[
      ('home', const HomeScreen()),
      ('settings', const SettingsScreen()),
    ]) {
      final (widgets, idle, busy) = await frameCost(t, screen);
      // ignore: avoid_print
      print(
        'COST $name: $widgets widgets, 60 idle frames ${idle}ms, '
        '60 notified frames ${busy}ms',
      );
    }
  });
}
