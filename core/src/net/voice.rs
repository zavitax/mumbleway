//! UDP voice transport.
//!
//! Voice prefers UDP because the TLS control channel head-of-line blocks: one
//! delayed TCP segment stalls every subsequent audio frame. Many mobile carriers
//! and corporate networks block or aggressively NAT UDP though, so we probe with
//! pings and fall back to tunnelling through TLS when the probe fails.

use std::net::SocketAddr;
use std::time::{Duration, Instant};

use tokio::net::UdpSocket;

use crate::crypto::CryptState;
use crate::error::Result;
use crate::net::audio_packet::{self, VoicePacket};

/// Give up on UDP if no ping comes back within this window after connecting.
pub const UDP_PROBE_TIMEOUT: Duration = Duration::from_secs(3);

/// Consider UDP dead (and fall back to the tunnel) after this long with no
/// response, even if it worked earlier — carriers drop NAT bindings mid-ride.
pub const UDP_STALE_AFTER: Duration = Duration::from_secs(12);

/// Largest datagram we will accept; Mumble audio is far smaller.
const MAX_DATAGRAM: usize = 2048;

/// Something that arrived on the UDP socket.
#[derive(Debug)]
pub enum UdpEvent {
    Voice(VoicePacket),
    /// A ping echo, with the round-trip time.
    Pong {
        rtt: Duration,
    },
    /// A datagram we could not authenticate or parse.
    Rejected(&'static str),
    /// Decryption has been failing long enough to mean the cipher is out of
    /// step rather than the road being bad, and it is time to ask the server
    /// for its nonce. See [`crate::crypto::resync`].
    ResyncNeeded,
}

/// First wait before opening a new UDP socket after the last one failed.
pub const UDP_RETRY_FIRST: Duration = Duration::from_secs(5);

/// Longest wait between two attempts to get UDP back.
pub const UDP_RETRY_MAX: Duration = Duration::from_secs(60);

/// When to try UDP again after the socket died.
///
/// A socket error is usually the network changing under the phone — Wi-Fi to
/// mobile, a tunnel, a carrier dropping the NAT binding — and the next network
/// is often perfectly happy to carry UDP. Without a retry, one error kept the
/// rest of the session on the TCP tunnel, with its head-of-line blocking, for
/// however long the ride lasted.
///
/// The wait doubles on every failure so that a network which genuinely blocks
/// UDP costs one bind a minute, and starts short again only once a pong has
/// proved UDP works — not merely once a socket opened, which proves nothing.
#[derive(Debug)]
pub struct UdpRetry {
    delay: Duration,
    next: Option<Instant>,
}

impl Default for UdpRetry {
    fn default() -> Self {
        Self {
            delay: UDP_RETRY_FIRST,
            next: None,
        }
    }
}

impl UdpRetry {
    /// UDP failed at `now`: schedule the next attempt and lengthen the one after.
    pub fn failed(&mut self, now: Instant) {
        self.next = Some(now + self.delay);
        self.delay = (self.delay * 2).min(UDP_RETRY_MAX);
    }

    /// Whether a new socket should be opened now.
    pub fn is_due(&self, now: Instant) -> bool {
        self.next.is_some_and(|t| now >= t)
    }

    /// A socket is open again; nothing to schedule until it fails.
    pub fn reopened(&mut self) {
        self.next = None;
    }

    /// A pong arrived: UDP works, so the next failure gets the short wait.
    pub fn proven(&mut self) {
        self.delay = UDP_RETRY_FIRST;
    }

    /// Try at once if a retry is pending — the network has just come back.
    pub fn hurry(&mut self, now: Instant) {
        if self.next.is_some() {
            self.next = Some(now);
            self.delay = UDP_RETRY_FIRST;
        }
    }
}

pub struct VoiceSocket {
    socket: UdpSocket,
    crypt: CryptState,
    last_pong: Option<Instant>,
    last_ping_sent: Option<Instant>,
    established: bool,
    /// When decryption last worked, and when a resync was last asked for.
    resync: crate::crypto::ResyncGuard,
}

impl VoiceSocket {
    /// Binds a local socket and connects it to `peer` so it only sees that server.
    pub async fn bind(peer: SocketAddr, crypt: CryptState) -> Result<Self> {
        Self::try_bind(peer, crypt).await.map_err(|(e, _)| e)
    }

    /// Like [`Self::bind`], but hands the cipher back when the socket cannot
    /// be opened.
    ///
    /// **The cipher is the one thing that cannot be rebuilt mid-session**: it
    /// came from the server's `CryptSetup` at the handshake, and losing it
    /// here would leave UDP unrecoverable until the next reconnect — which is
    /// exactly how one socket error used to pin a session to the tunnel.
    pub async fn try_bind(
        peer: SocketAddr,
        crypt: CryptState,
    ) -> std::result::Result<Self, (crate::error::CoreError, CryptState)> {
        // Match the address family of the server we resolved.
        let bind_addr: SocketAddr = if peer.is_ipv4() {
            "0.0.0.0:0".parse().unwrap()
        } else {
            "[::]:0".parse().unwrap()
        };
        let socket = match UdpSocket::bind(bind_addr).await {
            Ok(s) => s,
            Err(e) => return Err((e.into(), crypt)),
        };
        if let Err(e) = socket.connect(peer).await {
            return Err((e.into(), crypt));
        }

        Ok(Self {
            socket,
            crypt,
            last_pong: None,
            last_ping_sent: None,
            established: false,
            resync: crate::crypto::ResyncGuard::new(Instant::now()),
        })
    }

    /// Whether UDP has ever completed a round trip.
    pub fn is_established(&self) -> bool {
        self.established
    }

    /// Whether UDP looks usable right now.
    pub fn is_healthy(&self, now: Instant) -> bool {
        match self.last_pong {
            Some(t) => now.duration_since(t) < UDP_STALE_AFTER,
            None => false,
        }
    }

    pub fn crypt_stats(&self) -> crate::crypto::CryptStats {
        self.crypt.stats()
    }

    /// Our encrypt nonce, which the server asks for when *its* decryption has
    /// lost step.
    pub fn encrypt_iv(&self) -> &[u8] {
        self.crypt.encrypt_iv()
    }

    /// Takes the nonce the server sent in answer to a resync request.
    pub fn set_decrypt_iv(&mut self, iv: &[u8]) -> Result<()> {
        self.crypt.set_decrypt_iv(iv)?;
        // Counted as good: the cipher is in step again as far as anybody here
        // knows, so the next failure is a fresh fault with its own patience
        // rather than one that asks again immediately.
        self.resync.note_good(Instant::now());
        Ok(())
    }

    /// Encrypts and sends a voice packet.
    pub async fn send_voice(&mut self, packet: &VoicePacket) -> Result<()> {
        let plain = packet.encode_outgoing();
        let sealed = self.crypt.encrypt(&plain)?;
        super::stats::note_bytes_out(sealed.len());
        super::stats::note_voice_out();
        self.socket.send(&sealed).await?;
        Ok(())
    }

    /// Sends an encrypted ping carrying `timestamp`.
    pub async fn send_ping(&mut self, timestamp: u64) -> Result<()> {
        let plain = audio_packet::encode_ping(timestamp);
        let sealed = self.crypt.encrypt(&plain)?;
        super::stats::note_bytes_out(sealed.len());
        self.socket.send(&sealed).await?;
        self.last_ping_sent = Some(Instant::now());
        Ok(())
    }

    /// Waits for the next datagram and decrypts it.
    ///
    /// Cancel-safe: `UdpSocket::recv` either yields a whole datagram or nothing,
    /// so this can be raced inside `select!` freely.
    pub async fn recv(&mut self) -> Result<UdpEvent> {
        let mut buf = [0u8; MAX_DATAGRAM];
        let n = self.socket.recv(&mut buf).await?;
        super::stats::note_bytes_in(n);

        let plain = match self.crypt.decrypt(&buf[..n]) {
            Ok(p) => {
                self.resync.note_good(Instant::now());
                p
            }
            // A failed decrypt is normal on a lossy link (replays, reordering
            // beyond the window). Report it but keep the socket alive — unless
            // it has been failing long enough to mean the two ends have lost
            // step, which nothing recovers from on its own.
            Err(_) => {
                return Ok(if self.resync.should_request(Instant::now()) {
                    UdpEvent::ResyncNeeded
                } else {
                    UdpEvent::Rejected("decryption failed")
                });
            }
        };

        if audio_packet::is_ping(&plain) {
            let echoed = audio_packet::decode_ping(&plain)?;
            let now = Instant::now();
            self.last_pong = Some(now);
            self.established = true;
            let rtt = self
                .last_ping_sent
                .map(|s| now.saturating_duration_since(s))
                .unwrap_or_default();
            // The server echoes our timestamp verbatim, so a mismatch means a
            // stale reply; we still count it as liveness.
            let _ = echoed;
            return Ok(UdpEvent::Pong { rtt });
        }

        match VoicePacket::decode_incoming(&plain) {
            Ok(p) => Ok(UdpEvent::Voice(p)),
            Err(_) => Ok(UdpEvent::Rejected("malformed voice packet")),
        }
    }

    /// Hands the cipher state back so tunnelled audio can keep using it.
    pub fn into_crypt(self) -> CryptState {
        self.crypt
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn udp_is_retried_with_a_doubling_wait() {
        let t0 = Instant::now();
        let mut r = UdpRetry::default();
        assert!(!r.is_due(t0), "nothing failed, nothing to retry");

        r.failed(t0);
        assert!(!r.is_due(t0 + UDP_RETRY_FIRST - Duration::from_millis(1)));
        assert!(r.is_due(t0 + UDP_RETRY_FIRST));

        r.reopened();
        assert!(!r.is_due(t0 + Duration::from_secs(3600)));
        r.failed(t0);
        assert!(!r.is_due(t0 + UDP_RETRY_FIRST));
        assert!(r.is_due(t0 + UDP_RETRY_FIRST * 2));

        for _ in 0..10 {
            r.failed(t0);
        }
        assert!(r.is_due(t0 + UDP_RETRY_MAX), "the wait is capped");
    }

    #[test]
    fn a_pong_resets_the_wait_and_network_return_hurries_it() {
        let t0 = Instant::now();
        let mut r = UdpRetry::default();
        r.failed(t0);
        r.failed(t0);
        r.reopened();
        r.proven();
        r.failed(t0);
        assert!(r.is_due(t0 + UDP_RETRY_FIRST));

        let mut r = UdpRetry::default();
        r.hurry(t0);
        assert!(!r.is_due(t0), "no pending retry, nothing to hurry");
        r.failed(t0);
        r.failed(t0);
        r.hurry(t0);
        assert!(r.is_due(t0));
    }

    #[tokio::test]
    async fn a_reopened_socket_keeps_the_same_cipher() {
        // What a rebind after a socket error relies on: the cipher handed back
        // by the dead socket goes into the new one unchanged, nonce and all.
        let (a, _) = crypt_pair();
        let iv = *a.encrypt_iv();
        let peer = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let first = VoiceSocket::bind(peer.local_addr().unwrap(), a)
            .await
            .unwrap();
        let crypt = first.into_crypt();
        let second = match VoiceSocket::try_bind(peer.local_addr().unwrap(), crypt).await {
            Ok(s) => s,
            Err((e, _)) => panic!("rebinding failed: {e}"),
        };
        assert_eq!(second.encrypt_iv(), &iv);
    }

    fn crypt_pair() -> (CryptState, CryptState) {
        let key = [0x42u8; 16];
        let a = [0x01u8; 16];
        let b = [0x02u8; 16];
        (
            CryptState::new(&key, &a, &b).unwrap(),
            CryptState::new(&key, &b, &a).unwrap(),
        )
    }

    #[tokio::test]
    async fn voice_and_ping_survive_a_real_udp_round_trip() {
        // Stand up a fake "server" socket that decrypts what we send and echoes.
        let server = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let server_addr = server.local_addr().unwrap();

        let (client_crypt, mut server_crypt) = crypt_pair();
        let mut client = VoiceSocket::bind(server_addr, client_crypt).await.unwrap();

        assert!(
            !client.is_established(),
            "not established before any round trip"
        );

        // Client sends a ping; server decrypts and echoes it back encrypted.
        client.send_ping(0xDEAD_BEEF).await.unwrap();
        let mut buf = [0u8; 2048];
        let (n, from) = server.recv_from(&mut buf).await.unwrap();
        let plain = server_crypt.decrypt(&buf[..n]).unwrap();
        assert!(audio_packet::is_ping(&plain));
        assert_eq!(audio_packet::decode_ping(&plain).unwrap(), 0xDEAD_BEEF);
        let echo = server_crypt.encrypt(&plain).unwrap();
        server.send_to(&echo, from).await.unwrap();

        match client.recv().await.unwrap() {
            UdpEvent::Pong { .. } => {}
            other => panic!("expected Pong, got {other:?}"),
        }
        assert!(client.is_established());
        assert!(client.is_healthy(Instant::now()));

        // Now a voice packet in the server-to-client direction.
        let vp = VoicePacket::speech(9, vec![1, 2, 3, 4], false);
        let mut wire = Vec::new();
        wire.push(4u8 << 5);
        crate::varint::encode(&mut wire, 77); // session id
        crate::varint::encode(&mut wire, vp.sequence);
        crate::varint::encode(&mut wire, vp.opus.len() as u64);
        wire.extend_from_slice(&vp.opus);
        let sealed = server_crypt.encrypt(&wire).unwrap();
        server.send_to(&sealed, from).await.unwrap();

        match client.recv().await.unwrap() {
            UdpEvent::Voice(p) => {
                assert_eq!(p.session, Some(77));
                assert_eq!(p.opus, vec![1, 2, 3, 4]);
            }
            other => panic!("expected Voice, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn garbage_datagrams_are_rejected_without_killing_the_socket() {
        let server = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let server_addr = server.local_addr().unwrap();
        let (client_crypt, mut server_crypt) = crypt_pair();
        let mut client = VoiceSocket::bind(server_addr, client_crypt).await.unwrap();

        // Force the server to learn our address.
        client.send_ping(1).await.unwrap();
        let mut buf = [0u8; 2048];
        let (_n, from) = server.recv_from(&mut buf).await.unwrap();

        // Unencrypted noise must be rejected, not accepted or fatal.
        server
            .send_to(b"total garbage payload", from)
            .await
            .unwrap();
        match client.recv().await.unwrap() {
            UdpEvent::Rejected(_) => {}
            other => panic!("expected Rejected, got {other:?}"),
        }

        // The socket must still work afterwards.
        let ping = audio_packet::encode_ping(5);
        let sealed = server_crypt.encrypt(&ping).unwrap();
        server.send_to(&sealed, from).await.unwrap();
        assert!(matches!(
            client.recv().await.unwrap(),
            UdpEvent::Pong { .. }
        ));
    }

    #[tokio::test]
    async fn a_socket_that_never_hears_back_is_unhealthy() {
        // Nothing is listening, so no pong will ever arrive.
        let (crypt, _) = crypt_pair();
        let dead: SocketAddr = "127.0.0.1:9".parse().unwrap();
        let client = VoiceSocket::bind(dead, crypt).await.unwrap();
        assert!(!client.is_healthy(Instant::now()));
        assert!(!client.is_established());
    }
}
