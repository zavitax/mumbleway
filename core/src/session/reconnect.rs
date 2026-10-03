//! Reconnection policy.
//!
//! Tuned for mobile use: a rider losing signal in a tunnel should be back
//! within seconds of regaining it.
//!
//! **Six attempts ten seconds apart, then every fifteen.** A minute of quick
//! retries covers what a tunnel, a bridge or a dead spot actually costs, and
//! after that the gap widens a little rather than growing without end — the
//! rider most in need of getting back is the one who has been out longest, and
//! a policy that doubles its way to several minutes punishes exactly them.
//!
//! # The server has an opinion about how often you may try
//!
//! An earlier version of this file argued for a flat ten seconds for ever, and
//! a real Murmur disproved the "for ever" half: it bans an address that
//! connects more than `autobanAttempts` (10) times within `autobanTimeframe`
//! (120 s), for `autobanTime` (300 s), and `Meta::banCheck` counts every
//! attempt rather than only the failed ones. The server log says
//! `Ignoring connection: … (Global ban)` while its ban list is empty, because
//! an autoban is not a ban.
//!
//! Where this schedule stands against that rule is worth stating plainly
//! rather than implying: an attempt at zero and then ten seconds apart to the
//! minute, fifteen after, is **eleven attempts inside the server's two-minute
//! window** — one over its default. [`BackoffPolicy::attempts_within`]
//! computes it and a test pins the number, so a future change to the schedule
//! has to look at it.
//!
//! That is a deliberate trade and not an oversight, for two reasons. The
//! attempts a rider makes from a dead spot **never reach the server**, so the
//! server never counts them — the case this schedule exists for is exactly the
//! case the rule cannot see. And where every attempt does reach a server that
//! is up and refusing, five minutes of being told to wait is survivable, while
//! a rider waiting minutes between attempts at the roadside is not.
//!
//! The first attempt is immediate, the countdown states the wait honestly, and
//! the reconnect is cancelled outright when the OS reports connectivity is
//! back — so the common case, a short gap, waits for none of this.
//!
//! A second of jitter is added either side, so a room full of clients that all
//! dropped together — which is what happens when a server restarts — spread
//! their return over a two-second window instead of arriving in lockstep.

use std::time::Duration;

use crate::error::DisconnectReason;

/// Wait between the first few reconnection attempts.
pub const RETRY_INTERVAL: Duration = Duration::from_secs(10);

/// How many attempts wait [`RETRY_INTERVAL`] before the longer gap starts.
pub const RETRY_QUICK_ATTEMPTS: u32 = 6;

/// Wait between attempts after the quick ones.
///
/// Longer, but not much: the point is to stop hammering a server that is not
/// answering, not to make a rider wait minutes at the roadside.
pub const RETRY_LATER_INTERVAL: Duration = Duration::from_secs(15);

/// What a default Murmur allows before it bans an address, and the window it
/// counts in — `autobanAttempts` and `autobanTimeframe`.
///
/// Here so the test that holds the policy under them says *why* the numbers
/// are what they are.
pub const SERVER_AUTOBAN_ATTEMPTS: u32 = 10;
pub const SERVER_AUTOBAN_WINDOW: Duration = Duration::from_secs(120);

/// How far either side of the interval an attempt may land.
pub const RETRY_JITTER: Duration = Duration::from_secs(1);

#[derive(Debug, Clone)]
pub struct BackoffPolicy {
    /// What the first [`BackoffPolicy::quick_attempts`] attempts wait.
    pub initial: Duration,
    /// What every attempt after those waits.
    pub later: Duration,
    /// How many attempts use [`BackoffPolicy::initial`].
    pub quick_attempts: u32,
    /// How far either side of the delay an attempt may land.
    ///
    /// An absolute amount rather than a fraction of the delay: the point is to
    /// break up a simultaneous stampede, which needs the same spread whatever
    /// the interval, and "give or take a second" is something the countdown
    /// beside it can be honest about.
    pub jitter: Duration,
}

impl Default for BackoffPolicy {
    fn default() -> Self {
        Self {
            initial: RETRY_INTERVAL,
            later: RETRY_LATER_INTERVAL,
            quick_attempts: RETRY_QUICK_ATTEMPTS,
            jitter: RETRY_JITTER,
        }
    }
}

impl BackoffPolicy {
    /// Delay before attempt `attempt` (0-based), before jitter.
    pub fn base_delay(&self, attempt: u32) -> Duration {
        if attempt < self.quick_attempts {
            self.initial
        } else {
            self.later
        }
    }

    /// Applies jitter using a caller-supplied sample in `0.0..=1.0`, which keeps
    /// this deterministic under test.
    pub fn delay_with_sample(&self, attempt: u32, sample: f64) -> Duration {
        let base = self.base_delay(attempt).as_millis() as f64;
        let jitter = self.jitter.as_millis() as f64;
        // Map 0..=1 onto -jitter..=+jitter, centred on the base delay.
        let offset = jitter * (sample.clamp(0.0, 1.0) * 2.0 - 1.0);
        // The ceiling bounds the curve, which `base_delay` has already applied.
        // Clamping again here would chop off the upper half of the jitter and
        // leave it one-sided, which is the opposite of spreading a stampede.
        Duration::from_millis((base + offset).max(0.0) as u64)
    }

    /// Applies jitter from the thread RNG.
    pub fn delay(&self, attempt: u32) -> Duration {
        use rand::Rng;
        self.delay_with_sample(attempt, rand::thread_rng().gen::<f64>())
    }
}

impl BackoffPolicy {
    /// How many connection attempts this policy makes inside `window`.
    ///
    /// Counted the way a server counts them: the attempt that failed and put
    /// this client into reconnecting is already on the server's tally, so it is
    /// the first one here. Jitter is left out — it is symmetrical, and the
    /// question is whether the *shape* fits.
    ///
    /// Exists because a server bans an address that tries too often; see the
    /// note at the top of this file.
    pub fn attempts_within(&self, window: Duration) -> u32 {
        let mut attempts = 1;
        let mut elapsed = Duration::ZERO;
        for attempt in 0..1_000 {
            elapsed += self.base_delay(attempt);
            if elapsed > window {
                break;
            }
            attempts += 1;
        }
        attempts
    }
}

/// How long a connection must stay healthy before we forgive earlier failures.
pub const HEALTHY_RESET_AFTER: Duration = Duration::from_secs(30);

/// Tracks retry state across a session's lifetime.
#[derive(Debug)]
pub struct ReconnectState {
    policy: BackoffPolicy,
    attempt: u32,
    /// Set when the user explicitly disconnected; blocks all automatic retries.
    stopped_by_user: bool,
}

impl ReconnectState {
    pub fn new(policy: BackoffPolicy) -> Self {
        Self {
            policy,
            attempt: 0,
            stopped_by_user: false,
        }
    }

    pub fn attempt(&self) -> u32 {
        self.attempt
    }

    pub fn stopped_by_user(&self) -> bool {
        self.stopped_by_user
    }

    /// Records a user-initiated disconnect. Nothing reconnects until [`Self::arm`].
    pub fn stop(&mut self) {
        self.stopped_by_user = true;
    }

    /// Re-enables automatic reconnection (a fresh user-initiated connect).
    pub fn arm(&mut self) {
        self.stopped_by_user = false;
        self.attempt = 0;
    }

    /// Called after a connection has been healthy long enough to count as good.
    pub fn note_healthy(&mut self) {
        self.attempt = 0;
    }

    /// Decides what to do after a disconnect.
    ///
    /// Returns `None` when the session should stay down, or the delay to wait
    /// before the next attempt.
    pub fn on_disconnect(&mut self, reason: &DisconnectReason) -> Option<Duration> {
        if self.stopped_by_user || !reason.is_recoverable() {
            self.stopped_by_user = true;
            return None;
        }

        let delay = self.policy.delay(self.attempt);
        self.attempt = self.attempt.saturating_add(1);
        Some(delay)
    }

    /// Called when the OS reports connectivity returned; collapses the backoff so
    /// the next attempt happens almost immediately.
    pub fn on_network_available(&mut self) {
        if !self.stopped_by_user {
            self.attempt = 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn six_attempts_ten_seconds_apart_then_fifteen() {
        // The whole rule, written out. A minute of quick retries covers what a
        // tunnel costs; after that the gap widens a little and stays there.
        let p = BackoffPolicy::default();
        for attempt in 0..RETRY_QUICK_ATTEMPTS {
            assert_eq!(
                p.base_delay(attempt),
                RETRY_INTERVAL,
                "attempt {attempt} should be one of the quick ones"
            );
        }
        for attempt in [RETRY_QUICK_ATTEMPTS, RETRY_QUICK_ATTEMPTS + 1, 50, 1000] {
            assert_eq!(
                p.base_delay(attempt),
                RETRY_LATER_INTERVAL,
                "attempt {attempt} should be one of the later ones"
            );
        }
    }

    #[test]
    fn the_wait_never_grows_without_end() {
        // The point of the second step is to stop hammering a server, not to
        // make a rider wait minutes at the roadside. Whatever else changes,
        // nothing here may exceed the later interval.
        let p = BackoffPolicy::default();
        for attempt in 0..100 {
            assert!(
                p.base_delay(attempt) <= RETRY_LATER_INTERVAL,
                "attempt {attempt} waits longer than the policy's longest gap"
            );
        }
    }

    #[test]
    fn how_this_stands_against_a_servers_autoban() {
        // **Pinned rather than asserted to be safe, because it is not.**
        // Murmur bans an address that connects more than ten times inside two
        // minutes, counting every attempt; this schedule makes eleven,
        // including the one that failed and started it. That is a deliberate
        // trade — see the note at the top of this file — and the number is
        // here so a future change to the schedule has to look at it.
        let p = BackoffPolicy::default();
        assert_eq!(
            p.attempts_within(SERVER_AUTOBAN_WINDOW),
            11,
            "the schedule changed; check it against autobanAttempts again"
        );
        assert_eq!(
            SERVER_AUTOBAN_ATTEMPTS, 10,
            "the server's default, for scale"
        );
    }

    #[test]
    fn a_policy_can_be_built_that_stays_under_the_rule() {
        // The mechanism allows it even though the default does not use it, so
        // the knob is real rather than theoretical: twenty seconds after the
        // quick ones brings it to ten attempts, which is inside the limit.
        let gentler = BackoffPolicy {
            initial: RETRY_INTERVAL,
            later: Duration::from_secs(20),
            quick_attempts: RETRY_QUICK_ATTEMPTS,
            jitter: RETRY_JITTER,
        };
        assert!(gentler.attempts_within(SERVER_AUTOBAN_WINDOW) <= SERVER_AUTOBAN_ATTEMPTS);
    }

    #[test]
    fn jitter_spreads_a_second_either_side_and_stays_centred() {
        let p = BackoffPolicy::default();
        for attempt in [0, 3, 40] {
            // The extremes land exactly one second out of whatever this
            // attempt's wait is, and the midpoint lands on it.
            let base = p.base_delay(attempt);
            assert_eq!(p.delay_with_sample(attempt, 0.0), base - RETRY_JITTER);
            assert_eq!(p.delay_with_sample(attempt, 0.5), base);
            assert_eq!(p.delay_with_sample(attempt, 1.0), base + RETRY_JITTER);

            // And nothing in between escapes the band.
            for i in 0..=20 {
                let d = p.delay_with_sample(attempt, i as f64 / 20.0);
                assert!(
                    d >= base - RETRY_JITTER && d <= base + RETRY_JITTER,
                    "sample {i} produced {d:?}, outside the band"
                );
            }
        }
    }

    #[test]
    fn jitter_is_not_chopped_off_at_the_ceiling() {
        // The ceiling bounds the curve, not the jitter. Clamping the jittered
        // value would leave the spread one-sided — every client landing at or
        // below the interval — which is the stampede it exists to break up.
        let p = BackoffPolicy::default();
        // On the longer step, where clipping would show.
        assert_eq!(p.base_delay(50), p.later);
        assert!(
            p.delay_with_sample(50, 1.0) > p.later,
            "upward jitter was clipped away"
        );
    }

    #[test]
    fn a_configured_policy_still_bounds_its_waits() {
        // The default is not the only shape the mechanism allows, and whatever
        // a caller configures must still plateau rather than grow without
        // bound — a wait nobody can predict is a wait nobody can be told about.
        let p = BackoffPolicy {
            initial: Duration::from_millis(500),
            later: Duration::from_secs(8),
            quick_attempts: 3,
            jitter: Duration::from_millis(250),
        };
        assert_eq!(p.base_delay(0), p.initial);
        assert_eq!(p.base_delay(3), p.later);
        for attempt in 0..40 {
            for sample in [0.0, 0.5, 1.0] {
                assert!(
                    p.delay_with_sample(attempt, sample) <= p.later + p.jitter,
                    "attempt {attempt} at sample {sample} exceeded the longest wait"
                );
            }
        }
    }

    #[test]
    fn user_disconnect_never_reconnects() {
        let mut s = ReconnectState::new(BackoffPolicy::default());
        assert_eq!(s.on_disconnect(&DisconnectReason::UserRequested), None);
        assert!(s.stopped_by_user());

        // Even a subsequent transport failure must not resurrect it.
        assert_eq!(
            s.on_disconnect(&DisconnectReason::TransportLost("reset".into())),
            None,
            "a user disconnect must latch"
        );
        // ...nor should a network-available signal.
        s.on_network_available();
        assert_eq!(s.on_disconnect(&DisconnectReason::PingTimeout), None);
    }

    #[test]
    fn ping_timeout_reconnects() {
        // This is the case the requirements call out explicitly.
        let mut s = ReconnectState::new(BackoffPolicy::default());
        assert!(s.on_disconnect(&DisconnectReason::PingTimeout).is_some());
        assert_eq!(s.attempt(), 1);
    }

    #[test]
    fn anything_that_might_pass_gets_retried() {
        for reason in [
            DisconnectReason::PingTimeout,
            DisconnectReason::TransportLost("reset by peer".into()),
            DisconnectReason::ServerRejected {
                reason: "server is full".into(),
                retry: true,
            },
            DisconnectReason::HandshakeTimeout,
            DisconnectReason::Error("unknown".into()),
        ] {
            let mut s = ReconnectState::new(BackoffPolicy::default());
            assert!(
                s.on_disconnect(&reason).is_some(),
                "{reason:?} should be retried"
            );
        }
    }

    #[test]
    fn a_rejection_that_cannot_change_stops_instead_of_looping() {
        // A wrong password is wrong at every attempt. Retrying it hammers the
        // server, risks a ban for repeated failures, and hides the server's
        // own explanation behind a spinner reading "reconnecting" — which
        // leaves the one person who could fix it with no idea what is wrong.
        let mut s = ReconnectState::new(BackoffPolicy::default());
        let reason = DisconnectReason::ServerRejected {
            reason: "wrong password".into(),
            retry: false,
        };
        assert!(s.on_disconnect(&reason).is_none());
        assert!(
            s.stopped_by_user(),
            "a permanent rejection must settle, not sit in a retry loop"
        );
    }

    #[test]
    fn reconnecting_after_an_explicit_reconnect_request_is_allowed() {
        let mut s = ReconnectState::new(BackoffPolicy::default());
        s.on_disconnect(&DisconnectReason::UserRequested);
        assert!(s.stopped_by_user());

        // The user pressing Connect again re-arms everything.
        s.arm();
        assert!(!s.stopped_by_user());
        assert_eq!(s.attempt(), 0);
        assert!(s.on_disconnect(&DisconnectReason::PingTimeout).is_some());
    }

    #[test]
    fn attempts_escalate_then_reset_when_healthy() {
        let mut s = ReconnectState::new(BackoffPolicy::default());
        for _ in 0..5 {
            s.on_disconnect(&DisconnectReason::PingTimeout);
        }
        assert_eq!(s.attempt(), 5);

        s.note_healthy();
        assert_eq!(
            s.attempt(),
            0,
            "a healthy connection forgives past failures"
        );
    }

    #[test]
    fn regained_connectivity_collapses_the_backoff() {
        let mut s = ReconnectState::new(BackoffPolicy::default());
        for _ in 0..6 {
            s.on_disconnect(&DisconnectReason::TransportLost("down".into()));
        }
        assert!(s.attempt() >= 6);

        s.on_network_available();
        assert_eq!(
            s.attempt(),
            0,
            "should retry immediately when signal returns"
        );
    }

    #[test]
    fn attempt_counter_cannot_overflow() {
        let mut s = ReconnectState::new(BackoffPolicy::default());
        s.attempt = u32::MAX - 1;
        s.on_disconnect(&DisconnectReason::PingTimeout);
        s.on_disconnect(&DisconnectReason::PingTimeout);
        assert_eq!(s.attempt(), u32::MAX, "must saturate, not wrap");
    }
}
