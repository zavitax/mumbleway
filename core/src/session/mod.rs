//! A single server session: connect, authenticate, stay alive, reconnect.

pub mod bans;
pub mod context_actions;
pub mod manager;
pub mod notes;
pub mod peers;
pub mod permissions;
pub mod pings;
pub mod profile;
pub mod quality;
pub mod reconnect;
pub mod types;

pub use reconnect::{BackoffPolicy, ReconnectState};
pub use types::*;

use std::collections::{HashMap, HashSet};
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
    /// A decrypt nonce from the server, waiting to be applied to whichever half
    /// of this session is holding the cipher. See the `CryptSetup` arm.
    pending_decrypt_iv: Option<Vec<u8>>,
    /// Which comments and pictures we hold, and which still have to be asked
    /// for. See [`notes`].
    blobs: notes::Blobs,
    /// Channels this client is listening to without having joined them.
    listening: Vec<u32>,
    /// Menu entries this server has registered. Per connection: they are the
    /// server's, and a different server has its own.
    context_actions: context_actions::ContextActions,
    /// The bandwidth allowance this server gave, in bits per second.
    max_bandwidth: Option<u32>,
    /// Round trips this client has measured, which the server stores and hands
    /// to anybody asking about this rider. See [`pings`].
    tcp_ping: pings::PingStats,
    udp_ping: pings::PingStats,
    /// Voice packets received, by the route they arrived on. Reported for the
    /// same reason and in the same message.
    udp_packets: u32,
    tcp_packets: u32,
    /// The last suppression state reported upward, so the announcement is
    /// made on a change rather than on every roster update.
    suppress_announced: Option<bool>,
    /// Riders somebody has asked to see the details of.
    ///
    /// **Stats replies are not labelled.** The quality poll and an explicit
    /// request come back as the same `UserStats` message, and a server tells
    /// only an admin the client version, the address and the certificate — so
    /// a reply to an ordinary rider's question carries none of the fields that
    /// used to be the signal to report it. The question therefore has to be
    /// remembered: without this, the dialog asked, the answer arrived, and it
    /// sat on "Asking the server…" for ever.
    details_wanted: HashSet<u32>,
    /// A channel whose access list should be read again, and whether the ban
    /// list should be.
    ///
    /// **Not read back in the same breath as the write.** A server rate-limits
    /// these — Murmur's `msgACL` opens its write path with `RATELIMIT` — and a
    /// write followed immediately by a read puts two messages into a bucket
    /// that may only have room for one. The read is the one that gets dropped,
    /// and the screen then shows the state before the change, which reads as
    /// the change having failed. A tick later there is room.
    reread_acl: Option<u32>,
    reread_bans: bool,
    /// Longest text message and longest image this server will take, in
    /// bytes; `0` from the server means "no limit". Exceeding either is
    /// refused with `TextTooLong` — for a picture too, which is the server's
    /// own confusion and not this client's.
    limits: ServerLimits,
    /// Whether this server says Mumble's recording feature is allowed.
    ///
    /// Recorded rather than enforced: this app's diagnostic recording is its
    /// own feature and not the protocol's, and what to do about a server that
    /// says no is a decision, not a default. Carried so that decision can be
    /// made from a fact instead of a guess.
    recording_allowed: Option<bool>,
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
            pending_decrypt_iv: None,
            blobs: notes::Blobs::default(),
            listening: Vec::new(),
            context_actions: context_actions::ContextActions::default(),
            max_bandwidth: None,
            tcp_ping: pings::PingStats::default(),
            udp_ping: pings::PingStats::default(),
            udp_packets: 0,
            tcp_packets: 0,
            recording_allowed: None,
            reread_acl: None,
            reread_bans: false,
            limits: ServerLimits::default(),
            suppress_announced: None,
            details_wanted: HashSet::new(),
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
                u.muted_you = self.peers.has_muted_us(u.session);
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

    /// Announces our own voice being silenced by the channel, or allowed again.
    ///
    /// **Read from the roster rather than from the message that changed it.**
    /// A server sends our own `UserState` *before* the `ServerSync` that says
    /// which session we are, so a rider who joins straight into a channel they
    /// may not speak in is suppressed before this client knows who it is —
    /// which is exactly the case a live server produced, with the roster
    /// correct and nothing announced.
    async fn note_suppress(&self, state: &mut LiveState) {
        let Some(now) = state
            .self_session
            .and_then(|s| state.users.get(&s))
            .map(|u| u.suppress)
        else {
            return;
        };
        if state.suppress_announced != Some(now) {
            state.suppress_announced = Some(now);
            self.emit(SessionEvent::SelfSuppressed(now)).await;
        }
    }

    /// Records a bandwidth allowance and reports it upward when it changes.
    ///
    /// Reported rather than acted on here: this session knows its own server's
    /// figure, and the encoder is shared by all of them, so the *smallest*
    /// allowance is the one that decides — which only the layer holding every
    /// session can work out. See `audio::bandwidth::tightest`.
    async fn note_bandwidth(&self, state: &mut LiveState, bps: u32) {
        if state.max_bandwidth == Some(bps) {
            return;
        }
        state.max_bandwidth = Some(bps);
        self.emit(SessionEvent::BandwidthCap(bps)).await;
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
            // Presented at the handshake, which is the only moment they can
            // open a channel this client is set to join automatically.
            tokens: self.config.profile.access_tokens.clone(),
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
                                                // Back up: the next outage starts from the quick attempts again,
                                                // whatever this one cost. Reset here rather than after a spell of
                                                // health — see `reconnect`.
        self.reconnect.note_healthy();
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
                    // Applied here because this is where the cipher is: the UDP
                    // socket owns it while one exists, and `state.crypt` only
                    // before the socket is built.
                    if let Some(iv) = state.pending_decrypt_iv.take() {
                        let applied = match udp.as_mut() {
                            Some(s) => s.set_decrypt_iv(&iv),
                            None => match state.crypt.as_mut() {
                                Some(c) => c.set_decrypt_iv(&iv),
                                None => Ok(()),
                            },
                        };
                        if let Err(e) = applied {
                            tracing::warn!("server sent a resync nonce we could not use: {e}");
                        }
                    }
                }

                // --- UDP voice ---------------------------------------------
                ev = udp_recv => {
                    match ev {
                        Ok(Some(UdpEvent::Voice(p))) => {
                            state.udp_packets = state.udp_packets.saturating_add(1);
                            self.on_voice(p, state).await;
                        }
                        Ok(Some(UdpEvent::Pong { rtt })) => {
                            state.stats.udp_ping_ms = rtt.as_secs_f32() * 1000.0;
                            state.udp_ping.record(state.stats.udp_ping_ms);
                            if state.transport != Transport::Udp {
                                state.transport = Transport::Udp;
                                tracing::info!("voice now direct over UDP");
                                self.emit(SessionEvent::TransportChanged(Transport::Udp)).await;
                            }
                        }
                        Ok(Some(UdpEvent::ResyncNeeded)) => {
                            // An empty CryptSetup means "tell me your nonce".
                            // Until this existed, a cipher that lost step
                            // stayed lost: every packet failed, the link looked
                            // healthy, and voice simply stopped arriving.
                            tracing::warn!("voice decryption out of step, asking the server to resync");
                            let m = mumble::CryptSetup::default();
                            writer.send(MessageType::CryptSetup, &m).await?;
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
                    // Everything the protocol asks for, not only the crypt
                    // counters. **The server does not measure a client's ping;
                    // it copies these fields** and hands them to whoever asks
                    // about this rider, so leaving them out made every
                    // MumbleWay rider read as 0 ms to everybody — in this
                    // app's own roster and in the official client alike.
                    let ping = mumble::Ping {
                        timestamp: Some(now_millis()),
                        good: Some(stats.good),
                        late: Some(stats.late),
                        lost: Some(stats.lost),
                        resync: Some(stats.resync),
                        udp_packets: Some(state.udp_packets),
                        tcp_packets: Some(state.tcp_packets),
                        udp_ping_avg: Some(state.udp_ping.mean()),
                        udp_ping_var: Some(state.udp_ping.variance()),
                        tcp_ping_avg: Some(state.tcp_ping.mean()),
                        tcp_ping_var: Some(state.tcp_ping.variance()),
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

                    // Anything written a moment ago, read back now that the
                    // server's rate limiter has had a tick to refill.
                    if let Some(channel) = state.reread_acl.take() {
                        let m = mumble::Acl {
                            channel_id: channel,
                            query: Some(true),
                            ..Default::default()
                        };
                        writer.send(MessageType::Acl, &m).await?;
                    }
                    if std::mem::take(&mut state.reread_bans) {
                        let m = mumble::BanList {
                            bans: Vec::new(),
                            query: Some(true),
                        };
                        writer.send(MessageType::BanList, &m).await?;
                    }

                    // Ask for the comments and pictures we have only hashes
                    // for. Here rather than on arrival of each hash, because a
                    // channel filling after a server restart would otherwise be
                    // one request per rider in the same instant.
                    let (comments, textures, channels) = state.blobs.next_request();
                    if !comments.is_empty() || !textures.is_empty() || !channels.is_empty() {
                        let m = mumble::RequestBlob {
                            session_comment: comments,
                            session_texture: textures,
                            channel_description: channels,
                        };
                        writer.send(MessageType::RequestBlob, &m).await?;
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
                        // A resync of the decrypt IV alone: either the answer
                        // to our own request, or the server volunteering one.
                        //
                        // **Kept for the caller to apply**, because once the
                        // session is connected the cipher lives inside the UDP
                        // socket rather than here — applying it to
                        // `state.crypt` would land on a `None` and the resync
                        // would silently do nothing, which is exactly how this
                        // path used to fail.
                        state.pending_decrypt_iv = Some(sn.to_vec());
                    }
                    _ => {}
                }
            }
            MessageType::ServerConfig => {
                let m = mumble::ServerConfig::decode(payload)?;
                // The allowance can change mid-session: an admin edits it and
                // the server says so again, with no other warning.
                if let Some(bps) = m.max_bandwidth {
                    self.note_bandwidth(state, bps).await;
                }
                let limits = ServerLimits {
                    message_length: m.message_length.unwrap_or(0),
                    image_message_length: m.image_message_length.unwrap_or(0),
                };
                if limits != state.limits {
                    state.limits = limits;
                    self.emit(SessionEvent::Limits(limits)).await;
                }
                if let Some(allowed) = m.recording_allowed {
                    if state.recording_allowed != Some(allowed) {
                        state.recording_allowed = Some(allowed);
                        // Logged and nothing more, for now. This app's
                        // diagnostic recorder is its own feature rather than
                        // Mumble's, so what to do about a server that says no
                        // is a decision to take deliberately — but it should be
                        // taken from a fact, and this is where the fact lands.
                        tracing::info!(
                            "server says recording is {}allowed",
                            if allowed { "" } else { "not " }
                        );
                    }
                }
            }
            MessageType::ServerSync => {
                let m = mumble::ServerSync::decode(payload)?;
                // The first place the allowance arrives, at the end of the
                // handshake — before a single packet has been sent.
                if let Some(bps) = m.max_bandwidth {
                    self.note_bandwidth(state, bps).await;
                }
                if let Some(s) = m.session {
                    state.self_session = Some(s);
                    self.emit(SessionEvent::SelfSession(s)).await;
                    // Now that we know which rider is us, the flags that
                    // arrived before this can be read.
                    self.note_suppress(state).await;
                }
                // **The root permissions are in this message too.** Kicking,
                // banning and registration are all read from the root mask, and
                // this arrives before any query could be answered — so the
                // interface knows what it may offer from the first moment
                // rather than a round trip later. The query still goes out: a
                // server that omits this field leaves us with nothing, and one
                // that changes an ACL later sends the update unprompted.
                //
                // **Reported here, not merely stored.** It was being put in the
                // map and left there: nothing emitted until the query came
                // back, so the round trip this field exists to save was still
                // being waited on. After the session id, because what a rider
                // may do *here* is read from the channel they are standing in,
                // and that is only known once we know which rider is us.
                if let Some(perms) = m.permissions {
                    state.perms.insert(permissions::ROOT_CHANNEL, perms as u32);
                    self.push_rights(state).await;
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
                        e.description = notes::strip_html(&d);
                        state.blobs.got_channel(id);
                    }
                    // Long ones arrive as a hash, exactly like a rider's
                    // comment, and are fetched the same way.
                    if let Some(h) = m.description_hash.as_ref() {
                        if h.is_empty() {
                            e.description.clear();
                        }
                        state.blobs.note_channel_hash(id, h);
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
                state.blobs.forget_channel(m.channel_id);
                if state.listening.contains(&m.channel_id) {
                    state.listening.retain(|c| *c != m.channel_id);
                    self.emit(SessionEvent::Listening(state.listening.clone()))
                        .await;
                }
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

                // Occupancy is derived from the roster rather than sent by the
                // server, so the channel list has to go out again whenever
                // somebody arrives or moves. Without this the counts beside the
                // channels keep whatever they said when a *channel* last
                // changed, which on a quiet server is for ever.
                let occupancy_changed = m.session.is_some_and(|s| match state.users.get(&s) {
                    None => true,
                    Some(prev) => m.channel_id.is_some_and(|c| prev.channel_id != c),
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
                        suppress: false,
                        mumbleway: None,
                        // These three live outside the roster map and are
                        // filled in by `user_list`; the copy kept here is
                        // always the empty one.
                        quality: None,
                        muted_you: false,
                        comment: String::new(),
                        priority_speaker: false,
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
                    if let Some(v) = m.priority_speaker {
                        e.priority_speaker = v;
                    }
                    if let Some(v) = m.suppress {
                        e.suppress = v;
                    }
                    // Listening is reported as two lists of changes rather
                    // than as a state, so the set is kept here and the server's
                    // word is what it holds — a refused listen simply never
                    // arrives, and the set stays as it was.
                    if Some(s) == state.self_session
                        && !(m.listening_channel_add.is_empty()
                            && m.listening_channel_remove.is_empty())
                    {
                        for c in &m.listening_channel_add {
                            if !state.listening.contains(c) {
                                state.listening.push(*c);
                            }
                        }
                        state
                            .listening
                            .retain(|c| !m.listening_channel_remove.contains(c));
                        self.emit(SessionEvent::Listening(state.listening.clone()))
                            .await;
                    }
                    // A comment arrives whole when it is short and as a hash
                    // when it is not; the body is then asked for once per hash.
                    if let Some(c) = m.comment {
                        e.comment = notes::strip_html(&c);
                        state.blobs.got_comment(s);
                    }
                    if let Some(h) = m.comment_hash.as_ref() {
                        if h.is_empty() {
                            e.comment.clear();
                        }
                        state.blobs.note_comment_hash(s, h);
                    }
                    if let Some(t) = m.texture.as_ref() {
                        state.blobs.got_texture(s);
                        // Pictures go out on their own rather than riding the
                        // roster: a roster emit is frequent and small, and a
                        // channel of riders with pictures is megabytes.
                        self.emit(SessionEvent::Avatar {
                            session: s,
                            image: t.to_vec(),
                        })
                        .await;
                    }
                    if let Some(h) = m.texture_hash.as_ref() {
                        if h.is_empty() {
                            self.emit(SessionEvent::Avatar {
                                session: s,
                                image: Vec::new(),
                            })
                            .await;
                        }
                        state.blobs.note_texture_hash(s, h);
                    }
                    self.emit(SessionEvent::Users(state.user_list())).await;
                    if occupancy_changed {
                        self.emit(SessionEvent::Channels(state.channel_list()))
                            .await;
                    }
                    // After the roster, so a change to our own entry — moving
                    // into a channel we may not speak in, or out of one — is
                    // announced from the state rather than from this message.
                    self.note_suppress(state).await;
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
            MessageType::ContextActionModify => {
                let m = mumble::ContextActionModify::decode(payload)?;
                // The operation field postdates Mumble 1.2.4; absent means add,
                // which is what a server old enough to omit it meant.
                let removing =
                    m.operation == Some(mumble::context_action_modify::Operation::Remove as i32);
                let changed = if removing {
                    state.context_actions.remove(&m.action)
                } else {
                    state.context_actions.add(
                        &m.action,
                        m.text.as_deref().unwrap_or_default(),
                        m.context.unwrap_or(0),
                    )
                };
                if changed {
                    self.emit(SessionEvent::ContextActions(state.context_actions.all()))
                        .await;
                }
            }
            MessageType::Acl => {
                let m = mumble::Acl::decode(payload)?;
                self.emit(SessionEvent::Acl(ChannelAcl {
                    channel_id: m.channel_id,
                    inherit_acls: m.inherit_acls.unwrap_or(true),
                    groups: m
                        .groups
                        .into_iter()
                        .map(|g| AclGroup {
                            name: g.name,
                            inherited: g.inherited.unwrap_or(true),
                            inherit: g.inherit.unwrap_or(true),
                            inheritable: g.inheritable.unwrap_or(true),
                            add: g.add,
                            remove: g.remove,
                            inherited_members: g.inherited_members,
                        })
                        .collect(),
                    rules: m
                        .acls
                        .into_iter()
                        .map(|r| AclRule {
                            apply_here: r.apply_here.unwrap_or(true),
                            apply_subs: r.apply_subs.unwrap_or(true),
                            inherited: r.inherited.unwrap_or(true),
                            user_id: r.user_id,
                            group: r.group,
                            grant: r.grant.unwrap_or(0),
                            deny: r.deny.unwrap_or(0),
                        })
                        .collect(),
                }))
                .await;
            }
            MessageType::QueryUsers => {
                let m = mumble::QueryUsers::decode(payload)?;
                // Ids and names arrive as two lists in the same order; a
                // mismatched pair is a server being odd, and zipping them
                // keeps the shorter one rather than inventing a name.
                self.emit(SessionEvent::UserNames(
                    m.ids.into_iter().zip(m.names).collect(),
                ))
                .await;
            }
            MessageType::UserList => {
                let m = mumble::UserList::decode(payload)?;
                self.emit(SessionEvent::Registered(
                    m.users
                        .into_iter()
                        .map(|u| RegisteredUser {
                            user_id: u.user_id,
                            name: u.name.unwrap_or_default(),
                            last_seen: u.last_seen.unwrap_or_default(),
                            last_channel: u.last_channel.unwrap_or(0),
                        })
                        .collect(),
                ))
                .await;
            }
            MessageType::BanList => {
                let m = mumble::BanList::decode(payload)?;
                self.emit(SessionEvent::Bans(
                    m.bans
                        .into_iter()
                        .map(|b| bans::BanEntry {
                            address: b.address.to_vec(),
                            mask: b.mask,
                            name: b.name.unwrap_or_default(),
                            hash: b.hash.unwrap_or_default(),
                            reason: b.reason.unwrap_or_default(),
                            start: b.start.unwrap_or_default(),
                            duration: b.duration.unwrap_or(0),
                        })
                        .collect(),
                ))
                .await;
            }
            MessageType::SuggestConfig => {
                let m = mumble::SuggestConfig::decode(payload)?;
                // Only what this app can act on. A suggested *client version*
                // is dropped: it names a Mumble build, this is not one, and
                // telling a rider to upgrade to something they cannot install
                // would be noise dressed as advice.
                if m.positional.is_some() || m.push_to_talk.is_some() {
                    self.emit(SessionEvent::ServerSuggests {
                        push_to_talk: m.push_to_talk,
                        positional: m.positional,
                    })
                    .await;
                }
            }
            MessageType::CodecVersion => {
                let m = mumble::CodecVersion::decode(payload)?;
                // This client speaks Opus and nothing else — the handshake
                // advertises an empty CELT list. A server that says it cannot
                // carry Opus is one nobody here can be heard on, and that is
                // worth a line in the log rather than silence and a mystery.
                if m.opus == Some(false) {
                    tracing::warn!(
                        "server does not advertise Opus; this client speaks nothing else"
                    );
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
                    // A reply carrying the privileged half is an answer to
                    // `RequestUserDetails` rather than to the quality poll.
                    // Reported separately: the poll runs every few seconds and
                    // this is a thing somebody asked to see.
                    if state.details_wanted.remove(&session)
                        || m.version.is_some()
                        || m.address.is_some()
                    {
                        let v = m.version.clone().unwrap_or_default();
                        self.emit(SessionEvent::UserDetails(UserDetails {
                            session,
                            release: v.release.unwrap_or_default(),
                            os: v.os.unwrap_or_default(),
                            os_version: v.os_version.unwrap_or_default(),
                            address: m
                                .address
                                .as_ref()
                                .map(|a| bans::address_text(a, 128))
                                .unwrap_or_default(),
                            strong_certificate: m.strong_certificate.unwrap_or(false),
                            online_secs: m.onlinesecs.unwrap_or(0),
                            idle_secs: m.idlesecs.unwrap_or(0),
                        }))
                        .await;
                    }
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
                state.blobs.forget(m.session);
                // The number goes back to the server and may be handed to
                // somebody who runs something else entirely.
                state.peers.forget(m.session);
                self.emit(SessionEvent::Users(state.user_list())).await;
                // They were standing in a channel, and the count beside it is
                // ours to keep true.
                self.emit(SessionEvent::Channels(state.channel_list()))
                    .await;
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
                    state.tcp_ping.record(rtt as f32);
                }
            }
            MessageType::UdpTunnel => {
                // Voice arriving over TLS because UDP is unavailable.
                if let Ok(p) = VoicePacket::decode_incoming(payload) {
                    state.tcp_packets = state.tcp_packets.saturating_add(1);
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
                } else if id == peers::DATA_ID_MUTED_YOU {
                    // Only from a peer that has identified itself, like every
                    // other exchange here: an unannounced session saying this
                    // is a client we know nothing about marking somebody's
                    // roster, and there is no name to put against it.
                    if !state.peers.is_mumbleway(sender) {
                        return Ok(None);
                    }
                    if let Some(muted) = m.data.as_deref().and_then(peers::decode_muted_you) {
                        state.peers.note_muted_us(sender, muted);
                        self.emit(SessionEvent::Users(state.user_list())).await;
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
                // **A message with no target reaches nobody.** The server
                // gathers recipients from the session, channel and tree lists
                // and sends to whoever is in them; all three empty is a
                // message delivered to zero people, with no error — so "no
                // channel given" has to mean *the one this rider is in*,
                // which is what a client with no target picker means by it.
                let target = channel_id.or_else(|| state.self_channel(state.self_session));
                let Some(target) = target else {
                    tracing::warn!("not sending a text message: no channel to send it to");
                    return Ok(());
                };
                let m = mumble::TextMessage {
                    actor: state.self_session,
                    session: Vec::new(),
                    channel_id: vec![target],
                    tree_id: Vec::new(),
                    message: state.limits.fit_text(&message),
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
            SessionCommand::SetAccessTokens(tokens) => {
                // Remembered for the next handshake as well as sent now: a
                // reconnect must present the same set, or a rider comes back
                // to a channel they can no longer enter.
                self.config.profile.access_tokens = tokens.clone();
                let m = mumble::Authenticate {
                    tokens,
                    // Username and password are left out deliberately. The
                    // server reads the tokens out of this message and nothing
                    // else once a session is authenticated, and resending
                    // credentials it is not asking for is how a client ends up
                    // re-authenticating by accident.
                    ..Default::default()
                };
                writer.send(MessageType::Authenticate, &m).await?;
            }
            SessionCommand::SetListening { add, remove } => {
                if let Some(me) = state.self_session {
                    let m = mumble::UserState {
                        session: Some(me),
                        listening_channel_add: add,
                        listening_channel_remove: remove,
                        ..Default::default()
                    };
                    writer.send(MessageType::UserState, &m).await?;
                }
            }
            SessionCommand::SetAvatar(image) => {
                if let Some(me) = state.self_session {
                    // Refused whole by the server when it is too big — with
                    // `TextTooLong`, for a picture — so a rider would see
                    // nothing happen and no reason why. Better to say the
                    // server will not take it than to send it and hope.
                    if !state.limits.image_fits(image.len()) {
                        self.emit(SessionEvent::Text {
                            from: "MumbleWay".into(),
                            message: format!(
                                "This server takes pictures up to {} bytes; yours is {}.",
                                state.limits.image_message_length,
                                image.len()
                            ),
                        })
                        .await;
                        return Ok(());
                    }
                    let m = mumble::UserState {
                        session: Some(me),
                        texture: Some(image.into()),
                        ..Default::default()
                    };
                    writer.send(MessageType::UserState, &m).await?;
                }
            }
            SessionCommand::SetComment(text) => {
                if let Some(me) = state.self_session {
                    // Cut to what this server takes. Over the limit it answers
                    // `TextTooLong` and keeps the old note, which reads as the
                    // app having quietly ignored the rider.
                    let text = state.limits.fit_text(&text);
                    let m = mumble::UserState {
                        session: Some(me),
                        comment: Some(text),
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

                // And told to the rider it is about, if their client can say
                // so. Nothing else can: the server is not party to a local
                // mute, so from their end being dropped looks exactly like
                // being heard, and they carry on talking to somebody who
                // stopped listening. See `peers::DATA_ID_MUTED_YOU`.
                if state.plugin_data && state.peers.accepts_mute_notice(session) {
                    let m = mumble::PluginDataTransmission {
                        sender_session: None,
                        receiver_sessions: vec![session],
                        data: Some(peers::encode_muted_you(muted).into()),
                        data_id: Some(peers::DATA_ID_MUTED_YOU.to_string()),
                    };
                    writer.send(MessageType::PluginDataTransmission, &m).await?;
                }
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
            SessionCommand::BanUser { session, reason } => {
                // `ban_certificate` and `ban_ip` are left unset on purpose: a
                // server defaults them to both, which is what makes a ban
                // survive the rider reconnecting from a new address.
                let m = mumble::UserRemove {
                    session,
                    actor: state.self_session,
                    reason: Some(reason),
                    ban: Some(true),
                    ban_certificate: None,
                    ban_ip: None,
                };
                writer.send(MessageType::UserRemove, &m).await?;
            }
            SessionCommand::MoveUser {
                session,
                channel_id,
            } => {
                let m = mumble::UserState {
                    session: Some(session),
                    channel_id: Some(channel_id),
                    ..Default::default()
                };
                writer.send(MessageType::UserState, &m).await?;
            }
            SessionCommand::TriggerContextAction {
                action,
                session,
                channel_id,
            } => {
                let m = mumble::ContextAction {
                    session,
                    channel_id,
                    action,
                };
                writer.send(MessageType::ContextAction, &m).await?;
            }
            SessionCommand::CreateChannel {
                parent,
                name,
                description,
                temporary,
            } => {
                // No channel_id: that is how the protocol says "make one".
                let m = mumble::ChannelState {
                    parent: Some(parent),
                    name: Some(name),
                    description: Some(state.limits.fit_text(&description))
                        .filter(|d| !d.is_empty()),
                    temporary: Some(temporary),
                    ..Default::default()
                };
                writer.send(MessageType::ChannelState, &m).await?;
            }
            SessionCommand::EditChannel {
                channel_id,
                name,
                description,
            } => {
                let m = mumble::ChannelState {
                    channel_id: Some(channel_id),
                    name,
                    description: description.map(|d| state.limits.fit_text(&d)),
                    ..Default::default()
                };
                writer.send(MessageType::ChannelState, &m).await?;
            }
            SessionCommand::RemoveChannel(channel_id) => {
                let m = mumble::ChannelRemove { channel_id };
                writer.send(MessageType::ChannelRemove, &m).await?;
            }
            SessionCommand::RequestAcl(channel_id) => {
                let m = mumble::Acl {
                    channel_id,
                    query: Some(true),
                    ..Default::default()
                };
                writer.send(MessageType::Acl, &m).await?;
            }
            SessionCommand::SetAcl(acl) => {
                let m = mumble::Acl {
                    channel_id: acl.channel_id,
                    inherit_acls: Some(acl.inherit_acls),
                    query: Some(false),
                    groups: acl
                        .groups
                        .into_iter()
                        .map(|g| mumble::acl::ChanGroup {
                            name: g.name,
                            inherited: Some(g.inherited),
                            inherit: Some(g.inherit),
                            inheritable: Some(g.inheritable),
                            add: g.add,
                            remove: g.remove,
                            inherited_members: g.inherited_members,
                        })
                        .collect(),
                    acls: acl
                        .rules
                        .into_iter()
                        // Inherited rules belong to the channel that defines
                        // them. The server drops them from anything written
                        // back, so sending them would be noise.
                        .filter(|r| !r.inherited)
                        .map(|r| mumble::acl::ChanAcl {
                            apply_here: Some(r.apply_here),
                            apply_subs: Some(r.apply_subs),
                            inherited: Some(false),
                            user_id: r.user_id,
                            group: r.group,
                            grant: Some(r.grant),
                            deny: Some(r.deny),
                        })
                        .collect(),
                };
                let channel_id = m.channel_id;
                writer.send(MessageType::Acl, &m).await?;
                // Read back rather than trusting the write, as with the bans:
                // the server decides what it kept. On the next tick — see
                // `reread_acl`.
                state.reread_acl = Some(channel_id);
            }
            SessionCommand::QueryUserNames(ids) => {
                let m = mumble::QueryUsers {
                    ids,
                    names: Vec::new(),
                };
                writer.send(MessageType::QueryUsers, &m).await?;
            }
            SessionCommand::RequestRegistered => {
                // An empty list is the request; the server answers with the
                // real one.
                let m = mumble::UserList { users: Vec::new() };
                writer.send(MessageType::UserList, &m).await?;
            }
            SessionCommand::UnregisterUsers(ids) => {
                // A registered user with no name is how the protocol spells
                // "remove this registration". Names are left out deliberately.
                let m = mumble::UserList {
                    users: ids
                        .into_iter()
                        .map(|user_id| mumble::user_list::User {
                            user_id,
                            name: None,
                            last_seen: None,
                            last_channel: None,
                        })
                        .collect(),
                };
                writer.send(MessageType::UserList, &m).await?;
            }
            SessionCommand::RegisterUser(session) => {
                // `user_id = 0` against somebody else's session asks the server
                // to give them an account — the same shape as registering
                // ourselves, pointed at another rider.
                let m = mumble::UserState {
                    session: Some(session),
                    user_id: Some(0),
                    ..Default::default()
                };
                writer.send(MessageType::UserState, &m).await?;
            }
            SessionCommand::SetPrioritySpeaker { session, priority } => {
                let m = mumble::UserState {
                    session: Some(session),
                    priority_speaker: Some(priority),
                    ..Default::default()
                };
                writer.send(MessageType::UserState, &m).await?;
            }
            SessionCommand::ResetUserContent {
                session,
                comment,
                texture,
            } => {
                // Clearing is sending an empty one. Both at once is one
                // message, which is also how the official client does it.
                let m = mumble::UserState {
                    session: Some(session),
                    comment: comment.then(String::new),
                    texture: texture.then(prost::bytes::Bytes::new),
                    ..Default::default()
                };
                writer.send(MessageType::UserState, &m).await?;
            }
            SessionCommand::RequestUserDetails(session) => {
                // Without `stats_only`, so the server includes the client
                // version, the address and the certificate — if it is willing
                // to tell this rider, which it is only for an admin.
                //
                // Noted as asked, so the reply is reported even when it says
                // none of those things: "the server did not say" is an answer
                // and a spinner is not.
                state.details_wanted.insert(session);
                let m = mumble::UserStats {
                    session: Some(session),
                    stats_only: Some(false),
                    ..Default::default()
                };
                writer.send(MessageType::UserStats, &m).await?;
            }
            SessionCommand::RequestBans => {
                let m = mumble::BanList {
                    bans: Vec::new(),
                    query: Some(true),
                };
                writer.send(MessageType::BanList, &m).await?;
            }
            SessionCommand::SetBans(list) => {
                let m = mumble::BanList {
                    bans: list
                        .into_iter()
                        .map(|b| mumble::ban_list::BanEntry {
                            address: b.address.into(),
                            mask: b.mask,
                            name: Some(b.name),
                            hash: Some(b.hash),
                            reason: Some(b.reason),
                            start: Some(b.start),
                            duration: Some(b.duration),
                        })
                        .collect(),
                    query: Some(false),
                };
                writer.send(MessageType::BanList, &m).await?;
                // Read back rather than trusting the write: between the list
                // being shown and being sent, another admin may have banned
                // somebody whose entry was never in this copy. On the next
                // tick, not now — see `reread_bans`.
                state.reread_bans = true;
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
