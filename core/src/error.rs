//! Error types shared across the core.

use std::fmt;

pub type Result<T> = std::result::Result<T, CoreError>;

#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("protocol error: {0}")]
    Protocol(&'static str),

    #[error("connection rejected by server: {0}")]
    Rejected(String),

    #[error("authentication failed: {0}")]
    Auth(String),

    #[error("cryptographic failure: {0}")]
    Crypto(&'static str),

    #[error("audio device error: {0}")]
    Audio(String),

    #[error("codec error: {0}")]
    Codec(String),

    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),

    #[error("TLS error: {0}")]
    Tls(String),

    #[error("decode error: {0}")]
    Decode(#[from] prost::DecodeError),

    #[error("connection timed out waiting for {0}")]
    Timeout(&'static str),

    #[error("disconnected: {0}")]
    Disconnected(DisconnectReason),

    #[error("{0}")]
    Other(String),
}

/// Why a session ended. The manager uses this to decide whether to reconnect —
/// everything except [`DisconnectReason::UserRequested`] is treated as recoverable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DisconnectReason {
    /// The user pressed disconnect. Never auto-reconnects.
    UserRequested,
    /// No ping response within the timeout window.
    PingTimeout,
    /// Socket closed or errored.
    TransportLost(String),
    /// Server actively rejected us.
    ///
    /// `retry` separates the rejections worth waiting out from the ones that
    /// will say exactly the same thing every ten seconds forever.
    ServerRejected { reason: String, retry: bool },
    /// Handshake did not complete in time.
    HandshakeTimeout,
    /// Anything else.
    Error(String),
}

impl DisconnectReason {
    /// Whether the session manager should attempt to reconnect.
    ///
    /// A rejection is the interesting case. Retrying a wrong password, a
    /// username already in use, or a client too old to be allowed in cannot
    /// ever succeed: the answer is a property of the request, not of the
    /// moment. Retrying anyway hammers the server, invites a ban for repeated
    /// authentication failures, and — worst of the three — hides what the
    /// server actually said behind a spinner that says "reconnecting", so the
    /// one person who could fix it never finds out what is wrong.
    pub fn is_recoverable(&self) -> bool {
        match self {
            DisconnectReason::UserRequested => false,
            DisconnectReason::ServerRejected { retry, .. } => *retry,
            _ => true,
        }
    }

    /// Whether backoff should reset — a clean transport loss after a long healthy
    /// session should retry immediately rather than inheriting old backoff.
    pub fn resets_backoff(&self) -> bool {
        matches!(
            self,
            DisconnectReason::PingTimeout | DisconnectReason::TransportLost(_)
        )
    }
}

impl fmt::Display for DisconnectReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DisconnectReason::UserRequested => write!(f, "disconnected by user"),
            DisconnectReason::PingTimeout => write!(f, "ping timeout"),
            DisconnectReason::TransportLost(e) => write!(f, "connection lost: {e}"),
            DisconnectReason::ServerRejected { reason, .. } => {
                write!(f, "rejected by server: {reason}")
            }
            DisconnectReason::HandshakeTimeout => write!(f, "handshake timed out"),
            DisconnectReason::Error(e) => write!(f, "{e}"),
        }
    }
}

impl From<anyhow::Error> for CoreError {
    fn from(e: anyhow::Error) -> Self {
        CoreError::Other(e.to_string())
    }
}
