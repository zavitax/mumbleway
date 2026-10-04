import Cocoa
import FlutterMacOS

@main
class AppDelegate: FlutterAppDelegate {
  /// A link that arrived before Flutter was listening.
  ///
  /// macOS delivers `application(_:open:)` as soon as the app is launched by a
  /// link, which on a cold start is long before any Dart has run. Holding it
  /// and handing it over when asked is the same arrangement iOS and Android
  /// already use — a link forwarded too early is a link silently dropped, and
  /// on a cold start that is every one of them.
  private var pendingLink: String?
  private var channel: FlutterMethodChannel?

  override func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
    return true
  }

  override func applicationDidFinishLaunching(_ notification: Notification) {
    super.applicationDidFinishLaunching(notification)
    guard let controller = mainFlutterWindow?.contentViewController as? FlutterViewController
    else { return }
    let channel = FlutterMethodChannel(
      name: "mumbleway/links",
      binaryMessenger: controller.engine.binaryMessenger)
    self.channel = channel
    channel.setMethodCallHandler { [weak self] call, result in
      guard call.method == "initialLink" else {
        result(FlutterMethodNotImplemented)
        return
      }
      result(self?.pendingLink)
      // Handed over once: a rider who backs out of the form should not have
      // the link reappear the next time anything asks.
      self?.pendingLink = nil
    }
  }

  override func application(_ application: NSApplication, open urls: [URL]) {
    for url in urls {
      let scheme = url.scheme?.lowercased()
      guard scheme == "mumble" || scheme == "mumble-proxy" else { continue }
      if let channel = channel {
        channel.invokeMethod("link", arguments: url.absoluteString)
      } else {
        pendingLink = url.absoluteString
      }
    }
  }

  override func applicationSupportsSecureRestorableState(_ app: NSApplication) -> Bool {
    return true
  }
}
