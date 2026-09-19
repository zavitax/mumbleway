//! Asking the server to re-sync the voice cipher, and not asking too often.
//!
//! Mumble's OCB2 state carries a nonce that both ends step forward with every
//! packet. A lossy link is fine — the decryptor searches a window either side —
//! but a long enough gap, a restarted server or a NAT that quietly hands the
//! socket to somebody else leaves the two ends out of step by more than the
//! window, and from then on **every** packet fails to decrypt. Voice stops, the
//! link looks perfectly alive, and nothing recovers it on its own.
//!
//! The protocol's answer is an empty `CryptSetup`: the server replies with its
//! current nonce and decryption starts working again.
//!
//! # When to ask
//!
//! The rule is the official client's, which is the one servers are used to: ask
//! only after decryption has been failing for [`RESYNC_AFTER`], and never more
//! than once per [`RESYNC_AFTER`].
//!
//! Both halves matter. Asking on the first failed packet would send a request
//! every time a packet arrived out of order on a bad road, which is ordinary and
//! self-correcting; the five seconds are what tell a desynchronised cipher from
//! a merely lossy one. And a broken cipher fails *every* packet — fifty a second
//! with a channel of riders talking — so without the second limit one fault
//! would turn into a flood of requests, against a server that is charging this
//! client for each of them.

use std::time::{Duration, Instant};

/// How long decryption must be failing before a resync is asked for, and the
/// shortest gap between two requests.
pub const RESYNC_AFTER: Duration = Duration::from_secs(5);

/// Decides when to ask the server to re-sync the cipher.
#[derive(Debug)]
pub struct ResyncGuard {
    /// Last time a packet decrypted, or the socket opening — from which the
    /// first "it has been broken for five seconds" is measured.
    last_good: Instant,
    last_request: Option<Instant>,
}

impl ResyncGuard {
    /// Starts the clock, as if the cipher had just worked.
    ///
    /// The socket has only just opened at this point, so treating that moment
    /// as the last good packet is what stops a client that hears nothing at all
    /// from asking for a resync before it has ever had a packet to fail on.
    pub fn new(now: Instant) -> Self {
        Self {
            last_good: now,
            last_request: None,
        }
    }

    /// Notes a packet that decrypted.
    pub fn note_good(&mut self, now: Instant) {
        self.last_good = now;
    }

    /// Whether this failure is worth a resync request, marking it sent if so.
    pub fn should_request(&mut self, now: Instant) -> bool {
        if now.saturating_duration_since(self.last_good) < RESYNC_AFTER {
            return false;
        }
        if self
            .last_request
            .is_some_and(|t| now.saturating_duration_since(t) < RESYNC_AFTER)
        {
            return false;
        }
        self.last_request = Some(now);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_lossy_road_is_not_a_broken_cipher() {
        // Packets going astray, and one arriving far enough out of order to
        // fail, is ordinary and fixes itself. Asking here would mean a request
        // per bad packet.
        let t0 = Instant::now();
        let mut g = ResyncGuard::new(t0);
        for ms in [10, 200, 1500, 4000] {
            assert!(!g.should_request(t0 + Duration::from_millis(ms)));
        }
    }

    #[test]
    fn nothing_decrypting_for_five_seconds_is_asked_about() {
        let t0 = Instant::now();
        let mut g = ResyncGuard::new(t0);
        assert!(g.should_request(t0 + RESYNC_AFTER));
    }

    #[test]
    fn one_request_per_five_seconds_however_many_packets_fail() {
        // A desynchronised cipher fails every packet — fifty a second with a
        // channel talking — and each request costs the client against the
        // server's rate limit.
        let t0 = Instant::now();
        let mut g = ResyncGuard::new(t0);
        assert!(g.should_request(t0 + RESYNC_AFTER));

        let mut asked = 0;
        for ms in (5_100..10_000).step_by(20) {
            if g.should_request(t0 + Duration::from_millis(ms)) {
                asked += 1;
            }
        }
        assert_eq!(asked, 0, "the first request covers the next five seconds");
        assert!(g.should_request(t0 + Duration::from_millis(10_100)));
    }

    #[test]
    fn a_packet_getting_through_starts_the_clock_again() {
        // The resync worked, or the gap closed by itself. Either way the next
        // failure is a fresh fault and gets its own five seconds of patience.
        let t0 = Instant::now();
        let mut g = ResyncGuard::new(t0);
        assert!(g.should_request(t0 + RESYNC_AFTER));

        g.note_good(t0 + Duration::from_secs(6));
        assert!(!g.should_request(t0 + Duration::from_secs(8)));
        assert!(g.should_request(t0 + Duration::from_secs(11)));
    }

    #[test]
    fn a_client_that_never_had_a_packet_waits_the_same_five_seconds() {
        // Opening the socket counts as the last good moment, so a rider who
        // connects into silence does not ask for a resync straight away.
        let t0 = Instant::now();
        let mut g = ResyncGuard::new(t0);
        assert!(!g.should_request(t0));
        assert!(!g.should_request(t0 + Duration::from_secs(4)));
    }
}
