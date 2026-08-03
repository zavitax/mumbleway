import AVFoundation
import Flutter

/// The iOS audio session, which has to be told what this app is for before
/// there is any microphone to open.
///
/// An app starts out in `soloAmbient`, a playback-only category, and in that
/// state the system reports **zero** input channels — not an error, just
/// nothing to record with. The engine then fails deep inside CoreAudio with
/// "channel count must be at least 1", which describes the symptom precisely
/// and says nothing about the cause. Neither macOS nor Windows has a session
/// to configure, so this is invisible everywhere except a real device.
///
/// Setting the category is also what makes Bluetooth intercoms reachable at
/// all, which for this app is not a nicety: a helmet headset is the normal
/// output, not an accessory.
final class AudioSession {
  private let channel: FlutterMethodChannel

  init(messenger: FlutterBinaryMessenger) {
    channel = FlutterMethodChannel(name: "mumbleway/audioSession", binaryMessenger: messenger)
    channel.setMethodCallHandler { [weak self] call, result in
      guard let self else {
        result(FlutterError(code: "gone", message: "The app is shutting down.", details: nil))
        return
      }
      switch call.method {
      case "prepare":
        self.prepare(result)
      default:
        result(FlutterMethodNotImplemented)
      }
    }

    NotificationCenter.default.addObserver(
      self,
      selector: #selector(interrupted(_:)),
      name: AVAudioSession.interruptionNotification,
      object: nil)
  }

  deinit {
    NotificationCenter.default.removeObserver(self)
  }

  /// Asks for the microphone, configures the session, and reports what the
  /// hardware then offers.
  ///
  /// Returns rather than throws on refusal: a declined microphone is an answer,
  /// not a failure, and the app has something useful to say about it.
  private func prepare(_ result: @escaping FlutterResult) {
    requestPermission { [weak self] granted in
      guard let self else { return }
      guard granted else {
        result(["granted": false, "inputChannels": 0, "sampleRate": 0.0])
        return
      }
      do {
        try self.activate()
        let session = AVAudioSession.sharedInstance()
        result([
          "granted": true,
          // Reported back so Dart can say "no input available" instead of
          // letting the engine fail with CoreAudio's wording.
          "inputChannels": session.inputNumberOfChannels,
          "sampleRate": session.sampleRate,
        ])
      } catch {
        result(
          FlutterError(
            code: "session", message: error.localizedDescription, details: nil))
      }
    }
  }

  private func activate() throws {
    let session = AVAudioSession.sharedInstance()
    try session.setCategory(
      .playAndRecord,
      // `.voiceChat` would be the obvious mode for a voice app, and it brings
      // the system's own voice processing with it. This app already does echo
      // cancellation and noise suppression itself, with settings the user has
      // tuned by ear, and a second canceller underneath them is exactly the
      // kind of thing that produces artefacts nobody can locate. `.default`
      // leaves the pipeline as the only thing processing the signal.
      mode: .default,
      options: [
        // Helmet intercoms are HFP devices. Without this they are not offered
        // as an input at all, and the phone quietly records from its own
        // microphone inside a helmet at seventy miles an hour.
        .allowBluetooth,
        .allowBluetoothA2DP,
        // Otherwise `playAndRecord` sends output to the earpiece receiver,
        // which is inaudible on a bike and sounds broken indoors.
        .defaultToSpeaker,
      ])
    try session.setActive(true)
  }

  private func requestPermission(_ done: @escaping (Bool) -> Void) {
    // The call moved to AVAudioApplication in iOS 17; the old one still works
    // but warns, and will eventually stop.
    if #available(iOS 17.0, *) {
      AVAudioApplication.requestRecordPermission { granted in
        DispatchQueue.main.async { done(granted) }
      }
    } else {
      AVAudioSession.sharedInstance().requestRecordPermission { granted in
        DispatchQueue.main.async { done(granted) }
      }
    }
  }

  /// Puts the session back after a phone call or a Siri request.
  ///
  /// iOS deactivates the session for the duration and does not restore it, so
  /// without this the app survives its first incoming call as a window that
  /// looks connected and carries no audio in either direction.
  @objc private func interrupted(_ note: Notification) {
    guard let raw = note.userInfo?[AVAudioSessionInterruptionTypeKey] as? UInt,
      let type = AVAudioSession.InterruptionType(rawValue: raw)
    else { return }

    switch type {
    case .began:
      // Nothing to do: the system has already taken the session away.
      break
    case .ended:
      // Reactivating is right even when `shouldResume` is absent. That flag is
      // about resuming *playback*, and this is a conversation the user is in
      // the middle of rather than a track they were listening to.
      try? activate()
      channel.invokeMethod("resumed", arguments: nil)
    @unknown default:
      break
    }
  }
}
