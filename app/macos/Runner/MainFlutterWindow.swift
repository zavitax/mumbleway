import Carbon
import Cocoa
import FlutterMacOS

class MainFlutterWindow: NSWindow {
  private var callPanel: FloatingPanel?
  private var cloud: CloudStore?
  private var logSink: EngineLogSink?
  private var identity: DeviceIdentity?

  /// Held for exactly as long as there is a conversation.
  ///
  /// The same bargain the phones make, in the form macOS offers. A Mac decides
  /// on its own that an app which is not in front and not visibly busy can be
  /// slowed down (App Nap), and that a machine nobody has touched can go to
  /// sleep — and either one cuts a call that was working. `beginActivity` is
  /// how an app says otherwise.
  ///
  /// The point is the *scope*. An assertion taken at launch and held for the
  /// life of the process would stop a laptop ever idling while MumbleWay sat
  /// open in a background tab, which is the desktop version of the wake lock
  /// this app used to hold on Android from the moment it was first opened.
  /// Held per call, a Mac with nothing connected sleeps exactly as it would
  /// with the app closed.
  private var callActivity: NSObjectProtocol?
  private var powerChannel: FlutterMethodChannel?
  private var buttonsChannel: FlutterMethodChannel?
  private var hotKeyHandlerRef: EventHandlerRef?
  private var registeredHotKeys: [UInt32: RegisteredHotKey] = [:]
  private var heldHotKeyIds: Set<UInt32> = []
  private var nextHotKeyId: UInt32 = 1

  private struct HotkeySpec: Hashable {
    let keyId: Int64
    let modifiers: Int
  }

  private struct RegisteredHotKey {
    let ref: EventHotKeyRef
    let keyId: Int64
    let modifiers: Int
  }

  override func awakeFromNib() {
    let flutterViewController = FlutterViewController()
    let windowFrame = self.frame
    self.contentViewController = flutterViewController
    self.setFrame(windowFrame, display: true)

    // Set here rather than by renaming the product. macOS takes the window's
    // default title from `PRODUCT_NAME`, which is also the bundle's name, what
    // it is signed as and what every path in the build refers to — so changing
    // it there to change a title bar would rename the application to rename a
    // string. This changes only the string. The menu bar keeps the short name,
    // which is what a menu bar is for.
    self.title = "MumbleWay - Voice for bikers"

    RegisterGeneratedPlugins(registry: flutterViewController)
    registerOverlayChannel(with: flutterViewController.engine.binaryMessenger)
    registerPowerChannel(with: flutterViewController.engine.binaryMessenger)
    registerButtonsChannel(with: flutterViewController.engine.binaryMessenger)
    cloud = CloudStore(messenger: flutterViewController.engine.binaryMessenger)
    logSink = EngineLogSink(messenger: flutterViewController.engine.binaryMessenger)
    identity = DeviceIdentity(messenger: flutterViewController.engine.binaryMessenger)

    super.awakeFromNib()
  }

  override func close() {
    unregisterAllHotKeys(emitHeldReleases: true)
    removeHotKeyHandler()
    super.close()
  }

  deinit {
    unregisterAllHotKeys(emitHeldReleases: false)
    removeHotKeyHandler()
  }

  private func registerOverlayChannel(with messenger: FlutterBinaryMessenger) {
    let channel = FlutterMethodChannel(
      name: "mumbleway/overlay", binaryMessenger: messenger)
    let panel = FloatingPanel(channel: channel)
    callPanel = panel

    channel.setMethodCallHandler { call, result in
      switch call.method {
      case "hasPermission", "requestPermission":
        // A floating panel needs no entitlement; there is nothing to ask for.
        result(true)

      case "show":
        panel.show()
        result(true)

      case "hide":
        panel.hide()
        result(true)

      case "update":
        guard let arguments = call.arguments as? [String: Any] else {
          result(false)
          return
        }
        panel.update(
          names: arguments["names"] as? [String] ?? [],
          transmitting: arguments["transmitting"] as? Bool ?? false,
          connected: arguments["connected"] as? Bool ?? false,
          muted: arguments["muted"] as? Bool ?? false,
          deafened: arguments["deafened"] as? Bool ?? false,
          level: arguments["level"] as? Double ?? 0,
          threshold: arguments["threshold"] as? Double ?? 0,
          noiseFloor: arguments["noiseFloor"] as? Double ?? 0,
          speaking: arguments["speaking"] as? Bool ?? false)
        result(true)

      default:
        result(FlutterMethodNotImplemented)
      }
    }
  }

  /// Kept apart from the overlay channel, which this platform registers but
  /// never hears from — Dart offers no floating window on a desktop, where the
  /// app is a window among windows. The power question is asked everywhere.
  private func registerPowerChannel(with messenger: FlutterBinaryMessenger) {
    let channel = FlutterMethodChannel(
      name: "mumbleway/power", binaryMessenger: messenger)
    channel.setMethodCallHandler { [weak self] call, result in
      switch call.method {
      case "callActive":
        self?.setCallActive(call.arguments as? Bool ?? false)
        result(true)
      default:
        result(FlutterMethodNotImplemented)
      }
    }
    powerChannel = channel
  }

  private func registerButtonsChannel(with messenger: FlutterBinaryMessenger) {
    let channel = FlutterMethodChannel(
      name: "mumbleway/buttons", binaryMessenger: messenger)
    channel.setMethodCallHandler { [weak self] call, result in
      guard let self else {
        result(nil)
        return
      }
      switch call.method {
      case "setGlobalHotkeys":
        let args = call.arguments as? [String: Any]
        let rawEntries = args?["hotkeys"] as? [Any] ?? []
        var specs: [HotkeySpec] = []
        for item in rawEntries {
          guard let entry = item as? [String: Any],
            let keyNum = entry["keyId"] as? NSNumber
          else {
            continue
          }
          let keyId = keyNum.int64Value
          let modifiers: Int
          if let rawMods = entry["modifiers"] {
            guard let modNum = rawMods as? NSNumber else { continue }
            modifiers = modNum.intValue & Self.modMask
          } else {
            modifiers = 0
          }
          guard Self.isSafeGlobalKey(keyId: keyId, modifiers: modifiers) else {
            continue
          }
          specs.append(HotkeySpec(keyId: keyId, modifiers: modifiers))
        }
        self.setGlobalHotkeys(specs)
        result(true)
      default:
        result(FlutterMethodNotImplemented)
      }
    }
    buttonsChannel = channel
  }

  private static let hotKeySignature: OSType = 0x4D57_484B  // 'MWHK'
  private static let modCtrl = 1
  private static let modAlt = 2
  private static let modShift = 4
  private static let modMeta = 8
  private static let modMask = modCtrl | modAlt | modShift | modMeta

  private func ensureHotKeyHandlerInstalled() -> Bool {
    if hotKeyHandlerRef != nil {
      return true
    }
    var eventTypes = [
      EventTypeSpec(
        eventClass: OSType(kEventClassKeyboard),
        eventKind: UInt32(kEventHotKeyPressed)),
      EventTypeSpec(
        eventClass: OSType(kEventClassKeyboard),
        eventKind: UInt32(kEventHotKeyReleased)),
    ]
    let status = InstallEventHandler(
      GetApplicationEventTarget(),
      { _, event, userData -> OSStatus in
        guard let event, let userData else {
          return OSStatus(eventNotHandledErr)
        }
        var hotKeyID = EventHotKeyID()
        let err = GetEventParameter(
          event,
          EventParamName(kEventParamDirectObject),
          EventParamType(typeEventHotKeyID),
          nil,
          MemoryLayout<EventHotKeyID>.size,
          nil,
          &hotKeyID)
        guard err == noErr,
          hotKeyID.signature == MainFlutterWindow.hotKeySignature
        else {
          return OSStatus(eventNotHandledErr)
        }
        let window = Unmanaged<MainFlutterWindow>.fromOpaque(userData)
          .takeUnretainedValue()
        let kind = GetEventKind(event)
        if kind == UInt32(kEventHotKeyPressed) {
          window.handleCarbonHotKey(id: hotKeyID.id, pressed: true)
          return noErr
        }
        if kind == UInt32(kEventHotKeyReleased) {
          window.handleCarbonHotKey(id: hotKeyID.id, pressed: false)
          return noErr
        }
        return OSStatus(eventNotHandledErr)
      },
      eventTypes.count,
      &eventTypes,
      Unmanaged.passUnretained(self).toOpaque(),
      &hotKeyHandlerRef)
    if status != noErr {
      hotKeyHandlerRef = nil
      return false
    }
    return true
  }

  private func removeHotKeyHandler() {
    if let handler = hotKeyHandlerRef {
      RemoveEventHandler(handler)
      hotKeyHandlerRef = nil
    }
  }

  private func unregisterAllHotKeys(emitHeldReleases: Bool) {
    if emitHeldReleases {
      for id in heldHotKeyIds {
        if let held = registeredHotKeys[id] {
          invokeGlobalHotkey(
            keyId: held.keyId,
            modifiers: held.modifiers,
            pressed: false)
        }
      }
    }
    heldHotKeyIds.removeAll()
    for entry in registeredHotKeys.values {
      UnregisterEventHotKey(entry.ref)
    }
    registeredHotKeys.removeAll()
    nextHotKeyId = 1
  }

  private func setGlobalHotkeys(_ hotkeys: [HotkeySpec]) {
    let nextSet = Set(hotkeys)
    for id in heldHotKeyIds {
      if let held = registeredHotKeys[id] {
        let heldSpec = HotkeySpec(keyId: held.keyId, modifiers: held.modifiers)
        if !nextSet.contains(heldSpec) {
          invokeGlobalHotkey(
            keyId: held.keyId,
            modifiers: held.modifiers,
            pressed: false)
        }
      }
    }
    unregisterAllHotKeys(emitHeldReleases: false)

    guard !hotkeys.isEmpty else {
      removeHotKeyHandler()
      return
    }
    guard ensureHotKeyHandlerInstalled() else { return }

    struct KeyCombo: Hashable {
      let keyCode: UInt32
      let carbonMods: UInt32
    }
    var seenCombos: Set<KeyCombo> = []

    for spec in hotkeys {
      guard Self.isSafeGlobalKey(keyId: spec.keyId, modifiers: spec.modifiers),
        let keyCode = Self.flutterKeyIdToCarbonKeyCode(spec.keyId)
      else {
        continue
      }
      let carbonMods = Self.carbonModifiers(from: spec.modifiers)
      let combo = KeyCombo(keyCode: keyCode, carbonMods: carbonMods)
      guard seenCombos.insert(combo).inserted else { continue }

      var hotKeyRef: EventHotKeyRef?
      let hotKeyID = EventHotKeyID(
        signature: Self.hotKeySignature,
        id: nextHotKeyId)
      let status = RegisterEventHotKey(
        keyCode,
        carbonMods,
        hotKeyID,
        GetApplicationEventTarget(),
        0,
        &hotKeyRef)
      if status == noErr, let ref = hotKeyRef {
        registeredHotKeys[nextHotKeyId] = RegisteredHotKey(
          ref: ref,
          keyId: spec.keyId,
          modifiers: spec.modifiers)
        nextHotKeyId &+= 1
        if nextHotKeyId == 0 {
          nextHotKeyId = 1
        }
      }
    }

    if registeredHotKeys.isEmpty {
      removeHotKeyHandler()
    }
  }

  private func handleCarbonHotKey(id: UInt32, pressed: Bool) {
    guard let entry = registeredHotKeys[id] else { return }
    if pressed {
      heldHotKeyIds.insert(id)
    } else {
      heldHotKeyIds.remove(id)
    }
    invokeGlobalHotkey(
      keyId: entry.keyId,
      modifiers: entry.modifiers,
      pressed: pressed)
  }

  private func invokeGlobalHotkey(keyId: Int64, modifiers: Int, pressed: Bool) {
    DispatchQueue.main.async { [weak self] in
      self?.buttonsChannel?.invokeMethod(
        "globalHotkey",
        arguments: [
          "keyId": keyId,
          "modifiers": modifiers,
          "pressed": pressed,
        ])
    }
  }

  private static func isModifierKeyId(_ keyId: Int64) -> Bool {
    switch keyId {
    case 0x00200000100,  // controlLeft
      0x00200000101,     // controlRight
      0x00200000102,     // shiftLeft
      0x00200000103,     // shiftRight
      0x00200000104,     // altLeft
      0x00200000105,     // altRight
      0x00200000106,     // metaLeft
      0x00200000107,     // metaRight
      0x20100000100,     // control synonym
      0x20100000102,     // shift synonym
      0x20100000104,     // alt synonym
      0x20100000106,     // meta synonym
      0x20200000100,     // control synonym (legacy/variant)
      0x20200000102,
      0x20200000104,
      0x20200000106:
      return true
    default:
      return false
    }
  }

  private static func isSafeGlobalKey(keyId: Int64, modifiers: Int) -> Bool {
    if isModifierKeyId(keyId) {
      return false
    }
    if (modifiers & modMask) != 0 {
      return true
    }
    // Function keys F1..F24.
    if keyId >= 0x00100000801 && keyId <= 0x00100000818 {
      return true
    }
    // Non-typing system/lock keys: PrintScreen, Pause, ScrollLock.
    return keyId == 0x00100000608
      || keyId == 0x00100000509
      || keyId == 0x0010000010c
  }

  private static func carbonModifiers(from modifiers: Int) -> UInt32 {
    var result: UInt32 = 0
    if (modifiers & modCtrl) != 0 { result |= UInt32(controlKey) }
    if (modifiers & modAlt) != 0 { result |= UInt32(optionKey) }
    if (modifiers & modShift) != 0 { result |= UInt32(shiftKey) }
    if (modifiers & modMeta) != 0 { result |= UInt32(cmdKey) }
    return result
  }

  private static func flutterKeyIdToCarbonKeyCode(_ keyId: Int64) -> UInt32? {
    let normalizedKeyId =
      (keyId >= 0x41 && keyId <= 0x5a) ? (keyId + 0x20) : keyId
    return logicalKeyIdToCarbonKeyCode[normalizedKeyId]
  }

  /// Maps `LogicalKeyboardKey.keyId` values to Carbon `kVK_*` virtual key
  /// codes (inverse of Flutter's `kMacOsToLogicalKey` and ASCII QWERTY map).
  private static let logicalKeyIdToCarbonKeyCode: [Int64: UInt32] = [
    // Function keys F1..F20 (HIToolbox Events.h stops at F20)
    0x00100000801: UInt32(kVK_F1),
    0x00100000802: UInt32(kVK_F2),
    0x00100000803: UInt32(kVK_F3),
    0x00100000804: UInt32(kVK_F4),
    0x00100000805: UInt32(kVK_F5),
    0x00100000806: UInt32(kVK_F6),
    0x00100000807: UInt32(kVK_F7),
    0x00100000808: UInt32(kVK_F8),
    0x00100000809: UInt32(kVK_F9),
    0x0010000080a: UInt32(kVK_F10),
    0x0010000080b: UInt32(kVK_F11),
    0x0010000080c: UInt32(kVK_F12),
    0x0010000080d: UInt32(kVK_F13),
    0x0010000080e: UInt32(kVK_F14),
    0x0010000080f: UInt32(kVK_F15),
    0x00100000810: UInt32(kVK_F16),
    0x00100000811: UInt32(kVK_F17),
    0x00100000812: UInt32(kVK_F18),
    0x00100000813: UInt32(kVK_F19),
    0x00100000814: UInt32(kVK_F20),

    // ASCII letters a..z
    0x61: UInt32(kVK_ANSI_A),
    0x62: UInt32(kVK_ANSI_B),
    0x63: UInt32(kVK_ANSI_C),
    0x64: UInt32(kVK_ANSI_D),
    0x65: UInt32(kVK_ANSI_E),
    0x66: UInt32(kVK_ANSI_F),
    0x67: UInt32(kVK_ANSI_G),
    0x68: UInt32(kVK_ANSI_H),
    0x69: UInt32(kVK_ANSI_I),
    0x6a: UInt32(kVK_ANSI_J),
    0x6b: UInt32(kVK_ANSI_K),
    0x6c: UInt32(kVK_ANSI_L),
    0x6d: UInt32(kVK_ANSI_M),
    0x6e: UInt32(kVK_ANSI_N),
    0x6f: UInt32(kVK_ANSI_O),
    0x70: UInt32(kVK_ANSI_P),
    0x71: UInt32(kVK_ANSI_Q),
    0x72: UInt32(kVK_ANSI_R),
    0x73: UInt32(kVK_ANSI_S),
    0x74: UInt32(kVK_ANSI_T),
    0x75: UInt32(kVK_ANSI_U),
    0x76: UInt32(kVK_ANSI_V),
    0x77: UInt32(kVK_ANSI_W),
    0x78: UInt32(kVK_ANSI_X),
    0x79: UInt32(kVK_ANSI_Y),
    0x7a: UInt32(kVK_ANSI_Z),

    // ASCII digits 0..9
    0x30: UInt32(kVK_ANSI_0),
    0x31: UInt32(kVK_ANSI_1),
    0x32: UInt32(kVK_ANSI_2),
    0x33: UInt32(kVK_ANSI_3),
    0x34: UInt32(kVK_ANSI_4),
    0x35: UInt32(kVK_ANSI_5),
    0x36: UInt32(kVK_ANSI_6),
    0x37: UInt32(kVK_ANSI_7),
    0x38: UInt32(kVK_ANSI_8),
    0x39: UInt32(kVK_ANSI_9),

    // US OEM punctuation
    0x3d: UInt32(kVK_ANSI_Equal),
    0x2d: UInt32(kVK_ANSI_Minus),
    0x5d: UInt32(kVK_ANSI_RightBracket),
    0x5b: UInt32(kVK_ANSI_LeftBracket),
    0x27: UInt32(kVK_ANSI_Quote),
    0x22: UInt32(kVK_ANSI_Quote),
    0x3b: UInt32(kVK_ANSI_Semicolon),
    0x5c: UInt32(kVK_ANSI_Backslash),
    0x2c: UInt32(kVK_ANSI_Comma),
    0x2f: UInt32(kVK_ANSI_Slash),
    0x2e: UInt32(kVK_ANSI_Period),
    0x60: UInt32(kVK_ANSI_Grave),

    // Navigation, editing, lock, and system keys
    0x0010000000d: UInt32(kVK_Return),
    0x00100000009: UInt32(kVK_Tab),
    0x00000000020: UInt32(kVK_Space),
    0x00100000008: UInt32(kVK_Delete),
    0x0010000001b: UInt32(kVK_Escape),
    0x00100000104: UInt32(kVK_CapsLock),
    0x0010000010a: UInt32(kVK_ANSI_KeypadClear),
    0x00100000a10: UInt32(kVK_VolumeUp),
    0x00100000a0f: UInt32(kVK_VolumeDown),
    0x00100000a11: UInt32(kVK_Mute),
    0x00100000505: UInt32(kVK_ContextualMenu),
    0x00100000407: UInt32(kVK_Help),
    0x00100000306: UInt32(kVK_Home),
    0x00100000308: UInt32(kVK_PageUp),
    0x0010000007f: UInt32(kVK_ForwardDelete),
    0x00100000305: UInt32(kVK_End),
    0x00100000307: UInt32(kVK_PageDown),
    0x00100000302: UInt32(kVK_LeftArrow),
    0x00100000303: UInt32(kVK_RightArrow),
    0x00100000301: UInt32(kVK_DownArrow),
    0x00100000304: UInt32(kVK_UpArrow),

    // Numpad
    0x0020000022e: UInt32(kVK_ANSI_KeypadDecimal),
    0x0020000022a: UInt32(kVK_ANSI_KeypadMultiply),
    0x0020000022b: UInt32(kVK_ANSI_KeypadPlus),
    0x0020000022f: UInt32(kVK_ANSI_KeypadDivide),
    0x0020000020d: UInt32(kVK_ANSI_KeypadEnter),
    0x0020000022d: UInt32(kVK_ANSI_KeypadMinus),
    0x0020000023d: UInt32(kVK_ANSI_KeypadEquals),
    0x00200000230: UInt32(kVK_ANSI_Keypad0),
    0x00200000231: UInt32(kVK_ANSI_Keypad1),
    0x00200000232: UInt32(kVK_ANSI_Keypad2),
    0x00200000233: UInt32(kVK_ANSI_Keypad3),
    0x00200000234: UInt32(kVK_ANSI_Keypad4),
    0x00200000235: UInt32(kVK_ANSI_Keypad5),
    0x00200000236: UInt32(kVK_ANSI_Keypad6),
    0x00200000237: UInt32(kVK_ANSI_Keypad7),
    0x00200000238: UInt32(kVK_ANSI_Keypad8),
    0x00200000239: UInt32(kVK_ANSI_Keypad9),
    0x0020000022c: UInt32(kVK_JIS_KeypadComma),
  ]

  /// Takes or drops the activity assertion as calls come and go.
  ///
  /// `.userInitiated` covers both halves of the problem: it keeps the process
  /// out of App Nap, and it stops the machine idle-sleeping under a call
  /// nobody happens to be touching — which on a desk is the normal state of a
  /// conversation. Deliberately *not*
  /// `.userInitiatedAllowingIdleSystemSleep`, which would let the Mac sleep
  /// mid-sentence.
  ///
  /// The display is left alone. Preventing sleep is not the same as keeping a
  /// screen lit, and a voice call has nothing anybody needs to look at.
  private func setCallActive(_ active: Bool) {
    if active {
      guard callActivity == nil else { return }
      callActivity = ProcessInfo.processInfo.beginActivity(
        options: [.userInitiated],
        reason: "MumbleWay call in progress")
    } else {
      guard let activity = callActivity else { return }
      ProcessInfo.processInfo.endActivity(activity)
      callActivity = nil
    }
  }
}
