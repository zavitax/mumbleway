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
/// relabel them, so they are spent on the three actions worth having a glove
/// on a handlebar reach for:
///
///   * play/pause     → talk
///   * skip backward  → mute
///   * skip forward   → deafen
///
/// Hang-up does not fit. The close button can be wired to it, but only behind
/// an explicit setting: tidying the window away is not a thing anyone expects
/// to drop their call, and the mistake is silent until someone talks into a
/// connection that is no longer there.
@available(iOS 15.0, *)
final class PipController: NSObject {
  private let channel: FlutterMethodChannel
  private weak var hostView: UIView?

  private var displayLayer: AVSampleBufferDisplayLayer?
  private var pipController: AVPictureInPictureController?
  private var pixelBufferPool: CVPixelBufferPool?
  private var renderTimer: Timer?

  private var snapshot = CallSnapshot()
  private var closeHangsUp = false
  /// Set when the user taps restore, so a close can be told from a return.
  private var restoring = false

  /// 16:9 at a size that stays legible when the system shrinks the window.
  private static let frameSize = CGSize(width: 480, height: 270)

  init(channel: FlutterMethodChannel, hostView: UIView) {
    self.channel = channel
    self.hostView = hostView
    super.init()
  }

  deinit {
    renderTimer?.invalidate()
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

      // The layer has to be in the hierarchy and not hidden, or the system
      // refuses to start. It sits behind Flutter's opaque view, so it takes
      // part in layout without ever being seen.
      let carrier = UIView(
        frame: CGRect(origin: .zero, size: Self.frameSize))
      carrier.isUserInteractionEnabled = false
      layer.frame = carrier.bounds
      carrier.layer.addSublayer(layer)
      hostView.insertSubview(carrier, at: 0)

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

    // Starting while already backgrounded is rejected; automatic start covers
    // that case, so a failure here is not fatal.
    if let pipController, !pipController.isPictureInPictureActive {
      pipController.startPictureInPicture()
    }
  }

  func stop() {
    renderTimer?.invalidate()
    renderTimer = nil
    if let pipController, pipController.isPictureInPictureActive {
      pipController.stopPictureInPicture()
    }
  }

  func update(_ next: CallSnapshot) {
    snapshot = next
    render()
  }

  func setCloseHangsUp(_ value: Bool) {
    closeHangsUp = value
  }

  // MARK: - Frame production

  /// A still image can leave the window looking stalled, and the render is a
  /// few hundred pixels of flat colour, so it is cheaper to keep feeding it
  /// than to reason about when the system needs a fresh frame.
  private func startRenderTimer() {
    renderTimer?.invalidate()
    let timer = Timer(timeInterval: 1.0, repeats: true) { [weak self] _ in
      self?.render()
    }
    RunLoop.main.add(timer, forMode: .common)
    renderTimer = timer
  }

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

    drawStatusDot(in: bounds)
    drawTitle(in: bounds)
    drawBadges(in: bounds)
    drawSpeakers(in: bounds)
  }

  private func drawStatusDot(in bounds: CGRect) {
    let colour: UIColor
    if !snapshot.connected {
      colour = UIColor(white: 0.45, alpha: 1)
    } else if snapshot.transmitting {
      colour = UIColor(red: 0.94, green: 0.27, blue: 0.27, alpha: 1)
    } else {
      colour = UIColor(red: 0.25, green: 0.78, blue: 0.45, alpha: 1)
    }

    let centre = CGPoint(x: bounds.midX, y: 96)
    let radius: CGFloat = snapshot.transmitting ? 34 : 28

    colour.withAlphaComponent(0.22).setFill()
    UIBezierPath(arcCenter: centre, radius: radius + 14, startAngle: 0, endAngle: .pi * 2, clockwise: true).fill()
    colour.setFill()
    UIBezierPath(arcCenter: centre, radius: radius, startAngle: 0, endAngle: .pi * 2, clockwise: true).fill()

    let glyph = snapshot.transmitting ? "\u{25CF}" : "\u{1F3A4}"
    drawText(
      glyph, in: CGRect(x: 0, y: centre.y - 16, width: bounds.width, height: 32),
      size: 22, weight: .bold, colour: .white, alignment: .center)
  }

  private func drawTitle(in bounds: CGRect) {
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
      text, in: CGRect(x: 12, y: 148, width: bounds.width - 24, height: 26),
      size: 20, weight: .semibold, colour: .white, alignment: .center)
  }

  private func drawBadges(in bounds: CGRect) {
    var labels: [String] = []
    if snapshot.muted { labels.append("MUTED") }
    if snapshot.deafened { labels.append("DEAFENED") }
    guard !labels.isEmpty else { return }
    drawText(
      labels.joined(separator: "  \u{00B7}  "),
      in: CGRect(x: 12, y: 178, width: bounds.width - 24, height: 20),
      size: 13, weight: .bold,
      colour: UIColor(red: 0.98, green: 0.72, blue: 0.35, alpha: 1),
      alignment: .center)
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
      text, in: CGRect(x: 12, y: 214, width: bounds.width - 24, height: 44),
      size: 17, weight: .medium,
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
    !snapshot.transmitting
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
    // Only the direction carries meaning. Backward mutes, forward deafens —
    // the wider action is the one further forward.
    channel.invokeMethod(skipInterval.seconds < 0 ? "toggleMute" : "toggleDeafen", arguments: nil)
    completionHandler()
  }
}

@available(iOS 15.0, *)
extension PipController: AVPictureInPictureControllerDelegate {
  func pictureInPictureController(
    _ pictureInPictureController: AVPictureInPictureController,
    failedToStartPictureInPictureWithError error: Error
  ) {
    NSLog("MumbleWay: Picture in Picture failed to start: \(error.localizedDescription)")
  }

  func pictureInPictureController(
    _ pictureInPictureController: AVPictureInPictureController,
    restoreUserInterfaceForPictureInPictureStopWithCompletionHandler
      completionHandler: @escaping (Bool) -> Void
  ) {
    restoring = true
    completionHandler(true)
  }

  func pictureInPictureControllerDidStopPictureInPicture(
    _ pictureInPictureController: AVPictureInPictureController
  ) {
    let wasRestore = restoring
    restoring = false

    // Only a genuine dismissal counts, and only when the user asked for it to
    // mean hang up. Coming back into the app must never drop the call.
    if closeHangsUp && !wasRestore {
      channel.invokeMethod("hangup", arguments: nil)
    }
    channel.invokeMethod("dismissed", arguments: nil)
  }
}
