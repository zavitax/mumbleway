//! Dialling a Mumble server through a proxy.
//!
//! **Both kinds here are *dial* protocols.** An HTTP `CONNECT` and a SOCKS5
//! `CONNECT` do the same thing in different words: open an ordinary TCP
//! connection to the proxy, exchange a few bytes, and from then on the socket is
//! a transparent pipe to the target. So TLS goes on top of the same
//! [`TcpStream`] it always did, the control channel's reader and writer keep
//! their concrete types, and nothing above this module needs to know whether a
//! proxy was involved at all.
//!
//! # Why this exists
//!
//! Measured on 2026-10-04 against one server: the TLS handshake completes, the
//! Mumble handshake completes, voice arrives over UDP — and ten to twenty
//! seconds later the TCP flow stops being delivered and is reset. From two
//! machines on one network, and never through a proxy, where the same session
//! answers every ping for as long as it is asked. Nothing in the client or the
//! server can fix a path that behaves like that; dialling through somewhere else
//! can.
//!
//! # Chaining
//!
//! [`dial`] takes a slice because a rider may ask for a server's own proxy to be
//! reached *through* another one. Each hop's handshake needs only the stream, so
//! a chain is a fold: dial the first proxy, ask it for the second proxy's
//! address, speak the second proxy's protocol down that same socket, ask *it* for
//! the server. Two hops is the only depth the interface produces today; the shape
//! does not care.
//!
//! # What is deliberately not here
//!
//! * **SOCKS5 UDP ASSOCIATE.** It would let voice stay on UDP through a SOCKS
//!   proxy rather than riding the control channel, but most proxies refuse it,
//!   and it needs a second socket whose lifetime is tied to this one. Voice
//!   through a proxy uses Mumble's own tunnel instead — see `session`.
//! * **SOCKS4/4a**, which has no credentials and no IPv6.
//! * **Digest, NTLM and Negotiate** proxy authentication. Basic only — but the
//!   failure names the scheme the proxy asked for, so "it will not connect"
//!   becomes "the proxy wants Negotiate".
//! * **An `https://` proxy**, where the connection to the proxy is itself TLS.
//!   That one would make the stream `TlsStream<TlsStream<TcpStream>>` and is the
//!   only thing here that would change a type outside this file.

use std::net::IpAddr;

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpStream;

use crate::error::{CoreError, Result};

/// A proxy failure worth another attempt in a moment: it could not reach
/// something, rather than refusing what was asked of it.
fn unreachable(reason: String) -> CoreError {
    CoreError::Proxy {
        reason,
        retry: true,
    }
}

/// A proxy failure that will say exactly the same thing next time.
fn refused(reason: String) -> CoreError {
    CoreError::Proxy {
        reason,
        retry: false,
    }
}

/// Longest proxy response header this will read before giving up.
///
/// A real `CONNECT` answer is a couple of hundred bytes. The cap is what stops a
/// proxy that answers with an endless stream of header lines from holding a
/// connection attempt open until the timeout.
const MAX_HEADER: usize = 8 * 1024;

/// Which language a proxy speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProxyKind {
    /// `CONNECT host:port HTTP/1.1` — what most corporate and self-hosted
    /// proxies offer.
    HttpConnect,
    /// RFC 1928, with RFC 1929 username and password where asked for.
    Socks5,
}

impl ProxyKind {
    /// The word a rider sees, and the word a link carries.
    pub fn as_str(self) -> &'static str {
        match self {
            ProxyKind::HttpConnect => "http",
            ProxyKind::Socks5 => "socks5",
        }
    }
}

/// One proxy, as a rider described it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProxySpec {
    pub kind: ProxyKind,
    pub host: String,
    pub port: u16,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
    /// Carry voice through this proxy as well, rather than sending it direct
    /// over UDP. The rider's choice; see `session`.
    #[serde(default)]
    pub tunnel_voice: bool,
}

impl ProxySpec {
    /// Whether the credentials fit what the protocols can carry.
    ///
    /// SOCKS5 writes each of them with a one-byte length, so 255 is the limit
    /// and anything longer would be silently truncated into a frame that means
    /// something else. Checked before dialling, so it reads as a thing the rider
    /// can fix rather than as a proxy that refuses them.
    fn check(&self) -> Result<()> {
        if self.host.trim().is_empty() {
            return Err(refused("the proxy has no address".into()));
        }
        if self.kind == ProxyKind::Socks5 {
            for (what, value) in [("username", &self.username), ("password", &self.password)] {
                if value.as_deref().is_some_and(|v| v.len() > 255) {
                    return Err(refused(format!(
                        "the proxy {what} is too long for SOCKS5 (255 bytes at most)"
                    )));
                }
            }
        }
        Ok(())
    }

    /// `kind://host:port`, for a log line or a link. Never the credentials.
    pub fn address(&self) -> String {
        format!("{}://{}:{}", self.kind.as_str(), self.host, self.port)
    }
}

/// Opens a TCP connection to `host:port` through every proxy in `chain`, in
/// order, and hands back the socket that reaches the far end.
///
/// An empty chain is an ordinary connection, so callers need no branch of their
/// own. The whole exchange is the caller's one timeout budget — see
/// `control::CONNECT_TIMEOUT`.
pub async fn dial(chain: &[ProxySpec], host: &str, port: u16) -> Result<TcpStream> {
    if chain.is_empty() {
        return Ok(TcpStream::connect((host, port)).await?);
    }
    for hop in chain {
        hop.check()?;
    }

    // The first hop is dialled directly; every later one is reached through the
    // socket the previous one handed back. The target of each handshake is the
    // *next* hop, and the last handshake asks for the server itself.
    let first = &chain[0];
    let mut stream = TcpStream::connect((first.host.as_str(), first.port))
        .await
        .map_err(|e| {
            unreachable(format!(
                "could not reach the proxy at {}:{}: {e}",
                first.host, first.port
            ))
        })?;

    for (i, hop) in chain.iter().enumerate() {
        let (next_host, next_port) = match chain.get(i + 1) {
            Some(next) => (next.host.as_str(), next.port),
            None => (host, port),
        };
        match hop.kind {
            ProxyKind::HttpConnect => http_connect(&mut stream, hop, next_host, next_port).await?,
            ProxyKind::Socks5 => socks5_connect(&mut stream, hop, next_host, next_port).await?,
        }
    }
    Ok(stream)
}

// ---------------------------------------------------------------------------
// HTTP CONNECT
// ---------------------------------------------------------------------------

async fn http_connect<S>(stream: &mut S, spec: &ProxySpec, host: &str, port: u16) -> Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    // The host goes out **as the rider typed it**: the proxy does the name
    // lookup, which is half the value of this on a network where DNS is
    // interfered with too. An IPv6 literal is bracketed, as the request line
    // cannot otherwise be told from a host with a port in it.
    let target = if host.contains(':') {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    };

    let mut request = format!("CONNECT {target} HTTP/1.1\r\nHost: {target}\r\n");
    if let Some(auth) = basic_auth(spec) {
        // Never logged, and never included in any error text.
        request.push_str(&format!("Proxy-Authorization: Basic {auth}\r\n"));
    }
    request.push_str("\r\n");
    stream.write_all(request.as_bytes()).await?;
    stream.flush().await?;

    let head = read_header(stream).await?;
    let status_line = head.lines().next().unwrap_or_default().to_string();
    if !status_line.starts_with("HTTP/") {
        return Err(refused(format!(
            "{} did not answer like an HTTP proxy",
            spec.address()
        )));
    }
    let code: u16 = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|c| c.parse().ok())
        .ok_or_else(|| refused(format!("the proxy answered \"{status_line}\"")))?;

    if (200..300).contains(&code) {
        return Ok(());
    }
    Err(match code {
        407 => {
            let scheme = head
                .lines()
                .find(|l| l.to_ascii_lowercase().starts_with("proxy-authenticate:"))
                .and_then(|l| l.split(':').nth(1))
                .map(|v| v.split_whitespace().next().unwrap_or("").to_string())
                .filter(|s| !s.is_empty());
            match (scheme.as_deref(), spec.username.is_some()) {
                (Some(s), _) if !s.eq_ignore_ascii_case("basic") => CoreError::ProxyAuth(format!(
                    "the proxy asked for {s} authentication, which MumbleWay does not speak"
                )),
                (_, true) => {
                    CoreError::ProxyAuth("the proxy rejected the username and password".into())
                }
                (_, false) => {
                    CoreError::ProxyAuth("the proxy wants a username and password".into())
                }
            }
        }
        403 => refused(format!(
            "the proxy refused to connect to {host}:{port} — many proxies allow only port 443"
        )),
        405 | 501 => refused("this proxy does not support CONNECT".into()),
        502..=504 => unreachable(format!("the proxy could not reach {host}:{port}")),
        _ => refused(format!("the proxy answered \"{status_line}\"")),
    })
}

/// Reads up to the blank line that ends an HTTP header, **one byte at a time**.
///
/// **This is not an oversight and must not be "optimised".** A buffered read
/// takes whatever the kernel has, and what follows the blank line is the first
/// byte of the server's TLS handshake. Swallowing it leaves TLS to start from
/// the middle of a record, which fails later and somewhere else; carrying the
/// leftovers forward would mean wrapping the stream in a buffer and dragging
/// that type through the control channel, which is exactly what keeping the
/// stream concrete avoids. One connection costs a couple of hundred one-byte
/// reads, once.
async fn read_header<S>(stream: &mut S) -> Result<String>
where
    S: AsyncRead + Unpin,
{
    let mut head = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        let n = stream.read(&mut byte).await?;
        if n == 0 {
            return Err(unreachable(
                "the proxy closed the connection without answering".into(),
            ));
        }
        head.push(byte[0]);
        if head.ends_with(b"\r\n\r\n") {
            break;
        }
        if head.len() > MAX_HEADER {
            return Err(refused("the proxy's answer never ended".into()));
        }
    }
    Ok(String::from_utf8_lossy(&head).into_owned())
}

/// `user:password` in base64, or `None` when there is nothing to send.
///
/// Hand-rolled rather than adding a crate: eleven lines against a dependency
/// carried through five cross-compiled targets.
fn basic_auth(spec: &ProxySpec) -> Option<String> {
    let user = spec.username.as_deref()?;
    let pass = spec.password.as_deref().unwrap_or("");
    Some(base64(format!("{user}:{pass}").as_bytes()))
}

fn base64(input: &[u8]) -> String {
    const SET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        out.push(SET[(n >> 18) as usize & 63] as char);
        out.push(SET[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            SET[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            SET[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

// ---------------------------------------------------------------------------
// SOCKS5
// ---------------------------------------------------------------------------

async fn socks5_connect<S>(stream: &mut S, spec: &ProxySpec, host: &str, port: u16) -> Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let offers_auth = spec.username.is_some();
    let greeting: &[u8] = if offers_auth {
        &[0x05, 0x02, 0x00, 0x02]
    } else {
        &[0x05, 0x01, 0x00]
    };
    stream.write_all(greeting).await?;
    stream.flush().await?;

    let mut answer = [0u8; 2];
    stream.read_exact(&mut answer).await?;
    // The mistake a rider will actually make is choosing the wrong kind in a
    // menu, and an HTTP proxy answers with the first letter of "HTTP/1.1".
    if answer[0] == b'H' {
        return Err(refused(format!(
            "{}:{} answered like an HTTP proxy, not SOCKS5",
            spec.host, spec.port
        )));
    }
    if answer[0] != 0x05 {
        return Err(refused(format!("{} is not a SOCKS5 proxy", spec.address())));
    }
    match answer[1] {
        0x00 => {}
        0x02 if offers_auth => socks5_authenticate(stream, spec).await?,
        0x02 => {
            return Err(CoreError::ProxyAuth(
                "the proxy wants a username and password".into(),
            ))
        }
        0xFF if offers_auth => {
            return Err(CoreError::ProxyAuth(
                "the proxy refused every way of authenticating that MumbleWay offers".into(),
            ))
        }
        0xFF => {
            return Err(CoreError::ProxyAuth(
                "the proxy wants a username and password".into(),
            ))
        }
        other => {
            return Err(refused(format!(
                "the proxy asked for authentication method {other}, which MumbleWay does not speak"
            )))
        }
    }

    // The address type says who resolves the name. A literal goes as a literal;
    // anything else goes as a domain, so the proxy does the lookup.
    let mut request = vec![0x05, 0x01, 0x00];
    match host.parse::<IpAddr>() {
        Ok(IpAddr::V4(v4)) => {
            request.push(0x01);
            request.extend_from_slice(&v4.octets());
        }
        Ok(IpAddr::V6(v6)) => {
            request.push(0x04);
            request.extend_from_slice(&v6.octets());
        }
        Err(_) => {
            if host.len() > 255 {
                return Err(refused("the server's name is too long for SOCKS5".into()));
            }
            request.push(0x03);
            request.push(host.len() as u8);
            request.extend_from_slice(host.as_bytes());
        }
    }
    request.extend_from_slice(&port.to_be_bytes());
    stream.write_all(&request).await?;
    stream.flush().await?;

    let mut head = [0u8; 4];
    stream.read_exact(&mut head).await?;
    if head[1] != 0x00 {
        return Err(socks5_failure(head[1], spec, host, port));
    }

    // **The bound address is the proxy's own, on the far side.** It is read to
    // keep the stream in step and then thrown away: it is not the server's
    // address and must never reach the UDP voice socket, however much it looks
    // like the thing you want.
    let mut discard = [0u8; 16];
    match head[3] {
        0x01 => {
            stream.read_exact(&mut discard[..4]).await?;
        }
        0x04 => {
            stream.read_exact(&mut discard[..16]).await?;
        }
        0x03 => {
            let mut len = [0u8; 1];
            stream.read_exact(&mut len).await?;
            let mut name = vec![0u8; len[0] as usize];
            stream.read_exact(&mut name).await?;
        }
        other => {
            return Err(refused(format!(
                "the proxy answered with address type {other}, which is not in SOCKS5"
            )))
        }
    }
    let mut bound_port = [0u8; 2];
    stream.read_exact(&mut bound_port).await?;
    Ok(())
}

async fn socks5_authenticate<S>(stream: &mut S, spec: &ProxySpec) -> Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let user = spec.username.as_deref().unwrap_or("");
    let pass = spec.password.as_deref().unwrap_or("");
    let mut message = vec![0x01, user.len() as u8];
    message.extend_from_slice(user.as_bytes());
    message.push(pass.len() as u8);
    message.extend_from_slice(pass.as_bytes());
    stream.write_all(&message).await?;
    stream.flush().await?;

    let mut answer = [0u8; 2];
    stream.read_exact(&mut answer).await?;
    if answer[1] != 0x00 {
        return Err(CoreError::ProxyAuth(
            "the proxy rejected the username and password".into(),
        ));
    }
    Ok(())
}

/// What a SOCKS5 refusal means, in words that name the right machine.
fn socks5_failure(code: u8, spec: &ProxySpec, host: &str, port: u16) -> CoreError {
    let proxy = spec.address();
    match code {
        0x01 => unreachable(format!("{proxy} could not open the connection")),
        0x02 => refused(format!("{proxy} refused to connect to {host}:{port}")),
        0x03 => unreachable(format!("{proxy} could not reach the network {host} is on")),
        0x04 => unreachable(format!("{proxy} could not reach {host}")),
        // The *server* said no, not the proxy — and a server that is
        // restarting says that for a few seconds and then stops.
        0x05 => unreachable(format!("{host}:{port} refused the connection")),
        0x06 => unreachable(format!("the connection to {host} timed out at {proxy}")),
        0x07 | 0x08 => refused(format!("{proxy} does not support this kind of connection")),
        other => refused(format!("{proxy} refused the connection (code {other})")),
    }
}

/// Whether the same failure is worth another attempt in a moment.
pub fn worth_retrying(e: &CoreError) -> bool {
    match e {
        CoreError::ProxyAuth(_) => false,
        CoreError::Proxy { retry, .. } => *retry,
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;

    /// A proxy that reads what we send it and answers with a fixed script.
    ///
    /// Returns the address to dial and a handle that yields everything the
    /// client wrote, so a test can assert the bytes rather than only the
    /// outcome.
    async fn fake_proxy(
        script: Vec<(usize, Vec<u8>)>,
    ) -> (std::net::SocketAddr, tokio::task::JoinHandle<Vec<u8>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut heard = Vec::new();
            for (expect, reply) in script {
                // `0` means "whatever arrives in one read", for a request whose
                // length the test does not want to count — an HTTP header.
                // Anything else is read exactly, however it was split up.
                if expect == 0 {
                    let mut buf = vec![0u8; 4096];
                    match socket.read(&mut buf).await {
                        Ok(0) | Err(_) => {}
                        Ok(n) => heard.extend_from_slice(&buf[..n]),
                    }
                } else {
                    let mut buf = vec![0u8; expect];
                    let mut got = 0;
                    while got < expect {
                        match socket.read(&mut buf[got..]).await {
                            Ok(0) | Err(_) => return heard,
                            Ok(n) => got += n,
                        }
                    }
                    heard.extend_from_slice(&buf[..got]);
                }
                if !reply.is_empty() && socket.write_all(&reply).await.is_err() {
                    return heard;
                }
            }
            // Hold the connection open so the client's own read is the thing
            // under test rather than a close racing it.
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            heard
        });
        (addr, handle)
    }

    fn spec(kind: ProxyKind, addr: std::net::SocketAddr) -> ProxySpec {
        ProxySpec {
            kind,
            host: addr.ip().to_string(),
            port: addr.port(),
            username: None,
            password: None,
            tunnel_voice: false,
        }
    }

    /// Reads one HTTP request header from the fake proxy's transcript.
    fn text(bytes: &[u8]) -> String {
        String::from_utf8_lossy(bytes).into_owned()
    }

    #[tokio::test]
    async fn http_connect_sends_the_host_as_typed_and_accepts_200() {
        // 46 bytes is the request below; the fake reads exactly that much.
        let (addr, heard) = fake_proxy(vec![(
            0,
            b"HTTP/1.1 200 Connection established\r\n\r\n".to_vec(),
        )])
        .await;
        let mut s = TcpStream::connect(addr).await.unwrap();
        let p = spec(ProxyKind::HttpConnect, addr);
        http_connect(&mut s, &p, "mumble.example", 64738)
            .await
            .expect("the proxy said 200");

        let sent = text(&heard.await.unwrap());
        assert!(
            sent.starts_with("CONNECT mumble.example:64738 HTTP/1.1\r\n"),
            "the name must go to the proxy unresolved: {sent:?}"
        );
        assert!(sent.contains("Host: mumble.example:64738\r\n"), "{sent:?}");
        assert!(
            !sent.contains("Proxy-Authorization"),
            "nothing to authenticate with: {sent:?}"
        );
    }

    #[tokio::test]
    async fn http_connect_does_not_swallow_the_first_bytes_after_the_header() {
        // **The test this module exists to keep passing.** The proxy writes its
        // answer and the server's first bytes in one go; a buffered read would
        // take both and TLS would then start from the middle of a record.
        let (addr, _heard) =
            fake_proxy(vec![(0, b"HTTP/1.1 200 OK\r\n\r\nSENTINEL".to_vec())]).await;
        let mut s = TcpStream::connect(addr).await.unwrap();
        let p = spec(ProxyKind::HttpConnect, addr);
        http_connect(&mut s, &p, "example.test", 64738)
            .await
            .unwrap();

        let mut rest = [0u8; 8];
        s.read_exact(&mut rest).await.unwrap();
        assert_eq!(&rest, b"SENTINEL", "the header read ate the server's bytes");
    }

    #[tokio::test]
    async fn http_connect_407_says_whether_to_type_a_password_or_give_up() {
        for (header, credentials, expected) in [
            (
                "Proxy-Authenticate: Basic realm=\"x\"",
                false,
                "wants a username",
            ),
            (
                "Proxy-Authenticate: Basic realm=\"x\"",
                true,
                "rejected the username",
            ),
            ("Proxy-Authenticate: Negotiate", false, "does not speak"),
        ] {
            let reply = format!("HTTP/1.1 407 Proxy Authentication Required\r\n{header}\r\n\r\n");
            let (addr, _) = fake_proxy(vec![(0, reply.into_bytes())]).await;
            let mut s = TcpStream::connect(addr).await.unwrap();
            let mut p = spec(ProxyKind::HttpConnect, addr);
            if credentials {
                p.username = Some("u".into());
                p.password = Some("p".into());
            }
            let e = http_connect(&mut s, &p, "example.test", 64738)
                .await
                .expect_err("407 is a failure");
            assert!(
                matches!(e, CoreError::ProxyAuth(_)),
                "a credential problem must not read as a server refusal: {e}"
            );
            assert!(e.to_string().contains(expected), "{e}");
            assert!(!worth_retrying(&e), "retrying this asks the same question");
        }
    }

    #[tokio::test]
    async fn http_connect_403_blames_the_proxy_and_502_blames_the_server() {
        let (addr, _) = fake_proxy(vec![(0, b"HTTP/1.1 403 Forbidden\r\n\r\n".to_vec())]).await;
        let mut s = TcpStream::connect(addr).await.unwrap();
        let p = spec(ProxyKind::HttpConnect, addr);
        let e = http_connect(&mut s, &p, "example.test", 64738)
            .await
            .unwrap_err();
        assert!(e.to_string().contains("refused to connect"), "{e}");
        assert!(
            e.to_string().contains("443"),
            "the usual cause is worth naming: {e}"
        );
        assert!(!worth_retrying(&e));

        let (addr, _) = fake_proxy(vec![(0, b"HTTP/1.1 502 Bad Gateway\r\n\r\n".to_vec())]).await;
        let mut s = TcpStream::connect(addr).await.unwrap();
        let p = spec(ProxyKind::HttpConnect, addr);
        let e = http_connect(&mut s, &p, "example.test", 64738)
            .await
            .unwrap_err();
        assert!(e.to_string().contains("could not reach"), "{e}");
        assert!(worth_retrying(&e), "the far end may simply be slow");
    }

    #[tokio::test]
    async fn http_connect_sends_basic_credentials_once_and_correctly() {
        let (addr, heard) = fake_proxy(vec![(0, b"HTTP/1.1 200 OK\r\n\r\n".to_vec())]).await;
        let mut s = TcpStream::connect(addr).await.unwrap();
        let mut p = spec(ProxyKind::HttpConnect, addr);
        p.username = Some("user".into());
        p.password = Some("pass".into());
        http_connect(&mut s, &p, "example.test", 64738)
            .await
            .unwrap();

        let sent = text(&heard.await.unwrap());
        assert!(
            sent.contains("Proxy-Authorization: Basic dXNlcjpwYXNz\r\n"),
            "{sent:?}"
        );
    }

    #[tokio::test]
    async fn an_answer_that_is_not_http_is_refused_rather_than_trusted() {
        let (addr, _) = fake_proxy(vec![(0, b"NOT HTTP AT ALL\r\n\r\n".to_vec())]).await;
        let mut s = TcpStream::connect(addr).await.unwrap();
        let p = spec(ProxyKind::HttpConnect, addr);
        let e = http_connect(&mut s, &p, "example.test", 64738)
            .await
            .unwrap_err();
        assert!(
            e.to_string().contains("did not answer like an HTTP proxy"),
            "{e}"
        );
    }

    #[tokio::test]
    async fn an_endless_header_is_refused_rather_than_read_for_ever() {
        let flood = vec![b'x'; MAX_HEADER + 64];
        let (addr, _) = fake_proxy(vec![(0, flood)]).await;
        let mut s = TcpStream::connect(addr).await.unwrap();
        let p = spec(ProxyKind::HttpConnect, addr);
        let e = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            http_connect(&mut s, &p, "example.test", 64738),
        )
        .await
        .expect("must not hang")
        .unwrap_err();
        assert!(e.to_string().contains("never ended"), "{e}");
    }

    #[tokio::test]
    async fn socks5_asks_the_proxy_to_resolve_the_name() {
        let (addr, heard) = fake_proxy(vec![
            (3, vec![0x05, 0x00]),
            (0, vec![0x05, 0x00, 0x00, 0x01, 127, 0, 0, 1, 0x00, 0x50]),
        ])
        .await;
        let mut s = TcpStream::connect(addr).await.unwrap();
        let p = spec(ProxyKind::Socks5, addr);
        socks5_connect(&mut s, &p, "mumble.example", 64738)
            .await
            .expect("the proxy said yes");

        let sent = heard.await.unwrap();
        // Greeting, then the request: version, connect, reserved, ATYP 3.
        assert_eq!(&sent[..3], &[0x05, 0x01, 0x00], "greeting");
        assert_eq!(
            &sent[3..7],
            &[0x05, 0x01, 0x00, 0x03],
            "a domain, not an address"
        );
        assert_eq!(sent[7] as usize, "mumble.example".len());
        assert_eq!(&sent[8..8 + 14], b"mumble.example");
        assert_eq!(&sent[22..24], &64738u16.to_be_bytes(), "port, big endian");
    }

    #[tokio::test]
    async fn socks5_sends_an_address_as_an_address() {
        let (addr, heard) = fake_proxy(vec![
            (3, vec![0x05, 0x00]),
            (0, vec![0x05, 0x00, 0x00, 0x01, 127, 0, 0, 1, 0x00, 0x50]),
        ])
        .await;
        let mut s = TcpStream::connect(addr).await.unwrap();
        let p = spec(ProxyKind::Socks5, addr);
        socks5_connect(&mut s, &p, "10.0.0.7", 64738).await.unwrap();

        let sent = heard.await.unwrap();
        assert_eq!(
            &sent[3..8],
            &[0x05, 0x01, 0x00, 0x01, 10],
            "ATYP 1 and the octets"
        );
    }

    #[tokio::test]
    async fn socks5_username_and_password_go_in_their_own_frame() {
        let (addr, heard) = fake_proxy(vec![
            (4, vec![0x05, 0x02]),
            (12, vec![0x01, 0x00]),
            (0, vec![0x05, 0x00, 0x00, 0x01, 0, 0, 0, 0, 0, 0]),
        ])
        .await;
        let mut s = TcpStream::connect(addr).await.unwrap();
        let mut p = spec(ProxyKind::Socks5, addr);
        p.username = Some("user".into());
        p.password = Some("secret".into());
        socks5_connect(&mut s, &p, "example.test", 64738)
            .await
            .unwrap();

        let sent = heard.await.unwrap();
        assert_eq!(
            &sent[..4],
            &[0x05, 0x02, 0x00, 0x02],
            "both methods offered"
        );
        assert_eq!(&sent[4..6], &[0x01, 4], "RFC 1929 version and name length");
        assert_eq!(&sent[6..10], b"user");
        assert_eq!(sent[10], 6, "password length");
        assert_eq!(&sent[11..17], b"secret");
    }

    #[tokio::test]
    async fn socks5_rejected_credentials_are_a_credential_failure() {
        let (addr, _) = fake_proxy(vec![(4, vec![0x05, 0x02]), (12, vec![0x01, 0x01])]).await;
        let mut s = TcpStream::connect(addr).await.unwrap();
        let mut p = spec(ProxyKind::Socks5, addr);
        p.username = Some("user".into());
        p.password = Some("secret".into());
        let e = socks5_connect(&mut s, &p, "example.test", 64738)
            .await
            .unwrap_err();
        assert!(matches!(e, CoreError::ProxyAuth(_)), "{e}");
        assert!(!worth_retrying(&e));
    }

    #[tokio::test]
    async fn socks5_refusals_name_the_right_machine() {
        for (code, expect, retry) in [
            (0x02u8, "refused to connect", false),
            (0x04, "could not reach", true),
            (0x05, "refused the connection", true),
            (0x07, "does not support", false),
        ] {
            let (addr, _) = fake_proxy(vec![
                (3, vec![0x05, 0x00]),
                (0, vec![0x05, code, 0x00, 0x01, 0, 0, 0, 0, 0, 0]),
            ])
            .await;
            let mut s = TcpStream::connect(addr).await.unwrap();
            let p = spec(ProxyKind::Socks5, addr);
            let e = socks5_connect(&mut s, &p, "example.test", 64738)
                .await
                .unwrap_err();
            assert!(e.to_string().contains(expect), "code {code:#x}: {e}");
            assert_eq!(worth_retrying(&e), retry, "code {code:#x}: {e}");
        }
    }

    #[tokio::test]
    async fn an_http_proxy_configured_as_socks_says_which_it_is() {
        // The mistake a rider actually makes is the one in the menu.
        let (addr, _) = fake_proxy(vec![(3, b"HTTP/1.1 400 Bad Request\r\n\r\n".to_vec())]).await;
        let mut s = TcpStream::connect(addr).await.unwrap();
        let p = spec(ProxyKind::Socks5, addr);
        let e = socks5_connect(&mut s, &p, "example.test", 64738)
            .await
            .unwrap_err();
        assert!(e.to_string().contains("like an HTTP proxy"), "{e}");
    }

    #[tokio::test]
    async fn credentials_too_long_for_socks5_are_refused_before_dialling() {
        let p = ProxySpec {
            kind: ProxyKind::Socks5,
            host: "127.0.0.1".into(),
            port: 1080,
            username: Some("u".repeat(300)),
            password: None,
            tunnel_voice: false,
        };
        let e = dial(std::slice::from_ref(&p), "example.test", 64738)
            .await
            .unwrap_err();
        assert!(e.to_string().contains("too long"), "{e}");
    }

    #[tokio::test]
    async fn an_empty_chain_is_an_ordinary_connection() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let _ = listener.accept().await;
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        });
        let s = dial(&[], "127.0.0.1", addr.port()).await.unwrap();
        assert_eq!(s.peer_addr().unwrap().port(), addr.port());
    }

    #[tokio::test]
    async fn a_chain_asks_each_hop_for_the_next_one() {
        // The second proxy is never dialled directly: the first is asked to
        // reach it, and the second is then asked for the server.
        let (second, second_heard) = fake_proxy(vec![
            (3, vec![0x05, 0x00]),
            (0, vec![0x05, 0x00, 0x00, 0x01, 0, 0, 0, 0, 0, 0]),
        ])
        .await;
        let (first, first_heard) = fake_proxy(vec![(0, b"HTTP/1.1 200 OK\r\n\r\n".to_vec())]).await;

        let chain = vec![
            spec(ProxyKind::HttpConnect, first),
            spec(ProxyKind::Socks5, second),
        ];
        // The first hop answers 200 and then relays nothing, so the SOCKS
        // handshake is answered by the fake *first* proxy's script — which is
        // enough to prove the ordering: what the first hop was asked for is the
        // second hop's address.
        let _ = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            dial(&chain, "mumble.example", 64738),
        )
        .await;

        let asked = text(&first_heard.await.unwrap());
        assert!(
            asked.starts_with(&format!("CONNECT {}:{} ", second.ip(), second.port())),
            "the first hop must be asked for the second, not for the server: {asked:?}"
        );
        drop(second_heard);
    }
}
