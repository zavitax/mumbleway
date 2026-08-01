//! Public data model shared with the UI layer.

use serde::{Deserialize, Serialize};

/// Where a session is in its lifecycle. This drives the status indicator.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ConnectionState {
    /// Configured but not connecting.
    Idle,
    /// TCP + TLS in progress.
    Connecting,
    /// Connected; exchanging Version/Authenticate/ServerSync.
    Handshaking,
    /// Fully connected and able to carry voice.
    Connected,
    /// Waiting to retry after a recoverable failure.
    Reconnecting {
        attempt: u32,
        /// Milliseconds until the next attempt, for a countdown in the UI.
        retry_in_ms: u64,
        reason: String,
    },
    /// Stopped and will not retry on its own.
    Disconnected { reason: String },
    /// Stopped because retrying cannot help (bad password, banned, cert mismatch).
    Failed { reason: String },
}

impl ConnectionState {
    /// Whether voice can flow right now.
    pub fn is_live(&self) -> bool {
        matches!(self, ConnectionState::Connected)
    }

    /// Whether the session is actively trying to establish or keep a connection.
    pub fn is_busy(&self) -> bool {
        matches!(
            self,
            ConnectionState::Connecting
                | ConnectionState::Handshaking
                | ConnectionState::Reconnecting { .. }
        )
    }
}

/// How voice is currently travelling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Transport {
    /// Low-latency UDP with OCB2 encryption.
    Udp,
    /// Tunnelled through the TLS control channel because UDP is blocked.
    TcpTunnel,
}

/// A saved server definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerProfile {
    pub id: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    /// Stored only if the user asked us to remember it.
    pub password: Option<String>,
    /// Pinned certificate fingerprint, set after the first successful connect.
    pub cert_fingerprint: Option<String>,
    /// Channel to join automatically once connected.
    pub auto_join_channel: Option<String>,
}

impl ServerProfile {
    pub fn new(
        name: impl Into<String>,
        host: impl Into<String>,
        port: u16,
        username: impl Into<String>,
    ) -> Self {
        let host = host.into();
        Self {
            // Stable id derived from the connection tuple, so re-adding the same
            // server does not silently duplicate its pinned certificate.
            id: format!("{}:{}", host, port),
            name: name.into(),
            host,
            port,
            username: username.into(),
            password: None,
            cert_fingerprint: None,
            auto_join_channel: None,
        }
    }
}

impl Default for ServerProfile {
    fn default() -> Self {
        Self::new("", "", 64738, "")
    }
}

/// A channel on the server.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChannelInfo {
    pub id: u32,
    pub parent: Option<u32>,
    pub name: String,
    pub description: String,
    pub position: i32,
    pub max_users: u32,
}

/// A connected user.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UserInfo {
    pub session: u32,
    pub name: String,
    pub channel_id: u32,
    /// Muted by an admin.
    pub mute: bool,
    /// Deafened by an admin.
    pub deaf: bool,
    pub self_mute: bool,
    pub self_deaf: bool,
    /// Set locally when we are receiving audio from this user.
    pub talking: bool,
}

/// Round-trip and quality figures for the status UI.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct NetworkStats {
    pub tcp_ping_ms: f32,
    pub udp_ping_ms: f32,
    pub packets_lost: u32,
    pub packets_late: u32,
    pub transport: Option<TransportStat>,
}

/// `Transport` in a serde-friendly shape for the stats struct.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransportStat {
    Udp,
    TcpTunnel,
}

impl From<Transport> for TransportStat {
    fn from(t: Transport) -> Self {
        match t {
            Transport::Udp => TransportStat::Udp,
            Transport::TcpTunnel => TransportStat::TcpTunnel,
        }
    }
}

/// Everything the UI observes about one session.
#[derive(Debug, Clone)]
pub enum SessionEvent {
    State(ConnectionState),
    Channels(Vec<ChannelInfo>),
    Users(Vec<UserInfo>),
    /// Our own session id, once the server assigns it.
    SelfSession(u32),
    Talking {
        session: u32,
        talking: bool,
    },
    Text {
        from: String,
        message: String,
    },
    Stats(NetworkStats),
    TransportChanged(Transport),
    /// The server's certificate, reported so the UI can pin or compare it.
    ServerCertificate {
        fingerprint: String,
        changed: bool,
    },
    Welcome(String),
}

/// Commands the UI issues to a session.
#[derive(Debug, Clone)]
pub enum SessionCommand {
    Connect,
    /// Explicit user disconnect — suppresses automatic reconnection.
    Disconnect,
    JoinChannel(u32),
    SendText {
        channel_id: Option<u32>,
        message: String,
    },
    SetSelfMute(bool),
    SetSelfDeaf(bool),
    /// Push-to-talk / voice-activation gate.
    SetTransmitting(bool),
    /// Accept a changed server certificate and re-pin it.
    AcceptCertificate,
    Shutdown,
}
