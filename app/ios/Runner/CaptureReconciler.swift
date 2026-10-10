import Foundation

/// What the session should be, and what to do about the difference.
///
/// **This is the half of `AudioSession` that kept being wrong, extracted so it
/// can be run without CoreAudio.** Three defects shipped from the route-change
/// handler in four days: it re-applied a category change and so re-triggered
/// itself endlessly; it read a still-settling input count as a lost
/// microphone and re-activated, which a rider heard as the audio going on, off
/// and on again; and a bounded retry was added to the second, which turned an
/// endless loop into three audible ones.
///
/// Each time the fix was reasoned from the API and shipped unverified, because
/// the iOS simulator cannot run this app's audio at all — connecting aborts
/// inside `AURemoteIO`. What the simulator *can* do is run a pure function, so
/// the decision is now one: no session, no CoreAudio, no notifications.
///
/// # Commanded against observed, which is the whole lesson
///
/// `AVAudioSession` has two kinds of property and they must never be used the
/// same way:
///
/// - **Commanded** — `category`, `mode`, options. You set it, you read it back
///   from the same API, and it is true the instant the setter returns. A
///   legitimate basis for a decision.
/// - **Observed** — `inputNumberOfChannels`, `currentRoute`, `sampleRate`.
///   These describe a route that has *settled*, and immediately after
///   `setActive(true)` it has not. **Never a decision input.** Acting on one
///   mid-settle is the on-off-on, and bounding how often you do it is a damper
///   on an oscillator rather than a fix.
///
/// `isInputAvailable` is the question actually worth asking — is there any
/// input hardware on this route — and it answers straight away. A zero count
/// with input available means "not yet", which is a third state and the one
/// the old code did not have: it had only *right* and *wrong*, so during the
/// settling window it was forced to guess, and guessed wrong roughly as often
/// as the window was open. That is why it only ever appeared without a
/// headset, where no SCO negotiation covers the gap.
enum CaptureReconciler {

  /// What this app has asked for. `active` is the one leg that cannot be read
  /// back — `AVAudioSession` exposes no `isActive` — so it is a belief, held
  /// by one writer, and the diagnostics panel labels it as such.
  struct Desired: Equatable {
    var active: Bool
    var capturing: Bool

    init(active: Bool, capturing: Bool) {
      self.active = active
      self.capturing = capturing
    }
  }

  /// One look at the live session.
  struct Reading: Equatable {
    /// `AVAudioSession.Category.rawValue`. Commanded, so safe to compare.
    var category: String
    /// Observed. Only ever used to tell *settled* from *settling*.
    var inputChannels: Int
    /// Answered immediately, unlike the count.
    var inputAvailable: Bool

    init(category: String, inputChannels: Int, inputAvailable: Bool) {
      self.category = category
      self.inputChannels = inputChannels
      self.inputAvailable = inputAvailable
    }
  }

  static let playback = "AVAudioSessionCategoryPlayback"
  static let playAndRecord = "AVAudioSessionCategoryPlayAndRecord"

  /// The category this app wants for a given intent.
  static func category(forCapturing capturing: Bool) -> String {
    capturing ? playAndRecord : playback
  }

  enum Action: Equatable {
    /// Already as asked. **The route change this app caused lands here**, which
    /// is what stops the handler reacting to itself.
    case none
    /// Set the category — the only commanded change there is.
    case setCategory(capturing: Bool)
    /// Settled on an input; make sure it is the helmet's and not the phone's.
    case preferHandsFreeInput
    /// Desired and actual differ only because the route has not settled. **Do
    /// nothing**: a settled route posts its own notification, and acting now
    /// is the fault this type exists to prevent.
    case waitForRouteToSettle
    /// There is genuinely no input hardware on this route. Re-activating
    /// cannot conjure one, so say so instead of trying again.
    case reportNoInput
  }

  /// One step. Pure, total, and a fixed point once it answers `.none`.
  static func next(desired: Desired, reading: Reading) -> Action {
    guard desired.active else { return .none }

    let wanted = category(forCapturing: desired.capturing)
    if reading.category != wanted {
      return .setCategory(capturing: desired.capturing)
    }

    guard desired.capturing else { return .none }

    if reading.inputChannels > 0 { return .preferHandsFreeInput }
    if reading.inputAvailable { return .waitForRouteToSettle }
    return .reportNoInput
  }
}
