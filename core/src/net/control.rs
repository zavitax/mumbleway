//! The TLS control channel.
//!
//! Carries every non-voice message, plus tunnelled audio when UDP is unavailable.
//! Mumble servers drop clients that go 30 s without a ping, so the session layer
//! pings every 5 s and treats a missing response as a lost connection.

use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt, ReadHalf, WriteHalf};
use tokio::net::TcpStream;
use tokio_rustls::client::TlsStream;
use tokio_rustls::TlsConnector;

use crate::error::{CoreError, Result};
use crate::net::frame::{self, Header, HEADER_LEN};
use crate::net::proxy::{self, ProxySpec};
use crate::net::tls::ObservedCert;

/// How long to wait for TCP + TLS before giving up on an attempt.
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Reader half of a control connection.
pub struct ControlReader {
    inner: ReadHalf<TlsStream<TcpStream>>,
}

/// Writer half of a control connection.
pub struct ControlWriter {
    inner: WriteHalf<TlsStream<TcpStream>>,
}

impl ControlReader {
    /// Reads one complete control message.
    ///
    /// **Not cancel-safe.** It performs two sequential `read_exact` calls, so a
    /// future dropped part-way through has already consumed bytes from the TLS
    /// stream that nobody will ever process — the next call then starts mid-
    /// message and the connection desynchronises for good. It must therefore
    /// never appear as a `select!` branch; own it from a dedicated task and
    /// forward completed messages over a channel instead.
    pub async fn recv(&mut self) -> Result<(u16, Vec<u8>)> {
        let mut hdr = [0u8; HEADER_LEN];
        self.inner.read_exact(&mut hdr).await?;
        let Header { msg_type, length } = Header::parse(&hdr)?;

        let mut payload = vec![0u8; length];
        if length > 0 {
            self.inner.read_exact(&mut payload).await?;
        }
        super::stats::note_bytes_in(HEADER_LEN + length);
        Ok((msg_type, payload))
    }
}

impl ControlWriter {
    pub async fn send_raw(&mut self, msg_type: u16, payload: &[u8]) -> Result<()> {
        let framed = frame::encode(msg_type, payload);
        super::stats::note_bytes_out(framed.len());
        self.inner.write_all(&framed).await?;
        self.inner.flush().await?;
        Ok(())
    }

    pub async fn send<M: prost::Message>(
        &mut self,
        msg_type: crate::proto::MessageType,
        msg: &M,
    ) -> Result<()> {
        let framed = frame::encode_proto(msg_type, msg);
        super::stats::note_bytes_out(framed.len());
        self.inner.write_all(&framed).await?;
        self.inner.flush().await?;
        Ok(())
    }

    /// Sends a voice packet through the TLS tunnel (used when UDP is blocked).
    pub async fn send_tunnel(&mut self, udp_packet: &[u8]) -> Result<()> {
        let framed = frame::encode_tunnel(udp_packet);
        super::stats::note_bytes_out(framed.len());
        // A tunnelled voice packet is still a voice packet; counting it only
        // on the UDP path would show nothing at all on a link that fell back.
        super::stats::note_voice_out();
        self.inner.write_all(&framed).await?;
        self.inner.flush().await?;
        Ok(())
    }

    pub async fn shutdown(&mut self) {
        let _ = self.inner.shutdown().await;
    }
}

/// Result of a successful TCP + TLS connection.
pub struct Connected {
    pub reader: ControlReader,
    pub writer: ControlWriter,
    /// Where UDP voice may be sent, when it may be sent anywhere.
    ///
    /// **Not simply the socket's peer, which through a proxy is the proxy.**
    /// Sending voice there would spray encrypted audio at a third party and
    /// fail in the quietest possible way: no pong comes back, so the session
    /// never promotes to UDP and nothing says why. `None` means voice goes
    /// through the tunnel — because the rider asked for that, or because the
    /// server's own address could not be resolved here, which is a thing this
    /// client already handles rather than a reason to refuse the connection.
    pub voice_peer: Option<std::net::SocketAddr>,
    /// The certificate the server presented.
    pub observed: Arc<ObservedCert>,
}

/// How long to spend resolving the server's own address for the voice socket.
///
/// Short on purpose: the control channel is already up by then, and a slow
/// resolver must not hold the connection. Failing simply means tunnelled voice.
const RESOLVE_TIMEOUT: Duration = Duration::from_secs(2);

/// Opens a TCP + TLS connection to a Mumble server, through `chain` if it is
/// not empty.
///
/// **No generics and no boxing, because a proxy here is a dial.** Both kinds
/// hand back an ordinary `TcpStream` once their handshake is done, so TLS sits
/// on the same type it always did and the reader and writer above stay
/// concrete. The one thing that would change that is an `https://` proxy, where
/// the connection *to the proxy* is itself TLS and the stream becomes
/// `TlsStream<TlsStream<TcpStream>>`; the way in would be a boxed
/// `trait Socket: AsyncRead + AsyncWrite + Send + Unpin` here and nowhere else.
pub async fn connect(
    host: &str,
    port: u16,
    chain: &[ProxySpec],
    config: Arc<rustls::ClientConfig>,
    observed: Arc<ObservedCert>,
) -> Result<Connected> {
    // One budget for the whole dial, however many hops it has: a proxy that
    // accepts and then says nothing must not be able to hold an attempt open
    // for longer than a direct connection could.
    let tcp = tokio::time::timeout(CONNECT_TIMEOUT, proxy::dial(chain, host, port))
        .await
        .map_err(|_| CoreError::Timeout("TCP connect"))??;

    // Voice latency is dominated by packetisation, so Nagle buffering only ever
    // hurts here — especially for tunnelled audio.
    tcp.set_nodelay(true).ok();
    let voice_peer = voice_address(&tcp, chain, host, port).await;

    let server_name = rustls::pki_types::ServerName::try_from(host.to_string())
        .map_err(|_| CoreError::Tls(format!("invalid server name: {host}")))?;

    let connector = TlsConnector::from(config);
    let tls = tokio::time::timeout(CONNECT_TIMEOUT, connector.connect(server_name, tcp))
        .await
        .map_err(|_| CoreError::Timeout("TLS handshake"))?
        .map_err(|e| {
            // A pinned-fingerprint mismatch surfaces here as a generic TLS error;
            // the observed-cert record tells the caller what really happened.
            if let Some(fp) = observed.mismatch() {
                CoreError::Tls(format!(
                    "server certificate changed (now {fp}); re-pin it to continue"
                ))
            } else {
                CoreError::Tls(e.to_string())
            }
        })?;

    let (r, w) = tokio::io::split(tls);
    Ok(Connected {
        reader: ControlReader { inner: r },
        writer: ControlWriter { inner: w },
        voice_peer,
        observed,
    })
}

/// Where voice may go, given how the control channel was dialled.
///
/// Direct, it is the address the kernel actually used — better than resolving
/// the name again, which on a round-robin can hand back a different server.
/// Through a proxy it has to be looked up, and is skipped entirely when voice
/// is going through the proxy anyway: no name lookup for a server on a network
/// the rider is deliberately proxying.
async fn voice_address(
    tcp: &TcpStream,
    chain: &[ProxySpec],
    host: &str,
    port: u16,
) -> Option<std::net::SocketAddr> {
    if chain.is_empty() {
        return tcp.peer_addr().ok();
    }
    if chain.iter().any(|p| p.tunnel_voice) {
        return None;
    }
    match tokio::time::timeout(RESOLVE_TIMEOUT, tokio::net::lookup_host((host, port))).await {
        Ok(Ok(mut addrs)) => {
            let found = addrs.next();
            if found.is_none() {
                tracing::warn!("{host} resolved to nothing; voice will go through the tunnel");
            }
            found
        }
        Ok(Err(e)) => {
            tracing::warn!("could not resolve {host} for voice ({e}); tunnelling it instead");
            None
        }
        Err(_) => {
            tracing::warn!("resolving {host} for voice timed out; tunnelling it instead");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn connecting_to_a_closed_port_fails_promptly() {
        let cfg = crate::net::tls::client_config(None, crate::net::TrustPolicy::AcceptAny).unwrap();
        // Port 1 on loopback has nothing listening.
        let r = connect("127.0.0.1", 1, &[], cfg.0, cfg.1).await;
        assert!(r.is_err(), "expected a connection failure");
    }

    #[tokio::test]
    async fn connecting_to_a_non_tls_listener_fails_rather_than_hanging() {
        // A plain TCP listener that never speaks TLS must produce an error,
        // not a hang, or reconnect logic would stall forever.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            if let Ok((mut s, _)) = listener.accept().await {
                // Say something that is definitively not a TLS ServerHello.
                use tokio::io::AsyncWriteExt;
                let _ = s.write_all(b"HELLO NOT TLS\r\n").await;
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        });

        let cfg = crate::net::tls::client_config(None, crate::net::TrustPolicy::AcceptAny).unwrap();
        let r = tokio::time::timeout(
            Duration::from_secs(15),
            connect("127.0.0.1", addr.port(), &[], cfg.0, cfg.1),
        )
        .await;
        assert!(r.is_ok(), "connect() must not hang past its own timeout");
        assert!(r.unwrap().is_err(), "a non-TLS peer must be rejected");
    }

    /// A proxy that accepts every CONNECT and then relays to a plain listener.
    ///
    /// Reaching TLS at all proves the tunnel was established: the error has to
    /// come from the far end not speaking TLS, not from the proxy exchange.
    async fn fake_http_proxy(relay_to: std::net::SocketAddr) -> std::net::SocketAddr {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            while let Ok((mut client, _)) = listener.accept().await {
                let mut head = Vec::new();
                let mut byte = [0u8; 1];
                while !head.ends_with(b"\r\n\r\n") {
                    match client.read(&mut byte).await {
                        Ok(1) => head.push(byte[0]),
                        _ => break,
                    }
                }
                if client
                    .write_all(b"HTTP/1.1 200 Connection established\r\n\r\n")
                    .await
                    .is_err()
                {
                    continue;
                }
                if let Ok(mut upstream) = TcpStream::connect(relay_to).await {
                    let _ = tokio::io::copy_bidirectional(&mut client, &mut upstream).await;
                }
            }
        });
        addr
    }

    #[tokio::test]
    async fn a_proxied_connection_reaches_the_far_end_and_fails_at_tls() {
        let far = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let far_addr = far.local_addr().unwrap();
        tokio::spawn(async move {
            if let Ok((mut s, _)) = far.accept().await {
                let _ = s.write_all(b"HELLO NOT TLS\r\n").await;
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        });
        let proxy_addr = fake_http_proxy(far_addr).await;

        let chain = [crate::net::ProxySpec {
            kind: crate::net::ProxyKind::HttpConnect,
            host: "127.0.0.1".into(),
            port: proxy_addr.port(),
            username: None,
            password: None,
            tunnel_voice: false,
        }];
        let cfg = crate::net::tls::client_config(None, crate::net::TrustPolicy::AcceptAny).unwrap();
        let r = tokio::time::timeout(
            Duration::from_secs(15),
            connect("127.0.0.1", far_addr.port(), &chain, cfg.0, cfg.1),
        )
        .await
        .expect("must not hang");
        let e = match r {
            Err(e) => e,
            Ok(_) => panic!("the far end does not speak TLS, so this cannot succeed"),
        };
        assert!(
            matches!(e, CoreError::Tls(_)),
            "the proxy exchange itself must have succeeded: {e}"
        );
    }

    #[tokio::test]
    async fn a_direct_connection_offers_the_socket_peer_for_voice() {
        // Nothing proxied: the address voice may use is the one the kernel
        // chose, which is what the UDP socket has always been given.
        let server = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = server.local_addr().unwrap();
        tokio::spawn(async move {
            if let Ok((mut s, _)) = server.accept().await {
                let _ = s.write_all(b"NOT TLS").await;
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        });
        // The connection fails at TLS, so reach the decision through its own
        // function rather than a `Connected` that never exists.
        let tcp = TcpStream::connect(addr).await.unwrap();
        let chosen = voice_address(&tcp, &[], "127.0.0.1", addr.port()).await;
        assert_eq!(chosen, Some(addr));
    }

    #[tokio::test]
    async fn a_proxied_connection_never_offers_the_proxy_for_voice() {
        // **The safety property.** A name that cannot resolve must produce no
        // address at all rather than falling back to the socket's peer, which
        // through a proxy is the proxy itself.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let _ = listener.accept().await;
            tokio::time::sleep(Duration::from_secs(1)).await;
        });
        let tcp = TcpStream::connect(proxy_addr).await.unwrap();
        let chain = [crate::net::ProxySpec {
            kind: crate::net::ProxyKind::Socks5,
            host: "127.0.0.1".into(),
            port: proxy_addr.port(),
            username: None,
            password: None,
            tunnel_voice: false,
        }];
        let chosen = voice_address(&tcp, &chain, "no-such-host.invalid", 64738).await;
        assert_eq!(chosen, None, "an unresolvable server means tunnelled voice");
        assert_ne!(chosen, Some(proxy_addr), "never the proxy");
    }

    #[tokio::test]
    async fn tunnelled_voice_does_not_even_resolve_the_server() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let _ = listener.accept().await;
            tokio::time::sleep(Duration::from_secs(1)).await;
        });
        let tcp = TcpStream::connect(proxy_addr).await.unwrap();
        let chain = [crate::net::ProxySpec {
            kind: crate::net::ProxyKind::HttpConnect,
            host: "127.0.0.1".into(),
            port: proxy_addr.port(),
            username: None,
            password: None,
            tunnel_voice: true,
        }];
        // `localhost` would resolve instantly; the point is that it is not asked.
        let chosen = voice_address(&tcp, &chain, "localhost", 64738).await;
        assert_eq!(chosen, None);
    }
}
