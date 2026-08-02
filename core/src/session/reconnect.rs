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
//! A second of jitter is added either side, so a room full of clients that all
//! dropped together — which is what happens when a server restarts — spread
//! their return over a two-second window instead of arriving in lockstep.
//!
//! The mechanism still takes a multiplier and a ceiling, because a caller with
//! a different server population may want them; it is only the default that is
//! flat.

use std::time::Duration;

use crate::error::DisconnectReason;

/// Wait between reconnection attempts.
pub const RETRY_INTERVAL: Duration = Duration::from_secs(10);

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
            max: RETRY_INTERVAL,
            multiplier: 1.0,
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
    fn the_interval_is_flat_at_every_attempt() {
        // No growth: the wait must not lengthen the longer someone has been out
        // of signal, since that is when they most want to be back.
        let p = BackoffPolicy::default();
        for attempt in [0, 1, 2, 5, 20, 100] {
            assert_eq!(
                p.base_delay(attempt),
                RETRY_INTERVAL,
                "attempt {attempt} did not wait the flat interval"
            );
        }
    }

    #[test]
    fn jitter_spreads_a_second_either_side_and_stays_centred() {
        let p = BackoffPolicy::default();
        for attempt in [0, 3, 40] {
            // The extremes land exactly one second out, and the midpoint lands
            // on the interval itself.
            assert_eq!(
                p.delay_with_sample(attempt, 0.0),
                RETRY_INTERVAL - RETRY_JITTER
            );
            assert_eq!(p.delay_with_sample(attempt, 0.5), RETRY_INTERVAL);
            assert_eq!(
                p.delay_with_sample(attempt, 1.0),
                RETRY_INTERVAL + RETRY_JITTER
            );

            // And nothing in between escapes the band.
            for i in 0..=20 {
                let d = p.delay_with_sample(attempt, i as f64 / 20.0);
                assert!(
                    d >= RETRY_INTERVAL - RETRY_JITTER && d <= RETRY_INTERVAL + RETRY_JITTER,
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
        assert_eq!(
            p.base_delay(0),
            p.max,
            "the flat default sits at its ceiling"
        );
        assert!(
            p.delay_with_sample(0, 1.0) > p.max,
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
