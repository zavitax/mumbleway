/// Who decides whether the microphone is open.
///
/// **One fact, in one place, decided by a pure function.** Before this, "is
/// the microphone open" lived in about twelve mutable flags across Dart,
/// Swift, Kotlin and Rust, and eight triggers wrote to them: the tap gesture,
/// a hold taken, a hold released, the auto-stop timer, connecting, monitoring,
/// the test tone and a diagnostic recording. Nearly every defect in this
/// feature was two of those representations disagreeing — capture declared
/// after the devices opened, a connect inheriting a meter's microphone, a
/// handler reacting to a change it had itself made. Each fix was locally
/// correct and the next fault was in the adjacent joint.
///
/// # It reconciles rather than transitions
///
/// The machine does not enumerate "on this event in that state, do this".
/// It computes **what should be true** from the standing facts, compares it to
/// what *is* true, and emits at most one action to close the gap. Every event
/// runs the same comparison.
///
/// That is the same shape the engine's device thread already uses — callers
/// set the desired flags and bump `device_generation`, and the thread rebuilds
/// the streams to match — and it is the shape the platform sides take too. It
/// is worth stating why it kills the bugs rather than guarding against them:
///
/// - **Nothing can be left behind.** A state that disagrees with the inputs is
///   corrected by the next event whatever that event was, so there is no path
///   that "forgets" to reconsider. The connect that inherited a hold's
///   microphone was an early return that reconciled in one direction only;
///   here there is one direction and it is both.
/// - **Order is a value.** The actions come back as a list a test can compare,
///   instead of being the order of statements in a 5000-line method, which
///   nothing could assert. Three separate defects were statement order.
/// - **It cannot oscillate.** At most one action per step, and an action is
///   only emitted when desired and actual differ — so applying the result and
///   stepping again is a fixed point.
///
/// # What it deliberately does not know
///
/// No `async`, no I/O, no Flutter, no timers, no platform. It cannot await the
/// session, so the in-flight states are explicit: [CaptureState.opening],
/// [CaptureState.starting] and [CaptureState.stopping] are waited out by the
/// executor, which feeds the answer back as an event. That is what makes every
/// case reachable in a unit test in microseconds.
library;

import 'package:flutter/foundation.dart' show immutable;

/// Where the session and the microphone have actually got to.
enum CaptureState {
  /// No session. The engine holds no devices.
  closed,

  /// A session was asked for and has not answered.
  opening,

  /// Session up, microphone closed — the state the whole feature exists for.
  listening,

  /// The microphone was asked for and has not answered. Over Bluetooth this
  /// is an SCO negotiation and lasts a second or two.
  starting,

  /// Session up, microphone open.
  capturing,

  /// The microphone was given back and has not answered.
  stopping;

  /// Whether a session exists at all, however the microphone stands.
  bool get sessionLive => this != CaptureState.closed && this != CaptureState.opening;

  /// Whether a request is in flight. The executor refuses to start another.
  bool get settling =>
      this == CaptureState.opening ||
      this == CaptureState.starting ||
      this == CaptureState.stopping;
}

/// Who is asking for the devices, and whether they need a microphone.
///
/// **A tagged claim, not a counter.** The flags this replaces were
/// `_audioHolds` (a count written by two doors), `monitoring` (a public field
/// feeding two different derived predicates), `_captureRequiredByHold` and
/// `_captureForcedByHold` — which was never a state at all but an *ownership*
/// question: who is this microphone for, and do they get it back?
///
/// Tagging answers that question by construction. Releasing a hold removes
/// the hold's claim and cannot touch the rider's, so a meter closing can no
/// longer revoke a microphone the rider asked for, and a meter opening can no
/// longer leave one behind.
enum CaptureClaim {
  /// A call is up. Wants the session; wants the microphone only when the
  /// gesture is not in charge of it.
  call,

  /// A screen is showing a live meter. **A meter reading nothing is
  /// indistinguishable from a broken one**, so this wants the microphone
  /// whatever the gesture says.
  meter,

  /// Monitoring — the rider listening to their own microphone.
  monitor,

  /// A diagnostic recording. Same reasoning as the meter: a recording of
  /// nothing is worse than no recording.
  recorder,

  /// The rider performed the gesture. The only claim the machine sets itself.
  rider,
}

/// The standing facts the decision is made from.
///
/// **The claim set is in here rather than reduced to a boolean first**, which
/// is the whole reason this is testable. Pre-deriving "a hold wants the
/// microphone" and passing that in would put the derivation — where the
/// one-directional-reconcile bug actually lived — outside the function under
/// test, and the tests would pass while the bug sat next door.
@immutable
class CaptureInputs {
  const CaptureInputs({this.claims = const {}, this.tapMode = false});

  /// Everyone currently asking. Empty means give the devices back.
  final Set<CaptureClaim> claims;

  /// Tap-to-capture is on *and* usable — `tapToCapture && micMode != pushToTalk`.
  ///
  /// When this is false the microphone follows the session, which is the
  /// behaviour every rider had before this feature and still has by default.
  final bool tapMode;

  /// Whether anything wants the devices at all.
  bool get audioWanted => claims.isNotEmpty;

  /// Claims that need a microphone regardless of the gesture.
  static const needMic = {
    CaptureClaim.meter,
    CaptureClaim.monitor,
    CaptureClaim.recorder,
  };

  /// Whether any present claim insists on a microphone.
  bool get holdWantsMic => claims.any(needMic.contains);

  CaptureInputs with_({Set<CaptureClaim>? claims, bool? tapMode}) =>
      CaptureInputs(claims: claims ?? this.claims, tapMode: tapMode ?? this.tapMode);

  @override
  bool operator ==(Object other) =>
      other is CaptureInputs &&
      other.tapMode == tapMode &&
      other.claims.length == claims.length &&
      other.claims.containsAll(claims);

  @override
  int get hashCode => Object.hash(tapMode, Object.hashAllUnordered(claims));

  @override
  String toString() =>
      'CaptureInputs(claims: {${claims.map((c) => c.name).join(', ')}}, '
      'tap: $tapMode)';
}

/// Something happened. The machine reconciles whatever it was.
enum CaptureEvent {
  /// The inputs changed — connected, disconnected, a hold taken or given back,
  /// the gesture switched on or off. Which one does not matter: the machine
  /// reads the new inputs and closes whatever gap they opened.
  inputsChanged,

  /// The rider performed the gesture.
  tapped,

  /// The silence timer ran out, and the rider asked for that to stop capture.
  autoStopElapsed,

  /// The session came up.
  sessionOpened,

  /// The session refused. Nothing is live.
  sessionFailed,

  /// The microphone is live.
  captureStarted,

  /// The microphone could not be had. Back to listening rather than claiming
  /// a microphone that is not there.
  captureFailed,

  /// The microphone is closed, confirmed.
  captureStopped,
}

/// What to do about it, in order.
@immutable
sealed class CaptureAction {
  const CaptureAction();
}

/// Open the session, **carrying the capture intent**.
///
/// One action rather than two, and that is the point: declaring capture in a
/// separate call let it be declared *after* the devices opened, which on iOS
/// meant building an input stream against a `.playback` session with no input
/// and failing every connect with "channel count must be at least 1". There is
/// no longer an order to get wrong.
final class StartSession extends CaptureAction {
  const StartSession({required this.capture});
  final bool capture;
  @override
  bool operator ==(Object other) => other is StartSession && other.capture == capture;
  @override
  int get hashCode => capture.hashCode;
  @override
  String toString() => 'StartSession(capture: $capture)';
}

/// Give the devices back.
final class StopSession extends CaptureAction {
  const StopSession();
  @override
  bool operator ==(Object other) => other is StopSession;
  @override
  int get hashCode => 1;
  @override
  String toString() => 'StopSession()';
}

/// Take the hands-free profile and open the microphone.
final class StartCapture extends CaptureAction {
  const StartCapture();
  @override
  bool operator ==(Object other) => other is StartCapture;
  @override
  int get hashCode => 2;
  @override
  String toString() => 'StartCapture()';
}

/// Give the profile back.
final class StopCapture extends CaptureAction {
  const StopCapture();
  @override
  bool operator ==(Object other) => other is StopCapture;
  @override
  int get hashCode => 3;
  @override
  String toString() => 'StopCapture()';
}

/// Which cue to play. The machine names it; the engine renders it.
enum CaptureCueKind {
  /// Connected in tap mode: capture is off and the rider must be told, or the
  /// first thing they do is talk into a microphone that is not there. It
  /// counts out the configured taps, so it also says what the gesture is.
  armed,

  /// The microphone is live.
  live,

  /// The microphone is off.
  stopped,
}

final class PlayCue extends CaptureAction {
  const PlayCue(this.kind);
  final CaptureCueKind kind;
  @override
  bool operator ==(Object other) => other is PlayCue && other.kind == kind;
  @override
  int get hashCode => kind.hashCode;
  @override
  String toString() => 'PlayCue(${kind.name})';
}

/// Tell the other riders. Capture closed is muted for every practical
/// purpose, so it rides the standard field and shows on every roster.
final class PublishMute extends CaptureAction {
  const PublishMute({required this.muted});
  final bool muted;
  @override
  bool operator ==(Object other) => other is PublishMute && other.muted == muted;
  @override
  int get hashCode => muted.hashCode;
  @override
  String toString() => 'PublishMute(muted: $muted)';
}

/// The result of one step.
@immutable
class CaptureStep {
  const CaptureStep(this.state, this.actions);
  final CaptureState state;
  final List<CaptureAction> actions;

  @override
  String toString() => '${state.name} -> $actions';
}

/// The machine. Holds the two things that are genuinely its own.
class CaptureMachine {
  // The fields are private and the parameters are not, which is deliberate:
  // callers name the concept, not the storage. An initializing formal would
  // put `_state` in the public signature.
  // ignore_for_file: prefer_initializing_formals
  CaptureMachine({
    CaptureState state = CaptureState.closed,
    bool riderWantsMic = false,
    bool openingWithCapture = false,
  }) : _state = state,
       _riderWantsMic = riderWantsMic,
       _openingWithCapture = openingWithCapture;

  CaptureState _state;

  /// **The latch the gesture sets, and the only memory in here.**
  ///
  /// In tap mode the microphone is not derivable from the standing facts —
  /// whether the rider has tapped it on is a fact of its own. Everything else
  /// is read from [CaptureInputs] every step, which is why nothing can go
  /// stale.
  bool _riderWantsMic;

  /// An exact copy, for exploring a branch without disturbing this one.
  CaptureMachine.from(CaptureMachine other)
    : _state = other._state,
      _riderWantsMic = other._riderWantsMic,
      _openingWithCapture = other._openingWithCapture;

  CaptureState get state => _state;
  bool get riderWantsMic => _riderWantsMic;

  /// What [StartSession] was asked for, so the answer can be read correctly.
  ///
  /// The session opens *with* its microphone or without it — that is the whole
  /// point of carrying the intent as an argument — so "the session is up" does
  /// not say where the microphone landed. Without this the machine reached
  /// `listening` on a session it had opened live and immediately asked for a
  /// microphone it already had.
  bool _openingWithCapture;

  /// Whether the microphone *should* be open, given the facts.
  ///
  /// Read this as the whole policy, because it is:
  ///
  /// - a hold wants a meter to read something, so it wins over everything;
  /// - with the gesture off, the microphone follows the session, which is the
  ///   behaviour that shipped for years;
  /// - with the gesture on, it is whatever the rider last tapped.
  static bool micWanted(CaptureInputs i, {required bool riderWantsMic}) =>
      i.holdWantsMic || !i.tapMode || riderWantsMic;

  /// One event, one reconciliation, at most one state-changing action.
  CaptureStep step(CaptureEvent event, CaptureInputs inputs) {
    switch (event) {
      case CaptureEvent.tapped:
        // Only the gesture's own mode listens to it. With the gesture off a
        // tap means nothing, and must not: push-to-talk promises immediacy
        // and a stray tap toggling the microphone would break that promise.
        if (inputs.tapMode) _riderWantsMic = !_riderWantsMic;
      case CaptureEvent.autoStopElapsed:
        if (inputs.tapMode) _riderWantsMic = false;
      case CaptureEvent.sessionOpened:
        if (_state == CaptureState.opening) {
          _state = _openingWithCapture
              ? CaptureState.capturing
              : CaptureState.listening;
        }
      case CaptureEvent.sessionFailed:
        _state = CaptureState.closed;
        _riderWantsMic = false;
      case CaptureEvent.captureStarted:
        if (_state == CaptureState.starting) _state = CaptureState.capturing;
      case CaptureEvent.captureFailed:
        if (_state == CaptureState.starting) {
          _state = CaptureState.listening;
          // The microphone was not had, so the rider's request is spent. Left
          // set, the next reconcile would ask again and keep asking.
          _riderWantsMic = false;
        }
      case CaptureEvent.captureStopped:
        if (_state == CaptureState.stopping) _state = CaptureState.listening;
      case CaptureEvent.inputsChanged:
        break;
    }
    return _reconcile(event, inputs);
  }

  CaptureStep _reconcile(CaptureEvent event, CaptureInputs inputs) {
    final actions = <CaptureAction>[];

    // Nothing is decided while a request is in flight. The answer is coming
    // and will bring its own reconciliation; acting now would be the
    // oscillation this design exists to prevent.
    if (_state.settling) return CaptureStep(_state, actions);

    if (!inputs.audioWanted) {
      if (_state != CaptureState.closed) {
        _state = CaptureState.closed;
        // **Cleared here, and this is the inherited-capture bug.** A session
        // can be live with the microphone open for a meter, and the ten-second
        // idle grace keeps it alive after the meter closes. Carrying the latch
        // across meant connecting in tap mode came up live — the one thing the
        // feature exists to prevent.
        _riderWantsMic = false;
        actions.add(const StopSession());
      }
      return CaptureStep(_state, actions);
    }

    final wantMic = micWanted(inputs, riderWantsMic: _riderWantsMic);

    if (_state == CaptureState.closed) {
      _state = CaptureState.opening;
      _openingWithCapture = wantMic;
      actions.add(StartSession(capture: wantMic));
      if (!wantMic) actions.add(const PlayCue(CaptureCueKind.armed));
      actions.add(PublishMute(muted: !wantMic));
      return CaptureStep(_state, actions);
    }

    // Session is up. The only question left is the microphone.
    final micOpen = _state == CaptureState.capturing;
    if (wantMic == micOpen) {
      // Already right. Announce the two transitions a listener can hear, and
      // only on the step that reached them, so a cue is never played twice for
      // one change.
      if (event == CaptureEvent.captureStarted ||
          (event == CaptureEvent.sessionOpened && _openingWithCapture)) {
        if (event == CaptureEvent.captureStarted) {
          actions.add(const PlayCue(CaptureCueKind.live));
        }
        actions.add(const PublishMute(muted: false));
      } else if (event == CaptureEvent.captureStopped) {
        actions.add(const PlayCue(CaptureCueKind.stopped));
        actions.add(const PublishMute(muted: true));
      }
      return CaptureStep(_state, actions);
    }

    if (wantMic) {
      _state = CaptureState.starting;
      actions.add(const StartCapture());
    } else {
      _state = CaptureState.stopping;
      actions.add(const StopCapture());
    }
    return CaptureStep(_state, actions);
  }
}
