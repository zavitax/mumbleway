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
        // The rider's setting, passed with the call rather than stored: it
        // decides the session *mode*, and a mode can only be chosen while the
        // session is being configured.
        let args = call.arguments as? [String: Any]
        self.voiceProcessing = args?["voiceProcessing"] as? Bool ?? false
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

    NotificationCenter.default.addObserver(
      self,
      selector: #selector(routeChanged(_:)),
      name: AVAudioSession.routeChangeNotification,
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
      // `inputNumberOfChannels` is not reliable this early. It reports the
      // channels of a route that has *settled*, and immediately after
      // `setActive(true)` the built-in microphone often has not — so it answers
      // 0, Dart reads that as "no microphone", and nothing records.
      //
      // It only ever showed up without a Bluetooth headset, which made it look
      // like a Bluetooth feature rather than a race: negotiating an SCO link
      // takes long enough that by the time this line runs with a headset
      // attached, the route has settled and the count is right.
      //
      // `isInputAvailable` is the question actually being asked -- is there any
      // input hardware on this route -- and it is answered correctly straight
      // away. A zero count with input available means "not known yet", which is
      // what -1 means to the Dart side already; the Android half has always
      // answered -1 for exactly this reason.
      let channels = session.inputNumberOfChannels
      let known = channels > 0 || !session.isInputAvailable
      result([
        "ok": true,
        // Reported so Dart can say "no input available" instead of letting the
        // engine fail with CoreAudio's wording about channel counts.
        "inputChannels": known ? channels : -1,
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

  /// Whether the rider has asked for the platform's own voice processing.
  ///
  /// **On iOS this switch means something different from Android's**, and the
  /// shared name is the rider's, not a claim that the platforms behave alike.
  /// There it stops another app capturing at the same time. Here iOS already
  /// refuses to let two apps record at once, so what it buys instead is
  /// Apple's echo cancellation — and what it costs is in `activate` below.
  private var voiceProcessing = false

  private func activate() throws {
    wantedActive = true
    let session = AVAudioSession.sharedInstance()
    try session.setCategory(
      .playAndRecord,
      // **`.voiceChat` was tried, and it clipped the microphone.**
      //
      // The reasoning for `.default` is that this app does its own echo
      // cancellation and noise suppression, tuned by ear, and a second
      // canceller underneath them produces artefacts nobody can locate. That
      // was challenged by a fair observation: the same music in `Helmet` was
      // suppressed better on Android than on an iPhone, because Android's
      // capture goes through AAudio's default `VOICE_RECOGNITION` preset and
      // gets Google's noise suppression before our chain sees a sample, while
      // `.default` here gave a genuinely raw microphone. The platforms were
      // never doing the same work.
      //
      // So `.voiceChat` went out for a build, and the diagnostic recordings
      // settled it. Speech, measured on the *raw* capture:
      //
      //     before .voiceChat   -11.9 dB   0.27% of samples at full scale
      //     with .voiceChat      -6.0 dB   9.61% of samples at full scale
      //
      // Apple's AGC drives the signal 11 dB hotter and into the ceiling, and
      // a tenth of every voiced sample is clipped. That is audible as
      // distortion, it happens *before* anything of ours runs, and no profile
      // can undo it — which is also why `Helmet` seemed to be the culprit: it
      // is the profile that makes already-clipped speech most obvious.
      //
      // The asymmetry with Android is real and stands. The answer to it is not
      // this.
      //
      // **It is now a setting, off by default, and the paragraph above is the
      // reason it is off rather than a reason nobody may try it.** A rider on
      // a device whose own canceller is better than ours should be able to
      // find that out, and the diagnostics panel's echo-returned figure is how
      // to see which is happening. The clipping measured above is the thing to
      // listen for when it is on: speech that is loud and harsh rather than
      // quiet and muddy.
      mode: voiceProcessing ? .voiceChat : .default,
      options: [
        // Helmet intercoms are HFP devices. Without this they are not offered
        // as an input at all, and the phone quietly records from its own
        // microphone inside a helmet at seventy miles an hour.
        Self.allowHandsFreeBluetooth,
        // `.allowBluetoothA2DP` is deliberately NOT here, and its absence is
        // the whole of a bug that made the app unusable on a Cardo Edge Pro:
        // recording worked until music started and then stopped, even
        // mid-transmission.
        //
        // A2DP is an *output-only* profile. It has no microphone at all.
        // Listing it tells the system it may route this session's output over
        // A2DP, and when music starts the system takes that offer, because
        // A2DP sounds better — and the input goes with it, because there is no
        // input on that profile to keep. Nothing logs an error; the microphone
        // simply stops producing samples.
        //
        // HFP alone forces the headset into the bidirectional hands-free
        // profile and keeps it there, which is what every other voice app on
        // the phone does and why they do not have this fault. The cost is that
        // music plays at telephone bandwidth for the duration of a call, which
        // for an intercom is the right trade and is what a rider expects.
        // Outside a call the session is not active at all — see `prepare` —
        // so nothing is degraded for as long as the app is merely open.
        //
        // Otherwise `playAndRecord` sends output to the earpiece receiver,
        // which is inaudible on a bike and sounds broken indoors.
        .defaultToSpeaker,
        // Lets music keep playing under a call, and — the half that is a bug
        // rather than a preference — stops music *starting* from killing the
        // microphone.
        //
        // Without this the session is exclusive in both directions. Activating
        // it interrupts whatever was playing, and when the rider then starts
        // music the system hands the session to that app and interrupts us:
        // the microphone stops mid-ride, and the only sign is that nobody
        // answers. Google Meet and Yandex Telemost do not behave that way on
        // the same phone, and this option is the difference.
        //
        // It is not the same fault as the A2DP one above, though it presents
        // almost identically. That one was the route being taken away; this is
        // the *session* being taken away. Fixing one did nothing for the other,
        // which is why it survived that fix.
        .mixWithOthers,
        // Music drops while somebody is talking and comes back afterwards.
        // With .mixWithOthers alone a rider hears both at once at full level,
        // and the voice is the half that matters.
        .duckOthers,
      ])
    try session.setActive(true)
    preferHandsFreeInput()
  }

  /// Points the session at the Bluetooth headset's microphone, if there is one.
  ///
  /// Belt and braces over the category options. Those say which routes are
  /// *permitted*; this says which is wanted. Left to itself the system picks
  /// by its own priority order, and it has been known to leave a connected
  /// helmet unit as the output while recording from the phone lying in a
  /// pocket — which sounds exactly like a headset with a broken microphone.
  private func preferHandsFreeInput() {
    let session = AVAudioSession.sharedInstance()
    guard
      let bluetooth = session.availableInputs?.first(where: {
        $0.portType == .bluetoothHFP
      })
    else {
      return
    }
    do {
      try session.setPreferredInput(bluetooth)
    } catch {
      // Not fatal. The route the system chose may well be the right one, and
      // refusing to start a call over a preference is worse than the
      // preference being ignored.
      NSLog("MumbleWay: could not prefer the Bluetooth input: \(error)")
    }
  }

  /// Re-establishes the input after the system moves the route out from under
  /// us.
  ///
  /// Bluetooth routes change for reasons that have nothing to do with this app:
  /// a headset reconnecting, a second device pairing, the system taking a route
  /// for a call of its own. The category options stop the commonest cause — see
  /// `activate` — but they cannot stop all of them, and the failure mode is
  /// always the same and always silent: output continues, input stops, and the
  /// far end hears nothing while everything on screen looks correct.
  ///
  /// So when the route changes and there is a call in progress, ask again for
  /// the microphone rather than trusting what is left.
  @objc private func routeChanged(_ note: Notification) {
    guard wantedActive else { return }
    let reasonValue =
      note.userInfo?[AVAudioSessionRouteChangeReasonKey] as? UInt
      ?? AVAudioSession.RouteChangeReason.unknown.rawValue
    let reason = AVAudioSession.RouteChangeReason(rawValue: reasonValue) ?? .unknown

    switch reason {
    case .oldDeviceUnavailable, .newDeviceAvailable, .override, .categoryChange,
      .routeConfigurationChange:
      // Route-change notifications arrive on whatever thread the system feels
      // like using, and a Flutter method channel may only be touched from the
      // platform thread. Off it, the failure is not a crash at the call site
      // but corruption that surfaces later somewhere unrelated.
      DispatchQueue.main.async { [weak self] in
        guard let self, self.wantedActive else { return }
        let session = AVAudioSession.sharedInstance()
        if session.inputNumberOfChannels == 0 {
          // The input is gone rather than merely different. Re-activating is
          // the only thing that brings it back; setting a preferred input on a
          // session that has none does nothing at all.
          try? self.activate()
        } else {
          self.preferHandsFreeInput()
        }
        self.channel.invokeMethod("routeChanged", arguments: nil)
      }
    default:
      break
    }
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
