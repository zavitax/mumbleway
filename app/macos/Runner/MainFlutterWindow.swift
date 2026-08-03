import Cocoa
import FlutterMacOS

class MainFlutterWindow: NSWindow {
  private var callPanel: FloatingPanel?
  private var cloud: CloudStore?
  private var logSink: EngineLogSink?

  override func awakeFromNib() {
    let flutterViewController = FlutterViewController()
    let windowFrame = self.frame
    self.contentViewController = flutterViewController
    self.setFrame(windowFrame, display: true)

    RegisterGeneratedPlugins(registry: flutterViewController)
    registerOverlayChannel(with: flutterViewController.engine.binaryMessenger)
    cloud = CloudStore(messenger: flutterViewController.engine.binaryMessenger)
    logSink = EngineLogSink(messenger: flutterViewController.engine.binaryMessenger)

    super.awakeFromNib()
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
}
