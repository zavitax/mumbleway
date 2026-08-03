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
  const ButtonBinding({required this.keyId, required this.action, this.label});

  /// `LogicalKeyboardKey.keyId`, which survives serialisation unlike the key
  /// object itself.
  final int keyId;
  final ButtonAction action;

  /// Human-readable name captured when the binding was learned. Bluetooth
  /// remotes often report keys Flutter has no friendly name for, so whatever
  /// we can show is worth keeping.
  final String? label;

  LogicalKeyboardKey get key => LogicalKeyboardKey(keyId);

  String get displayName {
    if (label != null && label!.isNotEmpty) return label!;
    if (key.keyLabel.isNotEmpty) return key.keyLabel;
    return 'Key 0x${keyId.toRadixString(16)}';
  }

  Map<String, dynamic> toJson() => {
    'keyId': keyId,
    'action': action.index,
    'label': label,
  };

  static ButtonBinding? fromJson(Map<String, dynamic> j) {
    final id = j['keyId'];
    final a = j['action'];
    if (id is! int || a is! int || a < 0 || a >= ButtonAction.values.length) {
      return null;
    }
    return ButtonBinding(
      keyId: id,
      action: ButtonAction.values[a],
      label: j['label'] as String?,
    );
  }
}

/// Routes hardware and Bluetooth buttons to app actions.
///
/// A handlebar remote is the only practical way to key a microphone with
/// gloves on at speed. Most present as a Bluetooth HID keyboard or a media
/// controller, so they arrive as ordinary key events вЂ” which is why this
/// listens for *any* key rather than a fixed list. Whatever the remote sends,
/// the user can bind it.
///
/// Media-button events while the app is backgrounded arrive separately, from
/// the Android foreground service, and are injected through [handleMediaButton].
class ButtonController {
  ButtonController._();
  static final ButtonController instance = ButtonController._();

  static const _channel = MethodChannel('mumbleway/buttons');

  final List<ButtonBinding> _bindings = [];
  List<ButtonBinding> get bindings => List.unmodifiable(_bindings);

  /// Called with true on press and false on release.
  void Function(bool pressed)? onTransmit;
  void Function()? onToggleMute;
  void Function()? onToggleDeafen;

  /// Set while learning a new binding; receives the next key pressed.
  void Function(int keyId, String label)? _learner;

  bool _installed = false;
  bool _toggleState = false;

  /// Whether a binding is currently being learned.
  bool get isLearning => _learner != null;

  void setBindings(List<ButtonBinding> bindings) {
    _bindings
      ..clear()
      ..addAll(bindings);
  }

  void addBinding(ButtonBinding b) {
    // One action per key: rebinding a key replaces whatever it did before,
    // rather than firing two things at once.
    _bindings.removeWhere((e) => e.keyId == b.keyId);
    _bindings.add(b);
  }

  void removeBinding(int keyId) =>
      _bindings.removeWhere((e) => e.keyId == keyId);

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
      }
      return null;
    });
  }

  void dispose() {
    if (!_installed) return;
    HardwareKeyboard.instance.removeHandler(_handleKeyEvent);
    _installed = false;
  }

  /// Captures the next key press as a new binding.
  void learnNext(void Function(int keyId, String label) onLearned) {
    _learner = onLearned;
  }

  void cancelLearning() => _learner = null;

  bool _handleKeyEvent(KeyEvent event) {
    final id = event.logicalKey.keyId;

    if (_learner != null) {
      if (event is KeyDownEvent) {
        final learner = _learner!;
        _learner = null;
        learner(id, _describe(event.logicalKey));
        // Swallow it: the key that is being bound should not also trigger
        // whatever it happens to be bound to already.
        return true;
      }
      return true;
    }

    return _dispatch(
      id,
      event is KeyDownEvent,
      isRepeat: event is KeyRepeatEvent,
    );
  }

  /// Handles a media button forwarded from the platform.
  ///
  /// Android key codes, not Flutter key ids, so they are mapped onto the same
  /// binding space by offset вЂ” see [mediaKeyId].
  void handleMediaButton(int androidKeyCode, bool pressed) {
    _dispatch(mediaKeyId(androidKeyCode), pressed);
  }

  /// Maps an Android media key code into a key id that cannot collide with a
  /// real `LogicalKeyboardKey`.
  ///
  /// Flutter's ids are allocated in documented planes; this uses a private
  /// range well above them so a media button and a keyboard key are never
  /// confused for one another.
  static int mediaKeyId(int androidKeyCode) => 0x7000_0000 + androidKeyCode;

  bool _dispatch(int keyId, bool pressed, {bool isRepeat = false}) {
    ButtonBinding? binding;
    for (final b in _bindings) {
      if (b.keyId == keyId) {
        binding = b;
        break;
      }
    }
    if (binding == null) return false;

    // Auto-repeat while a key is held would toggle continuously.
    if (isRepeat) return true;

    switch (binding.action) {
      case ButtonAction.pushToTalk:
        onTransmit?.call(pressed);
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
    if (key.keyLabel.isNotEmpty) return key.keyLabel;
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
      126: 'Play',
      127: 'Pause',
      24: 'Volume up',
      25: 'Volume down',
    };
    final code = keyId - 0x7000_0000;
    return names[code] ?? 'Media button $code';
  }
}
