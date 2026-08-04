import Cocoa
import FlutterMacOS

class MainFlutterWindow: NSWindow {
  private var callPanel: FloatingPanel?
  private var cloud: CloudStore?
  private var logSink: EngineLogSink?

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

  override func awakeFromNib() {
    let flutterViewController = FlutterViewController()
    let windowFrame = self.frame
    self.contentViewController = flutterViewController
    self.setFrame(windowFrame, display: true)

    RegisterGeneratedPlugins(registry: flutterViewController)
    registerOverlayChannel(with: flutterViewController.engine.binaryMessenger)
    registerPowerChannel(with: flutterViewController.engine.binaryMessenger)
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
