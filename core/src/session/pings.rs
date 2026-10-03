//! The round trips this client measures, for the server to pass on.
//!
//! **Mumble's ping figures are reported by the client, not measured by the
//! server.** `Server::msgPing` copies them straight across:
//!
//! ```text
//! uSource->dUDPPingAvg  = msg.udp_ping_avg();
//! uSource->dUDPPingVar  = msg.udp_ping_var();
//! uSource->uiUDPPackets = msg.udp_packets();
//! uSource->dTCPPingAvg  = msg.tcp_ping_avg();
//! ```
//!
//! and `msgUserStats` hands those stored values to anybody who asks about this
//! rider. So a client that leaves the fields out is a client whose ping reads as
//! **zero to everybody on the server** — in this app's own roster, and in the
//! official client's information window.
//!
//! That is exactly what this client did, and the unit tests could not have
//! caught it: they test what we do with the numbers the server sends, and the
//! server was faithfully sending back the nothing we gave it. It took a real
//! Murmur and two connected riders to see it.
//!
//! # What is reported
//!
//! An average and a variance over the whole session, by Welford's method — one
//! pass, no stored samples, and no drift from adding a thousand round trips to
//! a running total. Mumble's own client reports the same pair over the same
//! span, which is what makes the figures comparable between clients in a
//! roster.

/// A running mean and variance of one kind of round trip.
#[derive(Debug, Clone, Copy, Default)]
pub struct PingStats {
    count: u64,
    mean: f64,
    /// Sum of squared differences from the running mean.
    m2: f64,
}

impl PingStats {
    /// Notes one round trip, in milliseconds.
    ///
    /// Nonsense is dropped rather than averaged in: a negative or absurd
    /// figure means the clock moved or a reply was stale, and one of those in
    /// the mean would be visible to every other rider for the rest of the ride.
    pub fn record(&mut self, ms: f32) {
        if !ms.is_finite() || !(0.0..=60_000.0).contains(&ms) {
            return;
        }
        let x = ms as f64;
        self.count += 1;
        let delta = x - self.mean;
        self.mean += delta / self.count as f64;
        self.m2 += delta * (x - self.mean);
    }

    pub fn count(&self) -> u64 {
        self.count
    }

    /// The average round trip, or 0 before anything has been measured.
    ///
    /// Zero is also what the field means when it is absent, so a client with
    /// nothing to report says the same thing either way.
    pub fn mean(&self) -> f32 {
        self.mean as f32
    }

    /// The variance of the round trip, or 0 with fewer than two samples.
    pub fn variance(&self) -> f32 {
        if self.count < 2 {
            return 0.0;
        }
        (self.m2 / self.count as f64) as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_measured_reports_nothing() {
        let p = PingStats::default();
        assert_eq!(p.mean(), 0.0);
        assert_eq!(p.variance(), 0.0);
        assert_eq!(p.count(), 0);
    }

    #[test]
    fn a_steady_link_has_no_variance() {
        let mut p = PingStats::default();
        for _ in 0..10 {
            p.record(40.0);
        }
        assert!((p.mean() - 40.0).abs() < 1e-3);
        assert!(p.variance() < 1e-6);
    }

    #[test]
    fn the_average_is_of_everything_seen() {
        let mut p = PingStats::default();
        for ms in [10.0, 20.0, 30.0, 40.0] {
            p.record(ms);
        }
        assert!((p.mean() - 25.0).abs() < 1e-3);
        // Population variance of 10,20,30,40 is 125.
        assert!((p.variance() - 125.0).abs() < 1e-2);
    }

    #[test]
    fn a_single_sample_has_no_variance_rather_than_a_wrong_one() {
        let mut p = PingStats::default();
        p.record(33.0);
        assert_eq!(p.mean(), 33.0);
        assert_eq!(p.variance(), 0.0, "and not a division by zero");
    }

    #[test]
    fn a_clock_that_jumped_is_not_averaged_in() {
        // A reply timestamped in the future, or a system clock moved while
        // riding, would otherwise sit in the figure every other rider sees for
        // the rest of the session.
        let mut p = PingStats::default();
        p.record(40.0);
        p.record(-5.0);
        p.record(f32::NAN);
        p.record(1_000_000.0);
        assert_eq!(p.count(), 1);
        assert_eq!(p.mean(), 40.0);
    }

    #[test]
    fn a_long_session_does_not_drift() {
        // Ten thousand round trips around 40 ms: a running total would be
        // accumulating error by now, which is the reason for Welford.
        let mut p = PingStats::default();
        for i in 0..10_000 {
            p.record(if i % 2 == 0 { 39.0 } else { 41.0 });
        }
        assert!((p.mean() - 40.0).abs() < 1e-3);
        assert!((p.variance() - 1.0).abs() < 1e-2);
    }
}
