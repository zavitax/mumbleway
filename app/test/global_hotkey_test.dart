import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mumbleway/l10n/app_localizations.dart';
import 'package:mumbleway/screens/settings_screen.dart';
import 'package:mumbleway/services/button_controller.dart';
import 'package:mumbleway/state/app_state.dart';
import 'package:shared_preferences/shared_preferences.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  const channel = MethodChannel('mumbleway/buttons');
  final messenger =
      TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger;
  final buttons = ButtonController.instance;

  late List<MethodCall> calls;
  late DateTime now;

  List<MethodCall> hotkeySyncCalls() =>
      calls.where((c) => c.method == 'setGlobalHotkeys').toList();

  Future<void> flush() => Future<void>.delayed(Duration.zero);

  Future<void> sendGlobalHotkey(
    int keyId,
    bool pressed, {
    int? modifiers,
  }) async {
    final payload = <String, dynamic>{
      'keyId': keyId,
      'pressed': pressed,
      'modifiers': ?modifiers,
    };
    final data = const StandardMethodCodec().encodeMethodCall(
      MethodCall('globalHotkey', payload),
    );
    await messenger.handlePlatformMessage('mumbleway/buttons', data, (_) {});
  }

  setUp(() async {
    SharedPreferences.setMockInitialValues({});
    ButtonController.globalHotkeysSupportedOverride = true;
    now = DateTime(2026, 1, 1, 12, 0, 0);
    ButtonController.clockOverride = () => now;
    calls = [];
    messenger.setMockMethodCallHandler(channel, (call) async {
      calls.add(call);
      return null;
    });

    HardwareKeyboard.instance.clearState();
    buttons.dispose();
    buttons.cancelLearning();
    buttons.setBindings(const []);
    await flush();
    calls.clear();

    buttons.onTransmit = null;
    buttons.onToggleMute = null;
    buttons.onToggleDeafen = null;
    buttons.install();
  });

  tearDown(() async {
    buttons.dispose();
    buttons.cancelLearning();
    buttons.setBindings(const []);
    await flush();
    buttons.onTransmit = null;
    buttons.onToggleMute = null;
    buttons.onToggleDeafen = null;
    ButtonController.globalHotkeysSupportedOverride = null;
    ButtonController.clockOverride = null;
    HardwareKeyboard.instance.clearState();
    messenger.setMockMethodCallHandler(channel, null);
  });

  group('ButtonBinding model & serialization', () {
    test('defaults to modifiers=0 and global=false', () {
      final b = ButtonBinding(
        keyId: LogicalKeyboardKey.f9.keyId,
        action: ButtonAction.toggleMute,
      );
      expect(b.modifiers, 0);
      expect(b.global, isFalse);
      expect(b.displayName, 'F9');
    });

    test('backward-compatible fromJson defaults modifiers=0 and global=false', () {
      final legacy = ButtonBinding.fromJson({
        'keyId': LogicalKeyboardKey.f8.keyId,
        'action': ButtonAction.toggleDeafen.index,
        'label': 'F8',
      });
      expect(legacy, isNotNull);
      expect(legacy!.modifiers, 0);
      expect(legacy.global, isFalse);
      expect(legacy.displayName, 'F8');

      final modern = ButtonBinding(
        keyId: LogicalKeyboardKey.keyM.keyId,
        action: ButtonAction.toggleMute,
        label: 'M',
        modifiers: ButtonBinding.modCtrl | ButtonBinding.modShift,
        global: true,
      );
      final roundTrip = ButtonBinding.fromJson(modern.toJson())!;
      expect(roundTrip.keyId, modern.keyId);
      expect(roundTrip.action, modern.action);
      expect(roundTrip.label, 'M');
      expect(
        roundTrip.modifiers,
        ButtonBinding.modCtrl | ButtonBinding.modShift,
      );
      expect(roundTrip.global, isTrue);
    });

    test('displayName prefixes modifiers and copyWith updates fields', () {
      final ctrlM = ButtonBinding(
        keyId: LogicalKeyboardKey.keyM.keyId,
        action: ButtonAction.toggleMute,
        label: 'M',
        modifiers: ButtonBinding.modCtrl,
      );
      expect(ctrlM.displayName, 'Ctrl + M');

      final altShiftF9 = ButtonBinding(
        keyId: LogicalKeyboardKey.f9.keyId,
        action: ButtonAction.toggleDeafen,
        modifiers: ButtonBinding.modAlt | ButtonBinding.modShift,
      );
      expect(altShiftF9.displayName, 'Alt + Shift + F9');

      final ctrlSpace = ButtonBinding(
        keyId: LogicalKeyboardKey.space.keyId,
        action: ButtonAction.pushToTalk,
        modifiers: ButtonBinding.modCtrl,
      );
      expect(ctrlSpace.displayName, 'Ctrl + Space');

      final optedIn = ctrlM.copyWith(global: true);
      expect(optedIn.global, isTrue);
      expect(optedIn.modifiers, ButtonBinding.modCtrl);
      expect(optedIn.keyId, ctrlM.keyId);
    });
  });

  group('isSafeGlobalKey validation', () {
    test('rejects bare typing, navigation, and pure modifier keys', () {
      final unsafeBare = [
        LogicalKeyboardKey.space,
        LogicalKeyboardKey.keyA,
        LogicalKeyboardKey.keyM,
        LogicalKeyboardKey.digit1,
        LogicalKeyboardKey.semicolon,
        LogicalKeyboardKey.enter,
        LogicalKeyboardKey.tab,
        LogicalKeyboardKey.backspace,
        LogicalKeyboardKey.escape,
        LogicalKeyboardKey.arrowUp,
        LogicalKeyboardKey.arrowDown,
        LogicalKeyboardKey.arrowLeft,
        LogicalKeyboardKey.arrowRight,
        LogicalKeyboardKey.home,
        LogicalKeyboardKey.end,
        LogicalKeyboardKey.pageUp,
        LogicalKeyboardKey.pageDown,
        LogicalKeyboardKey.insert,
        LogicalKeyboardKey.delete,
        LogicalKeyboardKey.numpad1,
        LogicalKeyboardKey.numpadAdd,
        LogicalKeyboardKey.capsLock,
        LogicalKeyboardKey.numLock,
      ];
      for (final key in unsafeBare) {
        expect(
          ButtonController.isSafeGlobalKey(keyId: key.keyId),
          isFalse,
          reason: '${key.debugName} should not be safe without modifiers',
        );
      }

      for (final modKey in [
        LogicalKeyboardKey.controlLeft,
        LogicalKeyboardKey.controlRight,
        LogicalKeyboardKey.altLeft,
        LogicalKeyboardKey.altRight,
        LogicalKeyboardKey.shiftLeft,
        LogicalKeyboardKey.shiftRight,
        LogicalKeyboardKey.metaLeft,
        LogicalKeyboardKey.metaRight,
      ]) {
        expect(
          ButtonController.isSafeGlobalKey(keyId: modKey.keyId),
          isFalse,
          reason: 'pure modifier ${modKey.debugName} must be rejected',
        );
        expect(
          ButtonController.isSafeGlobalKey(
            keyId: modKey.keyId,
            modifiers: ButtonBinding.modCtrl,
          ),
          isFalse,
          reason: 'pure modifier ${modKey.debugName} rejected even with mods',
        );
      }
    });

    test('accepts F1..F24, system lock keys, media keys, and modified keys', () {
      expect(
        ButtonController.isSafeGlobalKey(keyId: LogicalKeyboardKey.f1.keyId),
        isTrue,
      );
      expect(
        ButtonController.isSafeGlobalKey(keyId: LogicalKeyboardKey.f8.keyId),
        isTrue,
      );
      expect(
        ButtonController.isSafeGlobalKey(keyId: LogicalKeyboardKey.f24.keyId),
        isTrue,
      );
      expect(
        ButtonController.isSafeGlobalKey(
          keyId: LogicalKeyboardKey.printScreen.keyId,
        ),
        isTrue,
      );
      expect(
        ButtonController.isSafeGlobalKey(keyId: LogicalKeyboardKey.pause.keyId),
        isTrue,
      );
      expect(
        ButtonController.isSafeGlobalKey(
          keyId: LogicalKeyboardKey.scrollLock.keyId,
        ),
        isTrue,
      );
      expect(
        ButtonController.isSafeGlobalKey(keyId: ButtonController.mediaKeyId(85)),
        isTrue,
      );
      expect(
        ButtonController.isSafeGlobalKey(
          keyId: LogicalKeyboardKey.space.keyId,
          modifiers: ButtonBinding.modCtrl,
        ),
        isTrue,
      );
      expect(
        ButtonController.isSafeGlobalKey(
          keyId: LogicalKeyboardKey.keyA.keyId,
          modifiers: ButtonBinding.modAlt,
        ),
        isTrue,
      );
    });
  });

  group('modifier learning & binding uniqueness', () {
    test('ignores pure modifier KeyDownEvent and captures Ctrl+M with global=false', () async {
      int? learnedId;
      String? learnedLabel;
      int? learnedMods;

      buttons.learnNextWithModifiers((id, label, mods) {
        learnedId = id;
        learnedLabel = label;
        learnedMods = mods;
      });

      // Press Ctrl alone: still learning.
      HardwareKeyboard.instance.handleKeyEvent(
        const KeyDownEvent(
          physicalKey: PhysicalKeyboardKey.controlLeft,
          logicalKey: LogicalKeyboardKey.controlLeft,
          timeStamp: Duration.zero,
        ),
      );
      expect(buttons.isLearning, isTrue);
      expect(learnedId, isNull);

      // Press M while Ctrl is held.
      HardwareKeyboard.instance.handleKeyEvent(
        const KeyDownEvent(
          physicalKey: PhysicalKeyboardKey.keyM,
          logicalKey: LogicalKeyboardKey.keyM,
          timeStamp: Duration(milliseconds: 10),
        ),
      );
      expect(buttons.isLearning, isFalse);
      expect(learnedId, LogicalKeyboardKey.keyM.keyId);
      expect(learnedLabel, 'M');
      expect(learnedMods, ButtonBinding.modCtrl);

      final binding = ButtonBinding(
        keyId: learnedId!,
        action: ButtonAction.toggleMute,
        label: learnedLabel,
        modifiers: learnedMods!,
      );
      expect(binding.global, isFalse);
      expect(binding.displayName, 'Ctrl + M');

      HardwareKeyboard.instance.handleKeyEvent(
        const KeyUpEvent(
          physicalKey: PhysicalKeyboardKey.keyM,
          logicalKey: LogicalKeyboardKey.keyM,
          timeStamp: Duration(milliseconds: 20),
        ),
      );
      HardwareKeyboard.instance.handleKeyEvent(
        const KeyUpEvent(
          physicalKey: PhysicalKeyboardKey.controlLeft,
          logicalKey: LogicalKeyboardKey.controlLeft,
          timeStamp: Duration(milliseconds: 30),
        ),
      );
    });

    test('uniqueness by (keyId, modifiers) preserves bare and modified siblings', () async {
      final keyM = LogicalKeyboardKey.keyM.keyId;

      buttons.addBinding(
        ButtonBinding(
          keyId: keyM,
          action: ButtonAction.toggleMute,
          modifiers: 0,
        ),
      );
      buttons.addBinding(
        ButtonBinding(
          keyId: keyM,
          action: ButtonAction.toggleDeafen,
          modifiers: ButtonBinding.modCtrl,
        ),
      );
      expect(buttons.bindings, hasLength(2));

      // Rebinding Ctrl+M replaces only Ctrl+M.
      buttons.addBinding(
        ButtonBinding(
          keyId: keyM,
          action: ButtonAction.pushToTalk,
          modifiers: ButtonBinding.modCtrl,
        ),
      );
      expect(buttons.bindings, hasLength(2));
      expect(
        buttons.bindings.firstWhere((b) => b.modifiers == 0).action,
        ButtonAction.toggleMute,
      );
      expect(
        buttons.bindings
            .firstWhere((b) => b.modifiers == ButtonBinding.modCtrl)
            .action,
        ButtonAction.pushToTalk,
      );

      // Removing Ctrl+M leaves bare M intact.
      buttons.removeBinding(keyM, modifiers: ButtonBinding.modCtrl);
      expect(buttons.bindings, hasLength(1));
      expect(buttons.bindings.single.modifiers, 0);
    });
  });

  group('setGlobalHotkeys sync & opt-in toggle', () {
    test('newly added and default bindings do not register global hotkeys until opted in', () async {
      final f9 = LogicalKeyboardKey.f9.keyId;
      final space = LogicalKeyboardKey.space.keyId;

      buttons.setBindings([
        ButtonBinding(keyId: f9, action: ButtonAction.toggleMute),
      ]);
      await flush();

      expect(hotkeySyncCalls(), hasLength(1));
      expect(hotkeySyncCalls().last.arguments, {
        'hotkeys': <Map<String, int>>[],
      });

      // Opting in F9 registers it globally.
      expect(buttons.setBindingGlobal(f9, 0, true), isTrue);
      await flush();
      expect(hotkeySyncCalls().last.arguments, {
        'hotkeys': [
          {'keyId': f9, 'modifiers': 0},
        ],
      });

      // Adding Ctrl+Space with global=true registers alongside F9.
      buttons.addBinding(
        ButtonBinding(
          keyId: space,
          action: ButtonAction.pushToTalk,
          modifiers: ButtonBinding.modCtrl,
          global: true,
        ),
      );
      await flush();
      expect(hotkeySyncCalls().last.arguments, {
        'hotkeys': [
          {'keyId': f9, 'modifiers': 0},
          {'keyId': space, 'modifiers': ButtonBinding.modCtrl},
        ],
      });

      // Toggling F9 back off updates payload immediately.
      expect(buttons.setBindingGlobal(f9, 0, false), isTrue);
      await flush();
      expect(hotkeySyncCalls().last.arguments, {
        'hotkeys': [
          {'keyId': space, 'modifiers': ButtonBinding.modCtrl},
        ],
      });

      buttons.removeBinding(space, modifiers: ButtonBinding.modCtrl);
      await flush();
      expect(hotkeySyncCalls().last.arguments, {
        'hotkeys': <Map<String, int>>[],
      });
    });

    test('excludes unsafe bare keys and synthetic mobile media keyIds even if global=true', () async {
      final space = LogicalKeyboardKey.space.keyId;
      final keyM = LogicalKeyboardKey.keyM.keyId;
      final mediaPlay = ButtonController.mediaKeyId(85);
      final f8 = LogicalKeyboardKey.f8.keyId;

      buttons.setBindings([
        ButtonBinding(
          keyId: mediaPlay,
          action: ButtonAction.pushToTalk,
          global: true,
        ),
        ButtonBinding(
          keyId: space,
          action: ButtonAction.toggleMute,
          global: true,
        ),
        ButtonBinding(
          keyId: keyM,
          action: ButtonAction.toggleDeafen,
          global: false,
        ),
        ButtonBinding(
          keyId: f8,
          action: ButtonAction.toggleMute,
          global: true,
        ),
      ]);
      await flush();

      expect(hotkeySyncCalls().last.arguments, {
        'hotkeys': [
          {'keyId': f8, 'modifiers': 0},
        ],
      });

      // Programmatic opt-in for unsafe bare key is refused.
      expect(buttons.setBindingGlobal(keyM, 0, true), isFalse);
      expect(
        buttons.bindings.firstWhere((b) => b.keyId == keyM).global,
        isFalse,
      );
    });

    test('pauses global hotkeys while learning and restores on cancel or finish', () async {
      final f9 = LogicalKeyboardKey.f9.keyId;
      buttons.setBindings([
        ButtonBinding(keyId: f9, action: ButtonAction.toggleMute, global: true),
      ]);
      await flush();
      calls.clear();

      buttons.learnNext((_, _) {});
      await flush();
      expect(hotkeySyncCalls().last.arguments, {
        'hotkeys': <Map<String, int>>[],
      });

      buttons.cancelLearning();
      await flush();
      expect(hotkeySyncCalls().last.arguments, {
        'hotkeys': [
          {'keyId': f9, 'modifiers': 0},
        ],
      });

      calls.clear();
      buttons.learnNext((_, _) {});
      await flush();
      expect(hotkeySyncCalls().last.arguments, {
        'hotkeys': <Map<String, int>>[],
      });

      HardwareKeyboard.instance.handleKeyEvent(
        const KeyDownEvent(
          physicalKey: PhysicalKeyboardKey.keyM,
          logicalKey: LogicalKeyboardKey.keyM,
          timeStamp: Duration.zero,
        ),
      );
      HardwareKeyboard.instance.handleKeyEvent(
        const KeyUpEvent(
          physicalKey: PhysicalKeyboardKey.keyM,
          logicalKey: LogicalKeyboardKey.keyM,
          timeStamp: Duration(milliseconds: 10),
        ),
      );
      await flush();
      expect(hotkeySyncCalls().last.arguments, {
        'hotkeys': [
          {'keyId': f9, 'modifiers': 0},
        ],
      });
    });

    test('does not invoke setGlobalHotkeys on unsupported platforms', () async {
      ButtonController.globalHotkeysSupportedOverride = false;
      buttons.setBindings([
        ButtonBinding(
          keyId: LogicalKeyboardKey.f9.keyId,
          action: ButtonAction.toggleMute,
          global: true,
        ),
      ]);
      await flush();

      expect(hotkeySyncCalls(), isEmpty);
    });
  });

  group('globalHotkey channel dispatch & modifier matching', () {
    test('dispatches actions with modifiers and defaults omitted modifiers to 0', () async {
      var mutes = 0;
      var deafens = 0;
      buttons.onToggleMute = () => mutes++;
      buttons.onToggleDeafen = () => deafens++;

      final f9 = LogicalKeyboardKey.f9.keyId;
      final keyD = LogicalKeyboardKey.keyD.keyId;
      buttons.setBindings([
        ButtonBinding(keyId: f9, action: ButtonAction.toggleMute, global: true),
        ButtonBinding(
          keyId: keyD,
          action: ButtonAction.toggleDeafen,
          modifiers: ButtonBinding.modAlt,
          global: true,
        ),
      ]);

      await sendGlobalHotkey(f9, true);
      await sendGlobalHotkey(f9, false);
      expect(mutes, 1);

      // Bare D does not match Alt+D.
      await sendGlobalHotkey(keyD, true, modifiers: 0);
      expect(deafens, 0);

      await sendGlobalHotkey(keyD, true, modifiers: ButtonBinding.modAlt);
      await sendGlobalHotkey(keyD, false, modifiers: ButtonBinding.modAlt);
      expect(deafens, 1);
    });

    test('dispatches pushToTalk press/release and toggleTransmit', () async {
      final transmits = <bool>[];
      buttons.onTransmit = transmits.add;

      final space = LogicalKeyboardKey.space.keyId;
      final keyM = LogicalKeyboardKey.keyM.keyId;
      buttons.setBindings([
        ButtonBinding(
          keyId: space,
          action: ButtonAction.pushToTalk,
          modifiers: ButtonBinding.modCtrl,
          global: true,
        ),
        ButtonBinding(
          keyId: keyM,
          action: ButtonAction.toggleTransmit,
          modifiers: ButtonBinding.modAlt,
          global: true,
        ),
      ]);

      await sendGlobalHotkey(space, true, modifiers: ButtonBinding.modCtrl);
      await sendGlobalHotkey(space, false, modifiers: ButtonBinding.modCtrl);
      expect(transmits, [true, false]);

      transmits.clear();
      await sendGlobalHotkey(keyM, true, modifiers: ButtonBinding.modAlt);
      await sendGlobalHotkey(keyM, false, modifiers: ButtonBinding.modAlt);
      now = now.add(const Duration(milliseconds: 100));
      await sendGlobalHotkey(keyM, true, modifiers: ButtonBinding.modAlt);
      await sendGlobalHotkey(keyM, false, modifiers: ButtonBinding.modAlt);
      expect(transmits, [true, false]);
    });
  });

  group('focus deduplication & PTT release with modifiers', () {
    test('deduplicates on (keyId, modifiers) without blocking different modifiers', () async {
      var mutes = 0;
      var deafens = 0;
      buttons.onToggleMute = () => mutes++;
      buttons.onToggleDeafen = () => deafens++;

      final keyM = LogicalKeyboardKey.keyM;
      buttons.setBindings([
        ButtonBinding(
          keyId: keyM.keyId,
          action: ButtonAction.toggleMute,
          modifiers: ButtonBinding.modCtrl,
          global: true,
        ),
        ButtonBinding(
          keyId: keyM.keyId,
          action: ButtonAction.toggleDeafen,
          modifiers: 0,
        ),
      ]);

      // Press Ctrl+M via HardwareKeyboard.
      HardwareKeyboard.instance.handleKeyEvent(
        const KeyDownEvent(
          physicalKey: PhysicalKeyboardKey.controlLeft,
          logicalKey: LogicalKeyboardKey.controlLeft,
          timeStamp: Duration.zero,
        ),
      );
      final handled = HardwareKeyboard.instance.handleKeyEvent(
        KeyDownEvent(
          physicalKey: PhysicalKeyboardKey.keyM,
          logicalKey: keyM,
          timeStamp: const Duration(milliseconds: 5),
        ),
      );
      expect(handled, isTrue);
      expect(mutes, 1);

      // Matching (keyM, modCtrl) globalHotkey within 80 ms is ignored.
      now = now.add(const Duration(milliseconds: 40));
      await sendGlobalHotkey(
        keyM.keyId,
        true,
        modifiers: ButtonBinding.modCtrl,
      );
      expect(mutes, 1);

      // Different modifiers (keyM, 0) within 80 ms is NOT deduplicated.
      await sendGlobalHotkey(keyM.keyId, true, modifiers: 0);
      expect(deafens, 1);

      // After 80 ms window, (keyM, modCtrl) fires again.
      now = now.add(const Duration(milliseconds: 41));
      await sendGlobalHotkey(
        keyM.keyId,
        true,
        modifiers: ButtonBinding.modCtrl,
      );
      expect(mutes, 2);

      HardwareKeyboard.instance.handleKeyEvent(
        KeyUpEvent(
          physicalKey: PhysicalKeyboardKey.keyM,
          logicalKey: keyM,
          timeStamp: const Duration(milliseconds: 90),
        ),
      );
      HardwareKeyboard.instance.handleKeyEvent(
        const KeyUpEvent(
          physicalKey: PhysicalKeyboardKey.controlLeft,
          logicalKey: LogicalKeyboardKey.controlLeft,
          timeStamp: Duration(milliseconds: 95),
        ),
      );
    });

    test('ignores HardwareKeyboard press within 80 ms after globalHotkey', () async {
      var mutes = 0;
      buttons.onToggleMute = () => mutes++;

      final f9 = LogicalKeyboardKey.f9;
      buttons.setBindings([
        ButtonBinding(keyId: f9.keyId, action: ButtonAction.toggleMute, global: true),
      ]);

      await sendGlobalHotkey(f9.keyId, true);
      expect(mutes, 1);

      now = now.add(const Duration(milliseconds: 20));
      final handled = HardwareKeyboard.instance.handleKeyEvent(
        KeyDownEvent(
          physicalKey: PhysicalKeyboardKey.f9,
          logicalKey: f9,
          timeStamp: const Duration(milliseconds: 20),
        ),
      );
      expect(handled, isTrue);
      expect(mutes, 1);
    });

    test('stops PTT when required modifier is released before main key', () {
      final transmits = <bool>[];
      buttons.onTransmit = transmits.add;

      final keyM = LogicalKeyboardKey.keyM;
      buttons.setBindings([
        ButtonBinding(
          keyId: keyM.keyId,
          action: ButtonAction.pushToTalk,
          modifiers: ButtonBinding.modCtrl,
        ),
      ]);

      HardwareKeyboard.instance.handleKeyEvent(
        const KeyDownEvent(
          physicalKey: PhysicalKeyboardKey.controlLeft,
          logicalKey: LogicalKeyboardKey.controlLeft,
          timeStamp: Duration.zero,
        ),
      );
      HardwareKeyboard.instance.handleKeyEvent(
        KeyDownEvent(
          physicalKey: PhysicalKeyboardKey.keyM,
          logicalKey: keyM,
          timeStamp: const Duration(milliseconds: 10),
        ),
      );
      expect(transmits, [true]);

      // Release Ctrl first while M is still held -> PTT stops immediately.
      HardwareKeyboard.instance.handleKeyEvent(
        const KeyUpEvent(
          physicalKey: PhysicalKeyboardKey.controlLeft,
          logicalKey: LogicalKeyboardKey.controlLeft,
          timeStamp: Duration(milliseconds: 30),
        ),
      );
      expect(transmits, [true, false]);

      // Releasing M afterwards must not emit a duplicate false.
      HardwareKeyboard.instance.handleKeyEvent(
        KeyUpEvent(
          physicalKey: PhysicalKeyboardKey.keyM,
          logicalKey: keyM,
          timeStamp: const Duration(milliseconds: 40),
        ),
      );
      expect(transmits, [true, false]);
    });

    test('stops PTT when main key is released even if another modifier was added', () {
      final transmits = <bool>[];
      buttons.onTransmit = transmits.add;

      final keyM = LogicalKeyboardKey.keyM;
      buttons.setBindings([
        ButtonBinding(
          keyId: keyM.keyId,
          action: ButtonAction.pushToTalk,
          modifiers: ButtonBinding.modCtrl,
        ),
      ]);

      HardwareKeyboard.instance.handleKeyEvent(
        const KeyDownEvent(
          physicalKey: PhysicalKeyboardKey.controlLeft,
          logicalKey: LogicalKeyboardKey.controlLeft,
          timeStamp: Duration.zero,
        ),
      );
      HardwareKeyboard.instance.handleKeyEvent(
        KeyDownEvent(
          physicalKey: PhysicalKeyboardKey.keyM,
          logicalKey: keyM,
          timeStamp: const Duration(milliseconds: 10),
        ),
      );
      HardwareKeyboard.instance.handleKeyEvent(
        const KeyDownEvent(
          physicalKey: PhysicalKeyboardKey.shiftLeft,
          logicalKey: LogicalKeyboardKey.shiftLeft,
          timeStamp: Duration(milliseconds: 20),
        ),
      );
      expect(transmits, [true]);

      // Releasing M while Ctrl+Shift are held stops PTT cleanly.
      HardwareKeyboard.instance.handleKeyEvent(
        KeyUpEvent(
          physicalKey: PhysicalKeyboardKey.keyM,
          logicalKey: keyM,
          timeStamp: const Duration(milliseconds: 30),
        ),
      );
      expect(transmits, [true, false]);
    });

    test('does not suppress rapid PTT release or let repeats block key-up', () async {
      final transmits = <bool>[];
      buttons.onTransmit = transmits.add;

      final space = LogicalKeyboardKey.space;
      buttons.setBindings([
        ButtonBinding(
          keyId: space.keyId,
          action: ButtonAction.pushToTalk,
          modifiers: ButtonBinding.modCtrl,
          global: true,
        ),
      ]);

      HardwareKeyboard.instance.handleKeyEvent(
        const KeyDownEvent(
          physicalKey: PhysicalKeyboardKey.controlLeft,
          logicalKey: LogicalKeyboardKey.controlLeft,
          timeStamp: Duration.zero,
        ),
      );
      HardwareKeyboard.instance.handleKeyEvent(
        KeyDownEvent(
          physicalKey: PhysicalKeyboardKey.space,
          logicalKey: space,
          timeStamp: const Duration(milliseconds: 5),
        ),
      );
      now = now.add(const Duration(milliseconds: 30));
      HardwareKeyboard.instance.handleKeyEvent(
        KeyRepeatEvent(
          physicalKey: PhysicalKeyboardKey.space,
          logicalKey: space,
          timeStamp: const Duration(milliseconds: 30),
        ),
      );
      now = now.add(const Duration(milliseconds: 10));
      await sendGlobalHotkey(
        space.keyId,
        false,
        modifiers: ButtonBinding.modCtrl,
      );

      expect(transmits, [true, false]);
    });
  });

  group('Settings UI & AppState integration', () {
    testWidgets('shows Global toggle for safe keys, disables unsafe keys with hint, and toggles payload', (t) async {
      final state = AppState();
      addTearDown(state.dispose);
      state.markReadyForTesting();

      final f9 = LogicalKeyboardKey.f9.keyId;
      final space = LogicalKeyboardKey.space.keyId;
      buttons.setBindings([
        ButtonBinding(keyId: f9, action: ButtonAction.toggleMute),
        ButtonBinding(keyId: space, action: ButtonAction.pushToTalk),
      ]);
      await t.pump();
      calls.clear();

      t.view.physicalSize = const Size(600, 2600);
      t.view.devicePixelRatio = 1.0;
      addTearDown(t.view.resetPhysicalSize);
      addTearDown(t.view.resetDevicePixelRatio);

      await t.pumpWidget(
        AppStateScope(
          state: state,
          child: MaterialApp(
            localizationsDelegates: L.localizationsDelegates,
            supportedLocales: L.supportedLocales,
            home: const SettingsScreen(),
          ),
        ),
      );
      await t.pump(const Duration(milliseconds: 50));

      await t.scrollUntilVisible(
        find.text('Global hotkeys require a modifier or an F-key'),
        300,
        scrollable: find.byType(Scrollable).first,
      );
      await t.pump(const Duration(milliseconds: 16));

      final f9Tile = find.widgetWithText(ListTile, 'F9');
      final spaceTile = find.widgetWithText(ListTile, 'Space');
      expect(f9Tile, findsOneWidget);
      expect(spaceTile, findsOneWidget);
      expect(
        find.text('Global hotkeys require a modifier or an F-key'),
        findsOneWidget,
      );

      final f9Checkbox = find.descendant(
        of: f9Tile,
        matching: find.byType(Checkbox),
      );
      final spaceCheckbox = find.descendant(
        of: spaceTile,
        matching: find.byType(Checkbox),
      );
      expect(t.widget<Checkbox>(f9Checkbox).onChanged, isNotNull);
      expect(t.widget<Checkbox>(f9Checkbox).value, isFalse);
      expect(t.widget<Checkbox>(spaceCheckbox).onChanged, isNull);
      expect(t.widget<Checkbox>(spaceCheckbox).value, isFalse);

      // Toggle F9 global on via UI.
      await t.tap(f9Checkbox);
      await t.pump(const Duration(milliseconds: 50));

      expect(buttons.bindings.firstWhere((b) => b.keyId == f9).global, isTrue);
      expect(hotkeySyncCalls().last.arguments, {
        'hotkeys': [
          {'keyId': f9, 'modifiers': 0},
        ],
      });
    });

    test('AppState.learnButton, setButtonBindingGlobal, and removeButtonBinding respect modifiers', () async {
      final state = AppState();
      addTearDown(state.dispose);

      ButtonBinding? bound;
      state.learnButton(ButtonAction.toggleMute, (b) => bound = b);
      HardwareKeyboard.instance.handleKeyEvent(
        const KeyDownEvent(
          physicalKey: PhysicalKeyboardKey.controlLeft,
          logicalKey: LogicalKeyboardKey.controlLeft,
          timeStamp: Duration.zero,
        ),
      );
      HardwareKeyboard.instance.handleKeyEvent(
        const KeyDownEvent(
          physicalKey: PhysicalKeyboardKey.keyM,
          logicalKey: LogicalKeyboardKey.keyM,
          timeStamp: Duration(milliseconds: 10),
        ),
      );
      await flush();

      expect(bound, isNotNull);
      expect(bound!.modifiers, ButtonBinding.modCtrl);
      expect(bound!.global, isFalse);
      expect(bound!.displayName, 'Ctrl + M');

      final keyM = LogicalKeyboardKey.keyM.keyId;
      buttons.addBinding(
        ButtonBinding(
          keyId: keyM,
          action: ButtonAction.toggleDeafen,
          label: 'M',
          modifiers: 0,
        ),
      );
      await flush();

      await state.setButtonBindingGlobal(keyM, ButtonBinding.modCtrl, true);
      expect(
        buttons.bindings
            .firstWhere((b) => b.modifiers == ButtonBinding.modCtrl)
            .global,
        isTrue,
      );
      expect(
        hotkeySyncCalls().last.arguments,
        {
          'hotkeys': [
            {'keyId': keyM, 'modifiers': ButtonBinding.modCtrl},
          ],
        },
      );

      await state.removeButtonBinding(keyM, modifiers: ButtonBinding.modCtrl);
      expect(buttons.bindings, hasLength(1));
      expect(buttons.bindings.single.modifiers, 0);
      expect(hotkeySyncCalls().last.arguments, {
        'hotkeys': <Map<String, int>>[],
      });
    });
  });
}
