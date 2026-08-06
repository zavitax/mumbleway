import FlutterMacOS
import Foundation

/// Answers who is logged in, which on a Mac is a question with a good answer.
///
/// `NSFullUserName()` is the name in the account's settings — "Ilya Melamed" —
/// and `NSUserName()` is the short login name. The full name first, because a
/// rider heard over an intercom is a person rather than a login; the Dart side
/// turns the space into a dash and truncates to what Mumble accepts.
///
/// Unlike the phones, neither of these needs a permission or an entitlement,
/// and neither has been taken away. Sandboxing does not hide them: they
/// describe the account the process is already running as.
final class DeviceIdentity {
  private let channel: FlutterMethodChannel

  init(messenger: FlutterBinaryMessenger) {
    channel = FlutterMethodChannel(name: "mumbleway/identity", binaryMessenger: messenger)
    channel.setMethodCallHandler { call, result in
      switch call.method {
      case "suggestedName":
        let full = NSFullUserName().trimmingCharacters(in: .whitespacesAndNewlines)
        if !full.isEmpty {
          result(full)
          return
        }
        let short = NSUserName().trimmingCharacters(in: .whitespacesAndNewlines)
        result(short.isEmpty ? nil : short)
      default:
        result(FlutterMethodNotImplemented)
      }
    }
  }
}
