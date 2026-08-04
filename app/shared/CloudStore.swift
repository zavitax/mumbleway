import Foundation
import Security

#if canImport(Flutter)
  import Flutter
#elseif canImport(FlutterMacOS)
  import FlutterMacOS
#endif

#if canImport(UIKit)
  import UIKit
#elseif canImport(AppKit)
  import AppKit
#endif

/// The server list in iCloud, shared by the iOS and macOS targets.
///
/// One file rather than one per platform because the interesting part is the
/// split between the two stores below, and that argument should not have to be
/// made twice and kept in step by hand.
///
/// The list goes into the ubiquitous key-value store: a handful of servers is
/// far inside its budget, it needs no container to be set up, and it arrives on
/// the other device by itself. What it is not is end-to-end encrypted — without
/// Advanced Data Protection, Apple can read what is in it.
///
/// So passwords do not go in it. They go into the keychain marked
/// synchronizable, which puts them in iCloud Keychain: end-to-end encrypted
/// always, no setting for the user to have failed to turn on. It also means a
/// password is never written into a preference file, on any of the devices.
///
/// The seam is the same one Dart draws when it hands over a payload and a
/// separate bag of secrets. Neither side has to know why; this is where the
/// reason lives.
final class CloudStore {
  /// One key. The whole list is a single value, so a device never sees half a
  /// merge: iCloud settles per key, and a list split across keys would arrive
  /// in pieces, with entries pointing at other entries that had not landed yet.
  private static let listKey = "servers"

  /// Well under the store's 1 MB ceiling. Hitting it means something is wrong
  /// upstream, and quietly failing to sync from then on is the worst way to
  /// find out.
  private static let maxPayloadBytes = 900_000

  private let channel: FlutterMethodChannel
  private let store = NSUbiquitousKeyValueStore.default
  private let service: String

  init(messenger: FlutterBinaryMessenger) {
    channel = FlutterMethodChannel(name: "mumbleway/cloud", binaryMessenger: messenger)
    let bundle = Bundle.main.bundleIdentifier ?? "com.mumbleway.mumbleway"
    service = "\(bundle).servers"

    channel.setMethodCallHandler { [weak self] call, result in
      self?.handle(call, result) ?? result(nil)
    }

    NotificationCenter.default.addObserver(
      self,
      selector: #selector(storeChangedExternally(_:)),
      name: NSUbiquitousKeyValueStore.didChangeExternallyNotification,
      object: store)

    // The same request, every time the app comes back to the front rather than
    // only when it is launched.
    //
    // A suspended app is not told that iCloud changed; it is told once it is
    // running again, if at all. Asking on activation is what turns "the list
    // updates eventually" into "the list is right when you look at it", which
    // is the only version anyone notices.
    #if canImport(UIKit)
      let becameActive = UIApplication.didBecomeActiveNotification
    #elseif canImport(AppKit)
      let becameActive = NSApplication.didBecomeActiveNotification
    #endif
    NotificationCenter.default.addObserver(
      self,
      selector: #selector(appBecameActive),
      name: becameActive,
      object: nil)

    // Asks for whatever arrived while the app was not running. Without this the
    // first read after launch returns the copy this device wrote last time.
    store.synchronize()
  }

  deinit {
    NotificationCenter.default.removeObserver(self)
  }

  // MARK: - Channel

  private func handle(_ call: FlutterMethodCall, _ result: @escaping FlutterResult) {
    switch call.method {
    case "available":
      // The documented test for "signed into iCloud". The store itself accepts
      // writes when nobody is signed in and simply never sends them anywhere,
      // which would leave the app claiming to sync while doing nothing.
      result(FileManager.default.ubiquityIdentityToken != nil)

    case "read":
      store.synchronize()
      result([
        "payload": store.string(forKey: Self.listKey) ?? "",
        "secrets": readSecrets(),
      ])

    case "write":
      guard let arguments = call.arguments as? [String: Any],
        let payload = arguments["payload"] as? String
      else {
        result(FlutterError(code: "bad-args", message: "Nothing to write.", details: nil))
        return
      }
      guard payload.utf8.count <= Self.maxPayloadBytes else {
        result(
          FlutterError(
            code: "too-large",
            message: "That server list is too large for iCloud to carry.", details: nil))
        return
      }
      store.set(payload, forKey: Self.listKey)
      store.synchronize()

      let secrets = arguments["secrets"] as? [String: String] ?? [:]
      for (id, password) in secrets {
        saveSecret(password, for: id)
      }
      // A password whose server is gone is deleted rather than left behind. It
      // would otherwise sit in the keychain waiting to be handed back if the
      // same address were ever added again, by someone who may not be the
      // person who typed it.
      if let live = arguments["liveIds"] as? [String] {
        pruneSecrets(keeping: Set(live))
      }
      result(nil)

    default:
      result(FlutterMethodNotImplemented)
    }
  }

  /// Pulls on activation, and tells Dart to reconcile either way.
  ///
  /// Two steps because `synchronize()` does not finish before it returns: it
  /// schedules the exchange, and anything read on the next line is still the
  /// copy this device already had. So the read below is the fast path — usually
  /// already correct — and `didChangeExternallyNotification` covers the case
  /// where the pull actually brought something new, a moment later.
  @objc private func appBecameActive() {
    store.synchronize()
    DispatchQueue.main.async { [weak self] in
      self?.channel.invokeMethod("remoteChanged", arguments: nil)
    }
  }

  @objc private func storeChangedExternally(_ note: Notification) {
    // Fires for local causes too, such as the initial sync pulling down what
    // this device itself wrote. Telling Dart anyway is harmless: it merges,
    // finds nothing new, and stops. Filtering here would mean reimplementing
    // that comparison in a second language.
    DispatchQueue.main.async { [weak self] in
      self?.channel.invokeMethod("remoteChanged", arguments: nil)
    }
  }

  // MARK: - Keychain

  private func baseQuery(account: String? = nil) -> [String: Any] {
    var q: [String: Any] = [
      kSecClass as String: kSecClassGenericPassword,
      kSecAttrService as String: service,
      // The point of the exercise: this is what puts the item in iCloud
      // Keychain rather than leaving it on one device.
      kSecAttrSynchronizable as String: kCFBooleanTrue!,
    ]
    if let account {
      q[kSecAttrAccount as String] = account
    }
    #if os(macOS)
      // macOS defaults to the old file-based keychain, which has no concept of
      // a synchronizable item. Without this the write appears to succeed and
      // never leaves the Mac.
      q[kSecUseDataProtectionKeychain as String] = true
    #endif
    return q
  }

  private func readSecrets() -> [String: String] {
    var query = baseQuery()
    query[kSecMatchLimit as String] = kSecMatchLimitAll
    query[kSecReturnAttributes as String] = true
    query[kSecReturnData as String] = true

    var out: CFTypeRef?
    guard SecItemCopyMatching(query as CFDictionary, &out) == errSecSuccess,
      let items = out as? [[String: Any]]
    else { return [:] }

    var secrets: [String: String] = [:]
    for item in items {
      guard let account = item[kSecAttrAccount as String] as? String,
        let data = item[kSecValueData as String] as? Data,
        let password = String(data: data, encoding: .utf8)
      else { continue }
      secrets[account] = password
    }
    return secrets
  }

  private func saveSecret(_ password: String, for id: String) {
    let data = Data(password.utf8)
    let query = baseQuery(account: id)

    let updated = SecItemUpdate(
      query as CFDictionary, [kSecValueData as String: data] as CFDictionary)
    if updated == errSecSuccess { return }

    var insert = query
    insert[kSecValueData as String] = data
    // Not ThisDeviceOnly, which cannot be synchronizable, and not WhenUnlocked,
    // which would put the password out of reach of a reconnect that happens
    // with the phone in a pocket.
    insert[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlock
    SecItemAdd(insert as CFDictionary, nil)
  }

  private func pruneSecrets(keeping live: Set<String>) {
    var query = baseQuery()
    query[kSecMatchLimit as String] = kSecMatchLimitAll
    query[kSecReturnAttributes as String] = true

    var out: CFTypeRef?
    guard SecItemCopyMatching(query as CFDictionary, &out) == errSecSuccess,
      let items = out as? [[String: Any]]
    else { return }

    for item in items {
      guard let account = item[kSecAttrAccount as String] as? String,
        !live.contains(account)
      else { continue }
      SecItemDelete(baseQuery(account: account) as CFDictionary)
    }
  }
}
