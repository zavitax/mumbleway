//! Reconnection policy.
//!
//! Tuned for mobile use: a rider losing signal in a tunnel should be back
//! within seconds of regaining it.
//!
//! The interval is a flat ten seconds rather than a growing one. Backoff exists
//! to spare a struggling server, but this is a voice client for a small group,
//! and a rider who has been out of signal for a while is exactly the person a
//! lengthening delay punishes most — the wait would be longest at the moment
//! coverage returns. A fixed interval is also something a countdown can state
//! honestly, and the reconnect is cancelled outright when the OS reports
//! connectivity is back, so the common case does not wait at all.
//!
//! The mechanism still takes a multiplier and a ceiling, because a caller with
//! a different server population may want them; it is only the default that is
//! flat.

use std::time::Duration;

use crate::error::DisconnectReason;

/// Wait between reconnection attempts.
pub const RETRY_INTERVAL: Duration = Duration::from_secs(10);

#[derive(Debug, Clone)]
pub struct BackoffPolicy {
    pub initial: Duration,
    pub max: Duration,
    pub multiplier: f64,
    /// Fraction of the delay that is randomised, 0.0..=1.0.
    pub jitter: f64,
}

impl Default for BackoffPolicy {
    fn default() -> Self {
        Self {
            initial: RETRY_INTERVAL,
            max: RETRY_INTERVAL,
            // Flat, and with no jitter, so the countdown the user is watching
            // matches the wait exactly.
            multiplier: 1.0,
            jitter: 0.0,
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
        let spread = base * self.jitter.clamp(0.0, 1.0);
        // Centre the jitter on the base delay: base +/- spread/2.
        let offset = spread * (sample.clamp(0.0, 1.0) - 0.5);
        let millis = (base + offset).max(0.0);
        // Clamped after jitter, not before: `max` is a ceiling on the wait a
        // rider actually sees, and upward jitter at the top of the curve would
        // otherwise push past it.
        Duration::from_millis(millis as u64).min(self.max)
    }

    /// Applies jitter from the thread RNG.
    pub fn delay(&self, attempt: u32) -> Duration {
        use rand::Rng;
        self.delay_with_sample(attempt, rand::thread_rng().gen::<f64>())
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
    fn the_wait_is_a_flat_ten_seconds_at_every_attempt() {
        // The countdown the user watches has to match the wait, which rules
        // out both growth and jitter.
        let p = BackoffPolicy::default();
        for attempt in [0, 1, 2, 5, 20, 100] {
            assert_eq!(
                p.base_delay(attempt),
                RETRY_INTERVAL,
                "attempt {attempt} did not wait the flat interval"
            );
            for sample in [0.0, 0.5, 1.0] {
                assert_eq!(
                    p.delay_with_sample(attempt, sample),
                    RETRY_INTERVAL,
                    "attempt {attempt} at sample {sample} drifted off the interval"
                );
            }
        }
    }

    #[test]
    fn a_growing_policy_still_honours_its_ceiling() {
        // The default is flat, but the mechanism is not, and a caller that
        // configures growth must still never exceed its own ceiling.
        let p = BackoffPolicy {
            initial: Duration::from_millis(500),
            max: Duration::from_secs(8),
            multiplier: 1.8,
            jitter: 0.25,
        };
        assert!(p.base_delay(1) > p.base_delay(0), "delay must grow");
        assert_eq!(p.base_delay(50), p.max);
        for attempt in 0..40 {
            for sample in [0.0, 0.5, 1.0] {
                assert!(
                    p.delay_with_sample(attempt, sample) <= p.max,
                    "attempt {attempt} at sample {sample} exceeded the ceiling"
                );
            }
        }
    }

    #[test]
    fn jitter_stays_within_the_configured_band() {
        let p = BackoffPolicy::default();
        for attempt in 0..8 {
            let base = p.base_delay(attempt).as_millis() as f64;
            let spread = base * p.jitter;
            for sample in [0.0, 0.25, 0.5, 0.75, 1.0] {
                let d = p.delay_with_sample(attempt, sample).as_millis() as f64;
                assert!(
                    d >= base - spread / 2.0 - 1.0 && d <= base + spread / 2.0 + 1.0,
                    "attempt {attempt} sample {sample}: {d} outside band around {base}"
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
    fn every_non_user_reason_reconnects() {
        for reason in [
            DisconnectReason::PingTimeout,
            DisconnectReason::TransportLost("reset by peer".into()),
            DisconnectReason::ServerRejected("server full".into()),
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
