import AVFoundation
import AVKit
import CoreMedia
import Flutter
import UIKit

/// The call state the floating window draws. Mirrored from Dart; this side
/// never decides any of it, so that the window and the app can never disagree.
struct CallSnapshot {
  var names: [String] = []
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
  /// Frame at which an armed hang-up stops counting, or nil when not armed.
  private var hangupArmedUntil: UInt64?

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
    hangupArmedUntil = nil

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
    // The system caches the answer above and only asks again when told to.
    if next.transmitting != wasTransmitting {
      pipController?.invalidatePlaybackState()
    }
    render()
  }

  private var hangupArmed: Bool {
    guard let hangupArmedUntil else { return false }
    return frame < hangupArmedUntil
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

    drawOnAir(in: bounds)
    drawTitle(in: bounds)
    drawBadges(in: bounds)
    drawMeter(in: bounds)
    drawSpeakers(in: bounds)
  }

  /// The transmit indicator: a filled ring that blinks while the microphone is
  /// actually going out, the way a studio on-air light does. Blinking rather
  /// than merely turning red because a steady colour is easy to lose track of,
  /// and leaving a channel keyed open by accident is the failure that matters.
  private func drawOnAir(in bounds: CGRect) {
    let centre = CGPoint(x: bounds.midX, y: 92)
    let radius: CGFloat = 30

    let colour: UIColor
    if !snapshot.connected {
      colour = UIColor(white: 0.45, alpha: 1)
    } else if snapshot.transmitting {
      colour = UIColor(red: 0.94, green: 0.24, blue: 0.24, alpha: 1)
    } else if snapshot.speaking {
      colour = UIColor(red: 0.25, green: 0.78, blue: 0.45, alpha: 1)
    } else {
      colour = UIColor(white: 0.55, alpha: 1)
    }

    let lit = !snapshot.transmitting || onAirVisible

    if snapshot.transmitting {
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

    if snapshot.transmitting {
      drawText(
        "ON AIR", in: CGRect(x: 0, y: centre.y - 9, width: bounds.width, height: 20),
        size: 14, weight: .heavy, colour: .white, alignment: .center)
    } else {
      drawText(
        "\u{1F3A4}", in: CGRect(x: 0, y: centre.y - 14, width: bounds.width, height: 28),
        size: 20, weight: .bold, colour: .white, alignment: .center)
    }
  }

  private func drawTitle(in bounds: CGRect) {
    // The arming prompt outranks everything: it is the only state here with a
    // deadline, and the frame is the only place it can be said.
    if hangupArmed {
      drawText(
        "Skip forward again to hang up",
        in: CGRect(x: 12, y: 136, width: bounds.width - 24, height: 26),
        size: 17, weight: .heavy,
        colour: UIColor(red: 1.0, green: 0.42, blue: 0.38, alpha: 1),
        alignment: .center)
      return
    }

    let text: String
    if !snapshot.connected {
      text = "Not connected"
    } else if snapshot.transmitting {
      text = "Talking"
    } else if snapshot.deafened {
      text = "Deafened"
    } else if snapshot.muted {
      text = "Muted"
    } else {
      text = "Listening"
    }
    drawText(
      text, in: CGRect(x: 12, y: 136, width: bounds.width - 24, height: 26),
      size: 19, weight: .semibold, colour: .white, alignment: .center)
  }

  private func drawBadges(in bounds: CGRect) {
    var labels: [String] = []
    if snapshot.muted { labels.append("MUTED") }
    if snapshot.deafened { labels.append("DEAFENED") }
    guard !labels.isEmpty else { return }
    drawText(
      labels.joined(separator: "  \u{00B7}  "),
      in: CGRect(x: 12, y: 162, width: bounds.width - 24, height: 18),
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
    let track = CGRect(x: 44, y: 194, width: bounds.width - 88, height: 14)
    let radius = track.height / 2

    UIColor(white: 1, alpha: 0.12).setFill()
    UIBezierPath(roundedRect: track, cornerRadius: radius).fill()

    let level = CGFloat(max(0, min(1, snapshot.level)))
    if level > 0.001 {
      // Green below the threshold, red above it: the colour says whether this
      // level would open the gate, which is the only question the meter is
      // being asked.
      let open = snapshot.level >= snapshot.threshold
      let fill = open
        ? UIColor(red: 0.36, green: 0.85, blue: 0.45, alpha: 1)
        : UIColor(white: 0.72, alpha: 1)
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
      "noise", in: CGRect(x: 4, y: 193, width: 38, height: 16),
      size: 9, weight: .semibold, colour: UIColor(white: 1, alpha: 0.5),
      alignment: .right)
    drawText(
      "open", in: CGRect(x: bounds.width - 42, y: 193, width: 38, height: 16),
      size: 9, weight: .semibold, colour: UIColor(white: 1, alpha: 0.5),
      alignment: .left)

    // The system controls carry no labels and cannot be given any, so what
    // they do is spelled out here — the only surface this window has.
    drawText(
      "\u{25C0} mute      \u{25B6}\u{2016} talk      \u{25B6}\u{25B6} hang up",
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

  private func drawSpeakers(in bounds: CGRect) {
    // The window is small and the list is glanced at, not read.
    let visible = snapshot.names.prefix(2)
    let text: String
    if visible.isEmpty {
      text = snapshot.connected ? "\u{2014}" : ""
    } else if snapshot.names.count > visible.count {
      text = visible.joined(separator: ", ") + " +\(snapshot.names.count - visible.count)"
    } else {
      text = visible.joined(separator: ", ")
    }

    drawText(
      text, in: CGRect(x: 12, y: 224, width: bounds.width - 24, height: 40),
      size: 16, weight: .medium,
      colour: UIColor(red: 0.55, green: 0.83, blue: 1.0, alpha: 1),
      alignment: .center)
  }

  private func drawText(
    _ text: String, in rect: CGRect, size: CGFloat, weight: UIFont.Weight,
    colour: UIColor, alignment: NSTextAlignment
  ) {
    guard !text.isEmpty else { return }
    let paragraph = NSMutableParagraphStyle()
    paragraph.alignment = alignment
    paragraph.lineBreakMode = .byTruncatingTail
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
    channel.invokeMethod("setTransmitting", arguments: playing)
  }

  /// A finite range is what makes the system offer the two skip buttons; an
  /// infinite one marks the stream live and leaves only play/pause. The values
  /// are never used for anything else — nothing here is seekable.
  func pictureInPictureControllerTimeRangeForPlayback(
    _ pictureInPictureController: AVPictureInPictureController
  ) -> CMTimeRange {
    CMTimeRange(
      start: .zero, duration: CMTime(seconds: 3600, preferredTimescale: 600))
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
    return !snapshot.transmitting
  }

  func pictureInPictureController(
    _ pictureInPictureController: AVPictureInPictureController,
    didTransitionToRenderSize newRenderSize: CMVideoDimensions
  ) {
    render()
  }

  func pictureInPictureController(
    _ pictureInPictureController: AVPictureInPictureController,
    skipByInterval skipInterval: CMTime, completion completionHandler: @escaping () -> Void
  ) {
    // Only the direction carries meaning; the interval is ignored.
    if skipInterval.seconds < 0 {
      channel.invokeMethod("toggleMute", arguments: nil)
    } else if hangupArmed {
      hangupArmedUntil = nil
      channel.invokeMethod("hangup", arguments: nil)
    } else {
      // Roughly five seconds at ten frames a second: long enough to read the
      // prompt and press again, short enough that a stray tap does not leave
      // the call one press from ending for the rest of the ride.
      hangupArmedUntil = frame &+ 50
    }
    render()
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
  }

  func pictureInPictureControllerDidStopPictureInPicture(
    _ pictureInPictureController: AVPictureInPictureController
  ) {
    // Closing the window only closes the window. Hang-up has a button of its
    // own now, so dismissing this must never drop the call.
    // Dismissed from the system chrome rather than from settings, and it has
    // to dismantle just as thoroughly: otherwise the automatic-start flag is
    // still armed, and the window the user just closed comes back the next
    // time they leave the app.
    stop()
    channel.invokeMethod("dismissed", arguments: nil)
  }
}
