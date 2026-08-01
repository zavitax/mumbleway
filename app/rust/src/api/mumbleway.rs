//! Flutter-facing API.
//!
//! Everything here is deliberately plain data: `flutter_rust_bridge` mirrors
//! these types into Dart, so they avoid lifetimes, generics and borrowed data.
//! All real work happens on a background Tokio runtime owned by [`App`], and the
//! UI observes it through a single event stream.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use flutter_rust_bridge::frb;
use parking_lot::Mutex;
use tokio::sync::mpsc;

use mumbleway_core::audio::engine::{
    AudioConfig, AudioCue, AudioEngine, AudioShared, TransmitMode,
};
use mumbleway_core::audio::{NoiseProfile, Quality};
use mumbleway_core::net::tls::Identity;
use mumbleway_core::session::manager::{SessionManager, TaggedEvent};
use mumbleway_core::session::{
    AudioBridge, ConnectionState, ServerProfile, SessionCommand, SessionEvent, Transport,
    TransportStat,
};

use crate::frb_generated::StreamSink;

/// Required by flutter_rust_bridge; runs before any other call.
#[frb(init)]
pub fn init_app() {
    flutter_rust_bridge::setup_default_user_utils();
}

// ---------------------------------------------------------------------------
// Data mirrored into Dart
// ---------------------------------------------------------------------------

/// A server the user has configured.
#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub id: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: Option<String>,
    pub cert_fingerprint: Option<String>,
}

/// Connection status, flattened for easy rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnStatus {
    Idle,
    Connecting,
    Handshaking,
    Connected,
    Reconnecting,
    Disconnected,
    Failed,
}

/// A status change for one server.
#[derive(Debug, Clone)]
pub struct StatusUpdate {
    pub server_id: String,
    pub status: ConnStatus,
    /// Human-readable detail: the failure reason, or empty when healthy.
    pub detail: String,
    /// Reconnect attempt number, 0 when not reconnecting.
    pub attempt: u32,
    /// Milliseconds until the next retry, for a countdown.
    pub retry_in_ms: u64,
}

#[derive(Debug, Clone)]
pub struct UiUser {
    pub session: u32,
    pub name: String,
    pub channel_id: u32,
    pub talking: bool,
    /// Muted server-side or by themselves — nobody hears them.
    pub muted: bool,
    pub deafened: bool,
    /// Silenced by us alone. Needs no permission and is invisible to others.
    pub local_mute: bool,
    /// One word for the roster: talking, silent, muted, deafened, muted for you.
    pub status: String,
}

#[derive(Debug, Clone)]
pub struct UiChannel {
    pub id: u32,
    pub name: String,
    pub parent: Option<u32>,
    pub description: String,
    /// Users currently in this channel.
    pub user_count: u32,
    pub max_users: u32,
}

/// What an unauthenticated status probe reported about a server.
#[derive(Debug, Clone)]
pub struct UiServerStatus {
    pub server_id: String,
    pub reachable: bool,
    pub ping_ms: f64,
    pub users: u32,
    pub max_users: u32,
    pub version: String,
}

#[derive(Debug, Clone)]
pub struct UiStats {
    pub server_id: String,
    pub tcp_ping_ms: f32,
    pub udp_ping_ms: f32,
    /// "udp" for the low-latency path, "tcp" when tunnelling.
    pub transport: String,
}

/// Everything the UI can be told about.
#[derive(Debug, Clone)]
pub enum AppEvent {
    Status(StatusUpdate),
    Users {
        server_id: String,
        users: Vec<UiUser>,
    },
    Channels {
        server_id: String,
        channels: Vec<UiChannel>,
    },
    Text {
        server_id: String,
        from: String,
        message: String,
    },
    Stats(UiStats),
    /// Microphone level and speech detection, for the input meter.
    InputLevel {
        level_db: f32,
        speaking: bool,
    },
    /// The server presented a certificate. `changed` means it differs from the
    /// pinned one and the user must decide.
    Certificate {
        server_id: String,
        fingerprint: String,
        changed: bool,
    },
    Welcome {
        server_id: String,
        text: String,
    },
    /// Our own session id on this server. Needed to work out which channel we
    /// are in, and to keep ourselves out of the "other users" roster.
    SelfSession {
        server_id: String,
        session: u32,
    },
}

/// Noise-suppression strength, exposed as a simple selector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoiseSetting {
    Off,
    Light,
    Standard,
    /// Aggressive profile for a motorcycle helmet.
    Helmet,
}

fn to_profile(v: NoiseSetting) -> NoiseProfile {
    match v {
        NoiseSetting::Off => NoiseProfile::Off,
        NoiseSetting::Light => NoiseProfile::Light,
        NoiseSetting::Standard => NoiseProfile::Standard,
        NoiseSetting::Helmet => NoiseProfile::Helmet,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MicMode {
    VoiceActivity,
    PushToTalk,
    Continuous,
}

fn to_transmit(v: MicMode) -> TransmitMode {
    match v {
        MicMode::VoiceActivity => TransmitMode::VoiceActivity,
        MicMode::PushToTalk => TransmitMode::PushToTalk,
        MicMode::Continuous => TransmitMode::Continuous,
    }
}

/// Startup options.
#[derive(Debug, Clone)]
pub struct StartupOptions {
    /// Writable directory for the client identity certificate.
    pub storage_dir: String,
    pub noise: NoiseSetting,
    pub mic_mode: MicMode,
}

// ---------------------------------------------------------------------------
// Global application state
// ---------------------------------------------------------------------------

struct App {
    rt: tokio::runtime::Runtime,
    manager: tokio::sync::Mutex<SessionManager>,
    shared: Arc<AudioShared>,
    /// Kept alive for as long as the app runs; dropping it stops audio.
    _audio: AudioEngine,
    /// One sender per connected session, so a single encoded frame can be fanned
    /// out to every server at once.
    outgoing: Arc<Mutex<Vec<mpsc::Sender<(u64, Vec<u8>, bool)>>>>,
    /// Maps a server id to its audio slot, which namespaces speaker streams.
    slots: Mutex<HashMap<String, u16>>,
    /// Last status seen per server, so connection cues fire on transitions
    /// rather than on every repeated status event.
    last_status: Arc<Mutex<HashMap<String, ConnStatus>>>,
    identity: Identity,
}

/// Decides which audio cue, if any, a status transition should play.
///
/// Split out as a pure function so the rules are testable: cues that fire on
/// the wrong edge are worse than none, since a rider trusts them without
/// looking at the screen.
fn cue_for_transition(previous: Option<ConnStatus>, next: ConnStatus) -> Option<AudioCue> {
    let was_live = matches!(previous, Some(ConnStatus::Connected));
    match next {
        // Dropped out of a working connection.
        ConnStatus::Reconnecting | ConnStatus::Failed if was_live => {
            Some(AudioCue::Disconnected)
        }
        // Back after a drop. Deliberately not on the first connect: the user
        // is looking at the screen then, and a chime on every launch is noise.
        ConnStatus::Connected
            if matches!(
                previous,
                Some(ConnStatus::Reconnecting) | Some(ConnStatus::Failed)
            ) =>
        {
            Some(AudioCue::Reconnected)
        }
        _ => None,
    }
}

static APP: OnceLock<App> = OnceLock::new();
static EVENT_SINK: OnceLock<Mutex<Option<StreamSink<AppEvent>>>> = OnceLock::new();

fn app() -> anyhow::Result<&'static App> {
    APP.get()
        .ok_or_else(|| anyhow::anyhow!("call startEngine() before using the client"))
}

fn emit(event: AppEvent) {
    if let Some(cell) = EVENT_SINK.get() {
        if let Some(sink) = cell.lock().as_ref() {
            let _ = sink.add(event);
        }
    }
}

fn status_of(state: &ConnectionState) -> StatusUpdate {
    let (status, detail, attempt, retry_in_ms) = match state {
        ConnectionState::Idle => (ConnStatus::Idle, String::new(), 0, 0),
        ConnectionState::Connecting => (ConnStatus::Connecting, String::new(), 0, 0),
        ConnectionState::Handshaking => (ConnStatus::Handshaking, String::new(), 0, 0),
        ConnectionState::Connected => (ConnStatus::Connected, String::new(), 0, 0),
        ConnectionState::Reconnecting {
            attempt,
            retry_in_ms,
            reason,
        } => (
            ConnStatus::Reconnecting,
            reason.clone(),
            *attempt,
            *retry_in_ms,
        ),
        ConnectionState::Disconnected { reason } => {
            (ConnStatus::Disconnected, reason.clone(), 0, 0)
        }
        ConnectionState::Failed { reason } => (ConnStatus::Failed, reason.clone(), 0, 0),
    };
    StatusUpdate {
        server_id: String::new(),
        status,
        detail,
        attempt,
        retry_in_ms,
    }
}

// ---------------------------------------------------------------------------
// Exposed functions
// ---------------------------------------------------------------------------

/// Starts the engine. Must be called once before anything else.
pub fn start_engine(options: StartupOptions) -> anyhow::Result<()> {
    if APP.get().is_some() {
        return Ok(());
    }

    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;

    let dir = std::path::PathBuf::from(&options.storage_dir);
    let identity = Identity::load_or_create(&dir, "MumbleWay")?;

    let outgoing: Arc<Mutex<Vec<mpsc::Sender<(u64, Vec<u8>, bool)>>>> =
        Arc::new(Mutex::new(Vec::new()));

    // The audio engine hands every encoded frame to all connected sessions.
    let fanout = outgoing.clone();
    let audio = AudioEngine::start(
        AudioConfig {
            noise_profile: to_profile(options.noise),
            quality: Quality::Balanced,
            transmit_mode: to_transmit(options.mic_mode),
            input_device: None,
            output_device: None,
        },
        move |seq, packet, terminator| {
            let senders = fanout.lock();
            for s in senders.iter() {
                // Never block the DSP thread on a slow session.
                let _ = s.try_send((seq, packet.clone(), terminator));
            }
        },
    )?;
    let shared = audio.shared();

    // Aggregate every session's events onto the Dart stream.
    let (ev_tx, mut ev_rx) = mpsc::channel::<TaggedEvent>(512);
    let manager = SessionManager::new(identity.clone(), "MumbleWay 0.1", ev_tx);

    let level_shared = shared.clone();
    let cue_shared = shared.clone();
    let last_status: Arc<Mutex<HashMap<String, ConnStatus>>> =
        Arc::new(Mutex::new(HashMap::new()));
    let status_tracker = last_status.clone();

    rt.spawn(async move {
        while let Some(TaggedEvent { server_id, event }) = ev_rx.recv().await {
            match event {
                SessionEvent::State(s) => {
                    let mut u = status_of(&s);
                    u.server_id = server_id.clone();

                    // Signal drops and recoveries audibly: the phone is usually
                    // in a pocket or behind a navigation app, so a status
                    // change that is only visible is one the rider misses.
                    let previous = status_tracker.lock().insert(server_id, u.status);
                    if let Some(cue) = cue_for_transition(previous, u.status) {
                        cue_shared.play_cue(cue);
                    }

                    emit(AppEvent::Status(u));
                }
                SessionEvent::Users(users) => emit(AppEvent::Users {
                    server_id,
                    users: users
                        .into_iter()
                        .map(|u| {
                            // Derive the label before moving any fields out.
                            let status = u.status_label().to_string();
                            UiUser {
                                session: u.session,
                                name: u.name,
                                channel_id: u.channel_id,
                                talking: u.talking,
                                muted: u.mute || u.self_mute,
                                deafened: u.deaf || u.self_deaf,
                                local_mute: u.local_mute,
                                status,
                            }
                        })
                        .collect(),
                }),
                SessionEvent::Channels(chans) => emit(AppEvent::Channels {
                    server_id,
                    channels: chans
                        .into_iter()
                        .map(|c| UiChannel {
                            id: c.id,
                            name: c.name,
                            parent: c.parent,
                            description: c.description,
                            user_count: c.user_count,
                            max_users: c.max_users,
                        })
                        .collect(),
                }),
                SessionEvent::Text { from, message } => emit(AppEvent::Text {
                    server_id,
                    from,
                    message,
                }),
                SessionEvent::Stats(s) => emit(AppEvent::Stats(UiStats {
                    server_id,
                    tcp_ping_ms: s.tcp_ping_ms,
                    udp_ping_ms: s.udp_ping_ms,
                    transport: match s.transport {
                        Some(TransportStat::Udp) => "udp".to_string(),
                        _ => "tcp".to_string(),
                    },
                })),
                SessionEvent::TransportChanged(t) => emit(AppEvent::Stats(UiStats {
                    server_id,
                    tcp_ping_ms: 0.0,
                    udp_ping_ms: 0.0,
                    transport: match t {
                        Transport::Udp => "udp".to_string(),
                        Transport::TcpTunnel => "tcp".to_string(),
                    },
                })),
                SessionEvent::ServerCertificate {
                    fingerprint,
                    changed,
                } => emit(AppEvent::Certificate {
                    server_id,
                    fingerprint,
                    changed,
                }),
                SessionEvent::Welcome(text) => emit(AppEvent::Welcome { server_id, text }),
                SessionEvent::SelfSession(session) => {
                    emit(AppEvent::SelfSession { server_id, session })
                }
                // Talking state already rides along on the user roster.
                SessionEvent::Talking { .. } => {}
            }
        }
    });

    // Publish the microphone level a few times a second for the meter.
    rt.spawn(async move {
        let mut tick = tokio::time::interval(std::time::Duration::from_millis(100));
        loop {
            tick.tick().await;
            emit(AppEvent::InputLevel {
                level_db: level_shared.input_level_db(),
                speaking: level_shared.speech_detected(),
            });
        }
    });

    let _ = APP.set(App {
        rt,
        manager: tokio::sync::Mutex::new(manager),
        shared,
        _audio: audio,
        outgoing,
        slots: Mutex::new(HashMap::new()),
        last_status,
        identity,
    });
    Ok(())
}

/// Opens the event stream the UI listens on.
pub fn app_events(sink: StreamSink<AppEvent>) -> anyhow::Result<()> {
    let cell = EVENT_SINK.get_or_init(|| Mutex::new(None));
    *cell.lock() = Some(sink);
    Ok(())
}

/// Registers a server and starts its (initially idle) session.
pub fn add_server(config: ServerConfig) -> anyhow::Result<String> {
    let app = app()?;

    let mut profile = ServerProfile::new(
        config.name.clone(),
        config.host.clone(),
        config.port,
        config.username.clone(),
    );
    profile.password = config.password;
    profile.cert_fingerprint = config.cert_fingerprint;
    let id = profile.id.clone();

    // Wire this session into the audio engine.
    let (out_tx, out_rx) = mpsc::channel::<(u64, Vec<u8>, bool)>(64);
    let (in_tx, mut in_rx) = mpsc::channel::<mumbleway_core::net::VoicePacket>(256);

    let slot = {
        let mut slots = app.slots.lock();
        let next = slots.len() as u16;
        *slots.entry(id.clone()).or_insert(next)
    };

    let shared = app.shared.clone();
    app.rt.spawn(async move {
        while let Some(packet) = in_rx.recv().await {
            shared.push_incoming(slot, &packet);
        }
    });

    let bridge = AudioBridge {
        outgoing: out_rx,
        incoming: in_tx,
    };

    let result = app.rt.block_on(async {
        let mut m = app.manager.lock().await;
        m.add(profile, bridge)
    });

    match result {
        Ok(id) => {
            app.outgoing.lock().push(out_tx);
            Ok(id)
        }
        Err(e) => {
            app.slots.lock().remove(&id);
            Err(anyhow::anyhow!(e.to_string()))
        }
    }
}

fn send_command(server_id: String, cmd: SessionCommand) -> anyhow::Result<()> {
    let app = app()?;
    app.rt
        .block_on(async {
            let m = app.manager.lock().await;
            m.send(&server_id, cmd).await
        })
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    Ok(())
}

/// Connects (or reconnects) a server.
pub fn connect_server(server_id: String) -> anyhow::Result<()> {
    send_command(server_id, SessionCommand::Connect)
}

/// Disconnects a server. This is user-initiated, so it will not auto-reconnect.
pub fn disconnect_server(server_id: String) -> anyhow::Result<()> {
    send_command(server_id, SessionCommand::Disconnect)
}

/// Accepts a changed server certificate and re-pins it.
pub fn accept_certificate(server_id: String) -> anyhow::Result<()> {
    send_command(server_id, SessionCommand::AcceptCertificate)
}

pub fn join_channel(server_id: String, channel_id: u32) -> anyhow::Result<()> {
    send_command(server_id, SessionCommand::JoinChannel(channel_id))
}

pub fn send_text(server_id: String, message: String) -> anyhow::Result<()> {
    send_command(
        server_id,
        SessionCommand::SendText {
            channel_id: None,
            message,
        },
    )
}

pub fn set_self_mute(server_id: String, muted: bool) -> anyhow::Result<()> {
    send_command(server_id, SessionCommand::SetSelfMute(muted))
}

/// Removes a server and stops its session.
pub fn remove_server(server_id: String) -> anyhow::Result<()> {
    let app = app()?;
    app.rt
        .block_on(async {
            let mut m = app.manager.lock().await;
            m.remove(&server_id).await
        })
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    app.slots.lock().remove(&server_id);
    Ok(())
}

/// Mutes or unmutes the local microphone.
#[frb(sync)]
pub fn set_microphone_muted(muted: bool) -> anyhow::Result<()> {
    app()?.shared.set_muted(muted);
    Ok(())
}

/// Silences all incoming audio.
#[frb(sync)]
pub fn set_deafened(deafened: bool) -> anyhow::Result<()> {
    app()?.shared.set_deafened(deafened);
    Ok(())
}

/// Push-to-talk key state.
#[frb(sync)]
pub fn set_transmitting(on: bool) -> anyhow::Result<()> {
    app()?.shared.set_transmitting(on);
    Ok(())
}

/// Current microphone level in dBFS.
#[frb(sync)]
pub fn input_level_db() -> anyhow::Result<f32> {
    Ok(app()?.shared.input_level_db())
}

/// Available audio input device names.
pub fn audio_input_devices() -> Vec<String> {
    mumbleway_core::audio::engine::list_devices().0
}

/// Available audio output device names.
pub fn audio_output_devices() -> Vec<String> {
    mumbleway_core::audio::engine::list_devices().1
}

/// The SHA-256 fingerprint of our own client certificate, which servers use to
/// recognise a registered user.
pub fn client_certificate_fingerprint() -> anyhow::Result<String> {
    let app = app()?;
    let certs = rustls_pemfile::certs(&mut app.identity.cert_pem.as_bytes())
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| anyhow::anyhow!("reading identity: {e}"))?;
    let first = certs
        .first()
        .ok_or_else(|| anyhow::anyhow!("identity certificate was empty"))?;
    Ok(mumbleway_core::net::tls::fingerprint_of(first.as_ref()))
}

/// How many servers may be connected at once.
#[frb(sync)]
pub fn max_concurrent_servers() -> u32 {
    mumbleway_core::session::manager::MAX_CONCURRENT_SESSIONS as u32
}

/// Default Mumble port, so the UI can prefill it.
#[frb(sync)]
pub fn default_port() -> u16 {
    64738
}

// ---------------------------------------------------------------------------
// Server status probing
// ---------------------------------------------------------------------------

/// Queries a server's ping and occupancy without connecting or authenticating.
///
/// Never fails: an unreachable or non-responding server comes back with
/// `reachable == false`, because the caller is refreshing a list and a thrown
/// error per offline server would be noise.
pub async fn ping_server(server_id: String, host: String, port: u16) -> UiServerStatus {
    let result = mumbleway_core::net::ping::query(
        &host,
        port,
        std::time::Duration::from_secs(3),
    )
    .await;

    match result {
        Ok(s) => UiServerStatus {
            server_id,
            reachable: true,
            ping_ms: s.rtt_ms,
            users: s.users,
            max_users: s.max_users,
            version: s.version_string(),
        },
        Err(_) => UiServerStatus {
            server_id,
            reachable: false,
            ping_ms: 0.0,
            users: 0,
            max_users: 0,
            version: String::new(),
        },
    }
}

// ---------------------------------------------------------------------------
// Audio devices and levels
// ---------------------------------------------------------------------------

/// Selects capture and playback devices. `None` means the system default.
/// Takes effect within a few hundred milliseconds, without dropping sessions.
pub fn set_audio_devices(input: Option<String>, output: Option<String>) -> anyhow::Result<()> {
    app()?.shared.set_devices(input, output);
    Ok(())
}

/// Currently selected devices as `(input, output)`.
pub fn current_audio_devices() -> anyhow::Result<(Option<String>, Option<String>)> {
    Ok(app()?.shared.devices())
}

#[frb(sync)]
pub fn set_input_gain_db(db: f32) -> anyhow::Result<()> {
    app()?.shared.set_input_gain_db(db);
    Ok(())
}

#[frb(sync)]
pub fn input_gain_db() -> anyhow::Result<f32> {
    Ok(app()?.shared.input_gain_db())
}

#[frb(sync)]
pub fn set_output_volume_db(db: f32) -> anyhow::Result<()> {
    app()?.shared.set_output_volume_db(db);
    Ok(())
}

#[frb(sync)]
pub fn output_volume_db() -> anyhow::Result<f32> {
    Ok(app()?.shared.output_volume_db())
}

/// Playback level in dBFS, for an output meter.
#[frb(sync)]
pub fn output_level_db() -> anyhow::Result<f32> {
    Ok(app()?.shared.output_level_db())
}

/// Loopback monitoring: hear your own processed voice, to check the microphone
/// and the noise-suppression setting.
#[frb(sync)]
pub fn set_monitoring(on: bool) -> anyhow::Result<()> {
    app()?.shared.set_monitor(on);
    Ok(())
}

#[frb(sync)]
pub fn is_monitoring() -> anyhow::Result<bool> {
    Ok(app()?.shared.is_monitoring())
}

/// Plays a tone on the output device, to check the speaker choice.
#[frb(sync)]
pub fn play_test_tone(millis: u32) -> anyhow::Result<()> {
    app()?.shared.play_test_tone(millis);
    Ok(())
}

#[frb(sync)]
pub fn stop_test_tone() -> anyhow::Result<()> {
    app()?.shared.stop_test_tone();
    Ok(())
}

/// Gain limits, so the UI can build sliders that match the engine.
#[frb(sync)]
pub fn gain_limits() -> Vec<f32> {
    use mumbleway_core::audio::engine as e;
    vec![
        e::MIN_INPUT_GAIN_DB,
        e::MAX_INPUT_GAIN_DB,
        e::MIN_OUTPUT_VOLUME_DB,
        e::MAX_OUTPUT_VOLUME_DB,
    ]
}

// ---------------------------------------------------------------------------
// Users and channels
// ---------------------------------------------------------------------------

/// Silences another user for us only. Always permitted.
pub fn set_user_local_mute(
    server_id: String,
    session: u32,
    muted: bool,
) -> anyhow::Result<()> {
    send_command(server_id, SessionCommand::SetUserLocalMute { session, muted })
}

/// Silences another user for everyone. Requires the Mute permission; without it
/// the server replies with a permission-denied message that surfaces as text.
pub fn set_user_server_mute(
    server_id: String,
    session: u32,
    muted: bool,
) -> anyhow::Result<()> {
    send_command(server_id, SessionCommand::SetUserServerMute { session, muted })
}

/// Deafens another user server-side. Also permission-gated.
pub fn set_user_server_deaf(server_id: String, session: u32, deaf: bool) -> anyhow::Result<()> {
    send_command(server_id, SessionCommand::SetUserServerDeaf { session, deaf })
}

/// Channel to join automatically on every future connect. `None` clears it.
pub fn set_default_channel(server_id: String, channel: Option<String>) -> anyhow::Result<()> {
    send_command(server_id, SessionCommand::SetDefaultChannel(channel))
}

// ---------------------------------------------------------------------------
// Importing servers
// ---------------------------------------------------------------------------

/// Parses a `mumble://` link or a JSON profile file into server definitions.
///
/// Nothing is connected or saved here; the caller decides what to keep.
pub fn import_servers(
    text: String,
    fallback_username: String,
) -> anyhow::Result<Vec<ServerConfig>> {
    let profiles = mumbleway_core::session::profile::parse_any(&text, &fallback_username)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;

    Ok(profiles
        .into_iter()
        .map(|p| ServerConfig {
            id: p.id,
            name: p.name,
            host: p.host,
            port: p.port,
            username: p.username,
            password: p.password,
            cert_fingerprint: p.cert_fingerprint,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drop_cue_only_fires_when_a_working_connection_is_lost() {
        assert_eq!(
            cue_for_transition(Some(ConnStatus::Connected), ConnStatus::Reconnecting),
            Some(AudioCue::Disconnected)
        );
        assert_eq!(
            cue_for_transition(Some(ConnStatus::Connected), ConnStatus::Failed),
            Some(AudioCue::Disconnected)
        );
    }

    #[test]
    fn retrying_repeatedly_does_not_replay_the_drop_cue() {
        // Backoff cycles through Reconnecting -> Connecting -> Reconnecting.
        // Only the first transition out of Connected should sound.
        assert_eq!(
            cue_for_transition(Some(ConnStatus::Reconnecting), ConnStatus::Connecting),
            None
        );
        assert_eq!(
            cue_for_transition(Some(ConnStatus::Connecting), ConnStatus::Reconnecting),
            None
        );
        assert_eq!(
            cue_for_transition(Some(ConnStatus::Reconnecting), ConnStatus::Reconnecting),
            None
        );
    }

    #[test]
    fn resume_cue_fires_only_after_an_actual_drop() {
        assert_eq!(
            cue_for_transition(Some(ConnStatus::Reconnecting), ConnStatus::Connected),
            Some(AudioCue::Reconnected)
        );
        assert_eq!(
            cue_for_transition(Some(ConnStatus::Failed), ConnStatus::Connected),
            Some(AudioCue::Reconnected)
        );
    }

    #[test]
    fn first_connect_is_silent() {
        // The user is looking at the screen then; a chime on every launch is
        // noise rather than information.
        assert_eq!(cue_for_transition(None, ConnStatus::Connected), None);
        assert_eq!(
            cue_for_transition(Some(ConnStatus::Idle), ConnStatus::Connected),
            None
        );
        assert_eq!(
            cue_for_transition(Some(ConnStatus::Connecting), ConnStatus::Connected),
            None
        );
        assert_eq!(
            cue_for_transition(Some(ConnStatus::Handshaking), ConnStatus::Connected),
            None
        );
    }

    #[test]
    fn user_initiated_disconnect_is_silent() {
        // The user pressed the button; they know.
        assert_eq!(
            cue_for_transition(Some(ConnStatus::Connected), ConnStatus::Disconnected),
            None
        );
    }
}
