# Capture on demand, hi-fi output in between

**Status: specified, not yet built.** Written 2026-10-08. The measurements in
[Measurements](#measurements) have not been taken, and one of them can cancel the
whole feature — read that section before building anything.

## The problem

A rider wants two things Bluetooth makes mutually exclusive. **A2DP** is stereo,
48 kHz, AAC/aptX/LDAC — and output-only, no microphone. **HFP** has the microphone
and is mono, 8 kHz CVSD or 16 kHz mSBC, and while the headset is in it *everything*
sounds like a phone call, music included.

**MumbleWay pins the duplex side for the whole call, and that is why music is
degraded for an entire ride.** `AppState._audioNeeded` is `_callInProgress ||
monitoring || _audioHolds > 0`, and while that holds, the headset stays in HFP.
`AudioSession.swift` states the trade outright: *"HFP alone forces the headset into
the bidirectional hands-free profile and keeps it there… the cost is that music
plays at telephone bandwidth for the duration of a call, which for an intercom is
the right trade."*

That was right while capture was all-or-nothing for a call. It is wrong once
capture can be released and retaken, because most of a ride is listening.

**The gap is precise.** There is no A2DP-output / HFP-input split, no listen-only
state, and nothing that releases the communication device while the session stays
alive. On Android `setCommunicationDevice` is called once in `activate` and cleared
only in `deactivate`.

## The organising principle: the chain does not know

**The audio chain behaves exactly as it does today and is never told whether capture
is on. From its perspective, it always is.** Most of the design follows by
subtraction:

- **Capture off means no input stream** — not a flag, not a mute, not a gate hold.
  The worker is simply not fed.
- **So no `TransmitMode` needs a special case.** `Continuous` still computes
  `open = true`; with no blocks arriving, nothing is encoded. Starvation does what a
  mode matrix would otherwise have to, and there is no matrix to get wrong.
- **`CaptureProcessor`, the gate, `WARMUP_BLOCKS`, `reset()` and `paydown.rs` are
  untouched.** Keeping the 240 ms look-ahead and its measured repayment out of this
  is a deliberate non-goal: it is the most carefully tuned part of the transmit path
  and has nothing to gain here.
- **The bit-exact-zero watchdog stays correct, and that is the reason to close the
  stream rather than feed silence.** It counts zero blocks that were *delivered*; no
  delivery is not a delivery of zeros. Feeding silence would trip its two-second
  warning on every listening stretch.
- **On a capture start the chain sees what it sees on a connect today** — `reset()`,
  150 ms of `WARMUP_BLOCKS`, a floor rebuilding over six 0.25 s sub-windows, `Auto`
  re-converging. Today's accepted post-connect transient, happening more often. Not a
  new class of risk.

**What the principle costs:** a chain that cannot tell will process and transmit
whatever it is handed. If a transition lands on the phone's microphone instead of the
helmet's — which on Android is exactly what dropping `MODE_IN_COMMUNICATION` with an
open input stream does — the rider broadcasts pocket noise and nothing objects.
`CLAUDE.md` is blunt about the general case: *"audio carries no record of what
captured it, so a directory of recordings from the wrong microphone looks exactly like
one from the right microphone."* **Asserting the route on every transition is
load-bearing, not good practice.**

## The channel API

`mumbleway/audioSession` keeps `activate` and `deactivate` meaning *whether there is
an active session at all*, and gains **`startCapture`** and **`stopCapture`**, which
do the profile switching.

| | `activate` / `deactivate` — the session | `startCapture` / `stopCapture` — the profile |
|---|---|---|
| **Android** | `requestMusicFocus()` / `abandonMusicFocus()`, save and restore `previousMode`. Mode stays `MODE_NORMAL`, no communication device, so the media-usage output stream rides A2DP. | `mode = MODE_IN_COMMUNICATION` + `chooseCommunicationDevice()` (API 31+) or `awaitSco()` below it, plus `watchForSilencing()`. The reverse clears the device, stops the watch, restores the mode. |
| **iOS** | `.playback` + `setActive(true)` / `setActive(false, [.notifyOthersOnDeactivation])`. | `.playAndRecord` with the HFP options + `preferHandsFreeInput()`. The reverse returns the category to `.playback`. |

Every line already exists in `AudioRouting.activate`/`deactivate` and
`AudioSession.activate`/`deactivate`. This is largely redistribution along the
session-versus-profile line, which is the main argument for the shape.

**iOS listening is `.playback`, not `.playAndRecord` offering A2DP.** The documented
Cardo Edge Pro fault is specifically a `playAndRecord` session that lists A2DP and
then loses an input it wanted; with `.playback` there is no input to lose, so the
fault has no surface.

**`activate` calls `startCapture` itself unless capture is on demand.** A flag beside
the existing `voiceProcessing`, where **absent or false is today's behaviour
exactly**: one call, SCO negotiated, capture live, route returned. That flag is the
feature's kill switch.

Three contract details:

- **`startCapture` is asynchronous and returns the resulting route**, not merely
  success. Android's `activate(done:)` is already callback-shaped and `awaitSco`
  already waits on `ACTION_SCO_AUDIO_STATE_UPDATED`, so that machinery moves wholesale.
- **`inputChannels == 0` stays a fault on the old path and becomes legal on the new
  one.** `AudioSessionState.usable` is `granted && inputChannels != 0` and gates
  starting the engine, so an unconditional zero would stop the engine ever starting.
  Capture-started keeps today's meaning; listening expects zero and readiness is
  `startCapture`'s answer. The field's doc comment distinguishes 0 from −1 and needs
  the third case written down.
- **Ordering is the native side's business** — `startCapture` before `activate`,
  `stopCapture` when not capturing, `deactivate` while capturing. One answer in one
  place.

## What has to move in the engine

1. **Output-only operation.** `build_streams` returns `(input, output)` and the pair
   is always rebuilt together; listening needs output alive with no input at all. On
   iOS a `.playback` session has no input to open, so this is not optional.
2. **Reopen on every transition, because the device sample rate changes.** Nothing
   anywhere asks the platform for a rate: `default_output_config()` is taken as-is and
   a `Resampler` absorbs the mismatch. HFP reports 8 or 16 kHz, A2DP 44.1 or 48, so a
   profile change silently invalidates both configurations. `request_reopen()` already
   rebuilds both as a pair — the right primitive, and it must be awaited.
3. **Route-change notifications, currently dropped or absent.** iOS *sends*
   `routeChanged` and Dart silently drops it: `AudioSessionBridge._ensureHandler`
   handles only `resumed` and `micSilenced`. Android has none at all. Independently of
   this feature that already lets `routeCode` go stale when a headset reconnects
   mid-ride.

**Sequence on a request**, worth writing down because it is the part most likely to be
assembled wrong: `startCapture` → route confirmed and asserted → `setAudioRoute` →
`request_reopen()` → streams back → cue resolves.

## The cues

Audio lost during a transition is **accepted by decision**. Nothing buffers across the
gap, nothing crossfades, and `paydown.rs` is not touched. (It could not have helped:
`ONSET_LATENCY.md`'s correction measured p50 of a transmit run at 1.52 s, and at 1.10×
that repays ~150 ms of a debt an order of magnitude larger.) The cues are purely
informational — they make the loss predictable rather than mysterious.

### Capture coming

```rust
(660.0, 120), (0.0, 250),   // bop
(660.0, 120), (0.0, 250),   // bop
(660.0, 120), (0.0, 250),   // bop
(988.0, 140), (0.0,  20),   // bo
(1318.5, 230),              // beep  — 1500 ms total
```

- **Ends on 1318.5 Hz, which is already `TransmitStart`** — the countdown resolves
  onto the pitch that already means "you are live", so the grammar extends rather than
  competes.
- **Every frequency sits inside the telephone band.** This plays over HFP, where CVSD
  gives roughly 300–3400 Hz. A tone outside it would work on a desk and vanish on the
  bike.
- **Three evenly spaced bops make the wait legible.** Silence reads as a fault; a
  metronome reads as a wait with an end.
- **Variable length, because SCO is.** The negotiation has no fixed duration and the
  legacy Android path allows up to `SCO_TIMEOUT_MS = 4_000`, so a fixed render cannot
  end when capture becomes live. The bops are re-triggered one at a time while waiting
  and the rising pair fires on ready — `play_cue`'s replace-on-write supports this
  as-is. The list above is the shape, not a single render.

### Capture stopped

```rust
(1318.5, 120), (0.0, 20),
(988.0,  120), (0.0, 20),
(660.0,  160), (0.0, 15),
(-1.0,    60),              // squelch tail — negative frequency is a noise burst
```

~515 ms, the same three pitches descending. Deliberately short: there is no wait to
fill, and the asymmetry carries meaning — long and open-ended means *coming*, brief
and decisive means *done*. The closing squelch is borrowed from `TransmitEnd`, which
already uses it to mean "I have stopped transmitting".

**The failure modes are not symmetric and this one must err late.** An early start cue
costs a lost sentence; an early stop cue puts a curse on the channel. So: fire only
once the input stream is **confirmed closed**, and **if the assertion fails, play
nothing** — silence leaves the rider cautious, a false all-clear does the opposite.

### Tap mode armed (on connect)

**N short bops matching the configured tap count.** Required, not a nicety: in tap mode
capture starts off, so without it the first thing a rider does is talk into a dead
microphone. One cue answers both questions on connecting — tap mode is on, and it is
set to three — and it rehearses the rhythm they are about to perform, so the gesture
teaches itself. Generated from the setting rather than written down.

### Cue priorities

**A low-priority cue must not interrupt a high-priority one.** `play_cue` replaces the
queue unconditionally — `q.clear(); q.extend(pcm)` — so the last writer wins. Harmless
until now because every existing cue is 40–150 ms; a waiting countdown is live for as
long as a transition takes, so a cue landing mid-pattern becomes likely, and the one
that would do it (a participant joining) is the least important thing the app says.

A `priority()` on `AudioCue` beside `segments()` and `amplitude()`, replacing on
**incoming ≥ current**:

- Equal priority still replaces, preserving the existing reason for replace-on-write:
  a flapping connection must not build a backlog.
- A rejected cue is **dropped, not deferred.** No pending queue — that is the backlog
  `play_cue` exists to prevent.
- **The current priority must live under the same lock as `cue_queue`.** The queue is
  drained by the output callback, so whoever empties it clears the priority in the same
  breath. Split across a mutex and an atomic they can disagree — queue empty, priority
  still high, every later cue silently dropped. That failure is inaudible.

Levels: **high** — the capture cues, `MutedByOther`, `DeafenedByOther`,
`Disconnected`, `Test`; **normal** — `TransmitStart`/`End`, `Reconnected`, `Dialing`,
`Suppressed`/`Unsuppressed`; **low** — `ParticipantJoined`/`Left`.

The stop cue has the strongest claim. A truncated start cue leaves the rider waiting;
a truncated stop cue, especially one losing its squelch tail, leaves them unsure
whether the microphone is off — the one question it exists to answer.

**Its own commit, before the cues**, since it changes machinery fourteen existing cues
go through. The renderer is deterministic (fixed-seed LCG), so the test is exact: a low
cue fired during a high one leaves the queue byte-identical.

## Asking for capture: taps

### Taps do both directions, and the count is the robustness knob

Taps start *and* stop capture, so the hard case has to work: **iOS has a single input
route**, so while capturing over HFP the phone's own microphone does not exist and the
detector has only the accelerometer. The tap literature's remedy for pocket false
positives is audio first with the accelerometer confirming — exactly the half missing
when tap-to-stop needs it.

**The 2/3/4 selector buys back what the microphone would have given.** If
single-impulse false alarms arrive at rate λ and the pattern window is W, the chance of
N accidental impulses landing in the right timing lattice falls roughly as (λW)^(N−1),
so each extra tap cuts false starts multiplicatively. Four taps in a rough environment
is the correct answer to a worse signal, not belt-and-braces.

**Three is the minimum that carries timing evidence at all.** Two taps give exactly one
interval, so there is nothing to compare it against. Three give two, four give three,
and deliberate tapping is near-isochronous where road impulses are not. So interval
*consistency* becomes available at three. Two taps rely on magnitude matching alone,
which makes three the defensible default once the feature is enabled.

### The detector: Teager–Kaiser on gravity-removed acceleration

ψ[x(n)] = x(n)² − x(n+1)·x(n−1). Two multiplies, no window, no state, one sample of
latency. For a sinusoid it evaluates to roughly A²·sin²(ω), weighting by **amplitude
squared times frequency squared** — which is the whole reason to prefer it. Engine
rumble, road undulation and leg movement are low-frequency; a tap is a broadband
impulse. The ω² term does the discrimination a filter would otherwise need, with no
phase delay.

Per sample:

1. **Input** `userAcceleration` (gravity removed by the platform's fusion), the
   `gravity` vector and `rotationRate`. Both platforms provide all three —
   `CMDeviceMotion` on iOS, `TYPE_LINEAR_ACCELERATION` / `TYPE_GRAVITY` /
   `TYPE_ROTATION_VECTOR` on Android — and the platform's fusion is better than
   anything worth writing here.
2. **TKEO per axis**, summed to a scalar ψ, per-axis values kept for step 5.
3. **A tracked floor on ψ**, not a constant — the direct reuse of this project's
   existing insight: the floor rises with engine speed and road roughness exactly as
   the audio floor rises with wind, and a fixed threshold is deaf at 120 km/h or
   hair-triggered at idle. **`NoiseFloorTracker` (`dsp.rs:264`) fits almost verbatim**,
   and the cadence lines up: iOS sensors run at 100 Hz, one sample per 10 ms, the same
   rate as an audio block, so `NoiseFloorTracker::new(25)` gives the same ~1.5 s memory
   it gives the voice gate. Android's higher rate decimates onto the same grid.
4. **Candidate impulse** when ψ exceeds the floor by a margin **and falls back below
   within a maximum pulse width.** That "up and back down quickly" test is worth
   copying verbatim from the MEMS vendors (ST's DT0101, NXP's AN3919): it rejects large
   slow motions, which is most of what a thigh produces.
5. **Direction, as a soft prior rather than a gate.** Taps are accepted on the front,
   the back and at an angle, because a phone in a pocket shifts. So no device axis can
   be learned or required, and the per-axis sum in step 2 is what makes the detector
   orientation-agnostic. What is still usable is the *world* frame: gravity gives
   world-vertical in device coordinates, road shock arrives along it, a hand reaching
   to a thigh moves across it. A weighting, not a veto — a bag on top of the thigh is
   tapped downward and an angled tap splits its energy anyway.
6. **Gyro, also a prior**: a tap is linear where a suspension event rotates the bag.
   Established prior art rather than a guess.
7. **The pattern is the primary discriminator**, and allowing any orientation is what
   makes it so. N candidates, each passing the pulse-width test, with **matched
   magnitudes** and — from three up — **consistent intervals**. A latency dead time
   after each candidate skips the mechanical ring so it cannot be counted twice.

Every parameter — margin, pulse width, latency, window, magnitude tolerance, interval
tolerance, the gravity and gyro weightings — is set by measurement.

**Note what moved.** With a single learnable tap axis, direction was the strongest
feature. Supporting front, back and angled taps trades that away deliberately, and the
load shifts onto impulsiveness against the tracked floor plus the N-tap pattern.

### Where it lives

**Rust, beside the rest of the DSP**, because the floor tracker is already there, it
gets deterministic unit tests over recorded timelines, and every other signal decision
in this project lives there. Sensors arrive over a new `mumbleway/motion` channel
written into the existing native files — roughly sixty lines of `SensorManager` and
`CMDeviceMotion` — rather than a plugin, because the rate, batching and the Android 12
`HIGH_SAMPLING_RATE_SENSORS` permission all need explicit control and this project is
deliberately light on dependencies.

**There are no sensors in the project today**: no plugin, no permission, no
`SensorManager`/`CMMotionManager`, no Rust consumer. Greenfield including the
permission.

### Two honest cautions

The sub-1% false-positive figures in the tap literature come from **stationary**
settings and must not be carried across. And TKEO's ω² advantage is weakest exactly
where it is needed most: at iOS's 100 Hz ceiling a tap's high-frequency content is
above Nyquist and aliased, so ψ still spikes on the step but the frequency weighting
buys less than at Android's 400 Hz. That is the crux of whether iOS tap-to-start is
viable, and the first thing the rig should answer.

### A free feasibility probe

Both platforms ship their own tap detectors, tuned by people with far more IMU data:
iOS Back Tap (Settings → Accessibility → Touch) and Pixel's Quick Tap. Map either to
something observable, put the phone in the thigh bag, and ride. If a production
detector cannot do it through a padded bag at speed — or fires when nobody tapped —
that is decisive for the cost of one ride and no code.

## The settings

Two new controls, both **off by default**:

- **Start and stop capture by tapping the phone** — off. When on, a selector for
  **2, 3 or 4 taps**, defaulting to three.
  - **Nested beneath it: stop capturing after X seconds without speech** — off, X
    configurable, **disabled whenever tap mode is off.**

**Auto-stop is a sub-setting of tap mode**, because alone it is a trap: capture would
release itself after the timeout with nothing able to request it again. Nesting makes
the dependency visible rather than enforced by an invisible guard. The Network
section's proxy tile and its two dependent switches are the shape to follow. Turning
tap mode off leaves a stored auto-stop value **inert rather than cleared**, so turning
it back on restores what the rider chose.

**Tap mode and push-to-talk are mutually exclusive.** Push-to-talk exists to put the
microphone on the wire the instant the button goes down; tap mode means it is closed
until tapped, so the button would do nothing until a gesture had already been
performed. The exclusion runs both ways, each with a line saying why, and **neither may
silently change the other** — a rider with push-to-talk who turns on tap mode must be
told, not quietly moved to voice activation.

**Open mic is not excluded and does not need to be.** Tap to open the microphone, tap
to close it is coherent, and arguably the clearest use of the feature. Only
push-to-talk conflicts, because only push-to-talk promises immediacy.

**With tap mode off — the default — nothing changes for anybody.** `activate` starts
capture immediately and the behaviour is today's to the sample.

## Making the state apparent

### To the rider

**The music is the indicator, continuously and for nothing.** Listening is full
bandwidth, capturing is narrowband, so the rider can hear which state they are in at
any moment without a cue or a glance. The feature indicates itself as a side effect of
being the feature — which removes the need for any periodic reminder.

On top of that: the required connect cue above, and **a distinct state on the
microphone button and the overlay** — not the `StatusColors.reconnecting` orange that
already means *the server has silenced you*, which is a different fact. `ptt_button.dart`
only appears in push-to-talk mode, so in tap mode the microphone icon is the whole
visual surface and has to carry it.

### To everyone else

**Capture off is muted for every practical purpose**, so it is sent as muted:
`UserState.self_mute`. The muted icon then shows on every remote roster including
desktop Mumble and Mumla, with no protocol invention. `self_deaf` would be wrong — the
rider hears everything.

Then, **for MumbleWay clients, the rider's transmission mode in the hello** — not a
tap-mode flag. Push-to-talk deserves the same treatment: what a remote roster wants to
show is *why this person is quiet and what to expect from them*.

| Advertised mode | What a remote rider learns |
|---|---|
| Tap to capture | Muted between taps; expect a second or two before an answer |
| Push-to-talk | Quiet unless holding a button; silence means nothing is wrong |
| Voice activated | Will answer when they speak |
| Open mic | Always live |

**The decomposition is what makes this cheap:**

| | Carries | Changes |
|---|---|---|
| `self_mute` | the microphone is closed *now* | every transition |
| the hello's mode field | how this rider transmits | once on connect, and on a settings change |

**`core/src/session/peers.rs` is the carrier and it already exists** — a
MumbleWay-to-MumbleWay hello over `PluginDataTransmission`, whose header states the
purpose outright: *"the foundation for anything MumbleWay-specific that clients exchange
later."* One optional field on `Wire` with `#[serde(default)]`, carried by the same
announce-and-reply exchange that already tells both halves of every pair about each
other.

Three alternatives are ruled out, two of them in that module's own notes:

- ❌ **`Version.release` via `UserStats`** — admin-gated. The server hands a client's
  version only to somebody holding Ban on the root channel, *"so for an ordinary rider
  it is simply not there"*. Which is precisely why `peers.rs` exists.
- ❌ **`plugin_identity` / `plugin_context`** — the proto says on each: *"This value is
  not transmitted to clients."*
- ❌ **A custom `UserState` field** — Murmur reconstructs the message, so unknown
  protobuf fields are dropped before relay.

**No new capability and no new data ID.** The field's presence is self-describing: a
hello without it came from a client that does not send it, which is the "did not say"
case a capability would have encoded. `peers.rs`'s rule — add a capability rather than
bump `PROTOCOL` when a feature is added — is satisfied without either, since an absent
optional field breaks no parser in both directions. And the server's plugin-message
rate limiter, which drops silently when the sender's bucket is empty, stops being a
consideration. **Still a hint, never a credential**: a claimed mode decorates a roster
row and gates nothing.

**The hello is only authoritative if the mode cannot change mid-connection, and today
it can** — by two routes. `settings_screen.dart`'s `RadioGroup<MicMode>` calls
`updateMicMode` with **no connection-state gate**, and `micMode` is in
`_syncedSettings()`, so **another device can change it mid-call** over cloud sync. So
locking it means both: disable the radio group while connected with a line saying why,
*and* defer a synced change until the session ends, following the `canModifyServer`
guard that already defers edits for live sessions. Leaving the sync path open would let
a roster go stale with no visible cause.

The cost is real: a rider cannot switch to push-to-talk on entering a noisy stretch
without disconnecting. **If that proves unacceptable, keep the mode mutable and
re-announce** — `Announcer` and `Peers::unheard` already exist to send a hello again.
Same field, same message, no immutability needed.

**Composition, the bug worth avoiding by name.** The effective flag is *manual mute OR
capture closed*, and starting capture clears **only the capture half**. Otherwise a
rider who muted themselves deliberately and then taps is silently unmuted, surfacing as
"the app unmuted me". Locally that is two booleans behind one icon, with the server's
own mute (`silenced_by_server`) as a third and distinct state.

## A motion track in the recorder

**A third track alongside the existing two, and the first thing to build**, because
without it no claim about false positives or negatives can be checked. A recording
session writes a headerless `.s16` of the raw microphone plus a per-10 ms-block decision
CSV (`Recorded`); this adds a motion file beside them under the same
`start_diagnostic_recording` session — not a separate toggle.

**It must record whether or not tap mode is on.** Measuring *false positives* needs
rides with no taps in them, so the negative corpus can only be gathered while the
feature is off. If motion logging were tied to tap mode being enabled, the corpus that
matters most could never be collected. So the recorder starts the sensors itself for the
duration — which also makes the track useful before any detector exists.

**CSV, like the decision log, not headerless binary like the audio.** Binary for bulk,
CSV for things a person reads, and motion is read by a Python rig. **Do not decimate
Android to 100 Hz to save space**: the higher rate is what makes Android tractable where
iOS may not be.

**All three tracks split at an 18 MB boundary, which is a change of model rather than a
number.** `ROTATE_BYTES` is 16 MiB, justified as *"below Telegram's 20 MB ceiling with
room for the decision log alongside it"* — a **bundle** budget. And the check is
`sink.written >= ROTATE_BYTES` where `written` counts **PCM bytes only**, so rotation is
driven by audio size alone.

- **The boundary becomes 18 MB per file**, each track its own Telegram message rather
  than sharing an allowance. The comment must be rewritten to say so, or the two models
  contradict each other and the next reader "fixes" it back.
- **Check all three sinks, not just the audio.** Audio dominates today — roughly
  96 kB/s against perhaps 40 kB/s for motion at 400 Hz — so an audio-driven check would
  in fact keep the others under. But that is a coincidence of present rates: raise the
  sensor rate or add columns and a motion file would silently exceed the cap. Rotate
  when **any** sink reaches the boundary.
- **Rotate all three together, keeping the shared `{stem}-{index:03}` index.** That
  alignment already exists and is what makes a slice coherent: segment 7 of each file
  covers the same stretch, so a rider can send one segment's three files and the rig has
  a usable window. Independent rotation would force every analysis to stitch by
  timestamp first.

The cost is file count: 18 MB of audio is about three and a half minutes, so a half-hour
ride is roughly nine segments — twenty-seven files. That is the price of the per-file
model, and the reason `RECORDING_INTAKE.md` and `tools/vad/telegram_intake.py` both need
to learn about the third track rather than discover it.

**Alignment is worth designing rather than assuming.** Sensor samples arrive on another
thread with timestamps on a different clock — `SensorEvent.timestamp` is nanoseconds
since boot on Android, `CMLogItem.timestamp` is seconds since boot on iOS, and neither is
the audio callback's clock. So record **both**: the platform timestamp as given, an
arrival timestamp taken in Rust on the same monotonic clock the recorder uses, and the
audio block index the sample falls within. Tap tolerances are tens of milliseconds so
arrival stamping suffices — but keeping the platform stamp means channel jitter can be
*measured* rather than assumed.

**Columns**: the three vectors, both timestamps, the block index. Later, when the
detector exists, its own intermediate values join them — ψ, the tracked floor, candidate
and gesture state — so the rig can see *why* it fired rather than only that it did.
`Recorded`'s `Default` is tests-only and the worker must fill every field.

**It must not disturb the audio thread.** Its own bounded channel on the same
drop-and-count terms as `QUEUE_BLOCKS`, with the dropped count surfaced beside
`dropped_blocks` so a truncated motion track is visible rather than quietly misleading.

`privacy.md` needs the addition: motion joins an already opt-in, rider-shared recording,
so there is no new consent surface, but the document should say so rather than imply it.

## Ordered work

1. **Wire route changes end to end** — handle `routeChanged` in `_ensureHandler`, add an
   Android route callback, push the route to Rust on every change rather than only in
   `_acquireAudio`. Fixes the stale-`routeCode` bug on its own and is the prerequisite for
   the rest.
2. **Output-only operation in the engine**, so a session can exist with no input stream.
3. **`startCapture` / `stopCapture`** on both platforms; `activate` gains the on-demand
   flag; settle the `inputChannels` contract. Reword the A2DP rule in `CLAUDE.md`, which
   is phrased as an absolute prohibition and otherwise reads as a veto on this feature.
4. **Reopen and assert** on each transition; fail loudly on the wrong route.
5. **Cue priorities**, then the capture cues.
6. **The state machine** deciding when capture is wanted, with hysteresis in the spirit
   of `_audioIdleGrace` — 10 seconds, which exists because *"each reopen renegotiates an
   SCO link, which takes a second or two and is audible in the helmet."* Transitions
   should be reluctant: flapping mid-conversation is worse than staying narrowband through
   one.
7. **The motion track** in the recorder, with the 18 MB rotation change.
8. **Sensors** over `mumbleway/motion`, then `tools/tap/`, then the TKEO detector behind
   a calibration flow.
9. **The hello's mode field**, the mode lock, `self_mute` composition and the roster badge.
10. **Settings and docs** — both `.arb` files, `settings.md` and `ru/settings.md`,
    `diagnostics.md`, `privacy.md`.

## Measurements

1. **Does `stopCapture` actually release HFP on iOS without a full deactivate?** The
   `.notifyOthersOnDeactivation` comment says that option is what lets *"a headset fall
   back off the hands-free profile — which is most of the point"*, and that is on full
   deactivation. Whether a category change alone suffices is the first thing to find out,
   because the feature hinges on it.
2. **Transition latency both directions**, measured rather than inherited from "a second
   or two", and measured to the first block the chain will trust — which adds only the
   150 ms of `WARMUP_BLOCKS`, described there as "imperceptible at connect time".
3. **Whether retaking the helmet mic works repeatedly while backgrounded in a pocket.**
   Android hands a backgrounded app digital zero rather than an error, and this is the
   most likely way the feature fails silently in the field.
4. **What the device rate becomes in each state**, confirming the reopen is both
   necessary and sufficient.
5. **Leakage immediately after a transition, with music playing.** `CaptureProcessor::reset()`
   clears `floor`, `auto_floor`, `tilt` and `hangover`, so for over a second the background
   estimate is rebuilding and a too-low floor biases the gate toward **transmitting**. On
   `Auto` the profile chooser re-converges too, and `MUSIC_GATE.md` already attributes the
   music leak to an Auto convergence transient. This feature creates one on every switch,
   in precisely the situation where music is playing.
6. **A blind A/B under the helmet at speed, mSBC against A2DP**, with earplugs and wind.
   **If the difference is inaudible on the bike, this should not be built** — the cheapest
   outcome to discover, and this project's own standard that the rider's ear decides.

## Verification

- **Route asserted and logged at every transition**, with a test that a transition landing
  on the wrong microphone fails loudly rather than recording quietly. Given the chain
  cannot tell, this is the only guard that exists.
- **The bit-exact-zero watchdog exercised deliberately**: background the app, flip states
  repeatedly in a pocket, confirm it fires rather than recording silence.
- **Stream reopen proven** on a headset whose HFP and A2DP rates differ — no resampler
  artefacts, no underrun burst.
- **Cue priority test is exact**, because the renderer is deterministic: a low cue fired
  during a high one leaves the queue byte-identical.
- **`tools/tap/` reporting false arms per hour on held-out rides** — the number that
  decides shippability. Target worth agreeing before building: under one per hour.
- **Per-stage evidence is a desktop reconstruction, not an on-device dump.** The device
  yields the raw `.s16` plus the decision log; `core/tests/road.rs` (behind
  `MUMBLEWAY_ROAD_AUDIO` / `MUMBLEWAY_ROAD_DUMP`) and `preview.rs` rebuild the stages.
- **Time spent capturing per hour of riding** — the number that says whether the feature
  achieved anything.
- A ride with the group: music stays hi-fi, voices are audible over it, and capture comes
  and goes with nothing noticeable but the cues.

## Deliberately not in this version

- **Paydown across the switch gap**, for the reason measured above.
- **Throttle blips and brake taps** as triggers. They work with the phone anywhere on the
  bike, but they are control inputs — a brake tap flashes a false signal at whoever is
  following, and a blip changes speed.
- **Head gestures** — a helmet intercom exposes no motion data, and anything that would is
  additional hardware.
- **MumbleWay owning music playback.** It would allow precise ducking and keeping music
  audible through duplex, but the source is whatever the rider has open, so this rests on
  audio focus instead.
- **Per-utterance audio focus.** Focus is held for the whole session and
  `TRANSIENT_MAY_DUCK` / `.duckOthers` duck other apps for as long as it is held, so in the
  listening state music returns to full *bandwidth* but may stay at ducked *level*. If that
  matters, the answer is per-utterance focus — a separate change.
- **Fixing the music gate.** `MUSIC_GATE.md` stays open; nothing here improves or worsens
  it.
- **GPS for speed** — a location permission this app has deliberately never requested,
  against a no-telemetry promise.
