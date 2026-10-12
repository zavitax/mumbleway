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
      // No `setOthersTalking` here, and `setCapturingCategory` says why: on
      // this platform the duck cannot be changed on a live session without
      // taking another app's playback down with it. Dart sends the signal to
      // Android only.
      case "startCapture":
        self.startCapture(result)
      case "stopCapture":
        self.stopCapture(result)
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

  /// Which microphone the system actually put us on.
  ///
  /// **iOS answered 0 — "it did not say" — for the whole of this app's life,
  /// and that is the half of the route table that was never filled in.** The
  /// numbers are `Recorded::route`'s and are a wire format: every recording
  /// already on a rider's phone is read with that meaning, so they must not be
  /// renumbered and a new route takes the next free one.
  ///
  /// Read from `currentRoute` rather than from what was *asked* for.
  /// `preferHandsFreeInput` states a preference and the system is free to
  /// ignore it, which is exactly the case this value exists to catch — a
  /// connected helmet unit on output while the recording comes from a phone in
  /// a pocket.
  /// A route-change reason as something a reader can act on.
  static func reasonName(_ reason: AVAudioSession.RouteChangeReason) -> String {
    switch reason {
    case .newDeviceAvailable: return "new device"
    case .oldDeviceUnavailable: return "device gone"
    case .categoryChange: return "category"
    case .override: return "override"
    case .wakeFromSleep: return "wake"
    case .noSuitableRouteForCategory: return "no route"
    case .routeConfigurationChange: return "reconfigured"
    case .unknown: return "unknown"
    @unknown default: return "other"
    }
  }

  /// Whether a headset is attached at all, whichever microphone is live.
  ///
  /// **Not derivable from [`routeCode`], and that is the whole reason this
  /// exists.** The listening session deliberately prefers the built-in
  /// microphone and does not offer hands-free as an input, so a rider in
  /// pocket mode with a helmet on their head reports route 1 — the phone’s
  /// own. Deriving "is a helmet connected" from that would answer no for the
  /// entire state the answer matters in.
  ///
  /// The **outputs** are the half that knows while listening: the music is
  /// going to the helmet over A2DP, so the output port names it. The inputs
  /// are the half that knows while capturing, and a wired or USB microphone
  /// can show up on either. Both are consulted; either is enough.
  static func headsetPresent() -> Bool {
    let session = AVAudioSession.sharedInstance()
    for port in session.currentRoute.outputs {
      switch port.portType {
      case .bluetoothA2DP, .bluetoothHFP, .bluetoothLE, .headphones, .usbAudio,
        .carAudio, .lineOut:
        return true
      default: continue
      }
    }
    for port in session.availableInputs ?? [] {
      switch port.portType {
      case .bluetoothHFP, .headsetMic, .lineIn, .usbAudio, .carAudio:
        return true
      default: continue
      }
    }
    return false
  }

  static func routeCode() -> Int {
    let session = AVAudioSession.sharedInstance()
    guard let port = session.currentRoute.inputs.first?.portType else { return 0 }
    switch port {
    case .builtInMic: return 1
    // `.lineIn` beside `.headsetMic` because a wired adapter reports either
    // depending on the dongle, and both are the same fact to a rider.
    case .headsetMic, .lineIn: return 2
    case .bluetoothHFP: return 3
    case .usbAudio: return 4
    default: return 5
    }
  }

  /// Takes the session live for a conversation.
  ///
  /// Called as a call is being set up rather than as the first word is spoken.
  /// Activation is not instant — a Bluetooth headset has an SCO link to
  /// negotiate — and it can be refused outright by a phone call or a voice
  /// memo holding a session that will not mix. Both are worth several seconds
  /// of a connect and neither is worth clipping the start of a sentence.
  /// **This always takes capture**, and the branch that stopped at listening
  /// is gone.
  ///
  /// It used to take a `captureOnDemand` and, when set, activate a listening
  /// session with no input — so the single most consequential fact about a
  /// call was decided by an argument at connect time. Pocket mode closes the
  /// microphone afterwards, through `stopCapture`, which is reachable, named
  /// for what it does, and cannot be reached by accident. `activateListening`
  /// is still here and still used: it is what `stopCapture` hands the session
  /// back to.
  private func activateForCall(_ result: @escaping FlutterResult) {
    captureResult(result)
  }

  /// Takes the hands-free profile, which is what makes a microphone exist.
  private func startCapture(_ result: @escaping FlutterResult) {
    captureResult(result)
  }

  /// Gives the profile back and keeps the session.
  ///
  /// **A category change alone may not be enough**, and which it is has not
  /// been measured on a real headset yet — `deactivate`'s own comment says
  /// `.notifyOthersOnDeactivation` is "what lets a headset fall back off the
  /// hands-free profile", and that is on a full deactivation. So the session is
  /// put down with that option and brought straight back up as `.playback`,
  /// which is the version that cannot quietly leave a headset on HFP. The cost
  /// is a brief gap in output, which is the thing this feature already accepts.
  private func stopCapture(_ result: @escaping FlutterResult) {
    do {
      let session = AVAudioSession.sharedInstance()
      // **Only when there is a profile to hand back.** See
      // `CaptureReconciler.release`: deactivating stops the audio unit and
      // interrupts the stream the engine has open on it, and on a route with
      // no Bluetooth in it there is no hands-free profile that needed
      // releasing. That cost was being paid on every tap-to-stop, including
      // on the phone's own speaker where it buys nothing at all.
      let bluetooth = session.currentRoute.outputs.contains {
        $0.portType == .bluetoothHFP || $0.portType == .bluetoothA2DP
          || $0.portType == .bluetoothLE
      }
      let rebuilt =
        CaptureReconciler.release(routeHasBluetooth: bluetooth)
        == .deactivateAndReactivate
      if rebuilt {
        try? session.setActive(false, options: [.notifyOthersOnDeactivation])
      }
      try activateListening()
      result([
        "ok": true,
        "inputChannels": 0,
        "sampleRate": session.sampleRate,
        "route": Self.routeCode(),
        "headset": Self.headsetPresent(),
        // **Reported because the comment above is true and nothing acted on
        // it.** Deactivating stops the audio unit and interrupts the streams
        // the engine has open — *both* of them — and the engine was never
        // told, so a rider stopping capture on a Bluetooth route was left with
        // a dead microphone and a dead speaker. They reported it as capture
        // stopping and then nothing at all: no cue, and taps heard on neither
        // microphone.
        //
        // This is commanded, not observed: we deactivated, so we know. The
        // route code beside it is read a line after `activateListening`, when
        // the route has by definition not settled, which is why keying the
        // rebuild on *that* changing was not enough.
        "sessionRebuilt": rebuilt,
      ])
    } catch {
      result([
        "ok": false,
        "inputChannels": 0,
        "sampleRate": 0.0,
        "error": error.localizedDescription,
      ])
    }
  }

  private func captureResult(_ result: @escaping FlutterResult) {
    do {
      try activateCapturing()
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
      channel.invokeMethod(
        "sessionEvent",
        arguments:
          "startCapture: in \(session.currentRoute.inputs.first?.portType.rawValue ?? "none"), "
          + "preferred \(session.preferredInput?.portType.rawValue ?? "none"), "
          + "ch \(channels), rate \(session.sampleRate)")
      result([
        "ok": true,
        // Reported so Dart can say "no input available" instead of letting the
        // engine fail with CoreAudio's wording about channel counts.
        "inputChannels": known ? channels : -1,
        "sampleRate": session.sampleRate,
        "route": Self.routeCode(),
        "headset": Self.headsetPresent(),
        // **Taking the hands-free profile moves the input device**, from the
        // phone's own microphone to the helmet's, so the engine has to open
        // the new one — its stream is bound to a device, and the rate it
        // resamples from was read when that stream was built. A rider
        // disarmed pocket mode and reported that the microphone did not go
        // back to the helmet, which is this: the session moved and the engine
        // was never told to follow.
        "sessionRebuilt": true,
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


  /// Whether a retry of [preferHandsFreeInput] is already scheduled.
  ///
  /// One at a time, which is what keeps a retry from becoming the fourth
  /// self-sustaining loop out of this file.
  private var handsFreeRetryPending = false

  /// Whether the hands-free profile is currently held.
  ///
  /// Decides which configuration a route change puts back, and nothing else.
  private var capturing = false

  /// The listening session: output only, and A2DP allowed to carry it.
  ///
  /// **`.playback`, not `.playAndRecord` offering A2DP.** The documented fault
  /// — see the comment in `activateCapturing` — is a `playAndRecord` session
  /// that lists A2DP and then loses an input it wanted. Here there is no input
  /// to lose, so the fault has no surface at all; choosing `.playAndRecord` and
  /// hoping would be walking back into it for no gain.
  ///
  /// `.mixWithOthers` carries over, so music still plays — at full bandwidth
  /// now, which is the whole point. **`.duckOthers` does not**, and that is
  /// deliberate: see the options below, where a rider's report that their
  /// music was quiet for the whole of pocket mode is written up.
  private func activateListening() throws {
    wantedActive = true
    capturing = false
    let session = AVAudioSession.sharedInstance()
    // **`.playAndRecord` with A2DP, not `.playback`, and the microphone is
    // the point.**
    //
    // The listening state used to have no input at all, deliberately: there
    // was nothing to record with and so nothing to lose. The tap gesture
    // changed what this state is for. The accelerometer could not feel a tap
    // at the 100 Hz iOS delivers — four samples where the microphone gives
    // forty — so the gesture is heard instead, and while listening the
    // phone's own microphone is the only one there is.
    //
    // **The risk is written down and taken knowingly.** `CLAUDE.md` records
    // that offering A2DP to a session that wants input lets iOS take the
    // input away when music starts, reported once as "recording only works
    // when music is not playing". That is still true, and the consequence
    // here is bounded: the gesture stops being felt, which the panel's frame
    // counter shows, rather than a call losing its microphone.
    //
    // No HFP option, which is the part that preserves the whole feature:
    // output stays on A2DP at full bandwidth and the input comes from the
    // phone, so a rider's music is untouched while they are only listening.
    //
    // **And a live input is what iOS does not suspend.** The gesture dying a
    // minute after the screen locked was the app being suspended; an app
    // holding a microphone is one the system keeps running, which is a larger
    // problem solved by the same change.
    // **Mixing, and ducking only if the rider asked for it.** One found their
    // music playing at full quality but much quieter throughout pocket mode,
    // and `.duckOthers` was why: Apple's documentation is exact — "ducking
    // begins when you activate your app's audio session and ends when you
    // deactivate the session". It is not tied to this app playing anything, so
    // a session held open to hear taps lowered the music for the whole ride.
    //
    // **`.mixWithOthers` has to stay explicit for that to be safe.** The same
    // page warns that "if you clear this option, activating your session
    // interrupts other audio sessions", because `.duckOthers` implies
    // `.mixWithOthers` — so dropping the duck while leaning on that
    // implication would stop the music outright instead of merely leaving it
    // loud. It is listed in its own right, so it does not.
    //
    // **And the capturing session no longer ducks either**, which this
    // paragraph used to say it did. The two options are never listed together
    // anywhere now: `setCapturingCategory` has the measurement that ended it.
    //
    // Built as a typed value rather than a literal with `.union` on the end:
    // chaining a method onto the literal costs it its contextual type, and
    // Swift infers `[Any]` and refuses every member. Caught by typechecking
    // on a Mac, which is the only thing short of CI that compiles this file.
    let listening: AVAudioSession.CategoryOptions = [
      .mixWithOthers,
      .allowBluetoothA2DP,
      .defaultToSpeaker,
    ]
    try session.setCategory(.playAndRecord, mode: .default, options: listening)
    try session.setActive(true)
    preferBuiltInInput()
  }

  /// Keeps the phone's own microphone for the listening state.
  ///
  /// The counterpart to `preferHandsFreeInput`: there, the helmet is wanted
  /// because the rider is talking; here the phone is, because what is being
  /// listened for is a tap on the phone itself. Stating the preference also
  /// keeps iOS from reaching for a headset microphone and dragging the route
  /// onto the hands-free profile, which would undo the feature.
  private func preferBuiltInInput() {
    let session = AVAudioSession.sharedInstance()
    guard
      let built = session.availableInputs?.first(where: { $0.portType == .builtInMic })
    else {
      return
    }
    do {
      try session.setPreferredInput(built)
    } catch {
      NSLog("MumbleWay: could not prefer the built-in input: \(error)")
    }
  }

  private func activateCapturing() throws {
    try activate()
  }

  private func activate() throws {
    wantedActive = true
    capturing = true
    try setCapturingCategory()
    try AVAudioSession.sharedInstance().setActive(true)
    preferHandsFreeInput()
  }

  /// The capturing category, set once per transition and never re-set live.
  ///
  /// Factored out of `activate` rather than copied: two lists of category
  /// options for one session is the shape of fault this feature has had most
  /// of.
  private func setCapturingCategory() throws {
    let session = AVAudioSession.sharedInstance()
    // **There is no `.duckOthers` here, and this is the retraction of a
    // build that put one in.**
    //
    // The reasoning was sound and the measurement killed it. A standing
    // option is a standing duck — Apple's wording is that ducking "begins
    // when you activate your app's audio session and ends when you
    // deactivate the session" — so a rider's music was quieter for every
    // second of a call. The fix looked obvious: `setCategory` is documented
    // to work on a live session, "you can also set the category or mode
    // while the session is active, but doing so results in an immediate
    // change", so the duck was applied and withdrawn as people spoke.
    //
    // **Two sentences of Apple's own documentation disagree about that**, and
    // the one on this page is the one that governed: `setCategory` says a
    // change on a live session is immediate, and `.duckOthers` says ducking
    // "begins when you activate" and "ends when you deactivate". A duck is
    // not a setting the route layer re-reads; it is a property of an
    // activation. Which is also why the two options are never listed together
    // here: `.duckOthers` implies `.mixWithOthers`, so the pair is at best
    // redundant, and `activateListening` records why `.mixWithOthers` is
    // nevertheless always explicit in its own right.
    //
    // On a Bluetooth headset that **stops the rider's background audio
    // permanently.** Reported from build 184: another party spoke, and
    // whatever was playing stopped and never came back. Only on the headset
    // mic — on the phone's own microphone nothing happened at all.
    //
    // Which says what it is. This option set deliberately excludes
    // `.allowBluetoothA2DP` — see the comment on it below, and the Cardo
    // Edge Pro bug it exists for — and re-setting the category forces iOS to
    // decide the route again from the permitted set. The A2DP output the
    // music was playing on is not in that set, so it goes, the music app
    // sees its output route disappear, and it pauses exactly as Apple's own
    // guidance tells it to on `oldDeviceUnavailable`. Nothing resumes it:
    // `newDeviceAvailable` is not a resume cue, and the one signal that is —
    // `.notifyOthersOnDeactivation` — can only be passed on an explicit
    // `setActive(false, …)`, which this must never do mid-call because
    // deactivating stops the audio unit the engine has a stream open on.
    //
    // So the duck is a property of the session's lifetime on this platform,
    // and the only two shapes available are "for the whole session" and
    // "never". It is never, because a rider who heard both said the mix is
    // fine without it: incoming speech is normalised and carries over music.
    // Android is different in kind — there ducking is an audio-focus lease
    // that can be taken and given back per utterance, with a documented
    // resume cue — and it keeps the feature. `docs/CAPTURE_ON_DEMAND.md` has
    // both halves.
    //
    // Built as a typed value rather than a literal with `.union` chained
    // on: that costs the literal its contextual type, Swift infers `[Any]`
    // and refuses every member. This file has eaten that mistake twice.
    let options: AVAudioSession.CategoryOptions = [
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
    ]
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
      options: options)
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
      // **The port can lag the notification that announced the device.**
      // A rider connected a headset during a call and the input stayed on the
      // phone: `.newDeviceAvailable` arrives, this runs, and the hands-free
      // port is not in `availableInputs` yet — so it returned, nothing was
      // preferred, and no further notification came because nothing had
      // changed. One microphone, wrong one, for the rest of the call.
      //
      // Retried once, and only while a Bluetooth *output* says a headset is
      // genuinely attached, so a phone on its own never schedules anything.
      // Bounded by a flag rather than a count: this cannot become the
      // self-sustaining loop that three earlier defects in this file were.
      let headsetPresent = session.currentRoute.outputs.contains {
        $0.portType == .bluetoothHFP || $0.portType == .bluetoothA2DP
          || $0.portType == .bluetoothLE
      }
      if headsetPresent && !handsFreeRetryPending {
        handsFreeRetryPending = true
        channel.invokeMethod(
          "sessionEvent",
          arguments: "hands-free port not listed yet; retrying in 700 ms")
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.7) { [weak self] in
          guard let self else { return }
          self.handsFreeRetryPending = false
          guard self.wantedActive, self.capturing else { return }
          self.preferHandsFreeInput()
        }
      }
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
        // **Only chase the input back when we are supposed to have one.** In
        // the listening state there is no input by design, and re-activating a
        // capturing session to "fix" that would drag the headset back onto the
        // hands-free profile — undoing the whole feature on every route change,
        // which is a loop a rider would experience as music that keeps going
        // narrowband by itself.
        // **The decision lives in `CaptureReconciler`, which is a pure
        // function and therefore the only part of this file a test can run.**
        //
        // Three defects shipped out of these few lines in four days, each
        // reasoned from the API because the simulator cannot run this app's
        // audio: the handler re-applied a category change and re-triggered
        // itself for ever; it read a still-settling input count as a lost
        // microphone; and the bound added to that turned an endless loop into
        // three audible ones. `CaptureReconcilerTests` now pins all three.
        //
        // The rule the reconciler encodes, and the one that was missing:
        // **commanded properties may decide, observed ones may not.** The
        // category is commanded and true the instant its setter returns; the
        // input count describes a route that has settled, and right after
        // `setActive(true)` it has not.
        let desired = CaptureReconciler.Desired(
          active: self.wantedActive, capturing: self.capturing)
        let reading = CaptureReconciler.Reading(
          category: session.category.rawValue,
          options: session.categoryOptions.rawValue,
          inputChannels: session.inputNumberOfChannels,
          inputAvailable: session.isInputAvailable,
          // Commanded, not observed: what we asked for, which is true the
          // instant the setter returned. See `Reading.preferredIsHandsFree`
          // for the loop its absence left running for a whole call.
          preferredIsHandsFree: session.preferredInput?.portType == .bluetoothHFP)

        let action = CaptureReconciler.next(desired: desired, reading: reading)
        // **Reported, not just decided.** Everything this handler does was
        // invisible: `NSLog` reaches the Mac's console and nothing a rider can
        // read or send, so three faults in a row were diagnosed by reasoning
        // about the API instead of by reading what it had answered. This line
        // lands in the engine's log, which is on screen in the diagnostics
        // panel and travels with a shared recording.
        self.channel.invokeMethod(
          "sessionEvent",
          arguments:
            "route change (\(Self.reasonName(reason))): "
            + "capturing \(self.capturing), "
            + "in \(session.currentRoute.inputs.first?.portType.rawValue ?? "none"), "
            + "preferred \(session.preferredInput?.portType.rawValue ?? "none"), "
            + "ch \(session.inputNumberOfChannels), "
            + "available \(session.isInputAvailable) -> \(action)")

        switch action {
        case .none, .waitForRouteToSettle:
          // Nothing to do, and in the settling case that is the whole point:
          // a settled route posts its own notification.
          break
        case .setCategory(let capturing):
          try? capturing ? self.activateCapturing() : self.activateListening()
        case .preferHandsFreeInput:
          self.preferHandsFreeInput()
        case .reportNoInput:
          // Re-activating cannot conjure a microphone that is not there. Say
          // so once; the route report below carries the zero to Dart.
          NSLog("MumbleWay: the route has no input available")
        }

        // **Carries the new route, rather than merely saying something moved.**
        // Dart has no way to ask for it — `activate` is the only call that
        // reports one — so a bare notification left `routeCode` stale until the
        // next connect, which is the bug this fixes. Sent after the
        // re-activation above, so it describes where the session actually ended
        // up rather than where it was a moment ago.
        self.channel.invokeMethod(
          "routeChanged",
          arguments: [
            "route": Self.routeCode(),
            "headset": Self.headsetPresent(),
            "inputChannels": session.inputNumberOfChannels,
            "sampleRate": session.sampleRate,
          ])
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
      //
      // Whichever configuration was in force, not always the capturing one: a
      // phone call arriving while the rider was only listening must not hand
      // the headset back on the hands-free profile.
      try? (capturing ? activateCapturing() : activateListening())
      channel.invokeMethod("resumed", arguments: nil)
    @unknown default:
      break
    }
  }
}
