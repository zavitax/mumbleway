import 'package:flutter_test/flutter_test.dart';
import 'package:mumbleway/state/capture_machine.dart';

/// Every reachable sequence, to a depth that covers a whole transition.
///
/// **The event alphabet is eight and the inputs are a handful, so there is no
/// reason to sample it.** Breadth-first enumeration to depth five is tens of
/// thousands of cases against a pure function — milliseconds, deterministic,
/// no dependency — and when something fails it hands back the *shortest*
/// sequence that does it, which a random generator cannot.
///
/// Each invariant below is tied to a defect that shipped. That is the point:
/// the feature cost ten TestFlight builds in four days, and nearly every fault
/// was two representations of "is the microphone open" disagreeing. These
/// assert the disagreements cannot happen rather than that they did not.
void main() {
  /// The events the executor can feed back, given where the machine is.
  ///
  /// Modelling this matters: `captureStarted` cannot arrive unless something
  /// was starting, and enumerating impossible answers would spend the search
  /// on sequences no executor can produce.
  List<CaptureEvent> plausible(CaptureState s) => [
    CaptureEvent.inputsChanged,
    CaptureEvent.tapped,
    CaptureEvent.autoStopElapsed,
    if (s == CaptureState.opening) ...[
      CaptureEvent.sessionOpened,
      CaptureEvent.sessionFailed,
    ],
    if (s == CaptureState.starting) ...[
      CaptureEvent.captureStarted,
      CaptureEvent.captureFailed,
    ],
    if (s == CaptureState.stopping) CaptureEvent.captureStopped,
  ];

  const claimSets = <Set<CaptureClaim>>[
    {},
    {CaptureClaim.call},
    {CaptureClaim.call, CaptureClaim.meter},
    {CaptureClaim.meter},
  ];

  /// Walks every sequence to [depth], calling [check] on each step.
  ///
  /// `trail` is carried so a failure can name the exact path.
  void explore(
    int depth,
    void Function(
      List<String> trail,
      CaptureState before,
      CaptureInputs inputs,
      CaptureEvent event,
      CaptureStep step,
      CaptureMachine after,
    ) check,
  ) {
    void walk(CaptureMachine m, CaptureInputs inputs, List<String> trail) {
      if (trail.length >= depth) return;
      for (final tapMode in [false, true]) {
        for (final claims in claimSets) {
          for (final event in plausible(m.state)) {
            final next = CaptureMachine.from(m);
            final ins = CaptureInputs(claims: claims, tapMode: tapMode);
            final before = next.state;
            final step = next.step(event, ins);
            final line =
                '${event.name} @ $ins  =>  $step';
            final here = [...trail, line];
            check(here, before, ins, event, step, next);
            walk(next, ins, here);
          }
        }
      }
    }

    walk(CaptureMachine(), const CaptureInputs(), const []);
  }

  String why(List<String> trail) => '\n  ${trail.join('\n  ')}\n';

  group('invariants, over every sequence to depth 4', () {
    test('a microphone is never asked for without a session', () {
      explore(4, (trail, before, ins, event, step, after) {
        if (step.actions.any((a) => a is StartCapture)) {
          // The resulting state is what matters: `opening` is not yet a
          // session, and `sessionOpened` legitimately resolves it within the
          // same step before the reconcile runs.
          expect(
            step.state,
            CaptureState.starting,
            reason: 'StartCapture did not leave the machine starting:'
                '${why(trail)}',
          );
          expect(
            step.state.sessionLive,
            isTrue,
            reason: 'StartCapture with no session:${why(trail)}',
          );
        }
      });
    });

    test('the capture intent is an argument of the open, never a second call', () {
      // The shipped "channel count must be at least 1": capture was declared
      // after the devices opened, so iOS built an input stream against a
      // `.playback` session that has none. Here it cannot be a separate step.
      explore(4, (trail, before, ins, event, step, after) {
        final opens = step.actions.whereType<StartSession>().toList();
        if (opens.isEmpty) return;
        expect(opens, hasLength(1), reason: why(trail));
        expect(
          step.actions.any((a) => a is StartCapture || a is StopCapture),
          isFalse,
          reason: 'a capture call beside the open:${why(trail)}',
        );
      });
    });

    test('no step both starts and stops the microphone', () {
      explore(4, (trail, before, ins, event, step, after) {
        expect(
          step.actions.whereType<StartCapture>().isEmpty ||
              step.actions.whereType<StopCapture>().isEmpty,
          isTrue,
          reason: why(trail),
        );
      });
    });

    test('nothing is decided while a request is in flight', () {
      // Acting mid-transition is the oscillation this design exists to
      // prevent — the audible on-off-on.
      explore(4, (trail, before, ins, event, step, after) {
        // Only for events that do NOT resolve the transition. An answer
        // arriving is exactly what is allowed to act.
        const resolving = {
          CaptureEvent.sessionOpened,
          CaptureEvent.sessionFailed,
          CaptureEvent.captureStarted,
          CaptureEvent.captureFailed,
          CaptureEvent.captureStopped,
        };
        if (!before.settling || resolving.contains(event)) return;
        expect(
          step.actions.whereType<StartCapture>().isEmpty &&
              step.actions.whereType<StopCapture>().isEmpty &&
              step.actions.whereType<StartSession>().isEmpty,
          isTrue,
          reason: 'acted while settling:${why(trail)}',
        );
      });
    });

    test('reconciling twice over asks for nothing the second time', () {
      // Quiescence. If a repeat of the same event could still emit an action,
      // the machine would chase itself exactly as the route-change handler did.
      explore(4, (trail, before, ins, event, step, after) {
        if (step.state.settling) return;
        final again =
            CaptureMachine.from(after).step(CaptureEvent.inputsChanged, ins);
        expect(
          again.actions.where((a) => a is! PublishMute && a is! PlayCue),
          isEmpty,
          reason: 'not a fixed point:${why(trail)}\n  then: $again',
        );
      });
    });

    test('a cue is only ever played for a transition that happened', () {
      // "If the assertion fails, play nothing." An early stop cue puts a
      // curse on the channel; an early live cue costs a sentence.
      explore(4, (trail, before, ins, event, step, after) {
        for (final cue in step.actions.whereType<PlayCue>()) {
          switch (cue.kind) {
            case CaptureCueKind.live:
              expect(
                event,
                CaptureEvent.captureStarted,
                reason: 'live cue without a confirmed start:${why(trail)}',
              );
            case CaptureCueKind.stopped:
              expect(
                event,
                CaptureEvent.captureStopped,
                reason: 'stop cue without a confirmed stop:${why(trail)}',
              );
            case CaptureCueKind.armed:
              expect(
                step.actions.whereType<StartSession>().single.capture,
                isFalse,
                reason: 'armed cue on a session that opened live:${why(trail)}',
              );
          }
        }
      });
    });
  });

  group('the faults that shipped', () {
    test('connecting in tap mode never inherits a meter\'s microphone', () {
      // The rider turned the gesture on in Settings, backed out and connected
      // inside the ten-second idle grace. The session was still live with
      // capture held open for the meter, and the early return reconciled in
      // one direction only — so they came up live, which is the one thing
      // tap-to-capture exists to prevent.
      final m = CaptureMachine();
      const meter = CaptureInputs(claims: {CaptureClaim.meter}, tapMode: true);
      m.step(CaptureEvent.inputsChanged, meter);
      m.step(CaptureEvent.sessionOpened, meter);
      expect(m.state, CaptureState.capturing, reason: 'the meter gets a mic');

      // The meter closes and a call begins, with the session never dropping.
      const call = CaptureInputs(claims: {CaptureClaim.call}, tapMode: true);
      final step = m.step(CaptureEvent.inputsChanged, call);
      expect(step.actions, contains(const StopCapture()));
      m.step(CaptureEvent.captureStopped, call);
      expect(m.state, CaptureState.listening);
    });

    test('releasing a meter does not revoke a microphone the rider asked for', () {
      // `_captureForcedByHold` was a single boolean trying to answer an
      // ownership question. Tagged claims answer it by construction.
      final m = CaptureMachine();
      const both = CaptureInputs(
        claims: {CaptureClaim.call, CaptureClaim.meter},
        tapMode: true,
      );
      m.step(CaptureEvent.inputsChanged, both);
      m.step(CaptureEvent.sessionOpened, both);
      expect(m.state, CaptureState.capturing);

      // The rider taps while the meter is up: now they want it too.
      m.step(CaptureEvent.tapped, both);
      const call = CaptureInputs(claims: {CaptureClaim.call}, tapMode: true);
      final step = m.step(CaptureEvent.inputsChanged, call);
      expect(
        step.actions,
        isEmpty,
        reason: 'the rider still wants it, so nothing should change',
      );
      expect(m.state, CaptureState.capturing);
    });

    test('a tap during a transition is not thrown away', () {
      // "Gestures completed and were thrown away" shipped once already, and
      // `requestCapture` still drops a tap arriving while `_captureChanging`.
      // The latch moves even when the action cannot, and settling honours it.
      final m = CaptureMachine();
      const tap = CaptureInputs(claims: {CaptureClaim.call}, tapMode: true);
      m.step(CaptureEvent.inputsChanged, tap);
      m.step(CaptureEvent.sessionOpened, tap);
      expect(m.state, CaptureState.listening);

      m.step(CaptureEvent.tapped, tap);
      expect(m.state, CaptureState.starting);

      // A second tap lands mid-negotiation. It must not be lost.
      m.step(CaptureEvent.tapped, tap);
      expect(m.riderWantsMic, isFalse, reason: 'the second tap was heard');
      expect(m.state, CaptureState.starting, reason: 'but not acted on yet');

      final settled = m.step(CaptureEvent.captureStarted, tap);
      expect(
        settled.actions,
        contains(const StopCapture()),
        reason: 'and is honoured the moment the first finishes',
      );
    });

    test('with the gesture off nothing changes for anybody', () {
      final m = CaptureMachine();
      const plain = CaptureInputs(claims: {CaptureClaim.call});
      final open = m.step(CaptureEvent.inputsChanged, plain);
      expect(open.actions.first, const StartSession(capture: true));
      expect(
        open.actions.whereType<PlayCue>(),
        isEmpty,
        reason: 'no armed cue when the microphone is already live',
      );
      m.step(CaptureEvent.sessionOpened, plain);
      expect(m.state, CaptureState.capturing);

      // A tap means nothing in this mode.
      expect(m.step(CaptureEvent.tapped, plain).actions, isEmpty);
      expect(m.state, CaptureState.capturing);
    });

    test('the devices go back when the last claim does', () {
      final m = CaptureMachine();
      const call = CaptureInputs(claims: {CaptureClaim.call}, tapMode: true);
      m.step(CaptureEvent.inputsChanged, call);
      m.step(CaptureEvent.sessionOpened, call);
      m.step(CaptureEvent.tapped, call);
      m.step(CaptureEvent.captureStarted, call);
      expect(m.state, CaptureState.capturing);

      final gone = m.step(
        CaptureEvent.inputsChanged,
        const CaptureInputs(tapMode: true),
      );
      expect(gone.actions, contains(const StopSession()));
      expect(m.state, CaptureState.closed);
      expect(
        m.riderWantsMic,
        isFalse,
        reason: 'the latch must not survive the session that held it',
      );
    });
  });
}

