//! A single server session: connect, authenticate, stay alive, reconnect.

pub mod manager;
pub mod profile;
pub mod reconnect;
pub mod types;

pub use reconnect::{BackoffPolicy, ReconnectState};
pub use types::*;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use tokio::sync::mpsc;

use crate::crypto::CryptState;
use crate::error::{CoreError, DisconnectReason, Result};
use crate::net::audio_packet::VoicePacket;
use crate::net::control::{self, ControlReader, ControlWriter};
use crate::net::tls::{self, Identity, TrustPolicy};
use crate::net::voice::{UdpEvent, VoiceSocket};
use crate::proto::{mumble, version_v1, version_v2, MessageType};

/// How often we ping the server. Mumble drops clients after 30 s of silence.
const PING_INTERVAL: Duration = Duration::from_secs(5);

/// Declare the link dead if the server says nothing at all for this long.
///
/// 15 s is the specified budget: with a 5 s ping interval that is three missed
/// pings — long enough to ride out a brief stall, short enough that a rider
/// hears the drop cue promptly rather than talking into a dead link.
const SERVER_SILENCE_TIMEOUT: Duration = Duration::from_secs(15);

/// Cap on how long the Version/Authenticate/ServerSync exchange may take.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(15);

/// Channels connecting a session to the audio engine.
pub struct AudioBridge {
    /// Encoded Opus frames ready to transmit.
    pub outgoing: mpsc::Receiver<(u64, Vec<u8>, bool)>,
    /// Voice packets received from the server, tagged with the sender's session.
    pub incoming: mpsc::Sender<VoicePacket>,
}

/// Everything needed to run a session.
pub struct SessionConfig {
    pub profile: ServerProfile,
    pub identity: Identity,
    pub client_name: String,
    pub backoff: BackoffPolicy,
}

/// Live, mutable state for one connected session.
struct LiveState {
    self_session: Option<u32>,
    channels: HashMap<u32, ChannelInfo>,
    users: HashMap<u32, UserInfo>,
    crypt: Option<CryptState>,
    transport: Transport,
    stats: NetworkStats,
    /// Last time we heard anything at all from the server.
    last_heard: Instant,
    /// When the current connection became fully established.
    connected_at: Option<Instant>,
}

impl LiveState {
    fn new() -> Self {
        Self {
            self_session: None,
            channels: HashMap::new(),
            users: HashMap::new(),
            crypt: None,
            transport: Transport::TcpTunnel,
            stats: NetworkStats::default(),
            last_heard: Instant::now(),
            connected_at: None,
        }
    }

    fn channel_list(&self) -> Vec<ChannelInfo> {
        let mut v: Vec<ChannelInfo> = self.channels.values().cloned().collect();
        // Occupancy is derived rather than tracked: the server never sends it,
        // and deriving it keeps it correct as users move between channels.
        for c in v.iter_mut() {
            c.user_count = self.users.values().filter(|u| u.channel_id == c.id).count() as u32;
        }
        v.sort_by(|a, b| a.position.cmp(&b.position).then(a.name.cmp(&b.name)));
        v
    }

    /// Finds a channel by name, case-insensitively.
    fn channel_by_name(&self, name: &str) -> Option<u32> {
        self.channels
            .values()
            .find(|c| c.name.eq_ignore_ascii_case(name))
            .map(|c| c.id)
    }

    fn is_locally_muted(&self, session: u32) -> bool {
        self.users.get(&session).is_some_and(|u| u.local_mute)
    }

    /// Which channel a session is currently in, if we know about it.
    fn self_channel(&self, session: Option<u32>) -> Option<u32> {
        session
            .and_then(|s| self.users.get(&s))
            .map(|u| u.channel_id)
    }

    fn user_list(&self) -> Vec<UserInfo> {
        let mut v: Vec<_> = self.users.values().cloned().collect();
        v.sort_by_key(|u| u.name.to_lowercase());
        v
    }
}

/// Drives one server connection, reconnecting as needed, until told to shut down.
pub struct Session {
    config: SessionConfig,
    events: mpsc::Sender<SessionEvent>,
    commands: mpsc::Receiver<SessionCommand>,
    audio: AudioBridge,
    reconnect: ReconnectState,
    /// Set when the user accepts a changed certificate.
    accept_cert_once: bool,
}

impl Session {
    pub fn new(
        config: SessionConfig,
        events: mpsc::Sender<SessionEvent>,
        commands: mpsc::Receiver<SessionCommand>,
        audio: AudioBridge,
    ) -> Self {
        let backoff = config.backoff.clone();
        Self {
            config,
            events,
            commands,
            audio,
            reconnect: ReconnectState::new(backoff),
            accept_cert_once: false,
        }
    }

    async fn emit(&self, e: SessionEvent) {
        // A full or closed event channel must never stall the network loop.
        let _ = self.events.try_send(e);
    }

    async fn set_state(&self, s: ConnectionState) {
        self.emit(SessionEvent::State(s)).await;
    }

    /// Main loop: connect, run, decide whether to retry, repeat.
    pub async fn run(mut self) {
        // A session starts idle and only connects once asked.
        self.reconnect.stop();

        loop {
            if self.reconnect.stopped_by_user() {
                match self.wait_for_connect_command().await {
                    Some(true) => self.reconnect.arm(),
                    Some(false) => continue,
                    None => return, // shutdown
                }
            }

            self.set_state(ConnectionState::Connecting).await;

            let outcome = self.connect_and_run().await;
            let reason = match outcome {
                Ok(r) => r,
                Err(e) => Self::classify(e),
            };

            match self.reconnect.on_disconnect(&reason) {
                None => {
                    if matches!(reason, DisconnectReason::UserRequested) {
                        self.set_state(ConnectionState::Disconnected {
                            reason: reason.to_string(),
                        })
                        .await;
                    } else {
                        self.set_state(ConnectionState::Failed {
                            reason: reason.to_string(),
                        })
                        .await;
                    }
                }
                Some(delay) => {
                    self.set_state(ConnectionState::Reconnecting {
                        attempt: self.reconnect.attempt(),
                        retry_in_ms: delay.as_millis() as u64,
                        reason: reason.to_string(),
                    })
                    .await;

                    // Sleep, but stay responsive to Disconnect/Shutdown.
                    if !self.sleep_interruptibly(delay).await {
                        return;
                    }
                }
            }
        }
    }

    /// Maps a transport error onto a disconnect reason.
    fn classify(e: CoreError) -> DisconnectReason {
        match e {
            CoreError::Disconnected(r) => r,
            CoreError::Rejected(m) | CoreError::Auth(m) => DisconnectReason::ServerRejected(m),
            CoreError::Timeout(what) => {
                if what == "handshake" {
                    DisconnectReason::HandshakeTimeout
                } else {
                    DisconnectReason::TransportLost(what.to_string())
                }
            }
            CoreError::Io(e) => DisconnectReason::TransportLost(e.to_string()),
            other => DisconnectReason::Error(other.to_string()),
        }
    }

    /// Waits for a command while disconnected.
    /// `Some(true)` -> connect, `Some(false)` -> keep waiting, `None` -> shut down.
    async fn wait_for_connect_command(&mut self) -> Option<bool> {
        match self.commands.recv().await {
            Some(SessionCommand::Connect) => Some(true),
            Some(SessionCommand::AcceptCertificate) => {
                self.accept_cert_once = true;
                Some(true)
            }
            Some(SessionCommand::Shutdown) | None => None,
            Some(_) => Some(false),
        }
    }

    /// Sleeps, returning false if we should shut down entirely.
    async fn sleep_interruptibly(&mut self, delay: Duration) -> bool {
        let deadline = tokio::time::sleep(delay);
        tokio::pin!(deadline);
        loop {
            tokio::select! {
                _ = &mut deadline => return true,
                cmd = self.commands.recv() => match cmd {
                    Some(SessionCommand::Disconnect) => {
                        self.reconnect.stop();
                        self.set_state(ConnectionState::Disconnected {
                            reason: "disconnected by user".into(),
                        }).await;
                        return true;
                    }
                    Some(SessionCommand::AcceptCertificate) => {
                        self.accept_cert_once = true;
                        return true; // retry immediately with the new trust decision
                    }
                    Some(SessionCommand::Shutdown) | None => return false,
                    Some(_) => continue,
                }
            }
        }
    }

    /// One full connection attempt. Returns why it ended.
    async fn connect_and_run(&mut self) -> Result<DisconnectReason> {
        let policy = if self.accept_cert_once {
            TrustPolicy::AcceptAny
        } else {
            match &self.config.profile.cert_fingerprint {
                Some(fp) => TrustPolicy::Pinned(fp.clone()),
                None => TrustPolicy::TrustOnFirstUse,
            }
        };

        let (tls_config, observed) = tls::client_config(Some(&self.config.identity), policy)?;
        let conn = control::connect(
            &self.config.profile.host,
            self.config.profile.port,
            tls_config,
            observed,
        )
        .await?;

        let control::Connected {
            mut reader,
            mut writer,
            peer,
            observed,
        } = conn;

        if let Some(fp) = observed.fingerprint() {
            let changed = self
                .config
                .profile
                .cert_fingerprint
                .as_ref()
                .is_some_and(|p| !p.eq_ignore_ascii_case(&fp));
            self.emit(SessionEvent::ServerCertificate {
                fingerprint: fp.clone(),
                changed,
            })
            .await;
            // Pin on first successful contact.
            if self.config.profile.cert_fingerprint.is_none() || self.accept_cert_once {
                self.config.profile.cert_fingerprint = Some(fp);
            }
        }
        self.accept_cert_once = false;

        self.set_state(ConnectionState::Handshaking).await;
        let mut state = LiveState::new();

        // --- handshake -----------------------------------------------------
        let version = mumble::Version {
            version_v1: Some(version_v1(1, 4, 0)),
            version_v2: Some(version_v2(1, 4, 0)),
            release: Some(self.config.client_name.clone()),
            os: Some(std::env::consts::OS.to_string()),
            os_version: Some(std::env::consts::ARCH.to_string()),
        };
        writer.send(MessageType::Version, &version).await?;

        let auth = mumble::Authenticate {
            username: Some(self.config.profile.username.clone()),
            password: self.config.profile.password.clone(),
            tokens: Vec::new(),
            // Empty CELT list plus opus=true advertises an Opus-only client.
            celt_versions: Vec::new(),
            opus: Some(true),
            client_type: Some(0),
        };
        writer.send(MessageType::Authenticate, &auth).await?;

        // Pump messages until ServerSync arrives (or we are rejected).
        let sync_deadline = Instant::now() + HANDSHAKE_TIMEOUT;
        loop {
            if Instant::now() >= sync_deadline {
                return Err(CoreError::Timeout("handshake"));
            }
            let remaining = sync_deadline.saturating_duration_since(Instant::now());
            let (msg_type, payload) = match tokio::time::timeout(remaining, reader.recv()).await {
                Err(_) => return Err(CoreError::Timeout("handshake")),
                Ok(r) => r?,
            };
            state.last_heard = Instant::now();

            if let Some(reason) = self
                .handle_control(msg_type, &payload, &mut state, &mut writer)
                .await?
            {
                return Ok(reason);
            }
            if state.self_session.is_some() {
                break;
            }
        }

        // Join the remembered default channel. The server places us in the
        // root channel on connect, so this has to be an explicit move; matching
        // by name rather than id keeps it working if the server renumbers.
        if let Some(name) = self.config.profile.auto_join_channel.clone() {
            match state.channel_by_name(&name) {
                Some(id) if Some(id) != state.self_channel(state.self_session) => {
                    let m = mumble::UserState {
                        session: state.self_session,
                        channel_id: Some(id),
                        ..Default::default()
                    };
                    writer.send(MessageType::UserState, &m).await?;
                }
                Some(_) => {}
                None => {
                    self.emit(SessionEvent::Text {
                        from: "MumbleWay".into(),
                        message: format!("Default channel \"{name}\" no longer exists."),
                    })
                    .await;
                }
            }
        }

        // --- UDP setup -----------------------------------------------------
        let mut udp = match state.crypt.take() {
            Some(crypt) => match VoiceSocket::bind(peer, crypt).await {
                Ok(mut s) => {
                    let _ = s.send_ping(now_millis()).await;
                    Some(s)
                }
                Err(_) => None, // UDP unavailable; the tunnel still works
            },
            None => None,
        };

        state.connected_at = Some(Instant::now());
        state.transport = Transport::TcpTunnel; // until a pong proves UDP works
        self.set_state(ConnectionState::Connected).await;
        self.emit(SessionEvent::Channels(state.channel_list()))
            .await;
        self.emit(SessionEvent::Users(state.user_list())).await;

        // From here the reader lives in its own task; see [`spawn_reader`].
        let mut messages = spawn_reader(reader);
        self.run_connected(&mut messages, &mut writer, &mut udp, &mut state)
            .await
    }

    /// The steady-state loop once connected.
    async fn run_connected(
        &mut self,
        messages: &mut mpsc::Receiver<Result<(u16, Vec<u8>)>>,
        writer: &mut ControlWriter,
        udp: &mut Option<VoiceSocket>,
        state: &mut LiveState,
    ) -> Result<DisconnectReason> {
        let mut ping_timer = tokio::time::interval(PING_INTERVAL);
        ping_timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut health_timer = tokio::time::interval(Duration::from_secs(1));
        health_timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

        loop {
            // Splitting the borrow keeps `udp` usable inside the select arms.
            let udp_recv = async {
                match udp.as_mut() {
                    Some(s) => s.recv().await.map(Some),
                    None => {
                        // Nothing to poll; park forever so the branch never fires.
                        std::future::pending::<()>().await;
                        Ok(None)
                    }
                }
            };

            tokio::select! {
                // --- control channel ---------------------------------------
                // Cancel-safe: whole messages arrive over a channel, so losing
                // this race can never leave a half-read message on the socket.
                msg = messages.recv() => {
                    let (msg_type, payload) = match msg {
                        Some(Ok(m)) => m,
                        Some(Err(e)) => return Err(e),
                        None => {
                            return Ok(DisconnectReason::TransportLost(
                                "control connection closed".into()));
                        }
                    };
                    state.last_heard = Instant::now();
                    if let Some(reason) = self.handle_control(msg_type, &payload, state, writer).await? {
                        return Ok(reason);
                    }
                }

                // --- UDP voice ---------------------------------------------
                ev = udp_recv => {
                    match ev {
                        Ok(Some(UdpEvent::Voice(p))) => {
                            self.on_voice(p, state).await;
                        }
                        Ok(Some(UdpEvent::Pong { rtt })) => {
                            state.stats.udp_ping_ms = rtt.as_secs_f32() * 1000.0;
                            if state.transport != Transport::Udp {
                                state.transport = Transport::Udp;
                                self.emit(SessionEvent::TransportChanged(Transport::Udp)).await;
                            }
                        }
                        Ok(Some(UdpEvent::Rejected(_))) | Ok(None) => {}
                        Err(_) => {
                            // The UDP socket died; drop to the tunnel rather than
                            // tearing down a working control connection.
                            *udp = None;
                            if state.transport != Transport::TcpTunnel {
                                state.transport = Transport::TcpTunnel;
                                self.emit(SessionEvent::TransportChanged(Transport::TcpTunnel)).await;
                            }
                        }
                    }
                }

                // --- outgoing audio ----------------------------------------
                frame = self.audio.outgoing.recv() => {
                    // A `None` here means the audio engine went away; the session
                    // stays up regardless, so there is nothing to handle.
                    if let Some((sequence, opus, terminator)) = frame {
                        let packet = VoicePacket::speech(sequence, opus, terminator);
                        let sent_over_udp = match (state.transport, udp.as_mut()) {
                            (Transport::Udp, Some(s)) => s.send_voice(&packet).await.is_ok(),
                            _ => false,
                        };
                        if !sent_over_udp {
                            writer.send_tunnel(&packet.encode_outgoing()).await?;
                        }
                    }
                }

                // --- commands ----------------------------------------------
                cmd = self.commands.recv() => {
                    match cmd {
                        Some(SessionCommand::Disconnect) => {
                            self.reconnect.stop();
                            writer.shutdown().await;
                            return Ok(DisconnectReason::UserRequested);
                        }
                        Some(SessionCommand::Shutdown) | None => {
                            self.reconnect.stop();
                            writer.shutdown().await;
                            return Ok(DisconnectReason::UserRequested);
                        }
                        Some(c) => self.handle_command(c, state, writer).await?,
                    }
                }

                // --- keepalive ---------------------------------------------
                _ = ping_timer.tick() => {
                    let stats = udp.as_ref().map(|s| s.crypt_stats()).unwrap_or_default();
                    let ping = mumble::Ping {
                        timestamp: Some(now_millis()),
                        good: Some(stats.good),
                        late: Some(stats.late),
                        lost: Some(stats.lost),
                        resync: Some(stats.resync),
                        ..Default::default()
                    };
                    writer.send(MessageType::Ping, &ping).await?;
                    if let Some(s) = udp.as_mut() {
                        let _ = s.send_ping(now_millis()).await;
                    }
                }

                // --- liveness ----------------------------------------------
                _ = health_timer.tick() => {
                    let now = Instant::now();
                    if now.duration_since(state.last_heard) > SERVER_SILENCE_TIMEOUT {
                        return Ok(DisconnectReason::PingTimeout);
                    }
                    // Fall back to the tunnel if UDP goes quiet mid-session.
                    if state.transport == Transport::Udp
                        && !udp.as_ref().map(|s| s.is_healthy(now)).unwrap_or(false)
                    {
                        state.transport = Transport::TcpTunnel;
                        self.emit(SessionEvent::TransportChanged(Transport::TcpTunnel)).await;
                    }
                    if let Some(at) = state.connected_at {
                        if now.duration_since(at) > reconnect::HEALTHY_RESET_AFTER {
                            self.reconnect.note_healthy();
                        }
                    }
                    state.stats.transport = Some(state.transport.into());
                    self.emit(SessionEvent::Stats(state.stats)).await;
                }
            }
        }
    }

    async fn on_voice(&self, packet: VoicePacket, state: &mut LiveState) {
        // Locally muted users are dropped here, before the mixer ever sees
        // them. Doing it at this point rather than in the audio engine means
        // one check covers both the UDP and tunnelled paths.
        if let Some(session) = packet.session {
            if state.is_locally_muted(session) {
                return;
            }
        }
        if let Some(session) = packet.session {
            if let Some(u) = state.users.get_mut(&session) {
                if !u.talking {
                    u.talking = true;
                    self.emit(SessionEvent::Talking {
                        session,
                        talking: true,
                    })
                    .await;
                }
            }
            if packet.terminator {
                if let Some(u) = state.users.get_mut(&session) {
                    u.talking = false;
                }
                self.emit(SessionEvent::Talking {
                    session,
                    talking: false,
                })
                .await;
            }
        }
        let _ = self.audio.incoming.try_send(packet);
    }

    /// Handles one control message. Returns `Some(reason)` to end the session.
    async fn handle_control(
        &mut self,
        msg_type: u16,
        payload: &[u8],
        state: &mut LiveState,
        writer: &mut ControlWriter,
    ) -> Result<Option<DisconnectReason>> {
        use prost::Message;

        let Some(kind) = MessageType::from_u16(msg_type) else {
            return Ok(None); // forward compatible: ignore unknown types
        };

        match kind {
            MessageType::Reject => {
                let m = mumble::Reject::decode(payload)?;
                let reason = m.reason.unwrap_or_else(|| "rejected".into());
                return Ok(Some(DisconnectReason::ServerRejected(reason)));
            }
            MessageType::CryptSetup => {
                let m = mumble::CryptSetup::decode(payload)?;
                match (m.key, m.client_nonce, m.server_nonce) {
                    (Some(k), Some(cn), Some(sn)) => {
                        // client_nonce is our encrypt IV, server_nonce our decrypt IV.
                        state.crypt = Some(CryptState::new(&k, &cn, &sn)?);
                    }
                    (None, None, Some(sn)) => {
                        // Server-initiated resync of just the decrypt IV.
                        if let Some(c) = state.crypt.as_mut() {
                            c.set_decrypt_iv(&sn)?;
                        }
                    }
                    _ => {}
                }
            }
            MessageType::ServerSync => {
                let m = mumble::ServerSync::decode(payload)?;
                if let Some(s) = m.session {
                    state.self_session = Some(s);
                    self.emit(SessionEvent::SelfSession(s)).await;
                }
                if let Some(w) = m.welcome_text {
                    if !w.trim().is_empty() {
                        self.emit(SessionEvent::Welcome(w)).await;
                    }
                }
            }
            MessageType::ChannelState => {
                let m = mumble::ChannelState::decode(payload)?;
                if let Some(id) = m.channel_id {
                    let e = state.channels.entry(id).or_insert_with(|| ChannelInfo {
                        id,
                        parent: None,
                        name: String::new(),
                        description: String::new(),
                        position: 0,
                        max_users: 0,
                        user_count: 0,
                    });
                    if let Some(p) = m.parent {
                        e.parent = Some(p);
                    }
                    if let Some(n) = m.name {
                        e.name = n;
                    }
                    if let Some(d) = m.description {
                        e.description = d;
                    }
                    if let Some(p) = m.position {
                        e.position = p;
                    }
                    if let Some(mu) = m.max_users {
                        e.max_users = mu;
                    }
                    self.emit(SessionEvent::Channels(state.channel_list()))
                        .await;
                }
            }
            MessageType::ChannelRemove => {
                let m = mumble::ChannelRemove::decode(payload)?;
                state.channels.remove(&m.channel_id);
                self.emit(SessionEvent::Channels(state.channel_list()))
                    .await;
            }
            MessageType::UserState => {
                let m = mumble::UserState::decode(payload)?;
                if let Some(s) = m.session {
                    let e = state.users.entry(s).or_insert_with(|| UserInfo {
                        session: s,
                        name: String::new(),
                        channel_id: 0,
                        mute: false,
                        deaf: false,
                        self_mute: false,
                        self_deaf: false,
                        talking: false,
                        local_mute: false,
                    });
                    if let Some(n) = m.name {
                        e.name = n;
                    }
                    if let Some(c) = m.channel_id {
                        e.channel_id = c;
                    }
                    if let Some(v) = m.mute {
                        e.mute = v;
                    }
                    if let Some(v) = m.deaf {
                        e.deaf = v;
                    }
                    if let Some(v) = m.self_mute {
                        e.self_mute = v;
                    }
                    if let Some(v) = m.self_deaf {
                        e.self_deaf = v;
                    }
                    self.emit(SessionEvent::Users(state.user_list())).await;
                }
            }
            MessageType::UserRemove => {
                let m = mumble::UserRemove::decode(payload)?;
                state.users.remove(&m.session);
                self.emit(SessionEvent::Users(state.user_list())).await;
            }
            MessageType::TextMessage => {
                let m = mumble::TextMessage::decode(payload)?;
                let from = m
                    .actor
                    .and_then(|a| state.users.get(&a).map(|u| u.name.clone()))
                    .unwrap_or_else(|| "server".into());
                self.emit(SessionEvent::Text {
                    from,
                    message: m.message,
                })
                .await;
            }
            MessageType::Ping => {
                let m = mumble::Ping::decode(payload)?;
                if let Some(ts) = m.timestamp {
                    let rtt = now_millis().saturating_sub(ts);
                    state.stats.tcp_ping_ms = rtt as f32;
                }
            }
            MessageType::UdpTunnel => {
                // Voice arriving over TLS because UDP is unavailable.
                if let Ok(p) = VoicePacket::decode_incoming(payload) {
                    self.on_voice(p, state).await;
                }
            }
            MessageType::PermissionDenied => {
                let m = mumble::PermissionDenied::decode(payload)?;
                let reason = m.reason.unwrap_or_else(|| "permission denied".into());
                self.emit(SessionEvent::Text {
                    from: "server".into(),
                    message: reason,
                })
                .await;
            }
            MessageType::Version => {
                // Nothing to do; we already sent ours.
                let _ = writer;
            }
            _ => {}
        }
        Ok(None)
    }

    async fn handle_command(
        &mut self,
        cmd: SessionCommand,
        state: &mut LiveState,
        writer: &mut ControlWriter,
    ) -> Result<()> {
        match cmd {
            SessionCommand::JoinChannel(id) => {
                if let Some(me) = state.self_session {
                    let m = mumble::UserState {
                        session: Some(me),
                        channel_id: Some(id),
                        ..Default::default()
                    };
                    writer.send(MessageType::UserState, &m).await?;
                }
            }
            SessionCommand::SendText {
                channel_id,
                message,
            } => {
                let m = mumble::TextMessage {
                    actor: state.self_session,
                    session: Vec::new(),
                    channel_id: channel_id.into_iter().collect(),
                    tree_id: Vec::new(),
                    message,
                };
                writer.send(MessageType::TextMessage, &m).await?;
            }
            SessionCommand::SetSelfMute(v) => {
                if let Some(me) = state.self_session {
                    let m = mumble::UserState {
                        session: Some(me),
                        self_mute: Some(v),
                        ..Default::default()
                    };
                    writer.send(MessageType::UserState, &m).await?;
                }
            }
            SessionCommand::SetSelfDeaf(v) => {
                if let Some(me) = state.self_session {
                    let m = mumble::UserState {
                        session: Some(me),
                        self_deaf: Some(v),
                        ..Default::default()
                    };
                    writer.send(MessageType::UserState, &m).await?;
                }
            }
            SessionCommand::SetUserLocalMute { session, muted } => {
                // Purely client-side, so it needs no permission and no round
                // trip; report it straight back so the UI updates immediately.
                if let Some(u) = state.users.get_mut(&session) {
                    u.local_mute = muted;
                }
                self.emit(SessionEvent::Users(state.user_list())).await;
            }
            SessionCommand::SetUserServerMute { session, muted } => {
                let m = mumble::UserState {
                    session: Some(session),
                    mute: Some(muted),
                    ..Default::default()
                };
                writer.send(MessageType::UserState, &m).await?;
            }
            SessionCommand::SetUserServerDeaf { session, deaf } => {
                let m = mumble::UserState {
                    session: Some(session),
                    deaf: Some(deaf),
                    ..Default::default()
                };
                writer.send(MessageType::UserState, &m).await?;
            }
            SessionCommand::SetDefaultChannel(name) => {
                // Remembered for the next connect; the UI persists it too.
                self.config.profile.auto_join_channel = name;
            }

            // Transmission gating happens in the audio engine, not here.
            SessionCommand::SetTransmitting(_)
            | SessionCommand::Connect
            | SessionCommand::AcceptCertificate
            | SessionCommand::Disconnect
            | SessionCommand::Shutdown => {}
        }
        Ok(())
    }
}

/// Moves a [`ControlReader`] into its own task, forwarding whole messages.
///
/// `ControlReader::recv` is not cancel-safe, so it must not be a `select!`
/// branch. Channel receives are cancel-safe, so the session loop races this
/// receiver instead and a message is either fully read or not read at all.
fn spawn_reader(mut reader: ControlReader) -> mpsc::Receiver<Result<(u16, Vec<u8>)>> {
    let (tx, rx) = mpsc::channel(64);
    tokio::spawn(async move {
        loop {
            let result = reader.recv().await;
            let failed = result.is_err();
            if tx.send(result).await.is_err() || failed {
                break;
            }
        }
    });
    rx
}

/// Milliseconds since the Unix epoch, used for ping timestamps.
fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Convenience alias so callers can name the handle type.
pub type EventSender = mpsc::Sender<SessionEvent>;
pub type CommandSender = mpsc::Sender<SessionCommand>;
pub type SharedIdentity = Arc<Identity>;
