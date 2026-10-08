import CoreMotion
import Flutter

/// The phone's own motion, for tap detection and the recorder's third track.
///
/// The counterpart to `MotionSensors.kt`, and the asymmetry between them is a
/// finding rather than a detail: **iOS tops out dependably at 100 Hz**, where
/// Android gives 200–500. A tap's mechanical transient runs 20–80 ms, so at
/// 100 Hz it is three to eight samples and its onset is a step rather than a
/// slope — there is no shape to analyse, only a jump in magnitude. Worse, a
/// twin at 4000 rpm fires around 33–67 Hz, so with Nyquist at 50 the engine's
/// harmonics fold back into the band as content that moves with the throttle
/// and correlates with nothing observable.
///
/// Which is why the pattern — N matched impulses on an even beat — has to carry
/// the discrimination here rather than the shape of any one of them, and why
/// whether tap-to-start is viable on iOS at all is the first question the rig
/// should answer.
///
/// `deviceMotion` rather than the raw accelerometer, because it is the fused
/// product: `userAcceleration` already has gravity removed and `gravity` is the
/// part that was taken out. The pair is what separates a tap from road shock —
/// a hand reaching to a thigh moves across the world vertical where suspension
/// travel is along it — and Apple's fusion is better than anything worth
/// writing here.
final class MotionSensors {
  private let channel: FlutterMethodChannel
  private let motion = CMMotionManager()
  private let queue = OperationQueue()

  init(messenger: FlutterBinaryMessenger) {
    channel = FlutterMethodChannel(name: "mumbleway/motion", binaryMessenger: messenger)
    queue.name = "mumbleway.motion"
    queue.maxConcurrentOperationCount = 1

    channel.setMethodCallHandler { [weak self] call, result in
      guard let self else {
        result(FlutterError(code: "gone", message: "The app is shutting down.", details: nil))
        return
      }
      switch call.method {
      case "available":
        result(self.motion.isDeviceMotionAvailable)
      case "start":
        self.start()
        result(true)
      case "stop":
        self.stop()
        result(true)
      default:
        result(FlutterMethodNotImplemented)
      }
    }
  }

  private func start() {
    guard motion.isDeviceMotionAvailable, !motion.isDeviceMotionActive else { return }
    // 100 Hz is the ceiling in practice; asking for more is harmless and
    // getting less is the thing to notice, which is why the interval is stated
    // rather than left at the default.
    motion.deviceMotionUpdateInterval = 1.0 / 100.0
    motion.startDeviceMotionUpdates(to: queue) { [weak self] data, _ in
      guard let self, let d = data else { return }
      let a = d.userAcceleration
      let g = d.gravity
      let r = d.rotationRate

      // `CMLogItem.timestamp` is seconds since boot, as a Double. Converted to
      // nanoseconds to share one column with Android's, and passed through
      // otherwise unaltered: the recorder writes it beside an arrival stamp of
      // its own precisely so the delay between the two can be measured rather
      // than assumed.
      let stamp = Int64(d.timestamp * 1_000_000_000.0)

      // Apple reports acceleration in g, Android in m/s². Scaled here so the
      // motion track carries one unit — a detector tuned on a corpus of both
      // cannot have a threshold that means different things per platform, and
      // nothing downstream would reveal the mix.
      let gToMs2 = 9.806_65
      DispatchQueue.main.async {
        self.channel.invokeMethod(
          "sample",
          arguments: [
            "t": stamp,
            "a": [a.x * gToMs2, a.y * gToMs2, a.z * gToMs2],
            "g": [g.x * gToMs2, g.y * gToMs2, g.z * gToMs2],
            "r": [r.x, r.y, r.z],
          ])
      }
    }
  }

  private func stop() {
    guard motion.isDeviceMotionActive else { return }
    motion.stopDeviceMotionUpdates()
  }
}
