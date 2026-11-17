import 'dart:io' show Platform;

import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';

import '../l10n/app_localizations.dart';

/// What a bound button does.
enum ButtonAction {
  /// Transmit while held. What a handlebar remote is usually for.
  pushToTalk,

  /// Press once to start transmitting, again to stop. Useful for remotes that
  /// only send a momentary click and never a release.
  toggleTransmit,

  /// Mute and unmute the microphone.
  toggleMute,

  /// Deafen and undeafen.
  toggleDeafen,
}

extension ButtonActionLabel on ButtonAction {
  /// Takes the localisations rather than reading a global, because an enum has
  /// no context of its own and a label that quietly stays English is exactly
  /// what this went unnoticed as.
  String label(L l) => switch (this) {
    ButtonAction.pushToTalk => l.actionPushToTalkHold,
    ButtonAction.toggleTransmit => l.actionPushToTalkToggle,
    ButtonAction.toggleMute => l.actionToggleMute,
    ButtonAction.toggleDeafen => l.actionToggleDeafen,
  };
}

/// A single button binding.
class ButtonBinding {
  static const int modCtrl = 1;
  static const int modAlt = 2;
  static const int modShift = 4;
  static const int modMeta = 8;
  static const int _modMask = modCtrl | modAlt | modShift | modMeta;

  const ButtonBinding({
    required this.keyId,
    required this.action,
    this.label,
    this.modifiers = 0,
    this.global = false,
  });

  /// `LogicalKeyboardKey.keyId`, which survives serialisation unlike the key
  /// object itself.
  final int keyId;
  final ButtonAction action;

  /// Human-readable name captured when the binding was learned. Bluetooth
  /// remotes often report keys Flutter has no friendly name for, so whatever
  /// we can show is worth keeping.
  final String? label;

  /// Modifier bitmask ([modCtrl], [modAlt], [modShift], [modMeta]).
  final int modifiers;

  /// Whether this binding is also registered as a system-wide desktop hotkey.
  ///
  /// Defaults to `false` so bindings remain window-local unless the user
  /// explicitly opts in for a safe hotkey combination.
  final bool global;

  LogicalKeyboardKey get key => LogicalKeyboardKey(keyId);

  ButtonBinding copyWith({
    int? keyId,
    ButtonAction? action,
    String? label,
    int? modifiers,
    bool? global,
  }) => ButtonBinding(
    keyId: keyId ?? this.keyId,
    action: action ?? this.action,
    label: label ?? this.label,
    modifiers: modifiers ?? this.modifiers,
    global: global ?? this.global,
  );

  String get _baseDisplayName {
    if (label != null && label!.trim().isNotEmpty) return label!;
    if (key.keyLabel.trim().isNotEmpty) return key.keyLabel;
    final debug = key.debugName;
    if (debug != null && debug.isNotEmpty) return debug;
    return 'Key 0x${keyId.toRadixString(16)}';
  }

  static String _metaLabel() {
    if (!kIsWeb) {
      try {
        if (Platform.isMacOS) return 'Cmd';
        if (Platform.isWindows) return 'Win';
      } catch (_) {}
    }
    return 'Meta';
  }

  String get displayName {
    final mods = modifiers & _modMask;
    if (mods == 0) return _baseDisplayName;
    final parts = <String>[
      if ((mods & modCtrl) != 0) 'Ctrl',
      if ((mods & modAlt) != 0) 'Alt',
      if ((mods & modShift) != 0) 'Shift',
      if ((mods & modMeta) != 0) _metaLabel(),
      _baseDisplayName,
    ];
    return parts.join(' + ');
  }

  Map<String, dynamic> toJson() => {
    'keyId': keyId,
    'action': action.index,
    'label': label,
    'modifiers': modifiers,
    'global': global,
  };

  static ButtonBinding? fromJson(Map<String, dynamic> j) {
    final id = j['keyId'];
    final a = j['action'];
    if (id is! int || a is! int || a < 0 || a >= ButtonAction.values.length) {
      return null;
    }
    final rawMods = j['modifiers'];
    final modifiers = rawMods is int ? (rawMods & _modMask) : 0;
    final rawGlobal = j['global'];
    final global = rawGlobal is bool ? rawGlobal : false;
    final rawLabel = j['label'];
    return ButtonBinding(
      keyId: id,
      action: ButtonAction.values[a],
      label: rawLabel is String ? rawLabel : null,
      modifiers: modifiers,
      global: global,
    );
  }
}

/// Routes hardware and Bluetooth buttons to app actions.
///
/// A handlebar remote is the only practical way to key a microphone with
/// gloves on at speed. Most present as a Bluetooth HID keyboard or a media
/// controller, so they arrive as ordinary key events — which is why this
/// listens for *any* key rather than a fixed list. Whatever the remote sends,
/// the user can bind it.
///
/// Media-button events while the app is backgrounded arrive separately, from
/// the Android foreground service, and are injected through [handleMediaButton].
class ButtonController {
  ButtonController._();
  static final ButtonController instance = ButtonController._();

  static const _channel = MethodChannel('mumbleway/buttons');

  /// Window within which a duplicate edge on the other input path (global
  /// hotkey vs. focused [HardwareKeyboard]) is ignored so focused windows
  /// never double-toggle.
  static const _dedupWindow = Duration(milliseconds: 80);

  /// Overrides desktop platform detection for tests running on any host OS.
  @visibleForTesting
  static bool? globalHotkeysSupportedOverride;

  /// Overrides the wall clock for deterministic deduplication tests.
  @visibleForTesting
  static DateTime Function()? clockOverride;

  final List<ButtonBinding> _bindings = [];
  List<ButtonBinding> get bindings => List.unmodifiable(_bindings);

  /// Called with true on press and false on release.
  void Function(bool pressed)? onTransmit;
  void Function()? onToggleMute;
  void Function()? onToggleDeafen;

  /// Set while learning a new binding; receives the next key pressed.
  void Function(int keyId, String label, int modifiers)? _learner;

  /// A key whose press ended a learning round, so its release is ignored too.
  ///
  /// Learning finishes on the way down, which clears [_learner] — and then the
  /// way up finds nothing learning and dispatches normally. The rider has not
  /// pressed anything since; they are still letting go of the button they just
  /// bound, and that is not a command.
  int? _swallowRelease;

  bool _installed = false;
  bool _toggleState = false;
  int _syncGeneration = 0;

  /// Active PTT bindings `(keyId, modifiers)` so releasing either the main key
  /// or any required modifier stops transmission cleanly without double-firing.
  final Set<(int, int)> _activePtt = {};

  int? _lastHardwareKeyId;
  int? _lastHardwareModifiers;
  bool? _lastHardwarePressed;
  DateTime? _lastHardwareAt;

  int? _lastGlobalKeyId;
  int? _lastGlobalModifiers;
  bool? _lastGlobalPressed;
  DateTime? _lastGlobalAt;

  /// Whether a binding is currently being learned.
  bool get isLearning => _learner != null;

  static DateTime _now() => (clockOverride ?? DateTime.now)();

  static bool get supportsGlobalHotkeys {
    final override = globalHotkeysSupportedOverride;
    if (override != null) return override;
    if (kIsWeb) return false;
    try {
      return Platform.isWindows || Platform.isMacOS;
    } catch (_) {
      return false;
    }
  }

  static bool get _isDesktop {
    if (kIsWeb) return false;
    try {
      return Platform.isWindows || Platform.isMacOS || Platform.isLinux;
    } catch (_) {
      return false;
    }
  }

  static bool isSyntheticMediaKey(int keyId) =>
      keyId >= _mediaKeyBase && keyId < _flutterKeyPlaneStart;

  static bool _isModifierKeyId(int keyId) =>
      keyId == LogicalKeyboardKey.controlLeft.keyId ||
      keyId == LogicalKeyboardKey.controlRight.keyId ||
      keyId == LogicalKeyboardKey.control.keyId ||
      keyId == LogicalKeyboardKey.altLeft.keyId ||
      keyId == LogicalKeyboardKey.altRight.keyId ||
      keyId == LogicalKeyboardKey.alt.keyId ||
      keyId == LogicalKeyboardKey.shiftLeft.keyId ||
      keyId == LogicalKeyboardKey.shiftRight.keyId ||
      keyId == LogicalKeyboardKey.shift.keyId ||
      keyId == LogicalKeyboardKey.metaLeft.keyId ||
      keyId == LogicalKeyboardKey.metaRight.keyId ||
      keyId == LogicalKeyboardKey.meta.keyId;

  /// Whether `(keyId, modifiers)` is safe to register as a system-wide global
  /// hotkey without stealing ordinary typing or navigation keys.
  static bool isSafeGlobalKey({required int keyId, int modifiers = 0}) {
    if (_isModifierKeyId(keyId)) return false;
    if ((modifiers & ButtonBinding._modMask) != 0) return true;
    // Function keys F1..F24.
    if (keyId >= 0x00100000801 && keyId <= 0x00100000818) return true;
    // Synthetic remote/mobile media keys.
    if (isSyntheticMediaKey(keyId)) return true;
    // Non-typing lock/system keys: PrintScreen, Pause, ScrollLock.
    return keyId == 0x00100000608 ||
        keyId == 0x00100000509 ||
        keyId == 0x0010000010c;
  }

  static int _currentModifiers() {
    final pressed = HardwareKeyboard.instance.logicalKeysPressed;
    var mods = 0;
    if (pressed.contains(LogicalKeyboardKey.controlLeft) ||
        pressed.contains(LogicalKeyboardKey.controlRight) ||
        pressed.contains(LogicalKeyboardKey.control)) {
      mods |= ButtonBinding.modCtrl;
    }
    if (pressed.contains(LogicalKeyboardKey.altLeft) ||
        pressed.contains(LogicalKeyboardKey.altRight) ||
        pressed.contains(LogicalKeyboardKey.alt)) {
      mods |= ButtonBinding.modAlt;
    }
    if (pressed.contains(LogicalKeyboardKey.shiftLeft) ||
        pressed.contains(LogicalKeyboardKey.shiftRight) ||
        pressed.contains(LogicalKeyboardKey.shift)) {
      mods |= ButtonBinding.modShift;
    }
    if (pressed.contains(LogicalKeyboardKey.metaLeft) ||
        pressed.contains(LogicalKeyboardKey.metaRight) ||
        pressed.contains(LogicalKeyboardKey.meta)) {
      mods |= ButtonBinding.modMeta;
    }
    return mods;
  }

  /// Clears recent-event deduplication state, for tests.
  @visibleForTesting
  void resetDeduplication() {
    _lastHardwareKeyId = null;
    _lastHardwareModifiers = null;
    _lastHardwarePressed = null;
    _lastHardwareAt = null;
    _lastGlobalKeyId = null;
    _lastGlobalModifiers = null;
    _lastGlobalPressed = null;
    _lastGlobalAt = null;
  }

  void _pruneActivePtt(bool Function((int, int)) test) {
    if (_activePtt.isEmpty) return;
    _activePtt.removeWhere(test);
    if (_activePtt.isEmpty) {
      onTransmit?.call(false);
    }
  }

  void _releaseMissingPtt() {
    _pruneActivePtt(
      (active) => !_bindings.any(
        (b) =>
            b.keyId == active.$1 &&
            b.modifiers == active.$2 &&
            b.action == ButtonAction.pushToTalk,
      ),
    );
  }

  void setBindings(List<ButtonBinding> bindings) {
    _bindings
      ..clear()
      ..addAll(bindings);
    _releaseMissingPtt();
    _syncCapture();
  }

  void addBinding(ButtonBinding b) {
    // One action per (keyId, modifiers): rebinding a shortcut replaces whatever
    // it did before, rather than firing two things at once.
    _bindings.removeWhere(
      (e) => e.keyId == b.keyId && e.modifiers == b.modifiers,
    );
    _bindings.add(b);
    _releaseMissingPtt();
    _syncCapture();
  }

  void removeBinding(int keyId, {int modifiers = 0}) {
    _bindings.removeWhere(
      (e) => e.keyId == keyId && e.modifiers == modifiers,
    );
    _releaseMissingPtt();
    _syncCapture();
  }

  /// Enables or disables global hotkey registration for `(keyId, modifiers)`.
  bool setBindingGlobal(int keyId, int modifiers, bool global) {
    if (global && !isSafeGlobalKey(keyId: keyId, modifiers: modifiers)) {
      return false;
    }
    final idx = _bindings.indexWhere(
      (e) => e.keyId == keyId && e.modifiers == modifiers,
    );
    if (idx < 0) return false;
    if (_bindings[idx].global == global) return false;
    _bindings[idx] = _bindings[idx].copyWith(global: global);
    _syncCapture();
    return true;
  }

  /// Starts listening. Safe to call more than once.
  void install() {
    if (_installed) return;
    _installed = true;
    HardwareKeyboard.instance.addHandler(_handleKeyEvent);
    _channel.setMethodCallHandler((call) async {
      if (call.method == 'mediaButton') {
        final args = (call.arguments as Map).cast<String, dynamic>();
        handleMediaButton(
          args['keyCode'] as int? ?? 0,
          args['pressed'] as bool? ?? false,
        );
      } else if (call.method == 'globalHotkey') {
        final args = call.arguments;
        if (args is Map) {
          final keyId = args['keyId'];
          final pressed = args['pressed'];
          final rawMods = args['modifiers'];
          if (keyId is int && pressed is bool) {
            handleGlobalHotkey(
              keyId,
              pressed,
              modifiers: rawMods is int ? (rawMods & ButtonBinding._modMask) : 0,
            );
          }
        }
      }
      return null;
    });
  }

  void dispose() {
    if (!_installed) return;
    HardwareKeyboard.instance.removeHandler(_handleKeyEvent);
    _channel.setMethodCallHandler(null);
    _activePtt.clear();
    resetDeduplication();
    _installed = false;
  }

  /// Captures the next key press as a new binding.
  void learnNext(void Function(int keyId, String label) onLearned) {
    learnNextWithModifiers((keyId, label, _) => onLearned(keyId, label));
  }

  /// Captures the next key press along with active desktop modifier keys.
  void learnNextWithModifiers(
    void Function(int keyId, String label, int modifiers) onLearned,
  ) {
    _learner = onLearned;
    _syncCapture();
  }

  void cancelLearning() {
    _learner = null;
    _syncCapture();
  }

  /// Tells the platform whether it should claim the remote's media buttons.
  ///
  /// Only iOS acts on this, and only because it has to: there, play and
  /// track-skip never reach an app as key events, and the only way to see them
  /// is to become the Now Playing app — which stops the rider's music app
  /// answering its own remote. That is the right trade while a button is bound
  /// to push-to-talk and plain rude otherwise, so it is asked for only while a
  /// media binding exists or one is being learned.
  ///
  /// Android reports the same buttons through its media session without any of
  /// this, and has no handler for the call.
  void _syncCapture() {
    _syncGeneration++;
    final want =
        _learner != null || _bindings.any((b) => b.keyId >= _mediaKeyBase);
    _channel
        .invokeMethod<String>('captureMediaButtons', want)
        .then((reply) {
          captureState = reply;
          onCaptureChanged?.call();
        })
        .catchError((Object _) {
          // No handler here, which is every platform but iOS. Android reports
          // the same buttons through its media session without being asked.
          captureState = null;
          onCaptureChanged?.call();
          return null;
        });

    if (supportsGlobalHotkeys) {
      final seen = <(int, int)>{};
      // Pause desktop hotkey interception while learning so a previously bound
      // key reaches HardwareKeyboard cleanly instead of firing WM_HOTKEY.
      final hotkeys = _learner != null
          ? const <Map<String, int>>[]
          : <Map<String, int>>[
              for (final b in _bindings)
                if (b.global &&
                    !isSyntheticMediaKey(b.keyId) &&
                    isSafeGlobalKey(keyId: b.keyId, modifiers: b.modifiers) &&
                    seen.add((b.keyId, b.modifiers)))
                  {'keyId': b.keyId, 'modifiers': b.modifiers},
            ];
      _syncGlobalHotkeys(hotkeys);
    }
  }

  Future<void> _syncGlobalHotkeys(List<Map<String, int>> hotkeys) async {
    try {
      await _channel.invokeMethod<void>('setGlobalHotkeys', {
        'hotkeys': hotkeys,
      });
    } catch (_) {
      // Unsupported platforms or test environments without a platform runner.
    }
  }

  /// What the platform said when last asked to listen for remote buttons.
  ///
  /// Null where the question does not apply. Shown in settings because a
  /// platform that is not listening and a remote that is not sending look
  /// identical from the outside — which is exactly how the last round went.
  String? captureState;

  /// Called when [captureState] changes, so the screen showing it can rebuild.
  VoidCallback? onCaptureChanged;

  static const _mediaKeyBase = 0x7000_0000;
  static const _flutterKeyPlaneStart = 0x0100000000;

  bool _handleKeyEvent(KeyEvent event) {
    final id = event.logicalKey.keyId;

    // Recorded whether or not it is bound, and whether or not anything is
    // being learned. A remote whose buttons are ordinary keys and a remote
    // that reaches the app not at all look the same from the outside, and
    // telling them apart decides where to look next.
    if (event is KeyDownEvent) {
      lastKey = _describe(event.logicalKey);
      onCaptureChanged?.call();
    }

    if (_learner != null) {
      if (event is KeyDownEvent) {
        if (_isModifierKeyId(id)) {
          return true;
        }
        final learner = _learner!;
        final gen = _syncGeneration;
        final modifiers =
            (supportsGlobalHotkeys || _isDesktop) ? _currentModifiers() : 0;
        _learner = null;
        _swallowRelease = id;
        learner(id, _describe(event.logicalKey), modifiers);
        if (_learner == null && _syncGeneration == gen) {
          _syncCapture();
        }
        // Swallow it: the key that is being bound should not also trigger
        // whatever it happens to be bound to already.
        return true;
      }
      return true;
    }

    if (event is KeyUpEvent && _swallowRelease == id) {
      _swallowRelease = null;
      return true;
    }

    if (event is KeyRepeatEvent && _swallowRelease == id) {
      return true;
    }

    final currentModifiers = _currentModifiers();

    if (event is KeyRepeatEvent) {
      return _dispatch(id, false, modifiers: currentModifiers, isRepeat: true) ||
          _hasActivePttForKey(id);
    }

    if (event is! KeyDownEvent && event is! KeyUpEvent) {
      return false;
    }

    final pressed = event is KeyDownEvent;

    if (!pressed && _isModifierKeyId(id)) {
      _releaseActivePttWithMissingModifiers(currentModifiers);
      return false;
    }

    if (_isModifierKeyId(id)) {
      return false;
    }

    final now = _now();
    if (_isRecentDuplicate(
      id,
      currentModifiers,
      pressed,
      _lastGlobalKeyId,
      _lastGlobalModifiers,
      _lastGlobalPressed,
      _lastGlobalAt,
      now,
    )) {
      return _bindingFor(id, currentModifiers) != null ||
          (!pressed && _hasActivePttForKey(id));
    }

    _lastHardwareKeyId = id;
    _lastHardwareModifiers = currentModifiers;
    _lastHardwarePressed = pressed;
    _lastHardwareAt = now;

    if (!pressed) {
      return _dispatchRelease(id, currentModifiers);
    }

    return _dispatch(id, true, modifiers: currentModifiers);
  }

  /// Handles a desktop global hotkey forwarded from the platform runner.
  void handleGlobalHotkey(int keyId, bool pressed, {int modifiers = 0}) {
    if (_learner != null) return;
    final maskedMods = modifiers & ButtonBinding._modMask;

    final now = _now();
    if (_isRecentDuplicate(
      keyId,
      maskedMods,
      pressed,
      _lastHardwareKeyId,
      _lastHardwareModifiers,
      _lastHardwarePressed,
      _lastHardwareAt,
      now,
    )) {
      return;
    }

    if (_dispatch(keyId, pressed, modifiers: maskedMods)) {
      _lastGlobalKeyId = keyId;
      _lastGlobalModifiers = maskedMods;
      _lastGlobalPressed = pressed;
      _lastGlobalAt = now;
    }
  }

  static bool _isRecentDuplicate(
    int keyId,
    int modifiers,
    bool pressed,
    int? lastId,
    int? lastModifiers,
    bool? lastPressed,
    DateTime? lastAt,
    DateTime now,
  ) {
    if (lastId != keyId ||
        lastModifiers != modifiers ||
        lastPressed != pressed ||
        lastAt == null) {
      return false;
    }
    return now.difference(lastAt) <= _dedupWindow;
  }

  /// Handles a media button forwarded from the platform.
  ///
  /// Android key codes, not Flutter key ids, so they are mapped onto the same
  /// binding space by offset — see [mediaKeyId].
  void handleMediaButton(int androidKeyCode, bool pressed) {
    final id = mediaKeyId(androidKeyCode);

    // Recorded whether or not anything is bound to it. A remote that sends
    // nothing and a platform that hears nothing look identical otherwise, and
    // that ambiguity has already cost a round of builds.
    if (pressed) {
      lastMediaKey = describeMediaKey(id);
      onCaptureChanged?.call();
    }

    // Learning ends here too.
    //
    // It used to end only on the keyboard path, and a remote that sends media
    // buttons never reaches that one — which is every handlebar remote worth
    // having, and the only kind iOS forwards at all. So the press was seen,
    // named, and shown in the diagnostics panel, while the screen went on
    // waiting for a button that had already been pressed. It looked like the
    // remote was not reaching the app, which is the one thing the panel was
    // added to rule out.
    if (_learner != null) {
      if (pressed) {
        final learner = _learner!;
        _learner = null;
        _swallowRelease = id;
        learner(id, describeMediaKey(id), 0);
      }
      // Swallowed either way: the button being bound must not also fire
      // whatever it is bound to already.
      return;
    }

    if (!pressed && _swallowRelease == id) {
      _swallowRelease = null;
      return;
    }

    _dispatch(id, pressed);
  }

  /// The most recent remote button the platform passed up, for diagnosis.
  String? lastMediaKey;

  /// The most recent ordinary key press seen, for the same reason.
  String? lastKey;

  /// Maps an Android media key code into a key id that cannot collide with a
  /// real `LogicalKeyboardKey`.
  ///
  /// Flutter's ids are allocated in documented planes; this uses a private
  /// range well above them so a media button and a keyboard key are never
  /// confused for one another.
  static int mediaKeyId(int androidKeyCode) => _mediaKeyBase + androidKeyCode;

  ButtonBinding? _bindingFor(int keyId, int modifiers) {
    final masked = modifiers & ButtonBinding._modMask;
    for (final b in _bindings) {
      if (b.keyId == keyId && b.modifiers == masked) return b;
    }
    return null;
  }

  bool _hasActivePttForKey(int keyId) =>
      _activePtt.any((entry) => entry.$1 == keyId);

  void _releaseActivePttWithMissingModifiers(int currentModifiers) {
    _pruneActivePtt((entry) => (entry.$2 & currentModifiers) != entry.$2);
  }

  bool _dispatchRelease(int keyId, int currentModifiers) {
    final beforeCount = _activePtt.length;
    _pruneActivePtt((entry) => entry.$1 == keyId);
    return _activePtt.length != beforeCount ||
        _bindingFor(keyId, currentModifiers) != null;
  }

  bool _dispatch(
    int keyId,
    bool pressed, {
    int modifiers = 0,
    bool isRepeat = false,
  }) {
    final binding = _bindingFor(keyId, modifiers);
    if (binding == null) return false;

    // Auto-repeat while a key is held would toggle continuously.
    if (isRepeat) return true;

    switch (binding.action) {
      case ButtonAction.pushToTalk:
        if (pressed) {
          _activePtt.add((binding.keyId, binding.modifiers));
          onTransmit?.call(true);
        } else {
          final removed = _activePtt.remove((binding.keyId, binding.modifiers));
          if (removed && _activePtt.isEmpty) {
            onTransmit?.call(false);
          }
        }
      case ButtonAction.toggleTransmit:
        if (pressed) {
          _toggleState = !_toggleState;
          onTransmit?.call(_toggleState);
        }
      case ButtonAction.toggleMute:
        if (pressed) onToggleMute?.call();
      case ButtonAction.toggleDeafen:
        if (pressed) onToggleDeafen?.call();
    }
    // Consumed, so a bound media key does not also skip a track in whatever
    // music app is playing.
    return true;
  }

  /// Best-effort human-readable name for a key.
  static String _describe(LogicalKeyboardKey key) {
    if (key.keyLabel.trim().isNotEmpty) return key.keyLabel;
    if (key.debugName != null && key.debugName!.isNotEmpty) {
      return key.debugName!;
    }
    return 'Key 0x${key.keyId.toRadixString(16)}';
  }

  /// Friendly name for a media key id produced by [mediaKeyId].
  @visibleForTesting
  static String describeMediaKey(int keyId) {
    const names = {
      79: 'Headset hook',
      85: 'Play / pause',
      86: 'Stop',
      87: 'Next track',
      88: 'Previous track',
      89: 'Rewind',
      90: 'Fast forward',
      126: 'Play',
      127: 'Pause',
      24: 'Volume up',
      25: 'Volume down',
    };
    final code = keyId - 0x7000_0000;
    return names[code] ?? 'Media button $code';
  }
}
