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
    /// The control channel went quiet for longer than the timeout.
    ///
    /// **`voice_still_arriving` is the half that explains it.** The control
    /// channel is TCP and voice is UDP, and they are not the same path: a
    /// middlebox that cuts a long-lived TLS session leaves the UDP flowing, so
    /// the rider sees a healthy ping beside a link this client has just called
    /// dead. Said plainly, that reads as a contradiction and a bug in the app;
    /// said with the other half, it points at where the fault actually is.
    PingTimeout { voice_still_arriving: bool },
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
            DisconnectReason::PingTimeout { .. } | DisconnectReason::TransportLost(_)
        )
    }
}

impl fmt::Display for DisconnectReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DisconnectReason::UserRequested => write!(f, "disconnected by user"),
            DisconnectReason::PingTimeout {
                voice_still_arriving,
            } => {
                if *voice_still_arriving {
                    write!(
                        f,
                        "the server stopped answering on the control link,                          although voice was still arriving"
                    )
                } else {
                    write!(f, "the server stopped answering")
                }
            }
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

#[cfg(test)]
mod tests {
    use super::*;

    /// **A rider reads this line next to a ping in milliseconds.** The two are
    /// measured on different paths — the ping over UDP, the liveness rule over
    /// the TCP control channel — so "ping timeout" beside a healthy 47 ms is a
    /// contradiction, and the obvious reading of a contradiction is that the
    /// app is broken. Saying which of the two went quiet turns it into a fact
    /// about the link, which is what it is.
    #[test]
    fn a_silent_control_channel_says_so_rather_than_blaming_the_ping() {
        let cut = DisconnectReason::PingTimeout {
            voice_still_arriving: true,
        };
        let words = cut.to_string();
        assert!(
            words.contains("control link") && words.contains("voice"),
            "the half that explains it is missing: {words}"
        );
        assert!(
            !words.contains("ping"),
            "a rider is looking at a ping figure while reading this: {words}"
        );

        let gone = DisconnectReason::PingTimeout {
            voice_still_arriving: false,
        };
        assert_eq!(gone.to_string(), "the server stopped answering");
    }

    /// Both are worth retrying, and both reset the backoff: a link that has
    /// been healthy for an hour and then goes quiet deserves an immediate
    /// retry rather than whatever delay an earlier failure left behind.
    #[test]
    fn a_quiet_link_is_recoverable_either_way() {
        for voice_still_arriving in [true, false] {
            let r = DisconnectReason::PingTimeout {
                voice_still_arriving,
            };
            assert!(r.is_recoverable());
            assert!(r.resets_backoff());
        }
    }
}
