//! Unauthenticated server status query.
//!
//! Mumble servers answer a small UDP probe without any handshake, which is how
//! a client shows ping and occupancy for servers it is *not* connected to. This
//! is a different mechanism from the in-session pings in [`super::voice`]: those
//! are encrypted and require a established session, this one is anonymous.
//!
//! ```text
//! request   12 bytes   u32 zero, u64 client id
//! response  24 bytes   u32 version, u64 id, u32 users, u32 max_users, u32 bandwidth
//! ```
//!
//! All fields are big-endian. Servers may disable this, in which case the probe
//! simply times out.

use std::time::{Duration, Instant};

use tokio::net::UdpSocket;

use crate::error::{CoreError, Result};

const REQUEST_LEN: usize = 12;
const RESPONSE_LEN: usize = 24;

/// How long to wait for a reply before giving up on one attempt.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(3);

/// What a server reports about itself.
// No Eq: rtt_ms is a float.
#[derive(Debug, Clone, PartialEq)]
pub struct ServerStatus {
    pub major: u16,
    pub minor: u8,
    pub patch: u8,
    pub users: u32,
    pub max_users: u32,
    /// Maximum server-bound speech bandwidth per client, in bits per second.
    pub bandwidth: u32,
    /// Round-trip time in milliseconds.
    pub rtt_ms: f64,
}

impl ServerStatus {
    pub fn version_string(&self) -> String {
        format!("{}.{}.{}", self.major, self.minor, self.patch)
    }

    /// True when the server is at its user limit.
    pub fn is_full(&self) -> bool {
        self.max_users > 0 && self.users >= self.max_users
    }
}

/// Builds the 12-byte probe.
pub fn encode_request(id: u64) -> [u8; REQUEST_LEN] {
    let mut buf = [0u8; REQUEST_LEN];
    // Bytes 0..4 stay zero; that zero word is what marks this as a status probe
    // rather than a voice packet.
    buf[4..12].copy_from_slice(&id.to_be_bytes());
    buf
}

/// Parses a 24-byte reply, returning `None` if it is malformed or answers a
/// different probe than `expect_id`.
pub fn decode_response(buf: &[u8], expect_id: u64) -> Option<(u32, u32, u32, u32)> {
    if buf.len() < RESPONSE_LEN {
        return None;
    }
    let version = u32::from_be_bytes(buf[0..4].try_into().ok()?);
    let id = u64::from_be_bytes(buf[4..12].try_into().ok()?);
    if id != expect_id {
        return None;
    }
    let users = u32::from_be_bytes(buf[12..16].try_into().ok()?);
    let max_users = u32::from_be_bytes(buf[16..20].try_into().ok()?);
    let bandwidth = u32::from_be_bytes(buf[20..24].try_into().ok()?);
    Some((version, users, max_users, bandwidth))
}

/// Splits the packed version word. Mumble encodes 1.3.0 as `0x00010300`.
pub fn split_version(v: u32) -> (u16, u8, u8) {
    (
        ((v >> 16) & 0xFFFF) as u16,
        ((v >> 8) & 0xFF) as u8,
        (v & 0xFF) as u8,
    )
}

/// Queries a server's status over UDP.
pub async fn query(host: &str, port: u16, timeout: Duration) -> Result<ServerStatus> {
    // A per-probe id lets us ignore replies to earlier, already-timed-out probes
    // that arrive late on the same socket.
    let id = {
        use rand::Rng;
        rand::thread_rng().gen::<u64>()
    };

    let socket = UdpSocket::bind("0.0.0.0:0").await?;
    socket.connect((host, port)).await?;

    let request = encode_request(id);
    let sent_at = Instant::now();
    socket.send(&request).await?;

    let deadline = sent_at + timeout;
    let mut buf = [0u8; 64];

    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(CoreError::Timeout("server status probe"));
        }
        let n = match tokio::time::timeout(remaining, socket.recv(&mut buf)).await {
            Err(_) => return Err(CoreError::Timeout("server status probe")),
            Ok(r) => r?,
        };

        // Keep waiting rather than failing: a stale reply must not mask the
        // real one still in flight.
        if let Some((version, users, max_users, bandwidth)) = decode_response(&buf[..n], id) {
            let (major, minor, patch) = split_version(version);
            return Ok(ServerStatus {
                major,
                minor,
                patch,
                users,
                max_users,
                bandwidth,
                rtt_ms: sent_at.elapsed().as_secs_f64() * 1000.0,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_is_twelve_bytes_with_a_zero_header() {
        let r = encode_request(0x0123_4567_89AB_CDEF);
        assert_eq!(r.len(), 12);
        assert_eq!(&r[0..4], &[0, 0, 0, 0], "leading word must be zero");
        assert_eq!(&r[4..12], &0x0123_4567_89AB_CDEFu64.to_be_bytes());
    }

    #[test]
    fn response_roundtrips() {
        let mut buf = Vec::new();
        buf.extend_from_slice(&0x0001_0403u32.to_be_bytes()); // 1.4.3
        buf.extend_from_slice(&42u64.to_be_bytes());
        buf.extend_from_slice(&7u32.to_be_bytes());
        buf.extend_from_slice(&100u32.to_be_bytes());
        buf.extend_from_slice(&72000u32.to_be_bytes());

        let (v, users, max, bw) = decode_response(&buf, 42).unwrap();
        assert_eq!(split_version(v), (1, 4, 3));
        assert_eq!((users, max, bw), (7, 100, 72000));
    }

    #[test]
    fn response_for_a_different_probe_is_rejected() {
        let mut buf = vec![0u8; 24];
        buf[4..12].copy_from_slice(&999u64.to_be_bytes());
        assert!(
            decode_response(&buf, 42).is_none(),
            "a reply to another probe must not be accepted"
        );
    }

    #[test]
    fn truncated_responses_are_rejected() {
        for len in [0usize, 1, 12, 23] {
            assert!(decode_response(&vec![0u8; len], 0).is_none(), "len {len}");
        }
    }

    #[test]
    fn version_splitting_matches_mumble_encoding() {
        assert_eq!(split_version(0x0001_0300), (1, 3, 0));
        assert_eq!(split_version(0x0001_0405), (1, 4, 5));
        assert_eq!(split_version(0x0001_05FF), (1, 5, 255));
    }

    #[test]
    fn full_server_detection() {
        let mut s = ServerStatus {
            major: 1,
            minor: 4,
            patch: 0,
            users: 10,
            max_users: 10,
            bandwidth: 72000,
            rtt_ms: 5.0,
        };
        assert!(s.is_full());
        s.users = 9;
        assert!(!s.is_full());
        // An unlimited server never reports full.
        s.max_users = 0;
        s.users = 1000;
        assert!(!s.is_full());
        assert_eq!(s.version_string(), "1.4.0");
    }

    #[tokio::test]
    async fn queries_a_stub_server() {
        let server = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let addr = server.local_addr().unwrap();

        tokio::spawn(async move {
            let mut buf = [0u8; 64];
            let (n, from) = server.recv_from(&mut buf).await.unwrap();
            assert_eq!(n, 12);
            let id = u64::from_be_bytes(buf[4..12].try_into().unwrap());

            let mut reply = Vec::new();
            reply.extend_from_slice(&0x0001_0500u32.to_be_bytes()); // 1.5.0
            reply.extend_from_slice(&id.to_be_bytes());
            reply.extend_from_slice(&12u32.to_be_bytes());
            reply.extend_from_slice(&50u32.to_be_bytes());
            reply.extend_from_slice(&96000u32.to_be_bytes());
            server.send_to(&reply, from).await.unwrap();
        });

        let status = query("127.0.0.1", addr.port(), Duration::from_secs(5))
            .await
            .expect("stub server should answer");
        assert_eq!(status.version_string(), "1.5.0");
        assert_eq!(status.users, 12);
        assert_eq!(status.max_users, 50);
        assert_eq!(status.bandwidth, 96000);
        assert!(status.rtt_ms >= 0.0 && status.rtt_ms < 5000.0);
    }

    #[tokio::test]
    async fn ignores_a_stale_reply_and_accepts_the_real_one() {
        // A late answer to a previous probe must not be mistaken for ours.
        let server = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let addr = server.local_addr().unwrap();

        tokio::spawn(async move {
            let mut buf = [0u8; 64];
            let (_n, from) = server.recv_from(&mut buf).await.unwrap();
            let id = u64::from_be_bytes(buf[4..12].try_into().unwrap());

            // Wrong id first.
            let mut stale = vec![0u8; 24];
            stale[4..12].copy_from_slice(&id.wrapping_add(1).to_be_bytes());
            server.send_to(&stale, from).await.unwrap();

            let mut reply = Vec::new();
            reply.extend_from_slice(&0x0001_0400u32.to_be_bytes());
            reply.extend_from_slice(&id.to_be_bytes());
            reply.extend_from_slice(&3u32.to_be_bytes());
            reply.extend_from_slice(&20u32.to_be_bytes());
            reply.extend_from_slice(&72000u32.to_be_bytes());
            server.send_to(&reply, from).await.unwrap();
        });

        let status = query("127.0.0.1", addr.port(), Duration::from_secs(5))
            .await
            .unwrap();
        assert_eq!(status.users, 3, "accepted the stale reply");
    }

    #[tokio::test]
    async fn times_out_against_a_silent_server() {
        // Nothing listening: must return a timeout rather than hang.
        let r = query("127.0.0.1", 9, Duration::from_millis(400)).await;
        assert!(r.is_err());
    }
}
