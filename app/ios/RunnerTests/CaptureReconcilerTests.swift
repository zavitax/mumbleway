import XCTest

@testable import Runner

/// The three faults that shipped out of `routeChanged`, as tests.
///
/// This target existed as an empty `testExample()` stub, wired into the scheme
/// and never run by anything. Meanwhile the Swift half of capture-on-demand
/// produced three defects in four days, each reasoned from the API and each
/// verifiable only by a thirty-minute round trip through TestFlight and a
/// rider's ear.
///
/// The decision is a pure function now, so these run in milliseconds.
final class CaptureReconcilerTests: XCTestCase {

  typealias R = CaptureReconciler

  private func listening(channels: Int = 0, inputAvailable: Bool = true) -> R.Reading {
    R.Reading(
      category: R.playback, inputChannels: channels, inputAvailable: inputAvailable)
  }

  private func capturing(channels: Int, inputAvailable: Bool = true) -> R.Reading {
    R.Reading(
      category: R.playAndRecord, inputChannels: channels, inputAvailable: inputAvailable)
  }

  // MARK: - The endless loop

  /// **Shipped, and heard as the audio switching on and off for ever.**
  ///
  /// `activateListening` sets the category, setting the category posts a route
  /// change, and the handler called `activateListening` again without asking
  /// whether anything was wrong. The fix has to be that a session already in
  /// the wanted category produces no action at all — anything else re-enters.
  func testASessionAlreadyListeningIsLeftAlone() {
    let desired = R.Desired(active: true, capturing: false)
    XCTAssertEqual(R.next(desired: desired, reading: listening()), .none)
  }

  /// The same property stated as the thing that actually matters: applying the
  /// answer and looking again must ask for nothing.
  func testReconcilingIsAFixedPoint() {
    for capturingWanted in [false, true] {
      let desired = R.Desired(active: true, capturing: capturingWanted)
      var reading = listening(channels: 0)

      // Drive to convergence the way the real handler would, and assert it
      // gets there without ever oscillating.
      var seen: [R.Action] = []
      for _ in 0..<10 {
        let action = R.next(desired: desired, reading: reading)
        seen.append(action)
        switch action {
        case .setCategory(let c):
          reading.category = R.category(forCapturing: c)
          // A real route takes a moment; the count stays 0 for now.
          reading.inputChannels = 0
        case .waitForRouteToSettle:
          // The route settles on its own and posts a notification.
          reading.inputChannels = 1
        case .preferHandsFreeInput, .none, .reportNoInput:
          break
        }
        if action == .none || action == .preferHandsFreeInput { break }
      }

      XCTAssertFalse(
        seen.contains(.reportNoInput),
        "an available input was reported missing: \(seen)")
      // Having converged, it must stay converged.
      let again = R.next(desired: desired, reading: reading)
      XCTAssertTrue(
        again == .none || again == .preferHandsFreeInput,
        "not a fixed point: \(again) after \(seen)")
    }
  }

  // MARK: - The settling race

  /// **Shipped, and heard as on-off-on when capture started.**
  ///
  /// `inputNumberOfChannels` describes a settled route, and right after
  /// `setActive(true)` the built-in microphone often has not settled — so it
  /// answers 0 about a microphone that is perfectly fine. The old handler read
  /// that as "the input is gone" and re-activated, which set the category,
  /// which posted another route change.
  ///
  /// It appeared only without a Bluetooth headset, because an SCO negotiation
  /// takes long enough that the route has settled before any of it runs.
  func testAZeroCountWithAnInputAvailableIsNotAMissingMicrophone() {
    let desired = R.Desired(active: true, capturing: true)
    XCTAssertEqual(
      R.next(desired: desired, reading: capturing(channels: 0, inputAvailable: true)),
      .waitForRouteToSettle,
      "a settling route must produce no commanded change")
  }

  /// The settling window can last several notifications. None of them may act.
  func testNothingIsCommandedForAsLongAsTheRouteIsSettling() {
    let desired = R.Desired(active: true, capturing: true)
    for _ in 0..<20 {
      let action = R.next(
        desired: desired, reading: capturing(channels: 0, inputAvailable: true))
      XCTAssertEqual(
        action, .waitForRouteToSettle,
        "a bounded number of retries is a damper on an oscillator, not a fix")
    }
  }

  /// And when it does settle, the handler gets on with it.
  func testOnceSettledTheHelmetInputIsPreferred() {
    let desired = R.Desired(active: true, capturing: true)
    XCTAssertEqual(
      R.next(desired: desired, reading: capturing(channels: 1)),
      .preferHandsFreeInput)
  }

  /// No input hardware at all is a different answer from one that is late.
  /// Re-activating cannot conjure a microphone, so it must not be attempted.
  func testNoInputHardwareIsReportedRatherThanRetried() {
    let desired = R.Desired(active: true, capturing: true)
    XCTAssertEqual(
      R.next(desired: desired, reading: capturing(channels: 0, inputAvailable: false)),
      .reportNoInput)
  }

  // MARK: - Giving the microphone back

  /// **Shipped, and paid on every tap-to-stop including on the speaker.**
  ///
  /// `stopCapture` deactivated the whole session to force a headset off the
  /// hands-free profile — a guess the specification admits to, and one that
  /// has still never been measured on a headset. With no Bluetooth on the
  /// route there is no profile to release, and deactivating stops the audio
  /// unit and interrupts the stream the engine has open on it.
  func testGivingTheMicrophoneBackOnASpeakerDoesNotTearTheSessionDown() {
    XCTAssertEqual(R.release(routeHasBluetooth: false), .categoryOnly)
  }

  /// And the expensive path survives for the one case it was written for.
  func testAHeadsetStillGetsTheFullDeactivation() {
    XCTAssertEqual(
      R.release(routeHasBluetooth: true), .deactivateAndReactivate)
  }

  // MARK: - The transitions themselves

  func testStartingCaptureAsksForPlayAndRecord() {
    XCTAssertEqual(
      R.next(
        desired: R.Desired(active: true, capturing: true), reading: listening()),
      .setCategory(capturing: true))
  }

  func testStoppingCaptureAsksForPlayback() {
    XCTAssertEqual(
      R.next(
        desired: R.Desired(active: true, capturing: false),
        reading: capturing(channels: 1)),
      .setCategory(capturing: false))
  }

  /// With no session wanted there is nothing to reconcile, and in particular
  /// nothing that could drag a headset back onto the hands-free profile.
  func testAnInactiveSessionIsNeverTouched() {
    for reading in [listening(), capturing(channels: 0), capturing(channels: 2)] {
      XCTAssertEqual(
        R.next(desired: R.Desired(active: false, capturing: false), reading: reading),
        .none)
      XCTAssertEqual(
        R.next(desired: R.Desired(active: false, capturing: true), reading: reading),
        .none)
    }
  }

  /// Every combination, asserting the one property that matters above all:
  /// **a commanded change is only ever emitted when a commanded property is
  /// actually wrong.** Everything else is what re-triggered the handler.
  func testCommandedChangesOnlyFollowCommandedDifferences() {
    for active in [true, false] {
      for wantCapture in [true, false] {
        for category in [R.playback, R.playAndRecord] {
          for channels in [0, 1, 2] {
            for available in [true, false] {
              let desired = R.Desired(active: active, capturing: wantCapture)
              let reading = R.Reading(
                category: category, inputChannels: channels, inputAvailable: available)
              guard case .setCategory = R.next(desired: desired, reading: reading) else {
                continue
              }
              XCTAssertTrue(active, "commanded a change on an inactive session")
              XCTAssertNotEqual(
                category, R.category(forCapturing: wantCapture),
                "commanded a category that was already set — this is the loop")
            }
          }
        }
      }
    }
  }
}
