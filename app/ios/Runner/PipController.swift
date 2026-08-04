import AVFoundation
import AVKit
import CoreMedia
import Flutter
import UIKit

/// The call state the floating window draws. Mirrored from Dart; this side
/// never decides any of it, so that the window and the app can never disagree.
/// One voice coming in, and how loud it is.
struct Speaker {
  var name: String
  /// Already normalised to 0...1 by the Dart side, on the same scale as every
  /// other meter in the app.
  var level: Double
}

struct CallSnapshot {
  var speakers: [Speaker] = []
  /// How many saved servers are in each state. The rider wants one glance to
  /// answer "is anything wrong", and a single connected flag cannot say that
  /// when two servers are up and a third is trying to come back.
  /// 0 push to talk, 1 voice activated, 2 always on. Anything but the first
  /// means the talk button has nothing to do.
  var micMode = 0
  /// Whether audio is actually going out right now, by whatever route. Not the
  /// same as the talk button being held: in the hands-free modes nobody holds
  /// anything and the microphone still opens.
  var live = false
  /// Built in Dart, where counts and grammar live together.
  var connectionText = ""
  /// 0 idle, 1 well, 2 struggling, 3 lost. Colour only; no words.
  var connectionLevel = 0
  var moreSpeakersText = ""
  /// How many others are within earshot across every server, or that nobody
  /// is. Only shown when the channel is silent, where it answers the question
  /// silence raises: quiet company, or nobody at all.
  var othersOnlineText = ""
  var connectedCount = 0
  var reconnectingCount = 0
  var failedCount = 0
  var transmitting = false
  var connected = false
  var muted = false
  var deafened = false
  /// Meter values, already normalised to 0...1 by the Dart side so that the
  /// level, the threshold and the noise floor cannot end up on three
  /// different scales.
  var level: Double = 0
  var threshold: Double = 0
  var noiseFloor: Double = 0
  var speaking = false
}

/// Picture in Picture floating window for iOS and iPadOS.
///
/// Apple allows no system-wide overlay from a third-party app, so Picture in
/// Picture is the only way to stay visible after the user leaves. It is built
/// for video, and the way audio apps use it — Google Meet included — is to
/// render their own frames into an `AVSampleBufferDisplayLayer` and hand that
/// to `AVPictureInPictureController` as the content source.
///
/// The system owns the buttons. There are exactly three programmable ones —
/// play/pause and the two skip buttons — and no API to add a fourth or to
/// relabel them:
///
///   * play/pause     → talk
///   * skip backward  → mute
///   * skip forward   → hang up
///
/// Deafen is the one that does not fit, and it is the right one to drop: it is
/// a comfort setting, while the other three are the controls a call needs.
/// Deafen stays available in the app and on the other platforms.
///
/// Hang-up takes two taps. The skip buttons are momentary and unlabelled, they
/// sit next to the talk control, and ending a call is the one action here that
/// cannot be undone — so the first tap arms it and says so in the frame, and
/// the arming lapses on its own.
@available(iOS 15.0, *)
final class PipController: NSObject {
  private let channel: FlutterMethodChannel
  private weak var hostView: UIView?

  private var displayLayer: AVSampleBufferDisplayLayer?
  /// The view holding the display layer. Kept so teardown can remove it: a
  /// layer left in the hierarchy goes on showing the last frame it was given.
  private var carrierView: UIView?
  private var pipController: AVPictureInPictureController?
  private var pixelBufferPool: CVPixelBufferPool?
  private var renderTimer: Timer?

  private var snapshot = CallSnapshot()

  /// The frame's own wording, in the app's language.
  ///
  /// Handed over rather than written here: this is the one part of the app
  /// that draws its own text, so it is also the one part that would quietly
  /// stay English while everything around it changed language. Keys missing
  /// from the book fall back to the English below, which is what a build with
  /// a new string and an old app does.
  private var phrases: [String: String] = [:]

  private func phrase(_ key: String, _ fallback: String) -> String {
    phrases[key] ?? fallback
  }

  func setPhrases(_ next: [String: String]) {
    phrases = next
    render()
  }

  /// 16:9 at a size that stays legible when the system shrinks the window.
  private static let frameSize = CGSize(width: 480, height: 270)

  init(channel: FlutterMethodChannel, hostView: UIView) {
    self.channel = channel
    self.hostView = hostView
    super.init()
  }

  deinit {
    renderTimer?.invalidate()
    carrierView?.removeFromSuperview()
    NotificationCenter.default.removeObserver(self)
  }

  /// Opens the window again as the app is put away.
  ///
  /// `canStartPictureInPictureAutomaticallyFromInline` covers the first time
  /// and, it turns out, only the first time: once the window has been closed
  /// by hand or restored back into the app, leaving again did nothing and the
  /// only way to get it back was to turn the setting off and on. Asking
  /// explicitly on the way out costs nothing when the window is already open,
  /// because starting an active controller is a no-op.
  /// Closes the window when the app comes back to the front.
  ///
  /// The system does this itself when the rider uses the window's own restore
  /// control, and not at all when they come back through the task switcher —
  /// so the window stayed on top of the app it is meant to be a stand-in for.
  /// It reopens on the way out, which is the pair to this.
  @objc private func didBecomeActive() {
    guard let pipController, pipController.isPictureInPictureActive else {
      return
    }
    pipController.stopPictureInPicture()
  }

  @objc private func willResignActive() {
    guard let pipController else { return }
    guard !pipController.isPictureInPictureActive else { return }

    // Belt and braces with the same clear on the way out: whatever the system
    // last recorded about playback, there is no window and so nothing paused.
    pipController.invalidatePlaybackState()

    guard pipController.isPictureInPicturePossible else {
      report("The system would not open the window this time.")
      return
    }
    pipController.startPictureInPicture()
  }

  static var isAvailable: Bool {
    AVPictureInPictureController.isPictureInPictureSupported()
  }

  // MARK: - Lifecycle

  func start() throws {
    guard Self.isAvailable else {
      throw PipError.unsupported("This device does not support Picture in Picture.")
    }
    guard let hostView else {
      throw PipError.unsupported("The app window is not ready yet.")
    }

    if pipController == nil {
      let layer = AVSampleBufferDisplayLayer()
      layer.videoGravity = .resizeAspect

      // The layer has to be in the hierarchy, in a window, sized, composited
      // and not hidden, or the system will not start Picture in Picture. Doing
      // all that without the user seeing it is the whole difficulty, and it
      // has now been got wrong three ways.
      //
      // `hostView` is the FlutterView, and Flutter draws into that view's own
      // layer rather than into a child — so any subview of it composites on
      // top of the entire app, whatever index it is given. At full size and
      // opaque, which is where this started, that was a rectangle across the
      // top of the screen covering the controls.
      //
      // Behind the Flutter view instead: the overlay went, and so did the
      // window. Clipped to a single point: same. Both suggest the system wants
      // a source that is actually being composited at something like its real
      // size, and neither an occluded one nor a one-pixel one qualifies.
      //
      // So: full size, in the place that works, and made invisible by opacity
      // rather than by geometry or z-order. Two per cent is enough for the
      // compositor to have work to do and far too little to see — the frame is
      // dark and the app behind it is dark.
      let carrier = UIView(
        frame: CGRect(origin: .zero, size: Self.frameSize))
      carrier.isUserInteractionEnabled = false
      carrier.alpha = 0.02
      layer.frame = carrier.bounds
      carrier.layer.addSublayer(layer)
      hostView.insertSubview(carrier, at: 0)
      carrierView = carrier

      let source = AVPictureInPictureController.ContentSource(
        sampleBufferDisplayLayer: layer, playbackDelegate: self)
      let controller = AVPictureInPictureController(contentSource: source)
      controller.delegate = self
      // What makes it behave like Google Meet: the window appears by leaving
      // the app, not by pressing anything.
      controller.canStartPictureInPictureAutomaticallyFromInline = true

      displayLayer = layer
      pipController = controller

      NotificationCenter.default.addObserver(
        self, selector: #selector(willResignActive),
        name: UIApplication.willResignActiveNotification, object: nil)
      NotificationCenter.default.addObserver(
        self, selector: #selector(didBecomeActive),
        name: UIApplication.didBecomeActiveNotification, object: nil)
    }

    render()
    startRenderTimer()
    beginWhenPossible(attemptsLeft: 12)
  }

  /// Starts once the controller says it can.
  ///
  /// `isPictureInPicturePossible` only turns true after the layer has been
  /// handed at least one frame and laid out, which has not happened yet on the
  /// call that creates it. Starting anyway fails, and the failure is silent
  /// from the user's side — the window simply never appears — so this waits
  /// for the flag instead of firing once and hoping.
  private func beginWhenPossible(attemptsLeft: Int) {
    guard let pipController else { return }
    guard !pipController.isPictureInPictureActive else { return }

    if pipController.isPictureInPicturePossible {
      pipController.startPictureInPicture()
      return
    }
    guard attemptsLeft > 0 else {
      // Reported rather than logged. This has been the failure three times
      // running, and a device build's log is not somewhere the person seeing
      // it can look.
      report(
        "The system never allowed the window to open. It may still appear when you leave the app.")
      return
    }
    DispatchQueue.main.asyncAfter(deadline: .now() + 0.25) { [weak self] in
      self?.beginWhenPossible(attemptsLeft: attemptsLeft - 1)
    }
  }

  /// Closes the window and dismantles everything behind it.
  ///
  /// Stopping the session is not enough on its own. Left as it was, the window
  /// could not be got rid of short of killing the app, for two separate
  /// reasons that both have to be dealt with here.
  ///
  /// The controller keeps `canStartPictureInPictureAutomaticallyFromInline`,
  /// which is what makes the window appear by leaving the app rather than by
  /// pressing anything. Armed on a live content source, it brings the window
  /// straight back the next time the app goes to the background — so it is
  /// switched off explicitly rather than left to the controller being
  /// released.
  ///
  /// And the layer goes on displaying the last frame it was handed. It sits
  /// behind Flutter's view and is normally out of sight, but the system lifts
  /// the source into view while restoring, which is where the stale meter came
  /// from. Flushing it and taking the view out of the hierarchy leaves nothing
  /// to show.
  /// Sends a diagnosis to the app, to be shown beside the setting.
  ///
  /// Everything that can go wrong here goes wrong asynchronously, well after
  /// `show()` has returned success, so there is nothing for the caller to
  /// return and nowhere for a message to land unless it is pushed.
  private func report(_ message: String?) {
    DispatchQueue.main.async { [weak self] in
      self?.channel.invokeMethod("pipStatus", arguments: message)
    }
  }

  func stop() {
    renderTimer?.invalidate()
    renderTimer = nil

    if let pipController {
      pipController.canStartPictureInPictureAutomaticallyFromInline = false
      if pipController.isPictureInPictureActive {
        pipController.stopPictureInPicture()
      }
      pipController.delegate = nil
    }
    pipController = nil

    displayLayer?.flushAndRemoveImage()
    displayLayer?.removeFromSuperlayer()
    displayLayer = nil

    carrierView?.removeFromSuperview()
    carrierView = nil

    // Rebuilt by the next start(), along with everything else here.
    pixelBufferPool = nil
  }

  func update(_ next: CallSnapshot) {
    let wasTransmitting = snapshot.transmitting
    snapshot = next
    // The system caches whether playback is paused and only asks again when
    // told to. Every way of starting or stopping transmission ends up here —
    // the window's own button, the talk button in the app, a bound Bluetooth
    // key, voice activation opening the gate — so this one call is what keeps
    // the button honest whatever caused the change.
    if next.transmitting != wasTransmitting {
      pipController?.invalidatePlaybackState()
    }
    render()
  }

  // MARK: - Frame production

  /// A still image can leave the window looking stalled, and the render is a
  /// few hundred pixels of flat colour, so it is cheaper to keep feeding it
  /// than to reason about when the system needs a fresh frame.
  ///
  /// The rate is driven by the meter and the on-air flash rather than by the
  /// call state, which changes rarely. Ten a second is enough for a level bar
  /// to look continuous and for the flash to have clean edges; the frame is a
  /// few hundred pixels of flat colour, so the cost is negligible.
  private func startRenderTimer() {
    renderTimer?.invalidate()
    let timer = Timer(timeInterval: 0.1, repeats: true) { [weak self] _ in
      guard let self else { return }
      self.frame &+= 1
      self.render()
    }
    RunLoop.main.add(timer, forMode: .common)
    renderTimer = timer
  }

  /// Frames since the timer started, used for the on-air flash. A counter
  /// rather than a clock so the blink cannot drift with render timing.
  private var frame: UInt64 = 0

  /// Roughly 1.5 Hz at ten frames a second: fast enough to read as "live",
  /// slow enough not to strobe in peripheral vision on a moving bike.
  private var onAirVisible: Bool { (frame % 7) < 4 }

  private func render() {
    guard let displayLayer else { return }
    guard let pixelBuffer = makePixelBuffer() else { return }

    draw(into: pixelBuffer)

    guard let sampleBuffer = makeSampleBuffer(from: pixelBuffer) else { return }

    if #available(iOS 17.0, *) {
      let renderer = displayLayer.sampleBufferRenderer
      if renderer.status == .failed { renderer.flush() }
      if renderer.isReadyForMoreMediaData { renderer.enqueue(sampleBuffer) }
    } else {
      if displayLayer.status == .failed { displayLayer.flush() }
      if displayLayer.isReadyForMoreMediaData { displayLayer.enqueue(sampleBuffer) }
    }
  }

  private func makePixelBuffer() -> CVPixelBuffer? {
    if pixelBufferPool == nil {
      let attributes: [String: Any] = [
        kCVPixelBufferPixelFormatTypeKey as String: kCVPixelFormatType_32BGRA,
        kCVPixelBufferWidthKey as String: Int(Self.frameSize.width),
        kCVPixelBufferHeightKey as String: Int(Self.frameSize.height),
        kCVPixelBufferIOSurfacePropertiesKey as String: [:],
      ]
      var pool: CVPixelBufferPool?
      CVPixelBufferPoolCreate(
        kCFAllocatorDefault, nil, attributes as CFDictionary, &pool)
      pixelBufferPool = pool
    }
    guard let pixelBufferPool else { return nil }

    var buffer: CVPixelBuffer?
    guard
      CVPixelBufferPoolCreatePixelBuffer(kCFAllocatorDefault, pixelBufferPool, &buffer)
        == kCVReturnSuccess
    else { return nil }
    return buffer
  }

  private func makeSampleBuffer(from pixelBuffer: CVPixelBuffer) -> CMSampleBuffer? {
    var formatDescription: CMVideoFormatDescription?
    guard
      CMVideoFormatDescriptionCreateForImageBuffer(
        allocator: kCFAllocatorDefault,
        imageBuffer: pixelBuffer,
        formatDescriptionOut: &formatDescription) == noErr,
      let formatDescription
    else { return nil }

    var timing = CMSampleTimingInfo(
      duration: CMTime(value: 1, timescale: 30),
      presentationTimeStamp: CMClockGetTime(CMClockGetHostTimeClock()),
      decodeTimeStamp: .invalid)

    var sampleBuffer: CMSampleBuffer?
    guard
      CMSampleBufferCreateReadyWithImageBuffer(
        allocator: kCFAllocatorDefault,
        imageBuffer: pixelBuffer,
        formatDescription: formatDescription,
        sampleTiming: &timing,
        sampleBufferOut: &sampleBuffer) == noErr,
      let sampleBuffer
    else { return nil }

    // Without this the layer waits on a timebase this layer does not have,
    // and nothing is ever shown.
    if let attachments = CMSampleBufferGetSampleAttachmentsArray(
      sampleBuffer, createIfNecessary: true), CFArrayGetCount(attachments) > 0
    {
      let raw = CFArrayGetValueAtIndex(attachments, 0)
      let dictionary = unsafeBitCast(raw, to: CFMutableDictionary.self)
      CFDictionarySetValue(
        dictionary,
        Unmanaged.passUnretained(kCMSampleAttachmentKey_DisplayImmediately).toOpaque(),
        Unmanaged.passUnretained(kCFBooleanTrue).toOpaque())
    }

    return sampleBuffer
  }

  // MARK: - Drawing

  private func draw(into pixelBuffer: CVPixelBuffer) {
    CVPixelBufferLockBaseAddress(pixelBuffer, [])
    defer { CVPixelBufferUnlockBaseAddress(pixelBuffer, []) }

    guard let base = CVPixelBufferGetBaseAddress(pixelBuffer) else { return }
    let width = CVPixelBufferGetWidth(pixelBuffer)
    let height = CVPixelBufferGetHeight(pixelBuffer)

    guard
      let context = CGContext(
        data: base,
        width: width,
        height: height,
        bitsPerComponent: 8,
        bytesPerRow: CVPixelBufferGetBytesPerRow(pixelBuffer),
        space: CGColorSpaceCreateDeviceRGB(),
        bitmapInfo: CGImageAlphaInfo.noneSkipFirst.rawValue
          | CGBitmapInfo.byteOrder32Little.rawValue)
    else { return }

    // UIKit's origin is top-left, Core Graphics' is bottom-left.
    context.translateBy(x: 0, y: CGFloat(height))
    context.scaleBy(x: 1, y: -1)

    UIGraphicsPushContext(context)
    defer { UIGraphicsPopContext() }

    let bounds = CGRect(x: 0, y: 0, width: width, height: height)
    UIColor(red: 0.063, green: 0.094, blue: 0.133, alpha: 1).setFill()
    context.fill(bounds)

    // Two halves, because the window answers two questions that have nothing
    // to do with each other: what this phone is doing with the microphone, and
    // who else is talking. Stacked, they read as one list and the eye has to
    // work out which line belongs to which. Side by side, each half is glanced
    // at rather than read — which is all a rider has time for.
    let divider = (bounds.width * 0.52).rounded()
    let left = CGRect(x: 0, y: 0, width: divider, height: bounds.height)
    let right = CGRect(
      x: divider, y: 0, width: bounds.width - divider, height: bounds.height)

    UIColor(white: 1, alpha: 0.10).setFill()
    UIBezierPath(rect: CGRect(x: divider, y: 22, width: 1, height: bounds.height - 44))
      .fill()

    drawConnection(in: left)
    drawOnAir(in: left)
    drawTitle(in: left)
    drawBadges(in: left)
    drawMeter(in: left)
    drawSpeakers(in: right)
    drawLegend(in: bounds)
  }

  /// One line across the top saying whether the radio is up.
  ///
  /// A rider glancing at this wants one question answered — is anything wrong
  /// — and the answer has to survive being read at arm's length in daylight,
  /// so it is a coloured dot and a short phrase rather than a list. Amber
  /// outranks green: two servers up and one struggling is a problem, and
  /// showing "2 connected" would hide it.
  private func drawConnection(in bounds: CGRect) {
    // The phrase is built in Dart, where the counts can be put into a
    // sentence that agrees with itself in either language. Only the severity
    // crosses over, because a colour is not a matter of grammar.
    let text = snapshot.connectionText
    let colour: UIColor
    switch snapshot.connectionLevel {
    case 1: colour = UIColor(red: 0.36, green: 0.85, blue: 0.45, alpha: 1)
    case 2: colour = UIColor(red: 0.98, green: 0.75, blue: 0.25, alpha: 1)
    case 3: colour = UIColor(red: 0.94, green: 0.32, blue: 0.28, alpha: 1)
    default: colour = UIColor(white: 0.5, alpha: 1)
    }

    // Measured and then centred as one piece, rather than each part placed
    // against an edge: the phrase changes length as servers come and go, and
    // anchoring the dot would leave the line shuffling sideways every time.
    let size: CGFloat = 12
    let font = UIFont.systemFont(ofSize: size, weight: .bold)
    let width = (text as NSString).size(withAttributes: [.font: font]).width
    let diameter: CGFloat = 10
    let gap: CGFloat = 8
    let startX = (bounds.midX - (diameter + gap + width) / 2).rounded()
    let middle: CGFloat = 24

    colour.setFill()
    UIBezierPath(
      ovalIn: CGRect(
        x: startX, y: middle - diameter / 2, width: diameter, height: diameter)
    ).fill()

    // Centred on the dot rather than sharing its top edge, so the two read as
    // one line. Half the line height, not half the point size: text draws from
    // the top of its rectangle, and the difference between the two is a few
    // pixels of the text sitting high — which is exactly the misalignment this
    // is here to remove.
    drawText(
      text,
      in: CGRect(
        x: startX + diameter + gap, y: middle - font.lineHeight / 2,
        width: width + 4, height: font.lineHeight),
      size: size, weight: .bold, colour: colour, alignment: .left)
  }

  /// The transmit indicator: a filled ring that blinks while the microphone is
  /// actually going out, the way a studio on-air light does. Blinking rather
  /// than merely turning red because a steady colour is easy to lose track of,
  /// and leaving a channel keyed open by accident is the failure that matters.
  private func drawOnAir(in bounds: CGRect) {
    let centre = CGPoint(x: bounds.midX, y: 84)
    let radius: CGFloat = 28

    let colour: UIColor
    if !snapshot.connected {
      colour = UIColor(white: 0.45, alpha: 1)
    } else if snapshot.live {
      colour = UIColor(red: 0.94, green: 0.24, blue: 0.24, alpha: 1)
    } else if snapshot.speaking {
      colour = UIColor(red: 0.25, green: 0.78, blue: 0.45, alpha: 1)
    } else {
      colour = UIColor(white: 0.55, alpha: 1)
    }

    let lit = !snapshot.live || onAirVisible

    if snapshot.live {
      colour.withAlphaComponent(lit ? 0.30 : 0.10).setFill()
      UIBezierPath(
        arcCenter: centre, radius: radius + 16, startAngle: 0, endAngle: .pi * 2,
        clockwise: true
      ).fill()
    }

    colour.withAlphaComponent(lit ? 1.0 : 0.35).setFill()
    UIBezierPath(
      arcCenter: centre, radius: radius, startAngle: 0, endAngle: .pi * 2, clockwise: true
    ).fill()

    if snapshot.live {
      drawText(
        phrase("pipOnAir", "ON AIR"),
        in: CGRect(x: bounds.minX, y: centre.y - 9, width: bounds.width, height: 20),
        size: 13, weight: .heavy, colour: .white, alignment: .center)
    } else {
      drawText(
        "\u{1F3A4}",
        in: CGRect(x: bounds.minX, y: centre.y - 14, width: bounds.width, height: 28),
        size: 20, weight: .bold, colour: .white, alignment: .center)
    }
  }

  private func drawTitle(in bounds: CGRect) {
    // The arming prompt outranks everything: it is the only state here with a
    // deadline, and the frame is the only place it can be said.
    let text: String
    if !snapshot.connected {
      text = phrase("pipNotConnected", "Not connected")
    } else if snapshot.live {
      text = phrase("pipTalking", "Talking")
    } else if snapshot.deafened {
      text = phrase("pipDeafened", "Deafened")
    } else if snapshot.muted {
      text = phrase("pipMuted", "Muted")
    } else {
      // Not just "Listening": that reads as though the microphone is open, and
      // the whole point of this line is to say that it is not.
      text = phrase("pipListening", "Listening, but\nnot transmitting")
    }
    drawText(
      text,
      in: CGRect(x: bounds.minX + 8, y: 126, width: bounds.width - 16, height: 44),
      size: 15, weight: .semibold, colour: .white, alignment: .center, lines: 2)
  }

  private func drawBadges(in bounds: CGRect) {
    var labels: [String] = []
    if snapshot.muted { labels.append(phrase("pipBadgeMuted", "MUTED")) }
    if snapshot.deafened {
      labels.append(phrase("pipBadgeDeafened", "DEAFENED"))
    }
    guard !labels.isEmpty else { return }
    drawText(
      labels.joined(separator: "  \u{00B7}  "),
      in: CGRect(x: bounds.minX + 8, y: 172, width: bounds.width - 16, height: 18),
      size: 12, weight: .bold,
      colour: UIColor(red: 0.98, green: 0.72, blue: 0.35, alpha: 1),
      alignment: .center)
  }

  /// Input level with the two thresholds marked on the same scale.
  ///
  /// The noise floor and the activation threshold are drawn as separate ticks
  /// because the distance between them is the margin being tuned: on a bike
  /// the floor climbs with speed, and a meter showing only the threshold makes
  /// that look like a control that has drifted rather than wind.
  private func drawMeter(in bounds: CGRect) {
    let track = CGRect(
      x: bounds.minX + 40, y: 200, width: bounds.width - 80, height: 12)
    let radius = track.height / 2

    UIColor(white: 1, alpha: 0.12).setFill()
    UIBezierPath(roundedRect: track, cornerRadius: radius).fill()

    let level = CGFloat(max(0, min(1, snapshot.level)))
    if level > 0.001 {
      // Colour only while audio is actually going out, matching the meter on
      // the main screen. The bar still moves off air, so the microphone is
      // visibly alive; the colour is what says somebody is hearing it. The
      // speaker meters on the right keep their colours regardless, because
      // they show other people, who are being heard by definition.
      let open = snapshot.level >= snapshot.threshold
      let fill: UIColor
      if !snapshot.live {
        fill = UIColor(white: 0.55, alpha: 1)
      } else if open {
        fill = UIColor(red: 0.36, green: 0.85, blue: 0.45, alpha: 1)
      } else {
        fill = UIColor(white: 0.72, alpha: 1)
      }
      fill.setFill()
      let filled = CGRect(
        x: track.minX, y: track.minY, width: max(track.height, track.width * level),
        height: track.height)
      UIBezierPath(roundedRect: filled, cornerRadius: radius).fill()
    }

    drawTick(
      at: snapshot.noiseFloor, in: track,
      colour: UIColor(red: 0.55, green: 0.65, blue: 0.80, alpha: 1), height: 4)
    drawTick(
      at: snapshot.threshold, in: track,
      colour: UIColor(red: 1.0, green: 0.78, blue: 0.25, alpha: 1), height: 7)

    drawText(
      phrase("pipNoise", "noise"), in: CGRect(x: bounds.minX + 2, y: 198, width: 36, height: 16),
      size: 9, weight: .semibold, colour: UIColor(white: 1, alpha: 0.5),
      alignment: .right)
    drawText(
      phrase("pipOpen", "open"), in: CGRect(x: track.maxX + 2, y: 198, width: 36, height: 16),
      size: 9, weight: .semibold, colour: UIColor(white: 1, alpha: 0.5),
      alignment: .left)
  }

  /// What the one remaining system button does.
  ///
  /// The window has no labels of its own and cannot be given any, so the only
  /// place to say it is in the picture.
  private func drawLegend(in bounds: CGRect) {
    // Says what the one control does, and in the hands-free modes says that
    // there is nothing to press rather than labelling a button that is inert.
    let text: String
    switch snapshot.micMode {
    case 1:
      text = phrase("pipHandsFreeVoice", "hands-free \u{00B7} voice activated")
    case 2:
      text = phrase("pipHandsFreeAlways", "hands-free \u{00B7} always on")
    default:
      text = "\u{25B6}\u{2016} " + phrase("pipTalk", "talk")
    }
    drawText(
      text,
      in: CGRect(x: 8, y: 246, width: bounds.width - 16, height: 16),
      size: 10, weight: .semibold, colour: UIColor(white: 1, alpha: 0.45),
      alignment: .center)
  }

  private func drawTick(
    at value: Double, in track: CGRect, colour: UIColor, height: CGFloat
  ) {
    let clamped = CGFloat(max(0, min(1, value)))
    let x = track.minX + track.width * clamped
    colour.setFill()
    UIBezierPath(
      rect: CGRect(
        x: x - 1.5, y: track.minY - height, width: 3,
        height: track.height + height * 2)
    ).fill()
  }

  /// Who is talking, and how loudly.
  ///
  /// A name on its own says someone is connected; a name with a moving bar
  /// says they are being heard, which is the thing actually in doubt when a
  /// helmet has gone quiet. Four at once fits now that a name and its meter
  /// share a line, which covers most groups; a larger one is counted rather
  /// than listed.
  private func drawSpeakers(in bounds: CGRect) {
    let inset = bounds.insetBy(dx: 14, dy: 0)

    guard !snapshot.speakers.isEmpty else {
      drawText(
        snapshot.connected
          ? phrase("pipNobodySpeaks", "Nobody speaks") : "\u{2014}",
        in: CGRect(x: inset.minX, y: 122, width: inset.width, height: 22),
        size: 14, weight: .medium, colour: UIColor(white: 1, alpha: 0.45),
        alignment: .center)

      // Underneath, because silence on its own is ambiguous: it says nobody is
      // talking, not whether anybody is there to talk. Quieter and smaller than
      // the line above it, being the answer to a question rather than the
      // thing being reported.
      if !snapshot.othersOnlineText.isEmpty {
        drawText(
          snapshot.othersOnlineText,
          in: CGRect(x: inset.minX, y: 148, width: inset.width, height: 18),
          size: 12, weight: .medium, colour: UIColor(white: 1, alpha: 0.32),
          alignment: .center)
      }
      return
    }

    drawText(
      phrase("pipSpeaking", "SPEAKING"),
      in: CGRect(x: inset.minX, y: 30, width: inset.width, height: 14),
      size: 10, weight: .heavy, colour: UIColor(white: 1, alpha: 0.4),
      alignment: .left)

    // Name against the left edge, meter against the right, on one line and
    // centred on each other. A meter under its name reads as a second row and
    // costs the height of one, which is what limited this to three people; on
    // the same line the pairing is obvious and the list can breathe.
    //
    // A quarter of the width is enough for a level to be seen moving without
    // taking room from the names, which are what identifies who is talking.
    let visible = snapshot.speakers.prefix(4)
    let meterWidth = (inset.width * 0.25).rounded()
    let gap: CGFloat = 10
    let nameWidth = inset.width - meterWidth - gap
    let size: CGFloat = 15
    let font = UIFont.systemFont(ofSize: size, weight: .semibold)

    var y: CGFloat = 60
    for speaker in visible {
      drawText(
        speaker.name,
        in: CGRect(x: inset.minX, y: y, width: nameWidth, height: font.lineHeight),
        size: size, weight: .semibold,
        colour: UIColor(red: 0.55, green: 0.83, blue: 1.0, alpha: 1),
        alignment: .left)

      let height: CGFloat = 8
      let track = CGRect(
        x: inset.maxX - meterWidth, y: y + (font.lineHeight - height) / 2,
        width: meterWidth, height: height)
      UIColor(white: 1, alpha: 0.12).setFill()
      UIBezierPath(roundedRect: track, cornerRadius: height / 2).fill()

      let level = CGFloat(max(0, min(1, speaker.level)))
      if level > 0.001 {
        // The same three-colour scale as every meter in the app: green while
        // there is headroom, amber approaching the top, red at it.
        let colour: UIColor
        if level > 0.85 {
          colour = UIColor(red: 0.94, green: 0.32, blue: 0.28, alpha: 1)
        } else if level > 0.65 {
          colour = UIColor(red: 0.98, green: 0.75, blue: 0.25, alpha: 1)
        } else {
          colour = UIColor(red: 0.36, green: 0.85, blue: 0.45, alpha: 1)
        }
        colour.setFill()
        UIBezierPath(
          roundedRect: CGRect(
            x: track.minX, y: track.minY,
            width: max(height, track.width * level), height: height),
          cornerRadius: height / 2
        ).fill()
      }
      y += 34
    }

    if snapshot.speakers.count > visible.count {
      drawText(
        snapshot.moreSpeakersText,
        in: CGRect(x: inset.minX, y: y + 2, width: inset.width, height: 16),
        size: 11, weight: .medium, colour: UIColor(white: 1, alpha: 0.45),
        alignment: .left)
    }
  }

  private func drawText(
    _ text: String, in rect: CGRect, size: CGFloat, weight: UIFont.Weight,
    colour: UIColor, alignment: NSTextAlignment, lines: Int = 1
  ) {
    guard !text.isEmpty else { return }
    let paragraph = NSMutableParagraphStyle()
    paragraph.alignment = alignment
    paragraph.lineBreakMode = lines > 1 ? .byWordWrapping : .byTruncatingTail
    let attributes: [NSAttributedString.Key: Any] = [
      .font: UIFont.systemFont(ofSize: size, weight: weight),
      .foregroundColor: colour,
      .paragraphStyle: paragraph,
    ]
    (text as NSString).draw(in: rect, withAttributes: attributes)
  }

  enum PipError: Error {
    case unsupported(String)
  }
}

// MARK: - System control mapping

@available(iOS 15.0, *)
extension PipController: AVPictureInPictureSampleBufferPlaybackDelegate {
  func pictureInPictureController(
    _ pictureInPictureController: AVPictureInPictureController, setPlaying playing: Bool
  ) {
    // Ignored unless the rider is actually holding a talk button for a living.
    // The system always draws this control and gives no way to remove it, so
    // the honest thing is for it to do nothing in the modes where it means
    // nothing — rather than forcing push-to-talk on somebody who chose
    // hands-free and then leaving them to work out why the microphone shut.
    guard snapshot.micMode == 0 else { return }
    channel.invokeMethod("setTransmitting", arguments: playing)
  }

  /// An indefinite range marks this a live stream, which is what it is, and
  /// leaves the system showing play/pause alone.
  ///
  /// A finite range brings the two skip buttons with it. They were carrying
  /// mute and hang-up, since three controls were all the window could ever
  /// have — but they are unlabelled arrows next to the talk button, and an
  /// unlabelled arrow that ends the call is a poor thing to have under a glove
  /// at speed. Both actions remain in the app, and on the lock screen the
  /// window is now one button that does one thing.
  func pictureInPictureControllerTimeRangeForPlayback(
    _ pictureInPictureController: AVPictureInPictureController
  ) -> CMTimeRange {
    CMTimeRange(start: .negativeInfinity, duration: .positiveInfinity)
  }

  func pictureInPictureControllerIsPlaybackPaused(
    _ pictureInPictureController: AVPictureInPictureController
  ) -> Bool {
    // Never paused until the window exists. iOS does not open Picture in
    // Picture for content it believes is paused, and it declines in complete
    // silence — no willStart, no failedToStart, nothing at all. Which is what
    // was happening: the switch is turned on while nobody is talking, so this
    // answered "paused", and the request was dropped on the floor.
    //
    // The state was never the system's business anyway. Whether a rider is
    // transmitting is drawn into the frame, in a flashing indicator that says
    // so far more plainly than the shape of a button.
    guard pictureInPictureController.isPictureInPictureActive else {
      return false
    }
    // Hands-free: the button alters nothing, so it is left showing one thing
    // steadily rather than flickering along with a voice gate it cannot touch.
    guard snapshot.micMode == 0 else { return false }
    return !snapshot.transmitting
  }

  func pictureInPictureController(
    _ pictureInPictureController: AVPictureInPictureController,
    didTransitionToRenderSize newRenderSize: CMVideoDimensions
  ) {
    render()
  }

  /// Required by the protocol, and unreachable: an indefinite time range
  /// leaves the system with no skip buttons to offer.
  func pictureInPictureController(
    _ pictureInPictureController: AVPictureInPictureController,
    skipByInterval skipInterval: CMTime, completion completionHandler: @escaping () -> Void
  ) {
    completionHandler()
  }
}

@available(iOS 15.0, *)
extension PipController: AVPictureInPictureControllerDelegate {
  func pictureInPictureController(
    _ pictureInPictureController: AVPictureInPictureController,
    failedToStartPictureInPictureWithError error: Error
  ) {
    report("Picture in Picture failed to start: \(error.localizedDescription)")
  }

  func pictureInPictureController(
    _ pictureInPictureController: AVPictureInPictureController,
    restoreUserInterfaceForPictureInPictureStopWithCompletionHandler
      completionHandler: @escaping (Bool) -> Void
  ) {
    completionHandler(true)
  }

  func pictureInPictureControllerDidStartPictureInPicture(
    _ pictureInPictureController: AVPictureInPictureController
  ) {
    // Clears whatever the last attempt had to say about itself.
    report(nil)

    // The window had to claim to be playing to get itself opened — iOS will
    // not open one for content it believes is paused. That answer is cached,
    // so without this the window arrives showing a stop button while nobody
    // is transmitting, which is the one thing the button must never get
    // wrong. Asking again now gets the truth.
    pictureInPictureController.invalidatePlaybackState()
  }

  func pictureInPictureControllerDidStopPictureInPicture(
    _ pictureInPictureController: AVPictureInPictureController
  ) {
    // Closing the window only closes the window. Hang-up has a button of its
    // own now, so dismissing this must never drop the call.
    // Deliberately does not dismantle anything. This fires whenever the window
    // closes, and much the commonest reason is the rider coming back to the
    // app — not a decision to be rid of it. Tearing down here disarmed the
    // automatic start, so the window never returned on leaving again, and the
    // setting appeared to have switched itself off.
    //
    // Turning the setting off calls stop() directly, which is where taking it
    // apart belongs.
    //
    // Re-armed on the way out, because the flag does not survive a stop: the
    // window closed by hand stayed closed however many times the app was left
    // afterwards.
    pictureInPictureController.canStartPictureInPictureAutomaticallyFromInline = true

    // And the cached playback state is cleared, which is the other half of why
    // the window would not come back.
    //
    // While it was open and nobody was talking, this correctly reported
    // paused, and the system remembers that. iOS will not open Picture in
    // Picture for content it believes is paused — the same refusal that took
    // five builds to find the first time — so the next attempt was declined in
    // silence. Nothing is paused now: there is no window to pause. Asking the
    // system to look again is what lets the next one open.
    //
    // It is also why turning the setting off and on worked: that builds a new
    // controller, which has nothing cached.
    pictureInPictureController.invalidatePlaybackState()

    channel.invokeMethod("dismissed", arguments: nil)
  }
}
