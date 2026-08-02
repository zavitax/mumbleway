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
    /// The address we actually connected to, reused for the UDP voice socket.
    pub peer: std::net::SocketAddr,
    /// The certificate the server presented.
    pub observed: Arc<ObservedCert>,
}

/// Opens a TCP + TLS connection to a Mumble server.
pub async fn connect(
    host: &str,
    port: u16,
    config: Arc<rustls::ClientConfig>,
    observed: Arc<ObservedCert>,
) -> Result<Connected> {
    let tcp = tokio::time::timeout(CONNECT_TIMEOUT, TcpStream::connect((host, port)))
        .await
        .map_err(|_| CoreError::Timeout("TCP connect"))??;

    // Voice latency is dominated by packetisation, so Nagle buffering only ever
    // hurts here — especially for tunnelled audio.
    tcp.set_nodelay(true).ok();
    let peer = tcp.peer_addr()?;

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
        peer,
        observed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn connecting_to_a_closed_port_fails_promptly() {
        let cfg = crate::net::tls::client_config(None, crate::net::TrustPolicy::AcceptAny).unwrap();
        // Port 1 on loopback has nothing listening.
        let r = connect("127.0.0.1", 1, cfg.0, cfg.1).await;
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
            connect("127.0.0.1", addr.port(), cfg.0, cfg.1),
        )
        .await;
        assert!(r.is_ok(), "connect() must not hang past its own timeout");
        assert!(r.unwrap().is_err(), "a non-TLS peer must be rejected");
    }
}
