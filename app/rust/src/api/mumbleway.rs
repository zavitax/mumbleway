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

use mumbleway_core::audio::dehiss::DehissMode;
use mumbleway_core::audio::engine::{
    AudioConfig, AudioCue, AudioEngine, AudioShared, TransmitMode,
};
use mumbleway_core::audio::feedback::FeedbackMode;
use mumbleway_core::audio::{NoiseProfile, Quality};
use mumbleway_core::diag::{self, LogEntry, LogLevel};
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
    /// Channel to drop into on connecting, as the link carried it.
    ///
    /// Here because an invitation is about a place as much as a server: the
    /// `mumble://` scheme puts the channel in the path, the core has always
    /// parsed it, and it was then dropped on the way across this boundary —
    /// so following a link that named a channel landed the guest in the root
    /// and left them to find the conversation themselves.
    pub default_channel: Option<String>,
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
        /// Level voice activation opens at, tracking the background noise.
        threshold_db: f32,
        /// The tracked background noise itself. The gap up to `threshold_db`
        /// is the margin, which is what makes a rising floor readable as
        /// wind rather than as a mis-set control.
        noise_floor_db: f32,
    },
    /// Level of each speaker currently producing audio.
    ///
    /// The server never reports who is talking, so the only honest source is
    /// the audio itself.
    SpeakerLevels {
        levels: Vec<UiSpeakerLevel>,
    },
    /// Someone else changed our mute or deafen state.
    Moderated {
        server_id: String,
        muted: Option<bool>,
        deafened: Option<bool>,
        by: String,
    },
    /// The server presented a certificate. `changed` means it differs from the
    /// pinned one and the user must decide.
    Certificate {
        server_id: String,
        fingerprint: String,
        changed: bool,
    },
    /// The server refused an action -- muting somebody, joining a channel,
    /// sending a message. Carried on its own so the UI can put it in front of
    /// the user instead of into the chat log, where it used to go and where a
    /// refusal reads as somebody talking and then scrolls away.
    Refused {
        server_id: String,
        /// The server's own words. Often empty: most servers send only a type.
        reason: String,
        /// Mumble's `DenyType`, so the UI has something translatable to say
        /// when `reason` is empty.
        kind: u32,
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
    /// Lines the engine wrote about itself, for the log in the diagnostics
    /// panel and for the platform log behind it.
    Log {
        entries: Vec<UiLogEntry>,
    },
}

/// One line of the engine's log.
#[derive(Debug, Clone)]
pub struct UiLogEntry {
    /// Monotonic within a run, so the reader can ask for what it has not seen
    /// without relying on timestamps being unique.
    pub seq: u64,
    pub at_ms: u64,
    /// 0 trace, 1 debug, 2 info, 3 warn, 4 error. A number rather than an enum
    /// because the UI orders and filters by severity, and an enum would have to
    /// be mapped back to exactly this order to do it.
    pub level: u8,
    /// The subsystem that spoke: `session`, `engine`, `manager`.
    pub target: String,
    pub message: String,
}

impl From<LogEntry> for UiLogEntry {
    fn from(e: LogEntry) -> Self {
        UiLogEntry {
            seq: e.seq,
            at_ms: e.at_ms,
            level: match e.level {
                LogLevel::Trace => 0,
                LogLevel::Debug => 1,
                LogLevel::Info => 2,
                LogLevel::Warn => 3,
                LogLevel::Error => 4,
            },
            target: e.target,
            message: e.message,
        }
    }
}

/// One speaker's current loudness.
#[derive(Debug, Clone)]
pub struct UiSpeakerLevel {
    pub server_id: String,
    pub session: u32,
    /// dBFS, falling towards silence when they stop.
    pub level_db: f32,
}

/// Noise-suppression strength, exposed as a simple selector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoiseSetting {
    Off,
    Light,
    Standard,
    /// Aggressive profile for a motorcycle helmet.
    Helmet,
    /// Picks between the four above from what the microphone is hearing.
    ///
    /// Last in the list rather than first: it is an extra option, and putting
    /// it at the top would renumber every setting a rider has already stored.
    Auto,
}

fn to_profile(v: NoiseSetting) -> NoiseProfile {
    match v {
        NoiseSetting::Off => NoiseProfile::Off,
        NoiseSetting::Light => NoiseProfile::Light,
        NoiseSetting::Standard => NoiseProfile::Standard,
        NoiseSetting::Helmet => NoiseProfile::Helmet,
        NoiseSetting::Auto => NoiseProfile::Auto,
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

/// Encoded frame fan-out: `(sequence, opus payload, is_terminator)` per live
/// session. Named because the bare type appears in several signatures and is
/// unreadable spelled out.
type OutgoingFanout = Arc<Mutex<Vec<mpsc::Sender<(u64, Vec<u8>, bool)>>>>;

struct App {
    rt: tokio::runtime::Runtime,
    manager: tokio::sync::Mutex<SessionManager>,
    shared: Arc<AudioShared>,
    /// Kept alive for as long as the app runs; dropping it stops audio.
    _audio: AudioEngine,
    /// One sender per connected session, so a single encoded frame can be fanned
    /// out to every server at once.
    outgoing: OutgoingFanout,
    /// Maps a server id to its audio slot, which namespaces speaker streams.
    slots: Arc<Mutex<HashMap<String, u16>>>,
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
/// How often the dialing cue repeats while a connection is being chased.
///
/// Long enough not to nag over an engine, short enough that the gap never
/// reads as "it stopped trying" — the retry interval is ten seconds, so this
/// lands two or three times across one wait.
const WAITING_CUE_INTERVAL: std::time::Duration = std::time::Duration::from_secs(4);

/// Empty speaker reports still sent after the last voice stops.
///
/// The UI fades a meter out rather than blanking it, and it needs a report per
/// step to do it: `VoiceMeter.fallPerReportDb` is 9 dB, from a ceiling of 0 to
/// `silentDb` of -120, so fourteen steps empty the loudest possible meter and
/// one more clears the entry. Stopping the instant the mixer goes quiet would
/// leave every meter frozen part-way down instead of falling to nothing.
///
/// Deliberately generous, and deliberately spelled out rather than tuned: this
/// number is the only thing coupling the two sides, and being a few reports too
/// long costs nothing while being one too short is visible on every utterance.
const SILENT_LEVEL_TAIL: u32 = 16;

/// Whether a status means "still trying to get connected".
///
/// Covers the wait between attempts as well as the attempts themselves: from
/// the rider's side those are the same situation, and the silence in between is
/// the part that most needs filling.
fn is_waiting(status: ConnStatus) -> bool {
    matches!(
        status,
        ConnStatus::Connecting | ConnStatus::Handshaking | ConnStatus::Reconnecting
    )
}

fn cue_for_transition(previous: Option<ConnStatus>, next: ConnStatus) -> Option<AudioCue> {
    let was_live = matches!(previous, Some(ConnStatus::Connected));
    match next {
        // Dropped out of a working connection.
        ConnStatus::Reconnecting | ConnStatus::Failed if was_live => Some(AudioCue::Disconnected),
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
        // Dialing, but only for a connect the user asked for. Automatic retries
        // pass through Connecting too, and beeping on every one of them during
        // a bad stretch of road would be maddening — the drop cue already said
        // what happened.
        ConnStatus::Connecting
            if matches!(
                previous,
                None | Some(ConnStatus::Idle)
                    | Some(ConnStatus::Disconnected)
                    | Some(ConnStatus::Failed)
            ) =>
        {
            Some(AudioCue::Dialing)
        }
        _ => None,
    }
}

/// Picks the cue for having been muted or deafened by someone else.
///
/// Deafening is reported in preference to muting when both change at once,
/// because losing the ability to hear matters more than losing the microphone.
fn cue_for_moderation(muted: Option<bool>, deafened: Option<bool>) -> Option<AudioCue> {
    match (deafened, muted) {
        (Some(true), _) => Some(AudioCue::DeafenedByOther),
        (Some(false), _) => Some(AudioCue::UndeafenedByOther),
        (None, Some(true)) => Some(AudioCue::MutedByOther),
        (None, Some(false)) => Some(AudioCue::UnmutedByOther),
        (None, None) => None,
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

    // Before anything that might have something to say, so a failure during
    // startup is in the log rather than being the reason there is no log.
    diag::install();

    // Panics into the log.
    //
    // A panic on a worker thread kills that thread and nothing else: the
    // channel it was going to answer on simply closes, and the caller reports a
    // timeout for something that never had a chance. The message goes to
    // stderr, which on Android goes nowhere at all — so the one line saying
    // what actually happened was the one line nobody could read.
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        // With the frames, not just the line. Where a panic was raised is
        // often a helper several calls below the code that had the wrong idea
        // — a decoder given a short buffer, a lock taken twice — and the line
        // number alone names the victim rather than the cause.
        //
        // Forced rather than left to RUST_BACKTRACE: nobody can set an
        // environment variable on a phone, which is exactly where the crashes
        // nobody can reproduce happen.
        let trace = std::backtrace::Backtrace::force_capture();
        let where_ = match info.location() {
            Some(at) => format!("{} at {}:{}", info, at.file(), at.line()),
            None => info.to_string(),
        };
        diag::record(LogLevel::Error, "panic", format!("{where_}\n{trace}"));
        previous(info);
    }));

    // Recorded directly rather than through `tracing`, which this crate does
    // not depend on: the one line it has to say does not justify the dependency.
    diag::record(LogLevel::Info, "engine", "starting");

    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;

    let dir = std::path::PathBuf::from(&options.storage_dir);
    let identity = Identity::load_or_create(&dir, "MumbleWay")?;

    let outgoing: OutgoingFanout = Arc::new(Mutex::new(Vec::new()));

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
    // Shared with App so the level task can name the server a stream belongs to.
    let slots: Arc<Mutex<HashMap<String, u16>>> = Arc::new(Mutex::new(HashMap::new()));
    let level_slots = slots.clone();
    let cue_shared = shared.clone();
    let last_status: Arc<Mutex<HashMap<String, ConnStatus>>> = Arc::new(Mutex::new(HashMap::new()));
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
                SessionEvent::Refused { reason, kind } => emit(AppEvent::Refused {
                    server_id,
                    reason,
                    kind,
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
                SessionEvent::SelfModerated {
                    muted,
                    deafened,
                    by,
                } => {
                    // Audible, because this happens *to* the user: they are not
                    // looking at the screen when someone mutes them.
                    if let Some(cue) = cue_for_moderation(muted, deafened) {
                        cue_shared.play_cue(cue);
                    }
                    emit(AppEvent::Moderated {
                        server_id,
                        muted,
                        deafened,
                        by,
                    });
                }
                // Talking state already rides along on the user roster.
                SessionEvent::Talking { .. } => {}
            }
        }
    });

    // Publish the microphone level a few times a second for the meter.
    rt.spawn(async move {
        let mut tick = tokio::time::interval(std::time::Duration::from_millis(100));
        // Starts spent: until somebody speaks there is nothing to fade out, so
        // the first report goes out only when there is one to make.
        let mut silent_ticks = SILENT_LEVEL_TAIL;
        loop {
            tick.tick().await;

            // Nothing is being captured, so there is no level to report and
            // nothing drawing one — the interface says so in words instead.
            // Reporting silence ten times a second into a meter that is not on
            // screen is the shape of waste this whole pass is about.
            if !level_shared.audio_wanted() {
                silent_ticks = SILENT_LEVEL_TAIL;
                continue;
            }

            emit(AppEvent::InputLevel {
                level_db: level_shared.input_level_db(),
                speaking: level_shared.speech_detected(),
                threshold_db: level_shared.activation_threshold_db(),
                noise_floor_db: level_shared.noise_floor_db(),
            });

            // Who is speaking, and how loudly. Derived from the decoded audio
            // because nothing on the wire says it.
            let slots = level_slots.lock().clone();
            let levels: Vec<UiSpeakerLevel> = level_shared
                .speaker_levels()
                .into_iter()
                .filter_map(|(key, level_db)| {
                    let slot = (key >> 32) as u16;
                    let session = key as u32;
                    slots
                        .iter()
                        .find(|(_, s)| **s == slot)
                        .map(|(id, _)| UiSpeakerLevel {
                            server_id: id.clone(),
                            session,
                            level_db,
                        })
                })
                .collect();

            if !levels.is_empty() {
                silent_ticks = 0;
                emit(AppEvent::SpeakerLevels { levels });
            } else if silent_ticks < SILENT_LEVEL_TAIL {
                // Still fading the last speaker out; see [`SILENT_LEVEL_TAIL`].
                silent_ticks += 1;
                emit(AppEvent::SpeakerLevels { levels });
            }
            // Otherwise nobody is talking and every meter has already emptied.
            // The report would be an empty list, compared against an empty
            // list, to change nothing — ten times a second, for as long as the
            // app is open. Which is nearly all of the time.
        }
    });

    // Carry new log lines to the UI.
    //
    // Pushed on the event stream rather than polled by the panel, because the
    // lines also go to the platform log — Console on Apple, logcat on Android —
    // and that has to keep working while the panel is shut, which is nearly
    // always. Batched on a timer instead of sent per line so that a burst
    // during a failed connect does not turn into hundreds of separate hops
    // across the bridge.
    rt.spawn(async move {
        let mut tick = tokio::time::interval(std::time::Duration::from_millis(400));
        let mut sent = 0u64;
        loop {
            tick.tick().await;
            let fresh = diag::since(sent);
            if fresh.is_empty() {
                continue;
            }
            // Recorded before the send: the sink may be absent, and retrying
            // the same lines forever once the UI attaches would replay the
            // whole startup every time.
            sent = fresh.last().map(|e| e.seq).unwrap_or(sent);
            emit(AppEvent::Log {
                entries: fresh.into_iter().map(UiLogEntry::from).collect(),
            });
        }
    });

    // Keep the dialing cue going for as long as a connection is being chased.
    //
    // The transition cue alone marks the moment the attempt starts and then
    // leaves silence, which is indistinguishable from having given up — and a
    // rider cannot look at the screen to tell the difference. Repeating it says
    // "still trying" without needing a glance, and stops on its own the moment
    // the status leaves the waiting states.
    let waiting_shared = shared.clone();
    let waiting_status = last_status.clone();
    rt.spawn(async move {
        let mut tick = tokio::time::interval(WAITING_CUE_INTERVAL);
        // The first tick resolves immediately, and the transition cue has just
        // played; skipping it avoids a double beep at the start.
        tick.tick().await;
        loop {
            tick.tick().await;
            let waiting = waiting_status.lock().values().copied().any(is_waiting);
            if waiting {
                waiting_shared.play_cue(AudioCue::Dialing);
            }
        }
    });

    let _ = APP.set(App {
        rt,
        manager: tokio::sync::Mutex::new(manager),
        shared,
        _audio: audio,
        outgoing,
        slots,
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

    // Honour a caller-supplied id rather than always deriving host:port. That
    // derivation is a good default, but it makes duplicates impossible — and
    // keeping the same server twice under different usernames or channels is a
    // reasonable thing to want.
    if !config.id.trim().is_empty() {
        profile.id = config.id.clone();
    }
    let id = profile.id.clone();

    // Wire this session into the audio engine.
    let (out_tx, out_rx) = mpsc::channel::<(u64, Vec<u8>, bool)>(64);
    let (in_tx, mut in_rx) = mpsc::channel::<mumbleway_core::net::VoicePacket>(256);

    let slot = allocate_slot(&app.slots, &id);

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

/// Gives `id` an audio slot, which namespaces its speakers from every other
/// server's.
///
/// The slot is half of the key each incoming voice stream is filed under, and
/// it is also how a level found in the mixer is traced back to the server it
/// came from — so two servers holding the same slot is not a cosmetic clash.
/// It merges two people's audio into one jitter buffer whenever their session
/// ids happen to match, and it makes the reverse lookup ambiguous: levels for
/// both servers are then attributed to whichever of them the map happens to
/// yield first, and the other server's meters sit at silence for the whole
/// call while its audio plays perfectly.
///
/// This used to hand out `slots.len()`, which is only ever right if slots are
/// never given back. They are — a disconnect removes the entry — so connecting
/// to two servers, dropping the first and reconnecting it produced the pair
/// {A:1, B:1}. Two servers, one slot, and a rider watching a roster of people
/// they could plainly hear with nothing moving beside their names.
///
/// The lowest free number instead, which is stable, reuses slots that have
/// genuinely been released, and cannot collide.
fn allocate_slot(slots: &Arc<Mutex<HashMap<String, u16>>>, id: &str) -> u16 {
    let mut slots = slots.lock();
    if let Some(existing) = slots.get(id) {
        return *existing;
    }
    let taken: std::collections::HashSet<u16> = slots.values().copied().collect();
    let next = (0u16..).find(|s| !taken.contains(s)).unwrap_or(u16::MAX);
    slots.insert(id.to_string(), next);
    next
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
    // Forget the status too. The dialing cue repeats for as long as *any*
    // entry here is in a waiting state, and a server removed mid-connect
    // leaves one that nothing will ever move on — so the cue would carry on
    // every few seconds with nothing connected and no way to stop it.
    app.last_status.lock().remove(&server_id);
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
    let result =
        mumbleway_core::net::ping::query(&host, port, std::time::Duration::from_secs(3)).await;

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

/// Opens or closes the microphone and speaker.
///
/// The engine holds no devices until this is called. Ask for them as a call is
/// being set up, not as the first word is spoken: opening a Bluetooth headset
/// means negotiating an SCO link, which takes one to two seconds and is
/// audible, and a rider who presses talk into a device that is still opening
/// loses the beginning of what they said. A connect already takes that long,
/// so asking here costs nothing that is not already being waited for.
///
/// Turning them on blocks until the device answers, because the answer is the
/// point: no microphone, a refused permission or a headset held by another app
/// are all things the rider can do something about, and all of them surface
/// here. Turning them off returns at once — there is nothing to wait for and
/// nothing that can fail.
pub fn set_audio_active(on: bool) -> anyhow::Result<()> {
    let app = app()?;
    app.shared.set_audio_wanted(on);
    if !on {
        return Ok(());
    }
    app.shared
        .await_open(std::time::Duration::from_secs(10))
        .map_err(|e| anyhow::anyhow!(e))
}

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

/// Everything the diagnostics panel shows, gathered in one call.
///
/// One struct rather than a handful of getters because these numbers are only
/// meaningful against each other: playback gaps mean something different when
/// the microphone is also dropping, and invented audio means something
/// different when losses are climbing.
#[derive(Debug, Clone)]
pub struct UiDiagnostics {
    /// Audio the output had to invent because nothing was ready to play.
    pub playback_gap_ms: u64,
    /// Microphone audio discarded because the processing fell behind.
    pub capture_dropped_ms: u64,
    /// Incoming audio decoded from real packets.
    pub incoming_real_ms: u64,
    /// Incoming audio synthesised to cover gaps.
    pub incoming_invented_ms: u64,
    /// Gaps in incoming streams that had to be concealed.
    pub lost_packets: u64,
    /// Deepest jitter buffer currently held, in milliseconds.
    pub jitter_buffer_ms: u64,
    /// Speakers the mixer is currently tracking.
    pub speakers: u32,

    // Cumulative traffic counters. Rates are left to the caller, because a
    // rate depends on the interval it was measured over and only the caller
    // knows how long it waited.
    pub bytes_in: u64,
    pub bytes_out: u64,
    pub voice_packets_in: u64,
    pub voice_packets_out: u64,

    /// Share of one core this process is using, as a percentage.
    pub cpu_percent: f32,
    /// Busy share of each of the device's cores, or empty where the platform
    /// will not say.
    ///
    /// **The device's cores, not ours.** Every other figure here is about this
    /// process; a core is shared, so this includes everything else running.
    /// That is the point of showing it beside the total — a phone that is
    /// loaded and a phone where only we are loaded look identical otherwise.
    ///
    /// Empty is a real answer and the panel says so rather than drawing
    /// nothing: per-core times come only from the global `/proc/stat` on
    /// Linux, which is the file the Android sandbox denies us and the reason
    /// the CPU figure read 0% before it was measured a different way. Whether
    /// an ordinary app may read it could not be established off-device, so the
    /// app asks and reports what it got.
    pub cpu_per_core: Vec<f32>,
    /// Resident memory, in mebibytes.
    pub memory_mb: f32,
}

use mumbleway_core::usage::process_usage;

#[frb(sync)]
pub fn audio_diagnostics() -> anyhow::Result<UiDiagnostics> {
    let shared = &app()?.shared;
    let (underrun, dropped) = shared.glitch_counts();
    let (invented, decoded) = shared.frame_counts();
    let (lost, depth_frames) = shared.loss_summary();
    let (bytes_in, bytes_out, voice_packets_in, voice_packets_out) =
        mumbleway_core::net::stats::snapshot();
    let (cpu_percent, memory_mb) = process_usage();
    let ms = |samples: u64| samples * 1000 / mumbleway_core::audio::denoise::SAMPLE_RATE as u64;

    Ok(UiDiagnostics {
        playback_gap_ms: ms(underrun),
        capture_dropped_ms: ms(dropped),
        // Every frame the buffer hands out is 20 ms of audio.
        incoming_real_ms: decoded * 20,
        incoming_invented_ms: invented * 20,
        lost_packets: lost,
        jitter_buffer_ms: depth_frames as u64 * 20,
        speakers: shared.speaker_levels().len() as u32,
        bytes_in,
        bytes_out,
        voice_packets_in,
        voice_packets_out,
        cpu_percent,
        cpu_per_core: mumbleway_core::usage::per_core().unwrap_or_default(),
        memory_mb,
    })
}

#[frb(sync)]
pub fn reset_audio_glitches() -> anyhow::Result<()> {
    let app = app()?;
    app.shared.reset_glitch_counts();
    // The input peak is a running maximum, so Reset has to clear it too or it
    // reports the loudest thing that ever happened for the rest of the session.
    app.shared.reset_input_peak();
    // Likewise the stage costs: they carry a worst-ever per stage, so without
    // this the panel would keep reporting one bad block from an hour ago.
    app.shared.reset_stage_timings();
    Ok(())
}

/// What one capture block costs, stage by stage, in microseconds.
///
/// **The measurement that says whether a slow stage is slow.** These are wall
/// clock, so a stage that the operating system descheduled mid-block is charged
/// for the wait — which on a four-core phone running a UI, an audio callback
/// and this worker is a large effect. [`Self::unattributed_us`] is what
/// separates the two: it is the part of the block that no stage was holding a
/// stopwatch on, so a big number there means the worker is being interrupted
/// rather than running slowly, and making a stage cheaper will not help.
///
/// This is why the enhancer's own guard was misleading. It measured only
/// itself, saw frames over 10 ms, and concluded the model could not keep up —
/// when the same model measured alone on the same phone comes in at 6.2 ms.
#[derive(Debug, Clone)]
pub struct UiStageCosts {
    /// One entry per stage, in the order the chain runs them.
    pub stages: Vec<UiStageCost>,
    /// The whole iteration, mean and worst, in microseconds.
    pub block_mean_us: f32,
    pub block_worst_us: u32,
    /// The part of the block no stage accounted for: scheduling, mostly.
    pub unattributed_us: f32,
    /// Captured audio waiting for the worker when a block started, in ms.
    ///
    /// The consequence rather than a cost. A backlog that climbs is a chain
    /// that cannot keep up, and it says so before a sample is dropped.
    pub backlog_mean_ms: f32,
    pub backlog_worst_ms: f32,
    /// How many blocks these are averaged over. Zero means nothing has run.
    pub blocks: u64,
    /// The block budget, so the panel does not have to know it.
    pub budget_us: u32,
}

#[derive(Debug, Clone)]
pub struct UiStageCost {
    /// Stable identifier, for the panel to localise. Never shown raw.
    pub id: String,
    pub mean_us: f32,
    pub worst_us: u32,
}

/// Where a capture block's time goes.
///
/// Free and always current, like [`audio_chain_status`]: the worker keeps these
/// totals whether or not anybody is reading, because a cost that is only
/// measured while a panel is open is measured under different conditions than
/// the ones being complained about.
#[frb(sync)]
pub fn audio_stage_costs() -> anyhow::Result<UiStageCosts> {
    use mumbleway_core::audio::timing::{Stage, STAGE_NAMES};
    let t = app()?.shared.stage_timings();
    let order = [
        Stage::Input,
        Stage::Echo,
        Stage::Enhancer,
        Stage::Suppression,
        Stage::Feedback,
        Stage::Dehiss,
        Stage::Transmit,
        Stage::Encode,
    ];
    Ok(UiStageCosts {
        stages: order
            .iter()
            .map(|s| UiStageCost {
                id: STAGE_NAMES[*s as usize].to_string(),
                mean_us: t.mean_us(*s),
                worst_us: t.worst_us(*s),
            })
            .collect(),
        block_mean_us: t.block_mean_us(),
        block_worst_us: t.block_worst_us(),
        unattributed_us: t.unattributed_us(),
        backlog_mean_ms: t.backlog_mean_ms(),
        backlog_worst_ms: t.backlog_worst_ms(),
        blocks: t.blocks(),
        budget_us: 10_000,
    })
}

/// One frame of the capture-chain analyser.
///
/// Band levels are dBFS, floored, one entry per band, and the three traces are
/// always the same length as `centres_hz`.
#[derive(Debug, Clone)]
pub struct UiSpectrum {
    /// Centre frequency of each band. Sent every frame rather than fetched
    /// once, so the axis and the data can never disagree about how many bands
    /// there are.
    pub centres_hz: Vec<f32>,
    /// The microphone, before any processing.
    pub raw_db: Vec<f32>,
    /// What the noise gate was about to judge.
    pub pre_gate_db: Vec<f32>,
    /// What reached the encoder. Drawn whether or not it was transmitted;
    /// `transmitting` is what says which.
    pub sent_db: Vec<f32>,
    /// Quietest level in the data, for scaling the axis.
    pub floor_db: f32,
    /// How tonal the pre-gate signal is, 0..1.
    pub harmonicity: f32,
    /// Whether the block this frame describes actually went out.
    pub transmitting: bool,
    /// Frame counter. If this stops moving the worker has stopped, which on
    /// screen is indistinguishable from silence unless the reader checks.
    pub seq: u64,
}

/// How a stage of the chain is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StageState {
    /// Switched off, so it has no opinion.
    Off,
    /// Working, and passing audio on.
    Good,
    /// Working, but holding something back.
    Warn,
    /// Stopping audio here.
    Bad,
}

/// The rung a step count names.
///
/// **Walked rather than matched on an index.** The mapping from number to rung
/// is the ladder's business and has already changed twice; a `match` here drew
/// the wrong thing the first time the order moved, and there is no compiler
/// error for a stale number. Walking asks the ladder itself.
fn rung_at(steps: u8) -> mumbleway_core::audio::relief::Relief {
    use mumbleway_core::audio::relief::Relief;
    let mut rung = Relief::None;
    for _ in 0..steps {
        match rung.weaker() {
            Some(next) => rung = next,
            None => break,
        }
    }
    rung
}

/// One stage of the capture chain.
///
/// Carries no prose. The panel is fully localised, and a message composed in
/// Rust would be the one string in it that no translator can reach — so Dart
/// builds the label from `id`, `state` and `value`.
#[derive(Debug, Clone)]
pub struct UiStage {
    /// Stable identifier: `aec`, `rnnoise`, `gate`, `vad`, `harmonicity`,
    /// `agc`, `dehiss`, `feedback`, `profile`, `transmit`.
    pub id: String,
    pub state: StageState,
    /// The one number that stage is about, in whatever unit suits it — dB for
    /// the AEC and the AGC, 0..1 for harmonicity, unused elsewhere.
    pub value: f32,
}

/// The capture chain, stage by stage, as of the last block.
#[derive(Debug, Clone)]
pub struct UiChainStatus {
    /// In order, from the microphone to the wire.
    pub stages: Vec<UiStage>,
    /// Whether voice activation would open right now, whatever mode is set.
    pub would_pass_voice_activated: bool,
    /// Whether audio actually went out on the last block.
    pub transmitting: bool,
    /// Still starting up; nothing above should be believed yet.
    pub warming_up: bool,
    /// Level, the floor under it, and the level needed to open. All dBFS.
    pub level_db: f32,
    pub noise_floor_db: f32,
    pub activation_threshold_db: f32,
    /// The loudest microphone sample seen, in dBFS, and how many have hit full
    /// scale.
    ///
    /// **The only level here measured before the chain touches the block.**
    /// Everything else — including the meter beside the gain slider — is taken
    /// after suppression, which is why an overdriven microphone was invisible
    /// until this existed: the output can sit well below full scale while the
    /// input is pinned at it.
    pub input_peak_db: f32,
    pub input_clipped: u64,
    /// What the clip guard is holding back off the gain slider, in dB. Never
    /// positive, and `0.0` when it is idle.
    ///
    /// **The slider does not move with this, deliberately**, so the panel is
    /// the only place the two can be reconciled. A rider who set +18 on a
    /// microphone that cannot take it has a slider saying +18, a voice quieter
    /// than that implies, and — without this row — nothing at all connecting
    /// the two. It is runtime only: the rider's number is what gets saved, and
    /// this starts at zero every launch.
    pub input_trim_db: f32,
    /// Whether the noise floor is being held down right now, and for how long.
    ///
    /// The floor may not climb while something is speaking, which is what stops
    /// a held phrase dragging its own background estimate onto itself. Shown
    /// because a held floor and a low floor are the same number.
    pub floor_held: bool,
    pub floor_held_ms: u32,
    /// Times the freeze has been overruled by its watchdog this session.
    ///
    /// **Expected to be zero, and worth looking at when it is not.** The freeze
    /// is also triggered by the gate being open, and the gate opens relative to
    /// the floor that is frozen, so the pair can latch; the watchdog breaks it
    /// after a minute. Anything above zero means it had to.
    pub floor_watchdog_trips: u32,
    /// The onset SNR the `Auto` profile was chosen from, in dB — how far the
    /// rider's voice stood above their own background over the first second of
    /// the last phrase. `None` until somebody speaks, and always `None` when
    /// the profile was set by hand.
    ///
    /// Shown next to the profile because it is the evidence for it. A rider on
    /// a motorway reads about 13; in a quiet room, forty-odd.
    pub auto_snr_db: Option<f32>,
    /// How periodic the last block was, 0..1, and the bar in force.
    ///
    /// Drawn beside the spectrum rather than listed under it: a threshold is a
    /// comparison, and the panel could previously only show one side of it.
    /// The bar moves with the profile — Helmet asks for less periodicity, since
    /// it muffles the voice it is judging — so the score alone cannot be read.
    pub harmonicity: f32,
    pub voiced_threshold: f32,
    /// The suppression profile actually in force.
    ///
    /// **Never `Auto`.** Auto is a rule for choosing, not a profile, so what is
    /// in force is always one of the other four — and which one is the only
    /// thing about Auto a rider cannot see anywhere else. Reported whatever the
    /// setting is, because the panel is the place where "what is the chain
    /// doing" is answered and the answer should not depend on how it was
    /// arrived at.
    pub effective_profile: NoiseSetting,
    /// Stage ids the performance ladder has switched off on this device.
    ///
    /// **A stage that is not running reports the same greys and zeroes as one
    /// that is running with nothing to do**, so without this the panel cannot
    /// tell a quiet chain from a crippled one — and neither can a rider. The
    /// ids match [`UiStage::id`]; the panel strikes those names through.
    ///
    /// Ids rather than a rung number, because the mapping from rung to stages
    /// is the ladder's business and it will change as rungs are added.
    pub disabled_stages: Vec<String>,
    /// What the echo canceller worked out about the path it is cancelling.
    ///
    /// **Four numbers because one is not enough to tell working from idle.**
    /// ERLE alone reads the same for a headset with no echo, a filter that
    /// never located the echo, and a filter that located it and failed:
    /// nothing removed. `aec_lag_ms` with `aec_confidence` says whether it
    /// found anything; `aec_spread_ms` against `aec_window_ms` says whether
    /// there is a second arrival outside what the filter reaches.
    pub aec_enabled: bool,
    /// The ladder has it on the half-length filter: 512 taps instead of 1 024,
    /// about 10 ms of echo path instead of 21, for a fifth of the cost.
    ///
    /// **This is the one performance state the canceller has.** It is never
    /// switched off by the ladder — the feedback guard that would cover for it
    /// is given up two rungs lower, so dropping it would leave a speakerphone
    /// with nothing holding the loop open.
    pub aec_shortened: bool,
    pub aec_erle_db: f32,
    pub aec_lag_ms: f32,
    pub aec_confidence: f32,
    pub aec_spread_ms: f32,
    pub aec_window_ms: f32,
    /// Which canceller produced the five numbers above — AEC3, or the
    /// time-domain filter it replaced.
    ///
    /// **They do not mean the same thing for both, so the panel has to say
    /// which.** AEC3's confidence is only ever 0 or 1: it has located the echo
    /// or it has not, and it reports no fraction in between. It measures no
    /// spread at all, because it is partitioned across the whole plausible
    /// range rather than aimed at one arrival — so a spread of `0.0 ms` from it
    /// is the absence of a measurement, not a measurement of absence, and the
    /// panel hides the row rather than printing a number nothing established.
    pub aec3: bool,

    /// Whether the cheap noise model is the one loaded.
    ///
    /// Separate from `enhancer_effort`, which names a rung *within* whichever
    /// model is running — so without this a phone on the cheap model at full
    /// effort read identically to one on the expensive model at full effort,
    /// and toggling the setting changed nothing on screen.
    pub enhancer_simple_model: bool,
    /// How far down the whole-chain ladder this device has gone. 0 is nothing
    /// given up; the panel uses it only to decide whether to warn at all.
    pub relief: u32,
    /// Parts of this panel the ladder has switched off, before it would give
    /// up the enhancer.
    ///
    /// **Booleans rather than a rung number.** The mapping from rung to
    /// consequence is the ladder's business and has already changed twice; a
    /// panel that re-derived it from an index drew the wrong thing the first
    /// time the order moved.
    /// The analyser's bars stop easing down and sit where each frame puts
    /// them. The reading is untouched; only the animation is given up.
    pub analyser_decay_disabled: bool,
    /// Speakers show only that they are talking, not how loudly. The only one
    /// of these rungs visible outside the diagnostics panel, which is why it
    /// is the last of them.
    pub participant_meters_disabled: bool,
    pub analyser_disabled: bool,
    pub classifier_top_disabled: bool,
    pub live_dots_disabled: bool,
    /// The ladder has stopped running the classifier, so `Auto` can no longer
    /// change its mind and the profile is pinned wherever it stood.
    ///
    /// **Distinct from `classifier_top_disabled`**, which only stops drawing
    /// the three rows while the model keeps running for `Auto` to read. This
    /// one stops the inference, and the cost is not cosmetic: a rider who set
    /// `Auto` and rode from a car park onto a motorway will stay on the car
    /// park's profile. The panel has to say so, because every other number on
    /// it looks exactly as it did before.
    pub classifier_disabled: bool,
    /// How hard the speech enhancer is working: 0 full, 1 reduced, 2 ERB only,
    /// 3 bypassed.
    ///
    /// **A rider comparing two phones cannot otherwise tell why one sounds
    /// different.** The enhancer steps itself down on a device that misses the
    /// 10 ms block deadline, and every other number on this panel looks the
    /// same afterwards. An amber dot says something changed; this says what.
    pub enhancer_effort: u32,
}

/// The latest analyser frame, and an ask for the next one.
///
/// **Calling this is what makes the engine do the work.** The analyser is the
/// most expensive thing in the capture chain and worth nothing when nobody is
/// looking, so it runs only while it is being asked for, and the ask expires
/// after half a second. There is deliberately no matching "stop": every
/// explicit stop has a path that misses it — the diagnostics panel is never
/// disposed, the app can be backgrounded, the engine can be restarted — and a
/// missed stop leaves three transforms per block running in a rider's pocket.
///
/// So: poll it while the panel is open, stop when it closes, and the cost stops
/// with it. `None` means no frame has been produced yet.
#[frb(sync)]
pub fn audio_spectrum() -> anyhow::Result<Option<UiSpectrum>> {
    use mumbleway_core::audio::spectrum::{
        SpectrumAnalyser, FLOOR_DB, TAP_PRE_GATE, TAP_RAW, TAP_SENT,
    };

    let shared = &app()?.shared;
    let Some(frame) = shared.take_spectrum() else {
        return Ok(None);
    };

    Ok(Some(UiSpectrum {
        centres_hz: SpectrumAnalyser::band_centres().to_vec(),
        raw_db: frame.bands[TAP_RAW].to_vec(),
        pre_gate_db: frame.bands[TAP_PRE_GATE].to_vec(),
        sent_db: frame.bands[TAP_SENT].to_vec(),
        floor_db: FLOOR_DB,
        harmonicity: frame.harmonicity,
        transmitting: frame.transmitting,
        seq: frame.seq,
    }))
}

/// What the startup performance probe found.
#[derive(Debug, Clone)]
pub struct UiProbe {
    /// The rung the ladder will start at. 0 is the whole chain.
    pub relief: u32,
    /// The block time it was decided on, in microseconds — the second worst of
    /// the run, so one scheduler stall cannot dial a rider down.
    pub worst_us: u32,
    /// The single worst block, which the decision ignored. Shown beside the
    /// other so the panel is not quietly hiding the number it did not use.
    pub outlier_us: u32,
    /// How many rungs were given up.
    pub steps: u32,
    /// The bottom of the ladder still did not fit. The session starts there
    /// because there is nothing further to give.
    pub gave_up: bool,
    /// The expensive speech-enhancement model was timed over its ceiling and
    /// the cheap one was loaded before the ladder was walked at all.
    ///
    /// Worth showing on its own, because it changes how `relief` reads: the
    /// rung beside it is the rung the *cheap* model needed, which is usually a
    /// much better one than the expensive model would have reached.
    pub cheap_model: bool,
    /// What the expensive model measured, in microseconds a block. 0 when there
    /// is no model in this build.
    pub model_us: u32,
}

/// Measures this device against the block deadline and dials the ladder.
///
/// **Deliberately not `#[frb(sync)]`.** It loads the model and runs several
/// hundred blocks through the real chain, which is seconds on a slow phone —
/// so it has to run on a worker thread and not on the platform thread. Call it
/// once while the app is opening.
///
/// The answer is remembered process-wide, so every engine start afterwards
/// begins at the rung this found rather than discovering it again. See
/// `mumbleway_core::audio::probe`.
pub fn audio_probe_chain() -> anyhow::Result<UiProbe> {
    let got = mumbleway_core::audio::probe::probe(mumbleway_core::audio::probe::PROBE_BUDGET_US);
    // Into the app's own log rather than `tracing`, because this is a fact a
    // rider may need to quote back: it is the difference between "this phone
    // was measured and cannot keep up" and "something went wrong on the day".
    mumbleway_core::diag::record(
        mumbleway_core::diag::LogLevel::Info,
        "probe",
        format!(
            "startup probe: rung {} after {} steps, worst {:.1} ms (outlier {:.1} ms){}{}",
            got.rung.index(),
            got.steps,
            got.worst_us as f32 / 1000.0,
            got.outlier_us as f32 / 1000.0,
            if got.cheap_model {
                // Said before the rung is read, because it changes what the
                // rung means: everything after this was measured with the
                // cheap model loaded.
                format!(
                    "; the low-latency model measured {:.1} ms a block and the plain \
                     one was loaded instead",
                    got.model_us as f32 / 1000.0
                )
            } else {
                String::new()
            },
            if got.gave_up {
                ", still over budget at the bottom of the ladder"
            } else {
                ""
            }
        ),
    );
    Ok(UiProbe {
        relief: got.rung.index() as u32,
        worst_us: got.worst_us,
        outlier_us: got.outlier_us,
        steps: got.steps as u32,
        gave_up: got.gave_up,
        cheap_model: got.cheap_model,
        model_us: got.model_us,
    })
}

/// Where each stage of the capture chain stands.
///
/// Free, and always current: the chain publishes this as it runs whether or not
/// anybody is reading. Unlike [`audio_spectrum`] it arms nothing.
#[frb(sync)]
pub fn audio_chain_status() -> anyhow::Result<UiChainStatus> {
    let shared = &app()?.shared;
    let c = shared.chain_status();

    // Thresholds live here rather than in Dart because they are judgements
    // about the audio, not about the display, and they belong beside the values
    // they judge.
    // The echo canceller has four states worth telling apart, and the ERLE
    // alone distinguishes none of them.
    //
    // Nothing removed reads identically whether there is no echo to remove,
    // the filter has not found the one that is there, or it found it and
    // cannot cancel it. The alignment confidence is what separates those: a
    // confident lag with no ERLE is a filter that knows where the echo is and
    // is failing, which is a different problem from one that never located it.
    // The order of these arms is the diagnosis, not a preference: each one is
    // a different reason for the same reading, and the earlier arms are the
    // ones that explain the later ones.
    let aec = if !shared.echo_cancellation_enabled() {
        StageState::Off
    } else if !c.aec3 && c.aec_shortened && c.erle_db < 6.0 {
        // On the short filter and not cancelling much: the two are worth
        // showing together, because the shortened path is a plausible cause
        // and the panel is the only place that connection can be made.
        //
        // Only for the old filter. AEC3 has no tap count and `aec_shortened`
        // is always false for it, but saying so is cheaper than leaving the
        // next reader to work out why the arm cannot fire.
        StageState::Warn
    } else if c.erle_db < 0.0 {
        // Adding rather than subtracting. Should be transient — the old filter
        // backtracks to its last working coefficients and AEC3 has its own
        // divergence handling — so seeing this sit is worth reporting.
        StageState::Bad
    } else if c.aec_confidence < 0.5 {
        // **Has not located the echo**, which for AEC3 is the literal state of
        // its delay estimator rather than a weak correlation. Ordinary on a
        // headset, where there is no acoustic path and nothing to find; a fault
        // only beside a speaker, and the panel cannot tell which this is.
        StageState::Warn
    } else if c.erle_db < 6.0 {
        // Found it and is not removing much of it. On AEC3 this is the state
        // that would have said something on build 123, where the canceller was
        // confident and removing 0.2 dB.
        StageState::Warn
    } else {
        StageState::Good
    };

    // **In the order the chain runs them**, which is not the order they were
    // in and not an arrangement anyone should have to reconstruct from the
    // code. The panel draws this list left to right and a rider reads it as a
    // journey from the microphone to the wire, so an out-of-order dot does not
    // look wrong, it looks like the chain works differently than it does.
    //
    // Verified against the source, not remembered:
    //
    // | # | stage | where |
    // |---|---|---|
    // | 1 | aec | `engine.rs`, before the enhancer — see the note there |
    // | 2 | enhancer | `engine.rs`, on what the canceller left |
    // | 3 | rnnoise | `denoise.rs` step 2, after the rumble filter |
    // | 4 | vad | `denoise.rs` step 4, the speech decision |
    // | 5 | gate | `denoise.rs` step 5 |
    // | 6 | agc | `denoise.rs` step 6, with the limiter |
    // | 7 | feedback | `engine.rs`, after the processor returns |
    // | 8 | dehiss | `engine.rs`, straight after the feedback guard |
    // | 9 | transmit | the encoder |
    //
    // Two were wrong when this list was first written. **The enhancer was
    // second from last and ran first** — the largest stage in the chain, shown
    // after everything it preceded. And de-hiss was listed before the feedback
    // guard, where the guard runs first.
    //
    // The canceller has since moved ahead of the enhancer, so the first two
    // have swapped again. `engine.rs` says why; the short version is that an
    // adaptive filter cannot learn a room through a neural mask.
    //
    // `background` is not a stage at all: no audio passes through the
    // classifier. It sits before `transmit` because that is where it stopped
    // being confusing, not because anything flows through it.
    let stages = vec![
        UiStage {
            id: "aec".into(),
            state: aec,
            value: c.erle_db,
        },
        UiStage {
            id: "enhancer".into(),
            // Four states worth telling apart. Green: enhancing at full
            // effort. Amber: still enhancing, but stepped down because this
            // phone could not return a frame inside 10 ms — which sounds
            // different and is a fact about the device, so it must not read as
            // green. Red: it stepped all the way down to pass-through. Grey:
            // it never loaded at all, which is a build problem rather than
            // theirs.
            state: if c.enhancer_on && c.enhancer_effort == 0 {
                StageState::Good
            } else if c.enhancer_on {
                StageState::Warn
            } else if c.enhancer_gave_up {
                StageState::Bad
            } else {
                StageState::Off
            },
            // Worst frame in milliseconds, against a 10 ms budget. The mean
            // would hide exactly the frame that matters.
            value: c.enhancer_worst_us as f32 / 1000.0,
        },
        UiStage {
            id: "rnnoise".into(),
            state: if c.profile == 0 {
                StageState::Off
            } else if c.warming_up {
                StageState::Warn
            } else {
                StageState::Good
            },
            value: 0.0,
        },
        UiStage {
            id: "vad".into(),
            // Which half failed is the whole point: both agreeing is speech,
            // one agreeing is the interesting middle, neither is silence.
            state: match (c.vad_says_speech, c.snr_says_speech) {
                (true, true) => StageState::Good,
                (false, false) => StageState::Bad,
                _ => StageState::Warn,
            },
            value: c.level_db - c.noise_floor_db,
        },
        UiStage {
            id: "gate".into(),
            state: if c.gate_open {
                StageState::Good
            } else {
                StageState::Bad
            },
            value: c.activation_threshold_db,
        },
        UiStage {
            id: "agc".into(),
            state: if c.profile == 0 {
                StageState::Off
            } else if c.agc_gain_db.abs() >= 6.0 {
                StageState::Warn
            } else {
                StageState::Good
            },
            value: c.agc_gain_db,
        },
        UiStage {
            id: "feedback".into(),
            state: if c.feedback_mode == 0 {
                StageState::Off
            } else {
                StageState::Good
            },
            value: 0.0,
        },
        UiStage {
            id: "dehiss".into(),
            state: if c.dehiss_mode == 0 {
                StageState::Off
            } else {
                StageState::Good
            },
            value: 0.0,
        },
        UiStage {
            id: "background".into(),
            // Grey when nothing is classifying, which is a real and common
            // state — desktop, the setting off, or a profile chosen by hand —
            // and must not read as "the background is clear". Amber while the
            // hold runs, because that is when it is affecting something.
            state: match (shared.background_noisy(), c.music_hold) {
                (None, _) => StageState::Off,
                (_, true) => StageState::Warn,
                (Some(true), _) => StageState::Warn,
                (Some(false), _) => StageState::Good,
            },
            value: 0.0,
        },
        UiStage {
            id: "transmit".into(),
            state: if c.muted {
                StageState::Off
            } else if c.transmitting {
                StageState::Good
            } else if c.would_pass_voice_activated {
                // Speech got all the way here and the mode stopped it — the
                // rider is on push-to-talk and is not pressing, most likely.
                StageState::Warn
            } else {
                StageState::Bad
            },
            value: 0.0,
        },
    ];

    Ok(UiChainStatus {
        stages,
        aec_enabled: c.aec_enabled,
        aec_shortened: c.aec_shortened,
        aec_erle_db: c.erle_db,
        aec_lag_ms: c.aec_lag_ms,
        aec_confidence: c.aec_confidence,
        aec_spread_ms: c.aec_spread_ms,
        aec_window_ms: c.aec_window_ms,
        aec3: c.aec3,
        would_pass_voice_activated: c.would_pass_voice_activated,
        transmitting: c.transmitting,
        warming_up: c.warming_up,
        level_db: c.level_db,
        noise_floor_db: c.noise_floor_db,
        activation_threshold_db: c.activation_threshold_db,
        effective_profile: from_profile_index(c.profile),
        // Which dot to strike through, from the rung. The pitch search has no
        // dot of its own — it feeds the voiced relief rather than a decision a
        // rider can see — so it is named in the warning text instead.
        disabled_stages: {
            let rung = rung_at(c.relief);
            let mut off: Vec<String> = Vec::new();
            if rung.skip_feedback() {
                off.push("feedback".into());
            }
            if rung.skip_rnnoise() {
                off.push("rnnoise".into());
            }
            // Only when it is genuinely not running. The middle rungs still
            // enhance, and striking the name through would say otherwise.
            if c.enhancer_gave_up {
                off.push("enhancer".into());
            }
            off
        },
        relief: c.relief as u32,
        analyser_decay_disabled: rung_at(c.relief).skip_analyser_decay(),
        participant_meters_disabled: rung_at(c.relief).skip_participant_meters(),
        analyser_disabled: rung_at(c.relief).skip_analyser(),
        classifier_top_disabled: rung_at(c.relief).skip_classifier_top(),
        classifier_disabled: rung_at(c.relief).skip_classifier(),
        live_dots_disabled: rung_at(c.relief).skip_live_dots(),
        enhancer_effort: c.enhancer_effort as u32,
        enhancer_simple_model: c.enhancer_simple_model,
        input_peak_db: {
            let (peak, _) = shared.input_peak();
            if peak > 0.0 {
                20.0 * peak.log10()
            } else {
                -120.0
            }
        },
        input_clipped: shared.input_peak().1,
        input_trim_db: c.input_trim_db,
        floor_held: c.floor_held,
        floor_held_ms: c.floor_held_ms,
        floor_watchdog_trips: c.floor_watchdog_trips,
        auto_snr_db: c.auto_snr_db,
        harmonicity: c.harmonicity,
        voiced_threshold: c.voiced_threshold,
    })
}

/// A window of raw microphone audio for the background classifier.
#[derive(Debug, Clone)]
pub struct UiWaveform {
    /// 15 600 samples at 16 kHz — 0.975 s — which is the size YAMNet was built
    /// for. Crosses as a `Float32List`, so it is one 62 kB copy every few
    /// seconds rather than anything per block.
    pub samples: Vec<f32>,
    /// Increments per window. Not moving means the worker stopped, which to a
    /// classifier is indistinguishable from a very quiet ride.
    pub seq: u64,
}

/// The latest window of microphone audio, and an ask for the next one.
///
/// **Calling this is what makes the engine collect it.** Same self-expiring
/// arrangement as [`audio_spectrum`] and for a stronger reason: what reads this
/// runs a neural network, so a tap left running for a caller that stopped
/// asking is battery spent on nothing. The ask lasts five seconds.
///
/// `None` means no whole window is ready — either nothing has been collected
/// yet, or the last one has already been taken. A partly filled window is never
/// offered: it would be a fragment of a ride padded with silence, and the model
/// would classify the padding.
#[frb(sync)]
pub fn audio_waveform() -> anyhow::Result<Option<UiWaveform>> {
    let frame = match app()?.shared.take_waveform() {
        Some(f) => f,
        None => return Ok(None),
    };
    Ok(Some(UiWaveform {
        samples: frame.samples.to_vec(),
        seq: frame.seq,
    }))
}

/// Tells the chain what the background classifier concluded.
///
/// A supporting vote for `Helmet`, consulted only when the rider has chosen
/// `Auto`, and never anywhere near the transmit decision. Being wrong about a
/// profile costs some naturalness; being wrong at the gate cuts a rider off.
#[frb(sync)]
pub fn set_background_noisy(noisy: bool) -> anyhow::Result<()> {
    app()?.shared.set_background_noisy(noisy);
    Ok(())
}

/// Forgets the classifier's last word, when it stops running.
///
/// Not the same as reporting a clear background, and the difference is the
/// whole reason this exists: a verdict that stopped being updated would pin
/// `Helmet` for the rest of the session.
#[frb(sync)]
pub fn clear_background_noisy() -> anyhow::Result<()> {
    app()?.shared.clear_background_noisy();
    Ok(())
}

/// Tells the chain whether the classifier can hear a voice right now.
///
/// **This one does reach the gate**, indirectly and in one direction only: it
/// decides whether the noise floor may keep climbing, and the gate opens
/// relative to that floor. It can hold the floor down but never push it up, so
/// the worst it can do is leave the gate open on something that is not speech.
/// The failure it exists to prevent is the opposite one, and worse: a floor
/// that climbed onto a held phrase and cut the middle out of it.
#[frb(sync)]
pub fn set_classifier_voice(voice: bool) -> anyhow::Result<()> {
    app()?.shared.set_classifier_voice(voice);
    Ok(())
}

/// Forgets it, when the classifier stops running.
///
/// The chain then falls back to its own per-block opinion, which is the right
/// behaviour and not the same as being told there is no voice.
#[frb(sync)]
pub fn clear_classifier_voice() -> anyhow::Result<()> {
    app()?.shared.clear_classifier_voice();
    Ok(())
}

/// The profile index the chain publishes, back into the FFI enum.
///
/// By index rather than by importing the core enum's `TryFrom`, because there
/// is none — the chain stores a `u8` so the status struct can be `Copy` and
/// written under a lock every block. Anything unexpected reads as `Standard`:
/// this drives a label, and a label that says the middle profile is a smaller
/// lie than one that says suppression is off.
fn from_profile_index(i: u8) -> NoiseSetting {
    match i {
        0 => NoiseSetting::Off,
        1 => NoiseSetting::Light,
        3 => NoiseSetting::Helmet,
        _ => NoiseSetting::Standard,
    }
}

/// Whether a diagnostic recording is running, and how it is doing.
#[derive(Debug, Clone)]
pub struct UiRecordingState {
    pub active: bool,
    /// Blocks storage could not keep up with. Shown rather than hidden: a
    /// recording with gaps is still useful, and a recording with gaps nobody
    /// knows about is a measurement waiting to be wrong.
    pub dropped_blocks: u64,
    /// Where the files are, so the panel can offer to share them.
    pub directory: String,
}

/// Starts recording the microphone and what the chain decided about it.
///
/// **This writes the rider's microphone to storage.** It exists because every
/// measurement in this project was invalidated at once by discovering the
/// recordings behind it came from the phone's own microphone rather than the
/// headset's, and no amount of care in the analysis could have caught that.
/// Recording inside the app makes the audio the chain's own input by
/// construction.
///
/// Off unless asked for, started only from the diagnostics panel, and the
/// directory comes from the caller because only the Dart side knows where a
/// given platform lets an app put files a person can later get at.
#[frb(sync)]
pub fn start_diagnostic_recording(directory: String, tag: String) -> anyhow::Result<()> {
    app()?
        .shared
        .start_diagnostic_recording(std::path::Path::new(&directory), &tag)?;
    Ok(())
}

/// Stops the recording and closes the files, returning the blocks that were
/// dropped because storage could not keep up.
///
/// Waits for the writer to flush. That is a few milliseconds and it is not
/// optional: the next thing that happens is a rider sharing the file, and a
/// file still held open shares as a truncated one.
#[frb(sync)]
pub fn stop_diagnostic_recording() -> anyhow::Result<u64> {
    Ok(app()?.shared.stop_diagnostic_recording())
}

/// Where the recording stands. Free to call, and safe before the engine is up.
#[frb(sync)]
pub fn diagnostic_recording_state() -> UiRecordingState {
    // Deliberately not `app()?`: the panel asks this on every rebuild, and
    // before the engine exists the honest answer is "not recording" rather than
    // an error the interface has to render.
    let Ok(app) = app() else {
        return UiRecordingState {
            active: false,
            dropped_blocks: 0,
            directory: String::new(),
        };
    };
    let s = app.shared.diagnostic_recording_state();
    UiRecordingState {
        active: s.active,
        dropped_blocks: s.dropped_blocks,
        directory: s.directory,
    }
}

/// Everything the engine has logged so far.
///
/// The stream only carries lines recorded after the UI attached to it, and the
/// interesting ones — why the audio device would not open, what the first
/// connect said — are written before that. This fetches those. Deliberately not
/// gated on the engine being up: when startup is what failed, this is the only
/// place the reason exists.
#[frb(sync)]
pub fn recent_logs() -> Vec<UiLogEntry> {
    diag::snapshot().into_iter().map(UiLogEntry::from).collect()
}

/// Empties the log, so a reproduction attempt starts from a clean sheet.
#[frb(sync)]
pub fn clear_logs() {
    diag::clear();
}

/// Sounds an arrival or a departure from the channel.
///
/// Driven from the roster rather than the audio path, because someone joining
/// makes no sound of their own — which is exactly why it needs a cue.
#[frb(sync)]
pub fn play_participant_cue(joined: bool) -> anyhow::Result<()> {
    app()?.shared.play_cue(if joined {
        AudioCue::ParticipantJoined
    } else {
        AudioCue::ParticipantLeft
    });
    Ok(())
}

/// Asks for the cheap speech-enhancement model outright.
///
/// **Not an engine setting, which is why it takes no `app()`.** It has to be
/// set before the startup probe runs, and the probe runs while the app is
/// opening — before any engine exists. It is read by every enhancer built
/// afterwards: the worker's, the probe's, and the listen sheet's preview
/// chain.
///
/// Orthogonal to the performance ladder on purpose. The ladder's own
/// `SimpleModel` rung sits at the bottom, below giving up the pitch search,
/// RNNoise and the panel; a rider choosing this wants the opposite — to spend
/// what the cheaper model saves on *keeping* those. See
/// `mumbleway_core::audio::deepfilter`.
#[frb(sync)]
pub fn set_simple_model(on: bool) -> anyhow::Result<()> {
    mumbleway_core::audio::deepfilter::set_force_simple_model(on);
    Ok(())
}

#[frb(sync)]
pub fn is_simple_model() -> anyhow::Result<bool> {
    Ok(mumbleway_core::audio::deepfilter::force_simple_model())
}

/// A short room tail under incoming voices, so a gated talker does not stop
/// like a switch being thrown.
#[frb(sync)]
pub fn set_reverb(on: bool) -> anyhow::Result<()> {
    app()?.shared.set_reverb(on);
    Ok(())
}

#[frb(sync)]
pub fn is_reverb_enabled() -> anyhow::Result<bool> {
    Ok(app()?.shared.reverb_enabled())
}

/// Levels incoming speakers towards a common loudness.
#[frb(sync)]
pub fn set_level_normalisation(on: bool) -> anyhow::Result<()> {
    app()?.shared.set_normalise_levels(on);
    Ok(())
}

#[frb(sync)]
pub fn is_level_normalisation_enabled() -> anyhow::Result<bool> {
    Ok(app()?.shared.normalise_levels_enabled())
}

/// How much incoming audio to hold back before playing it, in milliseconds.
///
/// The trade the rider is making is delay against dropouts, and which way it
/// should go is a property of the network they are on rather than of the app.
/// Rounded to whole 20 ms packets, which is the unit voice arrives in.
#[frb(sync)]
pub fn set_jitter_buffer_ms(ms: u32) -> anyhow::Result<()> {
    app()?.shared.set_jitter_buffer_ms(ms);
    Ok(())
}

#[frb(sync)]
pub fn jitter_buffer_ms() -> anyhow::Result<u32> {
    Ok(app()?.shared.jitter_buffer_ms())
}

/// The range the setting above accepts, as `(minimum, maximum, step)` in ms.
///
/// Reported rather than written into the interface twice: the bounds come from
/// the buffer's own frame arithmetic, and a slider that let somebody pick a
/// value the engine then silently rounded would be lying about what it set.
#[frb(sync)]
pub fn jitter_buffer_bounds_ms() -> (u32, u32, u32) {
    (
        (mumbleway_core::audio::MIN_TARGET_FRAMES * 20) as u32,
        (mumbleway_core::audio::MAX_TARGET_FRAMES * 20) as u32,
        20,
    )
}

/// Acoustic echo cancellation, applied to the microphone before anything else.
#[frb(sync)]
pub fn set_echo_cancellation(on: bool) -> anyhow::Result<()> {
    app()?.shared.set_echo_cancellation(on);
    Ok(())
}

#[frb(sync)]
pub fn is_echo_cancellation_enabled() -> anyhow::Result<bool> {
    Ok(app()?.shared.echo_cancellation_enabled())
}

/// What to do about the speaker being heard by the microphone.
///
/// Distinct approaches rather than strengths of one: see `audio::feedback`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeedbackGuardMode {
    Off,
    Duck,
    HowlGuard,
    Residual,
}

/// How the microphone opens. Takes effect on the next block, not the next
/// launch — which is what it used to do, silently.
#[frb(sync)]
pub fn set_mic_mode(mode: MicMode) -> anyhow::Result<()> {
    app()?.shared.set_transmit_mode(to_transmit(mode));
    Ok(())
}

/// How hard the noise suppressor works. Also live rather than at launch.
#[frb(sync)]
pub fn set_noise(noise: NoiseSetting) -> anyhow::Result<()> {
    app()?.shared.set_noise_profile(to_profile(noise));
    Ok(())
}

/// Applied after the echo canceller, to whatever it could not model.
#[frb(sync)]
pub fn set_feedback_guard(mode: FeedbackGuardMode) -> anyhow::Result<()> {
    app()?.shared.set_feedback_mode(match mode {
        FeedbackGuardMode::Off => FeedbackMode::Off,
        FeedbackGuardMode::Duck => FeedbackMode::Duck,
        FeedbackGuardMode::HowlGuard => FeedbackMode::HowlGuard,
        FeedbackGuardMode::Residual => FeedbackMode::Residual,
    });
    Ok(())
}

/// How to deal with the steady hiss a microphone adds under speech.
///
/// Separate from noise suppression, which handles the road and the wind. Those
/// are loud and change with speed; hiss is quiet, high and unvarying, and the
/// two want opposite treatments.
pub enum DehissOption {
    /// Change nothing. The default, because both of the others discard
    /// something and a voice link that is working should be left alone.
    Off,
    /// Turns quiet passages down further, in proportion to how quiet they are.
    /// Cannot make speech sound synthetic; can make the floor breathe.
    Expander,
    /// Learns the noise spectrum while nobody talks and subtracts it per
    /// frequency. Removes hiss from under speech as well as between words; the
    /// price is a faint flicker in the gaps if it is pushed hard.
    Spectral,
}

#[frb(sync)]
pub fn set_dehiss(mode: DehissOption) -> anyhow::Result<()> {
    app()?.shared.set_dehiss_mode(match mode {
        DehissOption::Off => DehissMode::Off,
        DehissOption::Expander => DehissMode::Expander,
        DehissOption::Spectral => DehissMode::Spectral,
    });
    Ok(())
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
pub fn set_user_local_mute(server_id: String, session: u32, muted: bool) -> anyhow::Result<()> {
    send_command(
        server_id,
        SessionCommand::SetUserLocalMute { session, muted },
    )
}

/// Silences another user for everyone. Requires the Mute permission; without it
/// the server replies with a permission-denied message that surfaces as text.
pub fn set_user_server_mute(server_id: String, session: u32, muted: bool) -> anyhow::Result<()> {
    send_command(
        server_id,
        SessionCommand::SetUserServerMute { session, muted },
    )
}

/// Deafens another user server-side. Also permission-gated.
pub fn set_user_server_deaf(server_id: String, session: u32, deaf: bool) -> anyhow::Result<()> {
    send_command(
        server_id,
        SessionCommand::SetUserServerDeaf { session, deaf },
    )
}

/// Channel to join automatically on every future connect. `None` clears it.
pub fn set_default_channel(server_id: String, channel: Option<String>) -> anyhow::Result<()> {
    send_command(server_id, SessionCommand::SetDefaultChannel(channel))
}

/// Removes a user from the server. Requires the Kick permission; without it the
/// server answers with a permission-denied message that arrives as text.
///
/// This is a kick, not a ban — they may reconnect immediately.
pub fn kick_user(server_id: String, session: u32, reason: String) -> anyhow::Result<()> {
    send_command(server_id, SessionCommand::KickUser { session, reason })
}

// ---------------------------------------------------------------------------
// Sharing
// ---------------------------------------------------------------------------

/// Builds a `mumble://` invite link for a server and channel.
///
/// `include_password` is a deliberate choice by the caller: a link carrying a
/// password grants access to anyone who ever sees it, including whatever chat
/// app it travels through.
pub fn build_invite_link(
    config: ServerConfig,
    channel: Option<String>,
    include_password: bool,
) -> String {
    let profile = config_to_profile(config);
    mumbleway_core::session::profile::build_url(&profile, channel.as_deref(), include_password)
}

/// Builds the same invitation as an ordinary https link.
///
/// **This is the one to send somebody.** A `mumble://` link does not survive a
/// messaging app: Telegram is inconsistent about making one tappable at all,
/// and when it does, tapping opens its in-app browser, which tries to load the
/// scheme as a web address and fails. Both were measured on a device rather
/// than assumed. Every messenger linkifies https, and Android's App Links hand
/// a verified https URL to the app instead of to a browser.
///
/// The `mumble://` form above stays for the places it is better: a QR code a
/// phone's camera app can act on with no network, and anything expecting what
/// the official client registers.
pub fn build_invite_web_link(
    config: ServerConfig,
    channel: Option<String>,
    include_password: bool,
) -> String {
    let profile = config_to_profile(config);
    mumbleway_core::session::profile::build_web_url(&profile, channel.as_deref(), include_password)
}

/// Builds a shareable JSON profile file for one server.
pub fn build_invite_file(
    config: ServerConfig,
    channel: Option<String>,
    include_password: bool,
) -> anyhow::Result<String> {
    let profile = config_to_profile(config);
    mumbleway_core::session::profile::build_json(&profile, channel.as_deref(), include_password)
        .map_err(|e| anyhow::anyhow!(e.to_string()))
}

/// Builds a JSON file containing every supplied server, for backup or transfer.
pub fn export_servers(configs: Vec<ServerConfig>) -> anyhow::Result<String> {
    let entries: Vec<mumbleway_core::session::profile::ProfileFileEntry> = configs
        .into_iter()
        .map(|c| mumbleway_core::session::profile::ProfileFileEntry {
            host: c.host,
            name: Some(c.name),
            port: Some(c.port),
            username: Some(c.username),
            password: c.password,
            channel: c.default_channel,
            // Pinned fingerprints stay on the device that made the trust
            // decision; exporting them would launder it onto another machine.
            cert_fingerprint: None,
        })
        .collect();

    serde_json::to_string_pretty(&entries)
        .map_err(|e| anyhow::anyhow!("could not build the export: {e}"))
}

/// Converts the Dart-facing config into the core's profile type.
fn config_to_profile(c: ServerConfig) -> ServerProfile {
    let mut p = ServerProfile::new(c.name, c.host, c.port, c.username);
    p.password = c.password;
    p.cert_fingerprint = c.cert_fingerprint;
    p.auto_join_channel = c.default_channel;
    if !c.id.trim().is_empty() {
        p.id = c.id;
    }
    p
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
            default_channel: p.auto_join_channel,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slot_map() -> Arc<Mutex<HashMap<String, u16>>> {
        Arc::new(Mutex::new(HashMap::new()))
    }

    #[test]
    fn two_servers_never_share_an_audio_slot() {
        let slots = slot_map();
        assert_eq!(allocate_slot(&slots, "a"), 0);
        assert_eq!(allocate_slot(&slots, "b"), 1);

        // The failure this replaced. Slots are handed back on disconnect, so
        // counting the map gave the reconnecting server the number the one
        // still connected was already using — and the levels for both were
        // then attributed to whichever the map yielded first, leaving the
        // other server's meters at silence for the whole call.
        slots.lock().remove("a");
        assert_eq!(
            allocate_slot(&slots, "a"),
            0,
            "a reconnecting server took a slot that was still in use"
        );

        let taken: Vec<u16> = {
            let map = slots.lock();
            let mut v: Vec<u16> = map.values().copied().collect();
            v.sort_unstable();
            v
        };
        assert_eq!(taken, vec![0, 1]);
    }

    #[test]
    fn asking_twice_gives_the_same_slot() {
        // Reconnecting without disconnecting first must not consume a second
        // number, or the streams already filed under the old one are orphaned.
        let slots = slot_map();
        let first = allocate_slot(&slots, "a");
        assert_eq!(allocate_slot(&slots, "a"), first);
        assert_eq!(slots.lock().len(), 1);
    }

    #[test]
    fn a_released_slot_is_reused_rather_than_left_as_a_hole() {
        let slots = slot_map();
        for id in ["a", "b", "c"] {
            allocate_slot(&slots, id);
        }
        slots.lock().remove("b");
        assert_eq!(
            allocate_slot(&slots, "d"),
            1,
            "the lowest free slot should be taken before a new one"
        );
    }

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
    fn waiting_covers_the_gap_between_attempts_too() {
        // The silence while waiting for the next attempt is the part that most
        // needs the cue: from the rider's side it is the same situation as an
        // attempt in progress, and silence there reads as having given up.
        for s in [
            ConnStatus::Connecting,
            ConnStatus::Handshaking,
            ConnStatus::Reconnecting,
        ] {
            assert!(is_waiting(s), "{s:?} should keep the cue going");
        }
        for s in [
            ConnStatus::Idle,
            ConnStatus::Connected,
            ConnStatus::Disconnected,
            ConnStatus::Failed,
        ] {
            assert!(!is_waiting(s), "{s:?} should not keep the cue going");
        }
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
    fn dialing_plays_for_a_deliberate_connect_only() {
        // A connect the user asked for.
        for prev in [
            None,
            Some(ConnStatus::Idle),
            Some(ConnStatus::Disconnected),
            Some(ConnStatus::Failed),
        ] {
            assert_eq!(
                cue_for_transition(prev, ConnStatus::Connecting),
                Some(AudioCue::Dialing),
                "expected dialing from {prev:?}"
            );
        }

        // Automatic retries pass through Connecting constantly during a bad
        // stretch of road; beeping on each one would be maddening.
        assert_eq!(
            cue_for_transition(Some(ConnStatus::Reconnecting), ConnStatus::Connecting),
            None
        );
    }

    #[test]
    fn moderation_cues_prefer_deafening_over_muting() {
        // Losing the ability to hear matters more than losing the microphone,
        // so when both change at once that is the one reported.
        assert_eq!(
            cue_for_moderation(Some(true), Some(true)),
            Some(AudioCue::DeafenedByOther)
        );
        assert_eq!(
            cue_for_moderation(Some(false), Some(false)),
            Some(AudioCue::UndeafenedByOther)
        );

        assert_eq!(
            cue_for_moderation(Some(true), None),
            Some(AudioCue::MutedByOther)
        );
        assert_eq!(
            cue_for_moderation(Some(false), None),
            Some(AudioCue::UnmutedByOther)
        );
        assert_eq!(cue_for_moderation(None, None), None);
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

/// Feeds the output a stretch of a recording being previewed.
///
/// The transport is on the Dart side deliberately. Previewing means reading a
/// file, and a file read has no business anywhere near the audio thread — so
/// what crosses this boundary is decoded samples, and this end is a queue.
///
/// Returns how many were accepted, which is fewer than offered once the queue
/// is full. The caller uses that to pace itself rather than to discover later
/// that its playhead has drifted from what anybody heard.
#[frb(sync)]
pub fn preview_push(samples: Vec<f32>) -> anyhow::Result<u32> {
    Ok(app()?.shared.preview_push(&samples) as u32)
}

/// Samples still waiting to be heard.
///
/// The playhead is what was pushed minus this. It is the only honest source
/// for it: the queue drains at the speaker's rate, and a timer counting
/// forwards from "play" would run ahead the moment the device buffered.
#[frb(sync)]
pub fn preview_queued() -> anyhow::Result<u32> {
    Ok(app()?.shared.preview_queued() as u32)
}

/// The same, but through the capture chain, so a listener hears what the
/// others would have heard rather than what the microphone picked up.
///
/// Sync, and cheap: the chain lives on a thread of its own and this only hands
/// the samples over. The first call starts that thread, which then spends
/// seconds loading a model on a low-end phone — but it does that on its own
/// time, and [`preview_queued`] counts what it is holding, so the transport
/// waits for it instead of pushing the whole file at an empty queue.
#[frb(sync)]
pub fn preview_push_processed(samples: Vec<f32>) -> anyhow::Result<u32> {
    Ok(app()?.shared.preview_push_processed(&samples) as u32)
}

/// Throws away the preview chain, so the next listen starts clean.
///
/// Every stage in it adapts, and a seek jumps to unrelated audio: without
/// this, a noise floor learned from a motorway would be applied to a stretch
/// of speech in a room.
#[frb(sync)]
pub fn preview_reset_chain() -> anyhow::Result<()> {
    app()?.shared.preview_reset_chain();
    Ok(())
}

/// Stops a preview, and is also how a seek starts.
#[frb(sync)]
pub fn preview_clear() -> anyhow::Result<()> {
    app()?.shared.preview_clear();
    Ok(())
}
