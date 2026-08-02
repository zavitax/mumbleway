import AVFoundation
import Flutter
import UIKit

@main
@objc class AppDelegate: FlutterAppDelegate, FlutterImplicitEngineDelegate {
  private var pip: AnyObject?
  private var overlayChannel: FlutterMethodChannel?

  override func application(
    _ application: UIApplication,
    didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]?
  ) -> Bool {
    return super.application(application, didFinishLaunchingWithOptions: launchOptions)
  }

  func didInitializeImplicitFlutterEngine(_ engineBridge: FlutterImplicitEngineBridge) {
    GeneratedPluginRegistrant.register(with: engineBridge.pluginRegistry)
    registerOverlayChannel(with: engineBridge.applicationBinaryMessenger)
  }

  private func registerOverlayChannel(with messenger: FlutterBinaryMessenger) {
    let channel = FlutterMethodChannel(
      name: "mumbleway/overlay", binaryMessenger: messenger)
    overlayChannel = channel

    channel.setMethodCallHandler { [weak self] call, result in
      guard let self else {
        result(FlutterError(code: "gone", message: "The app is shutting down.", details: nil))
        return
      }
      // Below iOS 15 there is no sample-buffer content source, so there is no
      // way to do this at all. Say so rather than failing obscurely.
      guard #available(iOS 15.0, *), PipController.isAvailable else {
        result(
          FlutterError(
            code: "unsupported",
            message: "Picture in Picture needs iOS 15 or later.", details: nil))
        return
      }

      switch call.method {
      case "hasPermission", "requestPermission":
        // Nothing to grant: Picture in Picture is not permission-gated.
        result(true)

      case "show":
        do {
          guard let controller = self.controller(for: channel) else {
            result(
              FlutterError(
                code: "unavailable", message: "The app window is not ready yet.",
                details: nil))
            return
          }
          try controller.start()
          result(true)
        } catch {
          result(
            FlutterError(
              code: "unavailable", message: error.localizedDescription, details: nil))
        }

      case "hide":
        (self.pip as? PipController)?.stop()
        result(true)

      case "update":
        guard let arguments = call.arguments as? [String: Any] else {
          result(false)
          return
        }
        var snapshot = CallSnapshot()
        snapshot.names = arguments["names"] as? [String] ?? []
        snapshot.transmitting = arguments["transmitting"] as? Bool ?? false
        snapshot.connected = arguments["connected"] as? Bool ?? false
        snapshot.muted = arguments["muted"] as? Bool ?? false
        snapshot.deafened = arguments["deafened"] as? Bool ?? false
        (self.pip as? PipController)?.update(snapshot)
        result(true)

      case "setCloseHangsUp":
        (self.pip as? PipController)?.setCloseHangsUp(call.arguments as? Bool ?? false)
        result(true)

      default:
        result(FlutterMethodNotImplemented)
      }
    }
  }

  @available(iOS 15.0, *)
  private func controller(for channel: FlutterMethodChannel) -> PipController? {
    if let existing = pip as? PipController { return existing }
    guard let hostView = window?.rootViewController?.view else { return nil }
    let created = PipController(channel: channel, hostView: hostView)
    pip = created
    return created
  }
}
