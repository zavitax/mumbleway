import Flutter
import Foundation

/// `mumble://` links opened from outside the app.
///
/// A link tapped in a message, and — the case this feature is really for — a
/// QR code the phone's own Camera app recognised and offered to open. Both
/// arrive as a URL handed to the scene, not as anything this app polls for.
///
/// Held rather than pushed when it arrives first. A cold start delivers the URL
/// before the Flutter engine has run a line of Dart, so forwarding it then
/// would send it nowhere; the Dart side asks for it instead, once, on startup.
final class DeepLinks {
  static let shared = DeepLinks()

  private var channel: FlutterMethodChannel?

  /// A link that arrived before anybody was listening.
  private var pending: String?

  private init() {}

  func register(messenger: FlutterBinaryMessenger) {
    let channel = FlutterMethodChannel(name: "mumbleway/links", binaryMessenger: messenger)
    self.channel = channel

    channel.setMethodCallHandler { [weak self] call, result in
      guard call.method == "initialLink" else {
        result(FlutterMethodNotImplemented)
        return
      }
      result(self?.pending)
      // Handed over once. A rider who backs out of the form should not have
      // the link reappear the next time anything asks.
      self?.pending = nil
    }
  }

  /// Takes a URL from whichever delegate callback it arrived through.
  ///
  /// Returns whether it was ours, so a caller can pass on anything else.
  @discardableResult
  func handle(_ url: URL) -> Bool {
    guard url.scheme?.lowercased() == "mumble" else { return false }
    let text = url.absoluteString

    guard let channel else {
      pending = text
      return true
    }
    // The channel insists on the main thread, and `openURLContexts` is not
    // documented to promise one on every path.
    DispatchQueue.main.async { channel.invokeMethod("link", arguments: text) }
    return true
  }
}
