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
    /// How many users are currently in this channel.
    pub user_count: u32,
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
    /// Silenced by us only. Unlike [`UserInfo::mute`] this needs no permission
    /// and is invisible to everyone else — their audio is simply dropped before
    /// it reaches the mixer.
    pub local_mute: bool,
    /// The MumbleWay version this user's client reported, if it identified
    /// itself as MumbleWay. `None` means *not known* to run it — an older
    /// MumbleWay says nothing, and neither does anybody on a server too old to
    /// relay the handshake — so its absence is never proof of anything.
    ///
    /// Filled in by `LiveState::user_list` from `peers::Peers`, which is the one
    /// record of it; the copy kept in the roster map is always `None`.
    #[serde(default)]
    pub mumbleway: Option<String>,
    /// What the server measures about this rider's connection, once it has been
    /// asked — see [`crate::session::quality`]. `None` until the first reply
    /// arrives, and for anybody outside our own channel.
    #[serde(default)]
    pub quality: Option<crate::session::quality::Quality>,
    /// The note this rider hung beside their own name, as plain text.
    ///
    /// Empty when they have none, and stripped of the markup Mumble's own
    /// client writes it in — see [`crate::session::notes`].
    #[serde(default)]
    pub comment: String,
}

impl UserInfo {
    /// One-word description of what this user is doing, for the roster.
    pub fn status_label(&self) -> &'static str {
        if self.deaf || self.self_deaf {
            "deafened"
        } else if self.local_mute {
            "muted for you"
        } else if self.mute || self.self_mute {
            "muted"
        } else if self.talking {
            "talking"
        } else {
            "silent"
        }
    }

    /// Whether we can hear this user at all right now.
    pub fn is_audible(&self) -> bool {
        !self.local_mute && !self.mute && !self.self_mute
    }
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
    /// Someone else changed our own mute or deafen state.
    ///
    /// Reported separately from the roster because it needs an audible cue: it
    /// happens to the user rather than being done by them, and they will not be
    /// looking at the screen.
    SelfModerated {
        muted: Option<bool>,
        deafened: Option<bool>,
        by: String,
    },
    /// Another MumbleWay rider asked this client to turn its microphone off
    /// (`mute: true`) or back on.
    ///
    /// **A request, not a change.** Nothing has happened yet: the app decides —
    /// the rider may have refused remote unmute, it may already be in that
    /// state, or it may be too soon after the last one — and only then acts and
    /// plays the cue. See `peers::RemoteMuteGuard`.
    RemoteMuteRequested {
        mute: bool,
        by: String,
    },
    /// What the server's administrator asks riders to do here.
    ///
    /// A suggestion and nothing more: the server neither enforces it nor checks
    /// it, and this client changes no setting on its own. Only the two this app
    /// can act on are carried — a suggested Mumble *version* is dropped, since
    /// this is not a Mumble build and no rider could act on it.
    ServerSuggests {
        push_to_talk: Option<bool>,
        positional: Option<bool>,
    },
    /// A rider's picture, as the bytes the server holds — PNG, JPEG or
    /// whatever else they uploaded.
    ///
    /// Sent on its own rather than with the roster, which goes out many times a
    /// second in a busy channel; an empty image means they have removed theirs.
    Avatar {
        session: u32,
        image: Vec<u8>,
    },
    /// What the server says this rider may do, here and on this server.
    ///
    /// Sent when it changes: on connect, on moving channel, and whenever the
    /// server revises or flushes its answer. See [`crate::session::permissions`]
    /// — it is a hint for greying out what would be refused, never a substitute
    /// for the refusal itself.
    Rights(crate::session::permissions::Rights),
    /// The server's certificate, reported so the UI can pin or compare it.
    ServerCertificate {
        fingerprint: String,
        changed: bool,
    },
    Welcome(String),
    /// The server refused something we asked it to do.
    ///
    /// Reported apart from [`Text`](Self::Text), which is where this used to go.
    /// A refusal dressed as a chat line from "server" is indistinguishable from
    /// somebody talking, and it scrolls away — so the one message telling a user
    /// why their action did nothing was the easiest thing on screen to miss.
    ///
    /// `reason` is the server's own words and may be empty: most servers send
    /// only a type. `kind` is that type, kept as a stable number so the UI can
    /// say something translated rather than nothing.
    Refused {
        reason: String,
        kind: u32,
    },
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
    /// Silence another user for us only. Always permitted.
    SetUserLocalMute {
        session: u32,
        muted: bool,
    },
    /// Silence another user for everyone. Requires the Mute permission on the
    /// server; without it the server answers with PermissionDenied.
    SetUserServerMute {
        session: u32,
        muted: bool,
    },
    /// Deafen another user server-side. Also permission-gated.
    SetUserServerDeaf {
        session: u32,
        deaf: bool,
    },
    /// Remove a user from the server. Requires the Kick permission.
    ///
    /// Distinct from a ban: the user may reconnect immediately.
    KickUser {
        session: u32,
        reason: String,
    },
    /// Ask the server to register us as a permanent user.
    ///
    /// **Registration is what makes a name yours.** On an unregistered server
    /// account anybody may take the name once you disconnect, and no ACL can
    /// name you. Registered, the server remembers the certificate behind the
    /// name and can put you in groups.
    ///
    /// Requires the SelfRegister permission on the root channel, which many
    /// servers withhold; without it the server answers `PermissionDenied` and
    /// the refusal surfaces the same way a refused kick does. It also needs the
    /// certificate we already keep — see `net::tls`, which is careful not to
    /// regenerate it precisely because that would lose this.
    RegisterSelf,
    /// Sets the note shown beside our own name. Empty clears it.
    ///
    /// Sent as plain text. Mumble's own client writes markup here and this one
    /// strips it on the way in, so writing markup back would be the one place
    /// the app produced something it will not display.
    SetComment(String),
    /// Channel to join automatically on every future connect. `None` clears it.
    SetDefaultChannel(Option<String>),
    /// Push-to-talk / voice-activation gate.
    SetTransmitting(bool),
    /// Accept a changed server certificate and re-pin it.
    AcceptCertificate,
    Shutdown,
}
