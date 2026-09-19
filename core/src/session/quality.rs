//! What the server knows about each rider's connection, and how often to ask.
//!
//! Mumble measures every connection at the server — the round trip it sees, and
//! how many voice packets arrived, arrived late, or never arrived at all — and
//! hands the figures to any client that asks with a `UserStats` message. The
//! client's own ping says nothing about anybody else, so this is the only way to
//! know that the rider who keeps breaking up is losing a fifth of their packets
//! rather than riding out of range of their own headset.
//!
//! # What a rider is allowed to see
//!
//! Read out of the server's `msgUserStats` rather than assumed, because the
//! answer decides what can be drawn:
//!
//! - **Anyone who may enter that rider's channel** gets `udp_ping_avg`,
//!   `tcp_ping_avg`, their variances and the packet counts.
//! - **Anyone in the same channel** also gets `from_client` and `from_server` —
//!   the good/late/lost counts this module turns into loss — plus
//!   `rolling_stats`, bandwidth and idle time.
//! - **Only the rider themselves, or an admin holding Ban on the root channel**,
//!   gets the certificate chain, the address and the client version.
//!
//! The roster draws the users in our own channel, so the loss figures are always
//! available for exactly the rows that show them. Nothing here asks for the
//! privileged half: the request carries `stats_only`, which tells the server to
//! leave the certificates out even when it would be allowed to send them.
//!
//! # Rolling versus cumulative
//!
//! The server keeps both a count since the connection began and a rolling window
//! (a minute or so). **The rolling one is what a rider means by "how is it
//! now".** A connection that lost heavily for a minute under a bridge and has
//! been clean for an hour is fine now, and its cumulative figure says otherwise
//! for the rest of the ride. Until the connection is older than the window the
//! server fills the rolling stats with the cumulative ones, which is exactly
//! right — there is nothing else to say yet — and [`Quality::window_secs`]
//! reports which is being shown.
//!
//! # Asking without becoming the problem
//!
//! One `UserStats` per rider per round: a busy channel would be a burst of
//! messages every few seconds, and the server charges a client for each. So the
//! poll takes [`QUALITY_BATCH`] riders at a time, round-robin, every
//! [`QUALITY_INTERVAL`] — a quiet channel is fully refreshed every round, and a
//! crowded one takes several, which is the right trade for a figure that a rider
//! reads as "how is this connection lately".

use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

/// How often a round of stats requests goes out.
pub const QUALITY_INTERVAL: Duration = Duration::from_secs(5);

/// How many riders one round asks about.
pub const QUALITY_BATCH: usize = 8;

/// The server's own measurements of one rider's connection.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Quality {
    pub session: u32,
    /// Round trip in milliseconds, as the server measures it.
    pub ping_ms: f32,
    /// Whether [`Quality::ping_ms`] is the UDP figure. A rider whose voice is
    /// tunnelled over TCP has no UDP measurement, and the TCP one is the honest
    /// substitute rather than a missing number.
    pub udp: bool,
    /// Share of this rider's voice packets, from 0 to 1, that never reached the
    /// server.
    pub loss_up: f32,
    /// Share of the server's packets that never reached this rider.
    pub loss_down: f32,
    /// How many seconds the loss figures cover. Zero means *since they
    /// connected*, which is what the server sends before its rolling window has
    /// filled.
    pub window_secs: u32,
    /// Seconds since this rider last did anything.
    pub idle_secs: u32,
}

/// Counts as the server reports them, for one direction.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Counts {
    pub good: u32,
    pub late: u32,
    pub lost: u32,
}

impl Counts {
    /// Share of packets that never arrived, from 0 to 1.
    ///
    /// **Late packets count as arrived.** They were delivered and the jitter
    /// buffer had its own decision to make about them; calling them lost would
    /// report a connection as broken when what it is, is slow — two different
    /// faults with two different answers.
    pub fn loss(&self) -> f32 {
        let seen = self.good as u64 + self.late as u64 + self.lost as u64;
        if seen == 0 {
            return 0.0;
        }
        self.lost as f32 / seen as f32
    }
}

impl Quality {
    /// Builds one from the fields of a `UserStats` reply.
    ///
    /// `udp_ping` and the counts are what the server sent, so all of them are
    /// optional: which arrive depends on who is asking and where they are
    /// standing — see the module note.
    pub fn from_stats(
        session: u32,
        udp_ping: Option<f32>,
        tcp_ping: Option<f32>,
        up: Option<Counts>,
        down: Option<Counts>,
        window_secs: u32,
        idle_secs: u32,
    ) -> Self {
        // A server that has never had a UDP packet from somebody reports 0,
        // not nothing, so zero has to mean "no UDP measurement" as much as a
        // missing field does.
        let udp = udp_ping.filter(|p| *p > 0.0);
        Self {
            session,
            ping_ms: udp.or(tcp_ping).unwrap_or(0.0).max(0.0),
            udp: udp.is_some(),
            loss_up: up.unwrap_or_default().loss(),
            loss_down: down.unwrap_or_default().loss(),
            window_secs,
            idle_secs,
        }
    }
}

/// Decides when to ask, and who to ask about.
#[derive(Debug, Default)]
pub struct QualityPoller {
    next: Option<Instant>,
    /// Where the last round stopped, so the next one carries on rather than
    /// asking about the same first few riders for ever.
    cursor: usize,
}

impl QualityPoller {
    /// Whether a round is due, marking it sent if so.
    pub fn due(&mut self, now: Instant) -> bool {
        match self.next {
            Some(t) if now < t => false,
            _ => {
                self.next = Some(now + QUALITY_INTERVAL);
                true
            }
        }
    }

    /// The next batch of sessions to ask about, in a stable order.
    ///
    /// `present` is every rider whose quality is worth having — in practice the
    /// ones sharing our channel, since those are the rows that show it and the
    /// only ones the server will report loss for.
    pub fn next_batch(&mut self, present: &[u32]) -> Vec<u32> {
        if present.is_empty() {
            self.cursor = 0;
            return Vec::new();
        }
        let mut sorted: Vec<u32> = present.to_vec();
        sorted.sort_unstable();
        if self.cursor >= sorted.len() {
            self.cursor = 0;
        }
        let take = QUALITY_BATCH.min(sorted.len());
        let mut batch = Vec::with_capacity(take);
        for i in 0..take {
            batch.push(sorted[(self.cursor + i) % sorted.len()]);
        }
        self.cursor = (self.cursor + take) % sorted.len();
        batch
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn late_packets_arrived() {
        // 10 late out of 100 is a slow connection, not a lossy one, and the
        // difference is the whole point of reporting loss separately.
        let c = Counts {
            good: 90,
            late: 10,
            lost: 0,
        };
        assert_eq!(c.loss(), 0.0);
    }

    #[test]
    fn loss_is_a_share_of_everything_sent() {
        let c = Counts {
            good: 75,
            late: 5,
            lost: 20,
        };
        assert!((c.loss() - 0.2).abs() < 1e-6);
    }

    #[test]
    fn a_connection_that_has_sent_nothing_has_lost_nothing() {
        // Somebody who has just joined and not spoken. Zero over zero must not
        // come out as "100% lost", which is how a fresh rider would otherwise
        // be drawn as the worst connection in the channel.
        assert_eq!(Counts::default().loss(), 0.0);
    }

    #[test]
    fn udp_is_preferred_and_zero_is_not_a_measurement() {
        let tunnelled = Quality::from_stats(1, Some(0.0), Some(48.0), None, None, 0, 0);
        assert_eq!(tunnelled.ping_ms, 48.0);
        assert!(!tunnelled.udp, "a zero UDP ping means there is no UDP ping");

        let direct = Quality::from_stats(1, Some(22.5), Some(48.0), None, None, 0, 0);
        assert_eq!(direct.ping_ms, 22.5);
        assert!(direct.udp);
    }

    #[test]
    fn no_ping_at_all_is_zero_rather_than_a_panic() {
        let q = Quality::from_stats(1, None, None, None, None, 0, 0);
        assert_eq!(q.ping_ms, 0.0);
        assert!(!q.udp);
    }

    #[test]
    fn both_directions_are_kept_apart() {
        // Losing what a rider sends and losing what reaches them are different
        // faults: one makes them unintelligible to everybody, the other makes
        // everybody unintelligible to them.
        let q = Quality::from_stats(
            7,
            Some(30.0),
            Some(40.0),
            Some(Counts {
                good: 80,
                late: 0,
                lost: 20,
            }),
            Some(Counts {
                good: 99,
                late: 0,
                lost: 1,
            }),
            60,
            3,
        );
        assert!((q.loss_up - 0.2).abs() < 1e-6);
        assert!((q.loss_down - 0.01).abs() < 1e-6);
        assert_eq!(q.window_secs, 60);
        assert_eq!(q.idle_secs, 3);
    }

    #[test]
    fn a_round_is_due_once_per_interval() {
        let t0 = Instant::now();
        let mut p = QualityPoller::default();
        assert!(p.due(t0), "the first round goes out at once");
        assert!(!p.due(t0 + Duration::from_secs(1)));
        assert!(p.due(t0 + QUALITY_INTERVAL));
    }

    #[test]
    fn a_crowded_channel_is_covered_over_several_rounds() {
        let present: Vec<u32> = (1..=20).collect();
        let mut p = QualityPoller::default();

        let first = p.next_batch(&present);
        assert_eq!(first.len(), QUALITY_BATCH);
        let second = p.next_batch(&present);
        let third = p.next_batch(&present);

        let mut seen: Vec<u32> = first
            .iter()
            .chain(&second)
            .chain(&third)
            .copied()
            .collect::<Vec<_>>();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(
            seen.len(),
            20,
            "three rounds cover twenty riders exactly once"
        );
    }

    #[test]
    fn a_quiet_channel_is_covered_every_round() {
        let present = vec![4, 9, 2];
        let mut p = QualityPoller::default();
        let mut a = p.next_batch(&present);
        let mut b = p.next_batch(&present);
        a.sort_unstable();
        b.sort_unstable();
        assert_eq!(a, vec![2, 4, 9]);
        assert_eq!(
            b, a,
            "with room to spare, everybody is asked about each time"
        );
    }

    #[test]
    fn nobody_to_ask_about_is_not_an_error() {
        let mut p = QualityPoller::default();
        assert!(p.next_batch(&[]).is_empty());
    }

    #[test]
    fn a_channel_emptying_does_not_leave_the_cursor_past_the_end() {
        // The rider polls a crowd, everybody leaves but one, and the next round
        // must still ask about that one rather than skipping them for ever.
        let mut p = QualityPoller::default();
        p.next_batch(&(1..=20).collect::<Vec<u32>>());
        assert_eq!(p.next_batch(&[3]), vec![3]);
    }
}
