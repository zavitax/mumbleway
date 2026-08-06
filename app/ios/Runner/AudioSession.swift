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
      case "activate":
        self.activateForCall(result)
      case "deactivate":
        self.deactivate(result)
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

  /// Asks for the microphone and settles the category, without going live.
  ///
  /// Deliberately stops short of activating. An active `playAndRecord` session
  /// is not a passive declaration of intent: it lights the orange recording
  /// indicator, and it lets the system route a Bluetooth headset over the
  /// hands-free profile — so a podcast playing through a helmet intercom drops
  /// to telephone bandwidth for as long as this app is merely open. Both used
  /// to be true from launch until the app was killed.
  ///
  /// The channel count is no longer reported from here, because there is no
  /// session live to ask; it comes back from [activateForCall] instead, which
  /// is where the answer can still be acted on.
  ///
  /// Returns rather than throws on refusal: a declined microphone is an answer,
  /// not a failure, and the app has something useful to say about it.
  private func prepare(_ result: @escaping FlutterResult) {
    requestPermission { granted in
      guard granted else {
        result(["granted": false, "inputChannels": 0, "sampleRate": 0.0])
        return
      }
      // Negative rather than zero: "not asked" and "asked, and there is
      // nothing to record with" need different things from the user, and
      // treating the first as the second refuses to start over a question
      // nobody has put yet.
      result(["granted": true, "inputChannels": -1, "sampleRate": 0.0])
    }
  }

  /// Takes the session live for a conversation.
  ///
  /// Called as a call is being set up rather than as the first word is spoken.
  /// Activation is not instant — a Bluetooth headset has an SCO link to
  /// negotiate — and it can be refused outright by a phone call or a voice
  /// memo holding a session that will not mix. Both are worth several seconds
  /// of a connect and neither is worth clipping the start of a sentence.
  private func activateForCall(_ result: @escaping FlutterResult) {
    do {
      try activate()
      let session = AVAudioSession.sharedInstance()
      result([
        "ok": true,
        // Reported so Dart can say "no input available" instead of letting the
        // engine fail with CoreAudio's wording about channel counts.
        "inputChannels": session.inputNumberOfChannels,
        "sampleRate": session.sampleRate,
      ])
    } catch {
      // An answer, not a crash. Something else holds the microphone, and the
      // rider needs to be told which of their own actions to undo.
      result([
        "ok": false,
        "inputChannels": 0,
        "sampleRate": 0.0,
        "error": error.localizedDescription,
      ])
    }
  }

  /// Hands the session back when there is no longer a call.
  ///
  /// `notifyOthersOnDeactivation` is what lets whatever was playing before
  /// resume, and lets a headset fall back off the hands-free profile — which
  /// is most of the point: the helmet unit's own battery lasts longer, and
  /// music stops sounding like a telephone.
  private func deactivate(_ result: @escaping FlutterResult) {
    wantedActive = false
    do {
      try AVAudioSession.sharedInstance().setActive(
        false, options: [.notifyOthersOnDeactivation])
      result(true)
    } catch {
      // Not worth reporting upwards. The session being hard to put down costs
      // battery, not function, and there is nothing for the rider to do about
      // it — the next call will activate over the top regardless.
      NSLog("MumbleWay: could not deactivate the audio session: \(error)")
      result(false)
    }
  }

  /// Whether a call is in progress, so that an interruption ending knows
  /// whether the session is meant to come back at all.
  private var wantedActive = false

  /// Hands-free Bluetooth input, under whichever name the SDK in use calls it.
  ///
  /// The iOS 26 SDK renamed `.allowBluetooth` to `.allowBluetoothHFP` and
  /// deprecated the old spelling, so building against it warns. The two are the
  /// same flag — both are raw value `0x4` — and the new one is annotated
  /// `API_AVAILABLE(ios(1.0))`, so this changes nothing at runtime and needs no
  /// `#available` check. Only the symbol is new.
  ///
  /// Which means the guard has to be on the *compiler*, not the OS: an older
  /// Xcode has no `.allowBluetoothHFP` to refer to, and this project is built
  /// by CI on `macos-latest`, whose Xcode moves without asking. Swift 6.2 is
  /// what Xcode 26 ships, so that is the line.
  private static let allowHandsFreeBluetooth: AVAudioSession.CategoryOptions = {
    #if compiler(>=6.2)
      return .allowBluetoothHFP
    #else
      return .allowBluetooth
    #endif
  }()

  private func activate() throws {
    wantedActive = true
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
        Self.allowHandsFreeBluetooth,
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
      // Only if there was a conversation to come back to. The session is no
      // longer live for the whole life of the app, so an interruption ending
      // while nothing is connected must leave it down rather than quietly
      // taking the microphone back — which is the state this app spent its
      // first year in.
      guard wantedActive else { return }

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
