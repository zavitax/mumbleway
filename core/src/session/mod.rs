//! A single server session: connect, authenticate, stay alive, reconnect.

pub mod manager;
pub mod peers;
pub mod permissions;
pub mod profile;
pub mod quality;
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
    /// The app's own version, as other MumbleWay clients are told it.
    pub app_version: String,
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
    /// Other sessions that have identified themselves as MumbleWay.
    peers: peers::Peers,
    /// Whether this server relays `PluginDataTransmission`, and so whether the
    /// handshake can run at all. Set from the server's `Version`, and set again
    /// by receiving one — receipt is proof, whatever the version said.
    plugin_data: bool,
    /// Our own version, for our own roster entry.
    own_version: String,
    /// When to announce ourselves, and when to stop.
    announcer: peers::Announcer,
    /// The server's measurements of each rider's connection, by session.
    quality: HashMap<u32, quality::Quality>,
    /// Set by a stats reply, cleared when the roster carrying it goes out. One
    /// emit per round of replies rather than one per reply: the roster is the
    /// whole list, and sending it eight times in a second to move eight
    /// indicators would rebuild the interface eight times.
    quality_fresh: bool,
    /// Whose turn it is to be asked about, and when.
    quality_poller: quality::QualityPoller,
    /// What the server says this rider may do, per channel — the root's mask
    /// and the current channel's are the two that matter. See [`permissions`].
    perms: HashMap<u32, u32>,
    /// The last rights reported upward, so an unchanged answer is not sent
    /// again: the server re-sends its cached mask freely.
    rights_sent: Option<permissions::Rights>,
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
            peers: peers::Peers::default(),
            plugin_data: false,
            own_version: String::new(),
            announcer: peers::Announcer::armed(Instant::now()),
            quality: HashMap::new(),
            quality_fresh: false,
            quality_poller: quality::QualityPoller::default(),
            perms: HashMap::new(),
            rights_sent: None,
        }
    }

    /// What the rider may do, from the two masks that decide it.
    fn rights(&self) -> permissions::Rights {
        permissions::Rights::from_masks(
            self.perms.get(&permissions::ROOT_CHANNEL).copied(),
            self.self_channel(self.self_session)
                .and_then(|c| self.perms.get(&c).copied()),
        )
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

    /// Everybody in our own channel but us.
    ///
    /// The roster draws these, and they are also the only riders the server
    /// reports packet loss for — see [`quality`].
    fn channel_peers(&self) -> Vec<u32> {
        let Some(mine) = self.self_channel(self.self_session) else {
            return Vec::new();
        };
        self.users
            .values()
            .filter(|u| u.channel_id == mine && Some(u.session) != self.self_session)
            .map(|u| u.session)
            .collect()
    }

    fn user_list(&self) -> Vec<UserInfo> {
        let mut v: Vec<_> = self
            .users
            .values()
            .cloned()
            .map(|mut u| {
                u.mumbleway = if Some(u.session) == self.self_session {
                    // **Ourselves only where the handshake can run.** Badging
                    // our own row on a server too old to relay it would make
                    // everybody else's missing badge read as "nobody else runs
                    // MumbleWay", when the truth is that nobody can tell.
                    self.plugin_data.then(|| self.own_version.clone())
                } else {
                    self.peers.get(u.session).map(|p| p.version.clone())
                };
                u.quality = self.quality.get(&u.session).copied();
                u
            })
            .collect();
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

    /// Reports what the rider may do, when it has changed.
    ///
    /// The server re-sends a channel's mask freely — on every entry, and from
    /// its cache — and an unchanged answer is not news the interface needs.
    async fn push_rights(&self, state: &mut LiveState) {
        let now = state.rights();
        if state.rights_sent != Some(now) {
            state.rights_sent = Some(now);
            self.emit(SessionEvent::Rights(now)).await;
        }
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
            // The whole connection story passes through this loop, so logging
            // it here covers every attempt and every outcome without scattering
            // lines through the transport. Never the password: this log is
            // shown in the app and meant to be quoted back to us.
            tracing::info!(
                "connecting to {}:{}",
                self.config.profile.host,
                self.config.profile.port
            );

            let outcome = self.connect_and_run().await;
            let reason = match outcome {
                Ok(r) => r,
                Err(e) => Self::classify(e),
            };

            match self.reconnect.on_disconnect(&reason) {
                None => {
                    if matches!(reason, DisconnectReason::UserRequested) {
                        tracing::info!("disconnected by request");
                    } else {
                        tracing::error!("giving up: {reason}");
                    }
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
                    tracing::warn!(
                        "lost connection: {reason} — retry {} in {} ms",
                        self.reconnect.attempt(),
                        delay.as_millis()
                    );
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
            // No reject type reached us here, and both of these mean the
            // server declined this client rather than that it was busy.
            CoreError::Rejected(m) | CoreError::Auth(m) => DisconnectReason::ServerRejected {
                reason: m,
                retry: false,
            },
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
        state.own_version = self.config.app_version.clone();

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

        // The announcement is not sent here. It goes out on the first health
        // tick, from the same code that repeats it — see `run_connected`. That
        // tick fires immediately, so it costs nothing, and it leaves one path
        // rather than a first hello here and its repeats elsewhere.

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

        // Ask what this rider may do here, before they reach for anything that
        // needs it. A server that volunteers the answer costs one extra
        // message; one that does not would otherwise leave every moderation
        // action to be discovered by refusal.
        ask_rights_for(&mut writer, state.self_channel(state.self_session)).await?;

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
                                tracing::info!("voice now direct over UDP");
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
                                tracing::warn!("UDP socket failed, tunnelling voice over TCP");
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
                        tracing::warn!(
                            "no reply from the server for {} s, treating the link as dead",
                            now.duration_since(state.last_heard).as_secs()
                        );
                        return Ok(DisconnectReason::PingTimeout);
                    }
                    // Fall back to the tunnel if UDP goes quiet mid-session.
                    if state.transport == Transport::Udp
                        && !udp.as_ref().map(|s| s.is_healthy(now)).unwrap_or(false)
                    {
                        state.transport = Transport::TcpTunnel;
                        // Worth a line of its own: voice keeps working, so
                        // nothing else announces it, yet it changes latency and
                        // is the usual reason a link "sounds worse" for no
                        // visible cause.
                        tracing::warn!("UDP went quiet, tunnelling voice over TCP");
                        self.emit(SessionEvent::TransportChanged(Transport::TcpTunnel)).await;
                    }
                    if let Some(at) = state.connected_at {
                        if now.duration_since(at) > reconnect::HEALTHY_RESET_AFTER {
                            self.reconnect.note_healthy();
                        }
                    }
                    state.stats.transport = Some(state.transport.into());
                    self.emit(SessionEvent::Stats(state.stats)).await;

                    // Ask the server how everybody in this channel is doing,
                    // and put the answers from the last round in front of the
                    // rider. Both here rather than on each reply, so a busy
                    // channel costs one roster emit every few seconds.
                    if state.quality_poller.due(now) {
                        for session in state.quality_poller.next_batch(&state.channel_peers()) {
                            let m = mumble::UserStats {
                                session: Some(session),
                                // No certificate chain: it is the one part of
                                // the reply this never uses, it is by far the
                                // largest, and asking for less is the polite
                                // way to poll something every few seconds.
                                stats_only: Some(true),
                                ..Default::default()
                            };
                            writer.send(MessageType::UserStats, &m).await?;
                        }
                    }
                    if state.quality_fresh {
                        state.quality_fresh = false;
                        self.emit(SessionEvent::Users(state.user_list())).await;
                    }

                    // Answer every announcement heard since the last tick, all
                    // in one message. Once a second at most, however many
                    // arrived — which is what keeps a crowd reconnecting after
                    // a server restart under the server's burst limit.
                    if state.plugin_data {
                        let owed: Vec<u32> = state
                            .peers
                            .take_owed()
                            .into_iter()
                            .filter(|s| state.users.contains_key(s))
                            .collect();
                        send_hello(writer, &state.own_version, owed, true).await?;

                        // Say who we are, and say it again to anybody who has
                        // not answered — the server drops a plugin message
                        // over its rate limit without telling the sender, and
                        // that is the one way a hello is lost quietly.
                        if state.announcer.is_due(now) {
                            let present: Vec<u32> = state.users.keys().copied().collect();
                            let unheard = state.peers.unheard(&present, state.self_session);
                            if unheard.is_empty() {
                                // Everybody here has answered, or there is
                                // nobody else here at all.
                                state.announcer.stop();
                            } else {
                                send_hello(writer, &state.own_version, unheard, false).await?;
                                state.announcer.sent(now);
                            }
                        }
                    }
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
            // Our own voice, arriving back from the server, is dropped here.
            //
            // A Mumble server does not normally do this and this client never
            // asks it to — `TARGET_LOOPBACK` is never set on anything we send.
            // But the protocol has it, a relay or a mirrored channel has the
            // same effect, and so does the same certificate logged in twice.
            //
            // The reason to check rather than trust: this arrives as ordinary
            // speech from another session and is played out the speaker, where
            // it is *indistinguishable from acoustic echo* to everyone
            // downstream — except that the echo canceller cannot remove it at
            // any filter length, because it never entered the reference as
            // something we chose to play. It would be diagnosed as a broken
            // canceller for as long as it took somebody to think of this.
            if Some(session) == state.self_session {
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
        crate::net::stats::note_voice_in();
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

                // Only three of these change on their own. A full server
                // empties, an authenticator that fell over comes back, and a
                // server closed to new connections opens again — those are
                // worth waiting out. Everything else is a statement about this
                // client that will be just as true in ten seconds: the wrong
                // password, a name already taken, a version too old, no
                // certificate. Reconnecting through those achieves nothing
                // except to bury the explanation.
                //
                // An unrecognised or absent type counts as permanent. A
                // rejection nobody can classify is not evidence that trying
                // again will help, and stopping puts the server's own words in
                // front of the user.
                use mumble::reject::RejectType;
                let retry = matches!(
                    m.r#type.and_then(|t| RejectType::try_from(t).ok()),
                    Some(
                        RejectType::ServerFull
                            | RejectType::AuthenticatorFail
                            | RejectType::NoNewConnections
                    )
                );
                return Ok(Some(DisconnectReason::ServerRejected { reason, retry }));
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

                // Note what changed about *us*, and who did it, before the
                // roster is updated. An actor other than ourselves means this
                // was done to the user, which warrants an audible cue.
                let moderation = m
                    .session
                    .filter(|s| Some(*s) == state.self_session)
                    .and_then(|s| {
                        let actor = m.actor.filter(|a| Some(*a) != state.self_session)?;
                        let prev = state.users.get(&s);
                        let muted = m.mute.filter(|v| prev.map(|p| p.mute) != Some(*v));
                        let deafened = m.deaf.filter(|v| prev.map(|p| p.deaf) != Some(*v));
                        if muted.is_none() && deafened.is_none() {
                            return None;
                        }
                        Some((actor, muted, deafened))
                    });

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
                        mumbleway: None,
                        // Both of these live outside the roster map and are
                        // filled in by `user_list`; the copy kept here is
                        // always `None`.
                        quality: None,
                    });
                    if let Some(n) = m.name {
                        e.name = n;
                    }
                    if let Some(c) = m.channel_id {
                        let moved_ourselves = Some(s) == state.self_session && e.channel_id != c;
                        e.channel_id = c;
                        // Permissions are per channel, so moving invalidates
                        // the half of the answer that was about *here*. Asked
                        // on every move rather than only for a channel never
                        // seen: an ACL may have changed since we last stood in
                        // it, and one message is nothing.
                        if moved_ourselves {
                            ask_permissions(writer, c).await?;
                        }
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

                if let Some((actor, muted, deafened)) = moderation {
                    let by = state
                        .users
                        .get(&actor)
                        .map(|u| u.name.clone())
                        .unwrap_or_else(|| "an admin".into());
                    self.emit(SessionEvent::SelfModerated {
                        muted,
                        deafened,
                        by,
                    })
                    .await;
                }
            }
            MessageType::PermissionQuery => {
                let m = mumble::PermissionQuery::decode(payload)?;
                // The server says "forget everything I told you" when an ACL
                // changes, because what it told us may now be wrong for any
                // channel, not only the one that changed.
                let flushed = m.flush.unwrap_or(false);
                if flushed {
                    state.perms.clear();
                }
                if let (Some(channel), Some(bits)) = (m.channel_id, m.permissions) {
                    state.perms.insert(channel, bits);
                }
                if flushed {
                    // Being told to forget leaves the rider with no answer at
                    // all, so ask again rather than wait for a move that may
                    // never come.
                    ask_rights_for(writer, state.self_channel(state.self_session)).await?;
                }
                self.push_rights(state).await;
            }
            MessageType::UserStats => {
                let m = mumble::UserStats::decode(payload)?;
                if let Some(session) = m.session {
                    // Rolling first: a minute of loss is what "how is this
                    // connection now" means, and the count since they connected
                    // keeps reporting a bad bridge an hour after it.
                    let rolling = m.rolling_stats.as_ref();
                    let window = rolling.and_then(|r| r.time_window).unwrap_or(0);
                    let counts = |s: &mumble::user_stats::Stats| quality::Counts {
                        good: s.good.unwrap_or(0),
                        late: s.late.unwrap_or(0),
                        lost: s.lost.unwrap_or(0),
                    };
                    let up = rolling
                        .and_then(|r| r.from_client.as_ref())
                        .or(m.from_client.as_ref())
                        .map(counts);
                    let down = rolling
                        .and_then(|r| r.from_server.as_ref())
                        .or(m.from_server.as_ref())
                        .map(counts);
                    state.quality.insert(
                        session,
                        quality::Quality::from_stats(
                            session,
                            m.udp_ping_avg,
                            m.tcp_ping_avg,
                            up,
                            down,
                            window,
                            m.idlesecs.unwrap_or(0),
                        ),
                    );
                    state.quality_fresh = true;
                }
            }
            MessageType::UserRemove => {
                let m = mumble::UserRemove::decode(payload)?;
                state.users.remove(&m.session);
                state.quality.remove(&m.session);
                // The number goes back to the server and may be handed to
                // somebody who runs something else entirely.
                state.peers.forget(m.session);
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

                // A refusal of the Mute permission, about somebody we have just
                // sent a mute request to, is the expected half of a pair: the
                // request went through, and saying "the server refused" about
                // a mute that is happening would be the screen lying. Only that
                // exact refusal, only for that person, only for a few seconds.
                const DENY_PERMISSION: i32 = 1;
                const ACL_MUTE_DEAFEN: u32 = 0x10;
                let covered = m.r#type == Some(DENY_PERMISSION)
                    && m.permission == Some(ACL_MUTE_DEAFEN)
                    && m.session
                        .is_some_and(|s| state.peers.backup_covers(s, Instant::now()));
                if covered {
                    return Ok(None);
                }

                // Passed through as the server wrote it, empty included. A
                // placeholder invented here would be English text the UI could
                // not tell from the server's own and could not translate, and
                // it would hide the type — which is the only thing most servers
                // actually send.
                self.emit(SessionEvent::Refused {
                    reason: m.reason.unwrap_or_default(),
                    kind: m.r#type.unwrap_or(0) as u32,
                })
                .await;
            }
            MessageType::Version => {
                // Ours is already sent. Theirs decides whether the server relays
                // the MumbleWay handshake, which arrived in 1.4.0.
                if let Ok(m) = mumble::Version::decode(payload) {
                    state.plugin_data |=
                        peers::server_supports_plugin_data(m.version_v1, m.version_v2);
                }
                // No arm here sends anything, but the writer stays in the
                // signature for the ones that will; this is what keeps it.
                let _ = writer;
            }
            MessageType::PluginDataTransmission => {
                // **Written by another client, not by the server.** Every other
                // arm here may treat a message it cannot decode as a broken
                // connection; nothing another rider sends should be able to cost
                // us ours, so this one drops what it cannot read and carries on.
                let Ok(m) = mumble::PluginDataTransmission::decode(payload) else {
                    return Ok(None);
                };
                // Whatever the version said, a relayed message proves relaying.
                state.plugin_data = true;
                // The server stamps the sender; the client cannot choose it.
                let (Some(sender), Some(id)) = (m.sender_session, m.data_id.as_deref()) else {
                    return Ok(None);
                };
                if Some(sender) == state.self_session {
                    return Ok(None);
                }
                if id == peers::DATA_ID_HELLO {
                    if let Some(hello) = m.data.as_deref().and_then(peers::decode_hello) {
                        state.peers.on_hello(sender, hello);
                        self.emit(SessionEvent::Users(state.user_list())).await;
                    }
                } else if id == peers::DATA_ID_MUTE {
                    // Only from somebody who has said they run MumbleWay, so
                    // there is a name to tell the rider. Whether to act is not
                    // decided here: the rider's setting, their current state
                    // and the cooldown all live in the app, and one guard has
                    // to hold across every server they are on.
                    if !state.peers.is_mumbleway(sender) {
                        return Ok(None);
                    }
                    if let Some(mute) = m.data.as_deref().and_then(peers::decode_mute_request) {
                        let by = state
                            .users
                            .get(&sender)
                            .map(|u| u.name.clone())
                            .unwrap_or_default();
                        self.emit(SessionEvent::RemoteMuteRequested { mute, by })
                            .await;
                    }
                }
                // Other `mumbleway/` IDs are reserved for the exchanges this
                // handshake exists to enable, and nothing under any other
                // prefix is ours to read.
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
            SessionCommand::RegisterSelf => {
                if let Some(me) = state.self_session {
                    // **`user_id: 0` is the request, not an id.** Mumble has no
                    // "register me" message: a `UserState` naming yourself with
                    // a user id of zero is how the client asks, and the server
                    // answers by sending back a `UserState` carrying the real
                    // id it assigned. Any other value would be an attempt to
                    // set somebody's id, which is not what this is.
                    let m = mumble::UserState {
                        session: Some(me),
                        user_id: Some(0),
                        ..Default::default()
                    };
                    writer.send(MessageType::UserState, &m).await?;
                }
            }
            SessionCommand::SetUserServerMute { session, muted } => {
                let m = mumble::UserState {
                    session: Some(session),
                    mute: Some(muted),
                    ..Default::default()
                };
                writer.send(MessageType::UserState, &m).await?;

                // And, to a MumbleWay rider who will act on it, the same thing
                // as a request. The server mute is the binding one and needs a
                // permission most riders lack; this needs none, and it is also
                // the only thing that can reach a *self*-mute, which no server
                // command may touch. See `peers::DATA_ID_MUTE`.
                if state.plugin_data && state.peers.accepts_remote_mute(session) {
                    let m = mumble::PluginDataTransmission {
                        sender_session: None,
                        receiver_sessions: vec![session],
                        data: Some(peers::encode_mute_request(muted).into()),
                        data_id: Some(peers::DATA_ID_MUTE.to_string()),
                    };
                    writer.send(MessageType::PluginDataTransmission, &m).await?;
                    state.peers.note_backup(session, Instant::now());
                }
            }
            SessionCommand::SetUserServerDeaf { session, deaf } => {
                let m = mumble::UserState {
                    session: Some(session),
                    deaf: Some(deaf),
                    ..Default::default()
                };
                writer.send(MessageType::UserState, &m).await?;
            }
            SessionCommand::KickUser { session, reason } => {
                // UserRemove without `ban` is a kick: the server drops them and
                // tells every client, and they may reconnect straight away.
                let m = mumble::UserRemove {
                    session,
                    actor: state.self_session,
                    reason: Some(reason),
                    ban: Some(false),
                    ban_certificate: None,
                    ban_ip: None,
                };
                writer.send(MessageType::UserRemove, &m).await?;
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
/// Sends one hello to every session in `receivers`, as a single message.
///
/// One message whatever the count, because the server's rate limit is charged
/// per message rather than per recipient. The sender is left unset: the server
/// fills it in and would overwrite anything put here.
/// Asks what this rider may do in one channel.
async fn ask_permissions(writer: &mut ControlWriter, channel: u32) -> Result<()> {
    let m = mumble::PermissionQuery {
        channel_id: Some(channel),
        ..Default::default()
    };
    writer.send(MessageType::PermissionQuery, &m).await
}

/// Asks about both channels that decide what the rider may do: the root, which
/// carries kicking, banning and registration, and the one they are standing in.
async fn ask_rights_for(writer: &mut ControlWriter, here: Option<u32>) -> Result<()> {
    ask_permissions(writer, permissions::ROOT_CHANNEL).await?;
    match here {
        Some(c) if c != permissions::ROOT_CHANNEL => ask_permissions(writer, c).await,
        _ => Ok(()),
    }
}

async fn send_hello(
    writer: &mut ControlWriter,
    version: &str,
    receivers: Vec<u32>,
    reply: bool,
) -> Result<()> {
    if receivers.is_empty() {
        return Ok(());
    }
    let m = mumble::PluginDataTransmission {
        sender_session: None,
        receiver_sessions: receivers,
        data: Some(peers::encode_hello(version, peers::CAPABILITIES, reply).into()),
        data_id: Some(peers::DATA_ID_HELLO.to_string()),
    };
    writer.send(MessageType::PluginDataTransmission, &m).await
}

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
