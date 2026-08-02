import Cocoa
import FlutterMacOS

/// Input level with the noise floor and the activation threshold marked on the
/// same scale.
///
/// The two are drawn separately because the gap between them is the margin
/// being tuned: the floor climbs with road speed, and a meter showing only the
/// threshold makes that look like a control that has drifted rather than wind.
final class MeterView: NSView {
  var level: Double = 0 { didSet { needsDisplay = true } }
  var threshold: Double = 0 { didSet { needsDisplay = true } }
  var noiseFloor: Double = 0 { didSet { needsDisplay = true } }

  override var intrinsicContentSize: NSSize { NSSize(width: NSView.noIntrinsicMetric, height: 12) }

  override func draw(_ dirtyRect: NSRect) {
    let track = NSRect(x: 0, y: 3, width: bounds.width, height: 6)
    let radius = track.height / 2

    NSColor.labelColor.withAlphaComponent(0.15).setFill()
    NSBezierPath(roundedRect: track, xRadius: radius, yRadius: radius).fill()

    let clamped = CGFloat(max(0, min(1, level)))
    if clamped > 0.001 {
      // Colour answers the only question the meter is asked: would this level
      // open the gate?
      let open = level >= threshold
      (open ? NSColor.systemGreen : NSColor.secondaryLabelColor).setFill()
      let filled = NSRect(
        x: track.minX, y: track.minY,
        width: max(track.height, track.width * clamped), height: track.height)
      NSBezierPath(roundedRect: filled, xRadius: radius, yRadius: radius).fill()
    }

    tick(at: noiseFloor, in: track, colour: NSColor.systemBlue, overhang: 2)
    tick(at: threshold, in: track, colour: NSColor.systemOrange, overhang: 4)
  }

  private func tick(at value: Double, in track: NSRect, colour: NSColor, overhang: CGFloat) {
    let clamped = CGFloat(max(0, min(1, value)))
    colour.setFill()
    NSBezierPath(
      rect: NSRect(
        x: track.minX + track.width * clamped - 1, y: track.minY - overhang,
        width: 2, height: track.height + overhang * 2)
    ).fill()
  }
}

/// The transmit indicator. Blinks while the microphone is going out, the way a
/// studio on-air light does: a steady colour is easy to stop noticing, and a
/// channel left keyed open is the failure worth catching.
final class OnAirDot: NSView {
  var colour: NSColor = .secondaryLabelColor { didSet { needsDisplay = true } }
  var lit = true { didSet { needsDisplay = true } }

  override var intrinsicContentSize: NSSize { NSSize(width: 10, height: 10) }

  override func draw(_ dirtyRect: NSRect) {
    colour.withAlphaComponent(lit ? 1.0 : 0.25).setFill()
    NSBezierPath(ovalIn: bounds.insetBy(dx: 0.5, dy: 0.5)).fill()
  }
}

/// Always-on-top call controls for macOS.
///
/// macOS has no Picture in Picture for a third-party audio app and needs none:
/// a floating panel can sit above other windows with no permission and carry
/// real buttons, so this is the one Apple platform where the floating window
/// gets the full set — talk, mute, deafen and hang up.
///
/// It is an `NSPanel` rather than an `NSWindow` because a panel can be
/// non-activating: clicking talk must not pull the whole app forward and take
/// focus away from whatever the user was doing.
final class FloatingPanel: NSObject {
  private let channel: FlutterMethodChannel
  private var panel: NSPanel?

  private var talkButton: NSButton!
  private var muteButton: NSButton!
  private var deafenButton: NSButton!
  private var hangupButton: NSButton!
  private var statusLabel: NSTextField!
  private var speakersLabel: NSTextField!
  private var meter: MeterView!
  private var onAir: OnAirDot!
  private var flashTimer: Timer?

  private var transmitting = false
  private var connected = false
  private var muted = false
  private var deafened = false
  private var speaking = false
  private var names: [String] = []

  init(channel: FlutterMethodChannel) {
    self.channel = channel
    super.init()
  }

  // MARK: - Lifecycle

  func show() {
    if panel == nil { build() }
    panel?.orderFrontRegardless()
    refresh()
  }

  func hide() {
    panel?.orderOut(nil)
  }

  func update(
    names: [String], transmitting: Bool, connected: Bool, muted: Bool, deafened: Bool,
    level: Double, threshold: Double, noiseFloor: Double, speaking: Bool
  ) {
    self.names = names
    self.transmitting = transmitting
    self.connected = connected
    self.muted = muted
    self.deafened = deafened
    self.speaking = speaking
    meter?.level = level
    meter?.threshold = threshold
    meter?.noiseFloor = noiseFloor
    refresh()
  }

  deinit {
    flashTimer?.invalidate()
  }

  // MARK: - Construction

  private func build() {
    let panel = NSPanel(
      contentRect: NSRect(x: 0, y: 0, width: 232, height: 152),
      // `.nonactivatingPanel` is what keeps the app in the background when a
      // button is clicked; `.utilityWindow` keeps it out of the window list.
      styleMask: [.titled, .closable, .utilityWindow, .nonactivatingPanel],
      backing: .buffered,
      defer: false)
    panel.title = "MumbleWay"
    panel.titlebarAppearsTransparent = true
    panel.isFloatingPanel = true
    panel.level = .floating
    panel.hidesOnDeactivate = false
    panel.isMovableByWindowBackground = true
    // Follows the user across desktops and sits over a full-screen app, which
    // is the whole point of a floating control.
    panel.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary]

    let content = NSView(frame: panel.contentLayoutRect)
    content.autoresizingMask = [.width, .height]

    statusLabel = makeLabel(size: 13, weight: .semibold)
    speakersLabel = makeLabel(size: 11, weight: .regular)
    speakersLabel.textColor = .secondaryLabelColor

    talkButton = makeButton(title: "Talk", action: #selector(talkChanged(_:)))
    // A toggle, not a hold: a click and a release on the same button is how a
    // mouse works, and hold-to-talk would mean holding the mouse down.
    talkButton.setButtonType(.pushOnPushOff)

    muteButton = makeButton(title: "Mute", action: #selector(muteTapped))
    deafenButton = makeButton(title: "Deafen", action: #selector(deafenTapped))
    hangupButton = makeButton(title: "Hang up", action: #selector(hangupTapped))
    hangupButton.contentTintColor = .systemRed

    onAir = OnAirDot()
    meter = MeterView()

    let header = NSStackView(views: [onAir, statusLabel])
    header.orientation = .horizontal
    header.spacing = 6

    let secondary = NSStackView(views: [muteButton, deafenButton, hangupButton])
    secondary.orientation = .horizontal
    secondary.distribution = .fillEqually
    secondary.spacing = 6

    let stack = NSStackView(views: [
      header, meter, speakersLabel, talkButton, secondary,
    ])
    stack.orientation = .vertical
    stack.alignment = .centerX
    stack.spacing = 6
    stack.translatesAutoresizingMaskIntoConstraints = false
    content.addSubview(stack)

    NSLayoutConstraint.activate([
      stack.leadingAnchor.constraint(equalTo: content.leadingAnchor, constant: 10),
      stack.trailingAnchor.constraint(equalTo: content.trailingAnchor, constant: -10),
      stack.topAnchor.constraint(equalTo: content.topAnchor, constant: 4),
      talkButton.heightAnchor.constraint(equalToConstant: 30),
      secondary.widthAnchor.constraint(equalTo: stack.widthAnchor),
      meter.widthAnchor.constraint(equalTo: stack.widthAnchor),
      meter.heightAnchor.constraint(equalToConstant: 12),
      onAir.widthAnchor.constraint(equalToConstant: 10),
      onAir.heightAnchor.constraint(equalToConstant: 10),
    ])

    panel.contentView = content
    panel.delegate = self
    panel.center()
    self.panel = panel
  }

  private func makeLabel(size: CGFloat, weight: NSFont.Weight) -> NSTextField {
    let label = NSTextField(labelWithString: "")
    label.font = .systemFont(ofSize: size, weight: weight)
    label.alignment = .center
    label.lineBreakMode = .byTruncatingTail
    return label
  }

  private func makeButton(title: String, action: Selector) -> NSButton {
    let button = NSButton(title: title, target: self, action: action)
    button.bezelStyle = .rounded
    button.font = .systemFont(ofSize: 12, weight: .medium)
    return button
  }

  // MARK: - Presentation

  private func refresh() {
    guard panel != nil else { return }

    if !connected {
      statusLabel.stringValue = "Not connected"
      statusLabel.textColor = .secondaryLabelColor
    } else if transmitting {
      statusLabel.stringValue = "Talking"
      statusLabel.textColor = .systemRed
    } else if deafened {
      statusLabel.stringValue = "Deafened"
      statusLabel.textColor = .systemOrange
    } else if muted {
      statusLabel.stringValue = "Muted"
      statusLabel.textColor = .systemOrange
    } else {
      statusLabel.stringValue = "Listening"
      statusLabel.textColor = .systemGreen
    }

    let visible = names.prefix(2)
    if visible.isEmpty {
      speakersLabel.stringValue = connected ? "\u{2014}" : ""
    } else if names.count > visible.count {
      speakersLabel.stringValue =
        visible.joined(separator: ", ") + " +\(names.count - visible.count)"
    } else {
      speakersLabel.stringValue = visible.joined(separator: ", ")
    }

    if !connected {
      onAir.colour = .secondaryLabelColor
    } else if transmitting {
      onAir.colour = .systemRed
    } else if speaking {
      onAir.colour = .systemGreen
    } else {
      onAir.colour = .secondaryLabelColor
    }
    updateFlash()

    talkButton.state = transmitting ? .on : .off
    talkButton.isEnabled = connected
    muteButton.title = muted ? "Unmute" : "Mute"
    deafenButton.title = deafened ? "Undeafen" : "Deafen"
    muteButton.isEnabled = connected
    deafenButton.isEnabled = connected
    hangupButton.isEnabled = connected
  }

  /// The blink runs off its own timer rather than off state pushes, so the
  /// rate stays even no matter how often the app has something new to report.
  private func updateFlash() {
    guard transmitting else {
      flashTimer?.invalidate()
      flashTimer = nil
      onAir.lit = true
      return
    }
    guard flashTimer == nil else { return }
    let timer = Timer(timeInterval: 0.35, repeats: true) { [weak self] _ in
      guard let self else { return }
      self.onAir.lit.toggle()
    }
    RunLoop.main.add(timer, forMode: .common)
    flashTimer = timer
  }

  // MARK: - Actions

  @objc private func talkChanged(_ sender: NSButton) {
    channel.invokeMethod("setTransmitting", arguments: sender.state == .on)
  }

  @objc private func muteTapped() {
    channel.invokeMethod("toggleMute", arguments: nil)
  }

  @objc private func deafenTapped() {
    channel.invokeMethod("toggleDeafen", arguments: nil)
  }

  @objc private func hangupTapped() {
    channel.invokeMethod("hangup", arguments: nil)
  }
}

extension FloatingPanel: NSWindowDelegate {
  func windowWillClose(_ notification: Notification) {
    // Closing the panel turns the setting off rather than leaving a switch on
    // for a window that is no longer there.
    channel.invokeMethod("dismissed", arguments: nil)
  }
}
