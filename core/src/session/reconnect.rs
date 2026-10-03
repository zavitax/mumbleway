//! Reconnection policy.
//!
//! Tuned for mobile use: a rider losing signal in a tunnel should be back
//! within seconds of regaining it.
//!
//! # The interval used to be flat, and a real server disproved it
//!
//! **This file argued for a fixed ten seconds.** The reasoning was that backoff
//! exists to spare a struggling server, that this is a voice client for a small
//! group, and that a rider out of signal is the person a lengthening delay
//! punishes most — the wait would be longest at the moment coverage returns.
//!
//! Every word of that is still true and the conclusion was still wrong, because
//! it left the server out of it. **Murmur bans an address that connects too
//! often**, and the defaults are not generous: `autobanAttempts = 10` within
//! `autobanTimeframe = 120` seconds earns `autobanTime = 300` seconds of
//! refusal. `Meta::banCheck` counts *every* attempt, not only failed ones.
//!
//! Ten-second retries are twelve attempts in two minutes. So a rider in a long
//! dead spot was earning themselves a five-minute ban from their own server at
//! the two-minute mark — turning a two-minute outage into a seven-minute one,
//! and reporting it as a refused connection rather than as anything a rider
//! could act on. It was found by running this client against a real Murmur:
//! the server log says `Ignoring connection: … (Global ban)` while its ban list
//! is empty, because an autoban is not in the ban list.
//!
//! So the interval grows now, and the shape is chosen against that rule rather
//! than against a feeling: quick while a tunnel is the likely cause, and well
//! inside ten attempts per two minutes by the time it could matter.
//! [`BackoffPolicy::attempts_within`] measures it, and a test holds it there.
//!
//! The first attempt is still immediate, the countdown is still honest about
//! the wait, and the reconnect is still cancelled outright when the OS reports
//! connectivity is back — so the common case, which is a short gap, is
//! unaffected by any of this.
//!
//! A second of jitter is added either side, so a room full of clients that all
//! dropped together — which is what happens when a server restarts — spread
//! their return over a two-second window instead of arriving in lockstep.

use std::time::Duration;

use crate::error::DisconnectReason;

/// Wait before the first reconnection attempt.
///
/// Short, because the common reason to be here is a tunnel.
pub const RETRY_INTERVAL: Duration = Duration::from_secs(5);

/// How much longer each attempt waits than the one before.
pub const RETRY_MULTIPLIER: f64 = 1.7;

/// The longest this ever waits between attempts.
///
/// A minute is well inside what a rider will tolerate when the alternative is
/// being refused outright, and the OS telling us connectivity is back skips
/// the wait entirely.
pub const RETRY_MAX: Duration = Duration::from_secs(60);

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
    pub initial: Duration,
    pub max: Duration,
    pub multiplier: f64,
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
            max: RETRY_MAX,
            multiplier: RETRY_MULTIPLIER,
            jitter: RETRY_JITTER,
        }
    }
}

impl BackoffPolicy {
    /// Delay before attempt `attempt` (0-based), before jitter.
    pub fn base_delay(&self, attempt: u32) -> Duration {
        // Saturate the exponent rather than overflowing on a long outage.
        let factor = self.multiplier.powi(attempt.min(32) as i32);
        let millis = (self.initial.as_millis() as f64 * factor).min(self.max.as_millis() as f64);
        Duration::from_millis(millis as u64)
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
    fn the_first_retry_is_quick_because_a_tunnel_is_the_likely_reason() {
        let p = BackoffPolicy::default();
        assert_eq!(p.base_delay(0), RETRY_INTERVAL);
        assert!(
            p.base_delay(0) <= Duration::from_secs(5),
            "a short gap must not be made long by the policy"
        );
    }

    #[test]
    fn the_wait_grows_and_then_stops_growing() {
        let p = BackoffPolicy::default();
        let mut last = p.base_delay(0);
        for attempt in 1..8 {
            let d = p.base_delay(attempt);
            assert!(d >= last, "attempt {attempt} went backwards");
            last = d;
        }
        assert_eq!(p.base_delay(50), RETRY_MAX, "and it settles at the ceiling");
    }

    #[test]
    fn the_policy_stays_under_what_a_server_will_ban_for() {
        // **This is the test the flat ten-second interval failed.** Murmur
        // bans an address for five minutes after more than ten connection
        // attempts in two minutes, counting every attempt rather than only
        // the failures — so a rider in a long dead spot was earning a ban from
        // their own server at the two-minute mark.
        let p = BackoffPolicy::default();
        let attempts = p.attempts_within(SERVER_AUTOBAN_WINDOW);
        assert!(
            attempts < SERVER_AUTOBAN_ATTEMPTS,
            "{attempts} attempts in {SERVER_AUTOBAN_WINDOW:?} would be banned"
        );
        // And with room to spare, because the figures are a server's defaults
        // and an admin may tighten them.
        assert!(
            attempts <= SERVER_AUTOBAN_ATTEMPTS / 2 + 2,
            "{attempts} attempts leaves no margin for a stricter server"
        );
    }

    #[test]
    fn a_flat_ten_second_policy_would_still_be_banned() {
        // Kept as the counter-example: it is what this file used to do, and
        // the arithmetic that disproved it should be executable rather than
        // only written down.
        let flat = BackoffPolicy {
            initial: Duration::from_secs(10),
            max: Duration::from_secs(10),
            multiplier: 1.0,
            jitter: RETRY_JITTER,
        };
        assert!(
            flat.attempts_within(SERVER_AUTOBAN_WINDOW) > SERVER_AUTOBAN_ATTEMPTS,
            "the old policy should trip the rule this one avoids"
        );
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
        // At the ceiling, where clipping would show.
        assert_eq!(p.base_delay(50), p.max);
        assert!(
            p.delay_with_sample(50, 1.0) > p.max,
            "upward jitter was clipped away"
        );
    }

    #[test]
    fn a_growing_policy_still_bounds_its_curve() {
        // The default is flat, but the mechanism is not, and a caller that
        // configures growth must still plateau rather than grow without bound.
        let p = BackoffPolicy {
            initial: Duration::from_millis(500),
            max: Duration::from_secs(8),
            multiplier: 1.8,
            jitter: Duration::from_millis(250),
        };
        assert!(p.base_delay(1) > p.base_delay(0), "delay must grow");
        assert_eq!(p.base_delay(50), p.max);
        for attempt in 0..40 {
            for sample in [0.0, 0.5, 1.0] {
                assert!(
                    p.delay_with_sample(attempt, sample) <= p.max + p.jitter,
                    "attempt {attempt} at sample {sample} exceeded the ceiling"
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
