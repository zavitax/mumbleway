import Flutter
import UIKit

/// Answers what this device can say about who owns it.
///
/// Very little, and less every year. `UIDevice.name` used to be "Ilya's
/// iPhone", set when the phone was first configured; since iOS 16 it returns
/// the model name — plain "iPhone" — to every app without the
/// `com.apple.developer.device-information.user-assigned-device-name`
/// entitlement, which Apple grants for specific hardware-integration cases and
/// which a voice app has no case for.
///
/// There is nothing else to try. The Apple ID has never been readable, and the
/// contact card of the device's owner is behind the contacts permission, which
/// is a great deal to ask for a pre-filled text field.
///
/// So this returns the device name and lets the Dart side judge it. On iOS 15
/// and earlier that is a real person's name; on iOS 16 and later it is "iPhone",
/// which the sanitiser recognises as an answer that would give every rider in
/// the group the same name, and discards in favour of a word pair.
final class DeviceIdentity {
  private let channel: FlutterMethodChannel

  init(messenger: FlutterBinaryMessenger) {
    channel = FlutterMethodChannel(name: "mumbleway/identity", binaryMessenger: messenger)
    channel.setMethodCallHandler { call, result in
      switch call.method {
      case "suggestedName":
        let name = UIDevice.current.name.trimmingCharacters(in: .whitespacesAndNewlines)
        result(name.isEmpty ? nil : name)
      default:
        result(FlutterMethodNotImplemented)
      }
    }
  }
}
