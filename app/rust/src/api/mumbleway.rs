//! Flutter-facing API.
//!
//! Everything here is deliberately plain data: `flutter_rust_bridge` mirrors
//! these types into Dart, so they avoid lifetimes, generics and borrowed data.
//! All real work happens on a background Tokio runtime owned by [`App`], and the
//! UI observes it through a single event stream.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
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
use mumbleway_core::session::peers::{MuteCueEcho, RemoteMuteDecision, RemoteMuteGuard};
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
/// Which language a proxy speaks.
///
/// Named `ProxyScheme` rather than anything with "config" in it: the app
/// already has a `ProxyConfig` for the downloads proxy, in a file the state
/// object imports alongside this one, and the bridge mirrors these names into
/// Dart verbatim.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProxyScheme {
    /// `CONNECT host:port`, which is what most HTTP(S) proxies offer.
    HttpConnect,
    /// SOCKS5, with a username and password where the proxy asks for them.
    Socks5,
}

/// A proxy to dial a server through, as the app holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerProxy {
    pub scheme: ProxyScheme,
    pub host: String,
    pub port: u16,
    pub username: Option<String>,
    pub password: Option<String>,
    /// Carry voice through it as well, rather than sending it direct over UDP.
    pub tunnel_voice: bool,
}

impl ServerProxy {
    fn from_spec(spec: mumbleway_core::net::ProxySpec) -> Self {
        Self {
            scheme: match spec.kind {
                mumbleway_core::net::ProxyKind::HttpConnect => ProxyScheme::HttpConnect,
                mumbleway_core::net::ProxyKind::Socks5 => ProxyScheme::Socks5,
            },
            host: spec.host,
            port: spec.port,
            username: spec.username,
            password: spec.password,
            tunnel_voice: spec.tunnel_voice,
        }
    }

    fn into_spec(self) -> mumbleway_core::net::ProxySpec {
        mumbleway_core::net::ProxySpec {
            kind: match self.scheme {
                ProxyScheme::HttpConnect => mumbleway_core::net::ProxyKind::HttpConnect,
                ProxyScheme::Socks5 => mumbleway_core::net::ProxyKind::Socks5,
            },
            host: self.host,
            port: self.port,
            username: self.username,
            password: self.password,
            tunnel_voice: self.tunnel_voice,
        }
    }
}

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
    /// Access tokens to present at the handshake.
    ///
    /// Carried with the server rather than typed each time: a token is how a
    /// shut channel opens, and a rider who has to retype one at a junction
    /// does not have it.
    pub access_tokens: Vec<String>,
    /// Proxies to reach this server through, outermost first.
    ///
    /// **Already resolved.** "Use the app's default" is a choice the rider
    /// makes and the Dart side answers before it gets here, so this is a plain
    /// list: nothing in the engine has to know a default exists.
    pub proxy_chain: Vec<ServerProxy>,
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
    /// Muted by somebody else, for everyone: an admin's decision, not theirs.
    ///
    /// **These used to be `mute || self_mute`, and that hid the admin.** A
    /// moderator muting a rider who runs MumbleWay sends two things — the
    /// server mute and a request to their app, which closes their microphone
    /// itself — so `self_mute` comes back set on almost every imposed mute
    /// there is. A reader that took the two together could not tell whose
    /// decision it was, and the one that matters is the one the rider cannot
    /// undo by changing their mind. Anything that wants "can they be heard at
    /// all" takes the two fields together, which is a line of code, where
    /// telling them apart afterwards is impossible.
    pub muted: bool,
    /// Deafened by an admin. Their own choice is [`UiUser::self_deafened`].
    pub deafened: bool,
    /// Their own hand: they closed their microphone, or their ears.
    ///
    /// **The roster says different things about them.** A rider who closed
    /// their own microphone has made a decision; one an admin closed has had
    /// one made for them, and a rider who turned their own sound off cannot
    /// hear anybody — which is worth knowing before talking to them.
    pub self_muted: bool,
    pub self_deafened: bool,
    /// Silenced by us alone. Needs no permission and is invisible to others.
    pub local_mute: bool,
    /// The account this server knows them by, or `None` if they have none.
    ///
    /// **Presence is registration**, which is what the roster's menu needs:
    /// offering "register" to somebody who already has an account is an action
    /// that can only fail, and offering "unregister" to somebody who has none
    /// is the same. `Some(0)` is SuperUser, whose id really is zero.
    pub user_id: Option<u32>,
    /// Whether this rider has told us they have silenced *us* for themselves.
    ///
    /// The mirror of [`UiUser::local_mute`], and the only way that mute is
    /// ever visible from the other end: the server is not party to it, so
    /// being dropped by somebody looks exactly like being heard by them. Their
    /// client says so over the MumbleWay handshake's channel, which makes this
    /// the one field here resting on another client's honesty rather than on
    /// the server's word.
    pub muted_you: bool,
    /// One word for the roster: talking, silent, muted, deafened, muted for you.
    pub status: String,
    /// The MumbleWay version this user's client reported, or `None` if it has
    /// not identified itself as MumbleWay.
    ///
    /// **`None` is not "does not run MumbleWay".** A build from before the
    /// handshake says nothing, and on a server older than 1.4.0 nobody can say
    /// anything. For our own row it is set only where the handshake can run, so
    /// a badge on ourselves means everybody else's badges mean something too.
    pub mumbleway_version: Option<String>,
    /// Whether the server ducks everybody else while this rider talks.
    pub priority_speaker: bool,
    /// Silenced by the server because they lack Speak permission here.
    ///
    /// Not a mute anybody chose, and not visible from anything else: the
    /// server discards their voice without refusing anything.
    pub suppressed: bool,
    /// The note this rider hung beside their name, as plain text. Empty when
    /// they have none; the markup Mumble's own client writes is stripped in the
    /// core, so this is safe to put straight on screen.
    pub comment: String,
    /// How this rider's connection is doing, as the *server* measures it.
    ///
    /// `None` until the first stats reply, and for anybody outside our own
    /// channel — the server only reports loss to people standing in the same
    /// one. See `session::quality`.
    pub quality: Option<UiQuality>,
}

/// A menu entry this server registered.
///
/// The label is the server's own words, in whatever language it chose; nothing
/// here translates it, and nothing here knows what the action does.
#[derive(Debug, Clone)]
pub struct UiContextAction {
    /// Sent back when it is picked. Opaque, and never rewritten.
    pub action: String,
    pub label: String,
    /// Where the server said it belongs.
    pub for_user: bool,
    pub for_channel: bool,
    pub for_server: bool,
}

/// One rule in a channel's access list.
///
/// `grant` and `deny` are bitmasks of Mumble's channel permissions; the names
/// for them live in `services/channel_permissions.dart`.
#[derive(Debug, Clone)]
pub struct UiAclRule {
    pub apply_here: bool,
    pub apply_subs: bool,
    /// From a parent channel. Shown, never edited: the server drops inherited
    /// rules from anything written back, and the rule belongs to the channel
    /// that defines it.
    pub inherited: bool,
    pub user_id: Option<u32>,
    pub group: Option<String>,
    pub grant: u32,
    pub deny: u32,
}

/// A named group on a channel.
#[derive(Debug, Clone)]
pub struct UiAclGroup {
    pub name: String,
    pub inherited: bool,
    pub inherit: bool,
    pub inheritable: bool,
    pub add: Vec<u32>,
    pub remove: Vec<u32>,
    pub inherited_members: Vec<u32>,
}

/// A channel's whole access list.
#[derive(Debug, Clone)]
pub struct UiChannelAcl {
    pub channel_id: u32,
    pub inherit_acls: bool,
    pub groups: Vec<UiAclGroup>,
    pub rules: Vec<UiAclRule>,
}

/// One registered user's id and name.
#[derive(Debug, Clone)]
pub struct UiUserName {
    pub user_id: u32,
    pub name: String,
}

/// Somebody the server has an account for.
#[derive(Debug, Clone)]
pub struct UiRegisteredUser {
    pub user_id: u32,
    pub name: String,
    /// The server's own date string; empty when it did not say.
    pub last_seen: String,
}

/// What a server will tell an admin about one connected rider.
#[derive(Debug, Clone)]
pub struct UiUserDetails {
    pub session: u32,
    /// The client they run, as it describes itself — "MumbleWay 1.0.1", or
    /// whichever Mumble build.
    pub release: String,
    pub os: String,
    pub os_version: String,
    /// Where they connected from; empty when the server withheld it.
    pub address: String,
    pub strong_certificate: bool,
    pub online_secs: u32,
    pub idle_secs: u32,
}

/// One entry of the server's ban list.
///
/// Carries every field the protocol defines, including the ones nothing
/// displays: lifting a ban means sending the whole list back, so a field
/// dropped on the way through this type would silently rewrite somebody's ban.
#[derive(Debug, Clone)]
pub struct UiBan {
    /// The address, ready to read — IPv4 where the server holds a mapped one.
    pub address: String,
    /// Who it was, as recorded at the time.
    pub name: String,
    pub reason: String,
    /// The server's own date string for when it started.
    pub start: String,
    /// Seconds it lasts; 0 means until somebody lifts it.
    pub duration: u32,
    /// Opaque round-trip payload: this exact entry, as the server sent it.
    /// Handed back unchanged to keep a ban from being rewritten by being read.
    pub raw: String,
}

/// What the server says this rider may do, here and on this server.
///
/// For greying out what would be refused. **Never a substitute for handling the
/// refusal**: an ACL can change between this answer and the tap, so anything
/// that gets through is still sent and a refusal is still shown.
#[derive(Debug, Clone, Copy)]
pub struct UiRights {
    /// Whether the server has answered at all. Everything below is false until
    /// it has, which is not the same as being refused.
    pub known: bool,
    pub speak: bool,
    /// Mute and deafen others in this channel, for everyone.
    pub mute_deafen: bool,
    pub move_users: bool,
    pub text: bool,
    pub whisper: bool,
    pub make_channel: bool,
    /// Rename or re-describe this channel.
    pub write: bool,
    pub kick: bool,
    pub ban: bool,
    pub register_others: bool,
    pub self_register: bool,
}
/// The same permissions, in the shape the interface reads.
///
/// One place rather than two: the per-channel answer and the one for where the
/// rider is standing carry identical fields, and a copy of twelve assignments
/// is twelve chances to transpose two of them.
fn ui_rights(r: &mumbleway_core::session::permissions::Rights) -> UiRights {
    UiRights {
        known: r.known,
        speak: r.speak,
        mute_deafen: r.mute_deafen,
        move_users: r.move_users,
        text: r.text,
        whisper: r.whisper,
        make_channel: r.make_channel,
        write: r.write,
        kick: r.kick,
        ban: r.ban,
        register_others: r.register_others,
        self_register: r.self_register,
    }
}

/// What a rider may do in one channel, named by its id.
#[derive(Debug, Clone, Copy)]
pub struct UiChannelRights {
    pub channel_id: u32,
    pub rights: UiRights,
}


/// The server's measurements of one rider's connection.
#[derive(Debug, Clone, Copy)]
pub struct UiQuality {
    /// Round trip in milliseconds, measured at the server.
    pub ping_ms: f32,
    /// Whether the figure is the UDP one. A tunnelled rider has only the TCP
    /// measurement, which is worth telling apart: it includes the tunnel.
    pub udp: bool,
    /// Share of this rider's packets, 0 to 1, that never reached the server.
    pub loss_up: f32,
    /// Share of the server's packets that never reached this rider.
    pub loss_down: f32,
    /// Seconds the loss covers, or 0 when it is counted from their connect.
    pub window_secs: u32,
    /// Seconds since this rider last did anything.
    pub idle_secs: u32,
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
    /// This rider's own voice has been silenced by the server, or allowed
    /// again, because of the channel they are in.
    ///
    /// The cue has already played by the time this arrives. Unlike a mute,
    /// **this one does not go away by itself** — it lasts until they move or
    /// an admin changes the ACL — so the interface is expected to keep saying
    /// so rather than show one notice and forget.
    Suppressed {
        server_id: String,
        suppressed: bool,
    },
    /// The channels this rider is hearing without having joined them.
    Listening {
        server_id: String,
        channels: Vec<u32>,
    },
    /// What this server will accept: longest text, and largest picture, in
    /// bytes. Zero means it set no limit.
    Limits {
        server_id: String,
        message_length: u32,
        image_message_length: u32,
    },
    /// A server's bandwidth allowance, and what the encoder is doing about it.
    ///
    /// The bitrate is the app's, not this server's: one encoder feeds every
    /// connection, so the tightest allowance among them decides.
    Bandwidth {
        server_id: String,
        /// What this server allows each client, in bits per second.
        cap_bps: u32,
        /// What the encoder is now aiming for.
        bitrate_bps: u32,
        /// Whether an allowance, rather than this app's own choice, decided it.
        capped: bool,
        /// Whether even the lowest usable bitrate does not fit — voice will be
        /// dropped by the server, and nothing here can prevent it.
        below_floor: bool,
    },
    /// The menu entries this server has registered, whenever the set changes.
    ContextActions {
        server_id: String,
        actions: Vec<UiContextAction>,
    },
    /// One channel's access list, in answer to asking for it.
    Acl {
        server_id: String,
        acl: UiChannelAcl,
    },
    /// Names for registered user ids, in answer to asking.
    UserNames {
        server_id: String,
        names: Vec<UiUserName>,
    },
    /// Everybody this server has an account for.
    Registered {
        server_id: String,
        users: Vec<UiRegisteredUser>,
    },
    /// Everything the server will say about one rider, for an admin who asked.
    UserDetails {
        server_id: String,
        details: UiUserDetails,
    },
    /// The server's ban list, in answer to asking for it.
    Bans {
        server_id: String,
        bans: Vec<UiBan>,
    },
    /// What this server's administrator asks riders to do: push-to-talk,
    /// positional audio, or both. A suggestion, never enforced, and nothing is
    /// changed on the rider's behalf.
    ServerSuggests {
        server_id: String,
        push_to_talk: Option<bool>,
        positional: Option<bool>,
    },
    /// A rider's picture, as the server holds it. Empty means they removed it.
    ///
    /// Its own event rather than a roster field: the roster goes out many times
    /// a second in a busy channel, and these are measured in kilobytes.
    Avatar {
        server_id: String,
        session: u32,
        image: Vec<u8>,
    },
    /// What this rider may do on this server has been answered, or changed.
    Rights {
        server_id: String,
        rights: UiRights,
    },
    /// What this rider may do **in each channel** the server has answered
    /// about, as the whole set each time.
    ///
    /// Separate from `Rights` because that one is about the channel the rider
    /// is standing in, and a list of channels needs an answer per row: a rider
    /// with Write in one channel and not in another was seeing the management
    /// menu everywhere or nowhere, according to where they happened to be
    /// standing.
    ChannelRights {
        server_id: String,
        channels: Vec<UiChannelRights>,
    },
    /// Someone else changed our mute or deafen state.
    Moderated {
        server_id: String,
        muted: Option<bool>,
        deafened: Option<bool>,
        by: String,
    },
    /// Another MumbleWay rider asked for our microphone to be turned off or on,
    /// and the request was acted on — the microphone has already changed and
    /// the cue has already played by the time this arrives.
    ///
    /// Requests that were *not* acted on never reach the UI: a notice about a
    /// change that did not happen would be noise at best and alarming at
    /// worst.
    RemoteMuted {
        server_id: String,
        muted: bool,
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
    /// The app's version as the stores know it, e.g. `1.0.1`.
    ///
    /// From Dart, where it is read off the installed package, rather than from
    /// this crate's `CARGO_PKG_VERSION` — which is `0.1.0` and has never been
    /// bumped, and is how every server came to be told this was "MumbleWay 0.1".
    pub app_version: String,
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
    /// Whether another MumbleWay rider may turn this rider's microphone on.
    /// A setting, on by default; see `set_allow_remote_unmute`.
    allow_remote_unmute: Arc<AtomicBool>,
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
    // The name servers show in their user information, and the version other
    // MumbleWay clients are told in the handshake. One value for both, so the
    // two can never disagree about which build this is.
    let client_name = format!("MumbleWay {}", options.app_version)
        .trim()
        .to_string();
    let manager = SessionManager::new(identity.clone(), client_name, ev_tx)
        .with_app_version(options.app_version.clone());

    let level_shared = shared.clone();
    // Shared with App so the level task can name the server a stream belongs to.
    let slots: Arc<Mutex<HashMap<String, u16>>> = Arc::new(Mutex::new(HashMap::new()));
    let level_slots = slots.clone();
    let cue_shared = shared.clone();
    let last_status: Arc<Mutex<HashMap<String, ConnStatus>>> = Arc::new(Mutex::new(HashMap::new()));
    let status_tracker = last_status.clone();
    // On by default, and overwritten from the saved setting as soon as Dart has
    // read it. Default-on is the setting's own default, so the brief window
    // before that write cannot do anything the rider has not left enabled.
    let allow_remote_unmute = Arc::new(AtomicBool::new(true));
    let allow_unmute_task = allow_remote_unmute.clone();
    // One guard for the whole app, not one per server: a rider on two servers
    // at once still gets one cooldown, and nobody gets round it by asking from
    // the other one.
    let remote_mute_guard = Mutex::new(RemoteMuteGuard::default());
    // Every server's bandwidth allowance, by server. One encoder feeds them
    // all, so what it is set to is decided by the *tightest* of these — see
    // `audio::bandwidth::tightest`.
    let bandwidth_caps: Mutex<HashMap<String, u32>> = Mutex::new(HashMap::new());
    let bandwidth_shared = shared.clone();

    rt.spawn(async move {
        // Owned by this task, which is the only place either kind of mute cue
        // is played from; see `MuteCueEcho`.
        let mut mute_cue_echo = MuteCueEcho::default();
        let now = std::time::Instant::now;
        while let Some(TaggedEvent { server_id, event }) = ev_rx.recv().await {
            match event {
                SessionEvent::State(s) => {
                    let mut u = status_of(&s);
                    u.server_id = server_id.clone();

                    // Signal drops and recoveries audibly: the phone is usually
                    // in a pocket or behind a navigation app, so a status
                    // change that is only visible is one the rider misses.
                    // A server that has gone must stop deciding the bitrate.
                    // Its allowance is the tightest one often enough — that is
                    // why it was noticed — and leaving it in the map would
                    // hold the encoder down for every server still connected.
                    if matches!(
                        u.status,
                        ConnStatus::Disconnected | ConnStatus::Failed | ConnStatus::Idle
                    ) {
                        let tightest = {
                            let mut caps = bandwidth_caps.lock();
                            caps.remove(&server_id);
                            mumbleway_core::audio::bandwidth::tightest(
                                caps.values().copied().collect::<Vec<_>>(),
                            )
                        };
                        bandwidth_shared.set_bandwidth_cap(tightest.unwrap_or(0));
                    }

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
                                muted: u.mute,
                                deafened: u.deaf,
                                self_muted: u.self_mute,
                                self_deafened: u.self_deaf,
                                local_mute: u.local_mute,
                                user_id: u.user_id,
                                muted_you: u.muted_you,
                                status,
                                mumbleway_version: u.mumbleway,
                                priority_speaker: u.priority_speaker,
                                suppressed: u.suppress,
                                comment: u.comment,
                                quality: u.quality.map(|q| UiQuality {
                                    ping_ms: q.ping_ms,
                                    udp: q.udp,
                                    loss_up: q.loss_up,
                                    loss_down: q.loss_down,
                                    window_secs: q.window_secs,
                                    idle_secs: q.idle_secs,
                                }),
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
                SessionEvent::SelfSuppressed(suppressed) => {
                    // Loudly, and in the core, for the same reason the remote
                    // mute cue is here: a rider is not looking at the screen,
                    // and this is the only account they will get of why
                    // nobody can hear them.
                    cue_shared.play_cue(if suppressed {
                        AudioCue::Suppressed
                    } else {
                        AudioCue::Unsuppressed
                    });
                    diag::record(
                        LogLevel::Warn,
                        "suppressed",
                        if suppressed {
                            "this channel does not carry our voice: the server is discarding it"
                                .to_string()
                        } else {
                            "our voice is carried again".to_string()
                        },
                    );
                    emit(AppEvent::Suppressed {
                        server_id,
                        suppressed,
                    });
                }
                SessionEvent::Listening(channels) => emit(AppEvent::Listening {
                    server_id,
                    channels,
                }),
                SessionEvent::Limits(l) => emit(AppEvent::Limits {
                    server_id,
                    message_length: l.message_length,
                    image_message_length: l.image_message_length,
                }),
                SessionEvent::BandwidthCap(bps) => {
                    let tightest = {
                        let mut caps = bandwidth_caps.lock();
                        caps.insert(server_id.clone(), bps);
                        mumbleway_core::audio::bandwidth::tightest(
                            caps.values().copied().collect::<Vec<_>>(),
                        )
                    };
                    bandwidth_shared.set_bandwidth_cap(tightest.unwrap_or(0));
                    let budget = bandwidth_shared.bitrate_budget();
                    // Reported whether or not it changed anything: "this
                    // server allows 72 kbit/s and we are inside it" is the
                    // answer to a question a rider will ask exactly when
                    // something else has gone wrong.
                    emit(AppEvent::Bandwidth {
                        server_id,
                        cap_bps: bps,
                        bitrate_bps: budget.bitrate_bps,
                        capped: budget.capped,
                        below_floor: budget.below_floor,
                    });
                }
                SessionEvent::ContextActions(list) => emit(AppEvent::ContextActions {
                    server_id,
                    actions: list
                        .into_iter()
                        .map(|a| UiContextAction {
                            for_user: a.for_user(),
                            for_channel: a.for_channel(),
                            for_server: a.for_server(),
                            action: a.action,
                            label: a.label,
                        })
                        .collect(),
                }),
                SessionEvent::Acl(acl) => emit(AppEvent::Acl {
                    server_id,
                    acl: UiChannelAcl {
                        channel_id: acl.channel_id,
                        inherit_acls: acl.inherit_acls,
                        groups: acl
                            .groups
                            .into_iter()
                            .map(|g| UiAclGroup {
                                name: g.name,
                                inherited: g.inherited,
                                inherit: g.inherit,
                                inheritable: g.inheritable,
                                add: g.add,
                                remove: g.remove,
                                inherited_members: g.inherited_members,
                            })
                            .collect(),
                        rules: acl
                            .rules
                            .into_iter()
                            .map(|r| UiAclRule {
                                apply_here: r.apply_here,
                                apply_subs: r.apply_subs,
                                inherited: r.inherited,
                                user_id: r.user_id,
                                group: r.group,
                                grant: r.grant,
                                deny: r.deny,
                            })
                            .collect(),
                    },
                }),
                SessionEvent::UserNames(pairs) => emit(AppEvent::UserNames {
                    server_id,
                    names: pairs
                        .into_iter()
                        .map(|(user_id, name)| UiUserName { user_id, name })
                        .collect(),
                }),
                SessionEvent::Registered(list) => emit(AppEvent::Registered {
                    server_id,
                    users: list
                        .into_iter()
                        .map(|u| UiRegisteredUser {
                            user_id: u.user_id,
                            name: u.name,
                            last_seen: u.last_seen,
                        })
                        .collect(),
                }),
                SessionEvent::UserDetails(d) => emit(AppEvent::UserDetails {
                    server_id,
                    details: UiUserDetails {
                        session: d.session,
                        release: d.release,
                        os: d.os,
                        os_version: d.os_version,
                        address: d.address,
                        strong_certificate: d.strong_certificate,
                        online_secs: d.online_secs,
                        idle_secs: d.idle_secs,
                    },
                }),
                SessionEvent::Bans(list) => emit(AppEvent::Bans {
                    server_id,
                    bans: list
                        .into_iter()
                        .map(|b| UiBan {
                            address: mumbleway_core::session::bans::address_text(
                                &b.address, b.mask,
                            ),
                            name: b.name.clone(),
                            reason: b.reason.clone(),
                            start: b.start.clone(),
                            duration: b.duration,
                            // The entry itself, to hand back untouched.
                            raw: serde_json::to_string(&b).unwrap_or_default(),
                        })
                        .collect(),
                }),
                SessionEvent::ServerSuggests {
                    push_to_talk,
                    positional,
                } => emit(AppEvent::ServerSuggests {
                    server_id,
                    push_to_talk,
                    positional,
                }),
                SessionEvent::Avatar { session, image } => emit(AppEvent::Avatar {
                    server_id,
                    session,
                    image,
                }),
                SessionEvent::Rights(r) => emit(AppEvent::Rights {
                    server_id,
                    rights: ui_rights(&r),
                }),
                SessionEvent::ChannelRights(list) => emit(AppEvent::ChannelRights {
                    server_id,
                    channels: list
                        .into_iter()
                        .map(|c| UiChannelRights {
                            channel_id: c.channel_id,
                            rights: ui_rights(&c.rights),
                        })
                        .collect(),
                }),
                SessionEvent::RemoteMuteRequested { mute, by } => {
                    // Decided here rather than in the session, because this is
                    // where all three inputs meet: the microphone's real state,
                    // the rider's setting, and a cooldown that has to hold
                    // across every server.
                    let decision = remote_mute_guard.lock().decide(
                        mute,
                        cue_shared.is_muted(),
                        allow_unmute_task.load(Ordering::Relaxed),
                        now(),
                    );
                    match decision {
                        RemoteMuteDecision::Apply => {
                            // The change and the cue together, here, rather
                            // than leaving the change to Dart: the cue says the
                            // microphone moved, and it must not be able to play
                            // for a change that a busy UI thread then never
                            // makes.
                            cue_shared.set_muted(mute);
                            if mute_cue_echo.should_play(mute, now()) {
                                cue_shared.play_cue(if mute {
                                    AudioCue::MutedByOther
                                } else {
                                    AudioCue::UnmutedByOther
                                });
                            }
                            diag::record(
                                LogLevel::Info,
                                "remote-mute",
                                format!(
                                    "{} microphone at {}'s request",
                                    if mute { "muted" } else { "unmuted" },
                                    by
                                ),
                            );
                            emit(AppEvent::RemoteMuted {
                                server_id,
                                muted: mute,
                                by,
                            });
                        }
                        other => {
                            // Logged, because "I asked them to unmute and
                            // nothing happened" is a question somebody will
                            // ask, and this is the only place the answer is.
                            diag::record(
                                LogLevel::Info,
                                "remote-mute",
                                format!(
                                    "ignored a request to {} from {}: {:?}",
                                    if mute { "mute" } else { "unmute" },
                                    by,
                                    other
                                ),
                            );
                        }
                    }
                }
                SessionEvent::SelfModerated {
                    muted,
                    deafened,
                    by,
                } => {
                    // Audible, because this happens *to* the user: they are not
                    // looking at the screen when someone mutes them.
                    // A mute on its own may be the same change a request is
                    // about to make, and is heard once between the two.
                    let echo = deafened.is_none()
                        && muted.is_some_and(|m| !mute_cue_echo.should_play(m, now()));
                    if let Some(cue) = cue_for_moderation(muted, deafened).filter(|_| !echo) {
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
        allow_remote_unmute,
    });
    Ok(())
}

/// Whether another MumbleWay rider may turn this rider's microphone back on.
///
/// **Only unmuting is governed by this.** Being muted by somebody closes a
/// microphone and costs a rider nothing but the chance to be heard, which they
/// can take back with their own button; being unmuted opens it, and they are
/// on air from that moment, whatever they are saying. A rider who mutes to
/// talk to a passenger, or to take a call, should be able to say that nobody
/// else decides when that ends.
#[frb(sync)]
pub fn set_allow_remote_unmute(allow: bool) -> anyhow::Result<()> {
    app()?.allow_remote_unmute.store(allow, Ordering::Relaxed);
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

    // **One conversion, shared with the sharing paths.** This used to build the
    // profile by hand and copy three fields across, which is how a server could
    // be registered without the access tokens it was saved with: they were in
    // the config, in the saved entry and in the settings file, and absent from
    // the `Authenticate` that actually asks for them — so a rider came back
    // after a restart outside the channel their token opens, with nothing
    // anywhere saying why. A conversion that lists every field once cannot
    // drift from the one the invite links use.
    let profile = config_to_profile(config);
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

/// Asks the server what this rider may do in one channel.
///
/// Lazily, from the row that wants to know: a server with fifty channels would
/// otherwise be fifty queries at connect, for answers about rows nobody has
/// looked at. The answer arrives as an ordinary `ChannelRights` event.
pub fn ask_channel_permissions(server_id: String, channel_id: u32) -> anyhow::Result<()> {
    send_command(server_id, SessionCommand::AskPermissions(channel_id))
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

/// Picks one of the menu entries this server registered.
///
/// What happens next is the server's business — it may do nothing, and it
/// reports nothing back either way.
pub fn trigger_context_action(
    server_id: String,
    action: String,
    session: Option<u32>,
    channel_id: Option<u32>,
) -> anyhow::Result<()> {
    send_command(
        server_id,
        SessionCommand::TriggerContextAction {
            action,
            session,
            channel_id,
        },
    )
}

/// Makes a channel under `parent`. Needs MakeChannel there, or
/// MakeTempChannel when `temporary`.
pub fn create_channel(
    server_id: String,
    parent: u32,
    name: String,
    description: String,
    temporary: bool,
) -> anyhow::Result<()> {
    send_command(
        server_id,
        SessionCommand::CreateChannel {
            parent,
            name,
            description,
            temporary,
        },
    )
}

/// Renames a channel or re-describes it. Needs Write on it.
pub fn edit_channel(
    server_id: String,
    channel_id: u32,
    name: Option<String>,
    description: Option<String>,
) -> anyhow::Result<()> {
    send_command(
        server_id,
        SessionCommand::EditChannel {
            channel_id,
            name,
            description,
        },
    )
}

/// Removes a channel and everything under it. Needs Write on it.
pub fn remove_channel(server_id: String, channel_id: u32) -> anyhow::Result<()> {
    send_command(server_id, SessionCommand::RemoveChannel(channel_id))
}

/// Replaces the access tokens for this server, taking effect at once.
///
/// **A token is a password spelled as a group name.** A Mumble channel has no
/// password of its own: it has an ACL granting entry to a group, and a server
/// writes that group as `#name` so that anybody presenting a token `name` is
/// treated as a member. The server re-reads the tokens on a second
/// `Authenticate` and re-evaluates every channel against them, so a channel
/// that was shut can open without reconnecting.
pub fn set_access_tokens(server_id: String, tokens: Vec<String>) -> anyhow::Result<()> {
    send_command(server_id, SessionCommand::SetAccessTokens(tokens))
}

/// Starts or stops hearing channels without joining them.
///
/// The rider stays where they are and their voice still goes to their own
/// channel. Needs the Listen permission on each one, and a server may cap how
/// many listeners a channel takes or how many channels one rider may hear —
/// both come back as refusals.
pub fn set_listening(server_id: String, add: Vec<u32>, remove: Vec<u32>) -> anyhow::Result<()> {
    send_command(server_id, SessionCommand::SetListening { add, remove })
}

/// Asks for one channel's access list. Needs Write on that channel.
pub fn request_acl(server_id: String, channel_id: u32) -> anyhow::Result<()> {
    send_command(server_id, SessionCommand::RequestAcl(channel_id))
}

/// Replaces a channel's access list.
///
/// **Written whole**, because the protocol cannot change one rule. Inherited
/// rules are dropped on the way out: the server does not store them again, and
/// leaving them out is not a deletion.
pub fn set_acl(server_id: String, acl: UiChannelAcl) -> anyhow::Result<()> {
    send_command(
        server_id,
        SessionCommand::SetAcl(mumbleway_core::session::ChannelAcl {
            channel_id: acl.channel_id,
            inherit_acls: acl.inherit_acls,
            groups: acl
                .groups
                .into_iter()
                .map(|g| mumbleway_core::session::AclGroup {
                    name: g.name,
                    inherited: g.inherited,
                    inherit: g.inherit,
                    inheritable: g.inheritable,
                    add: g.add,
                    remove: g.remove,
                    inherited_members: g.inherited_members,
                })
                .collect(),
            rules: acl
                .rules
                .into_iter()
                .map(|r| mumbleway_core::session::AclRule {
                    apply_here: r.apply_here,
                    apply_subs: r.apply_subs,
                    inherited: r.inherited,
                    user_id: r.user_id,
                    group: r.group,
                    grant: r.grant,
                    deny: r.deny,
                })
                .collect(),
        }),
    )
}

/// Asks what the names are behind registered user ids, since an access list
/// names people by number.
pub fn query_user_names(server_id: String, ids: Vec<u32>) -> anyhow::Result<()> {
    send_command(server_id, SessionCommand::QueryUserNames(ids))
}

/// Asks for the registered users; they arrive as `AppEvent::Registered`.
pub fn request_registered(server_id: String) -> anyhow::Result<()> {
    send_command(server_id, SessionCommand::RequestRegistered)
}

/// Removes registrations by user id. Needs Register on the root channel.
pub fn unregister_users(server_id: String, user_ids: Vec<u32>) -> anyhow::Result<()> {
    send_command(server_id, SessionCommand::UnregisterUsers(user_ids))
}

/// Gives a connected rider an account on this server.
pub fn register_user(server_id: String, session: u32) -> anyhow::Result<()> {
    send_command(server_id, SessionCommand::RegisterUser(session))
}

/// Grants or withdraws priority speaker, which ducks everybody else while that
/// rider talks.
pub fn set_priority_speaker(server_id: String, session: u32, priority: bool) -> anyhow::Result<()> {
    send_command(
        server_id,
        SessionCommand::SetPrioritySpeaker { session, priority },
    )
}

/// Clears somebody's note, picture, or both.
pub fn reset_user_content(
    server_id: String,
    session: u32,
    comment: bool,
    texture: bool,
) -> anyhow::Result<()> {
    send_command(
        server_id,
        SessionCommand::ResetUserContent {
            session,
            comment,
            texture,
        },
    )
}

/// Asks for everything the server will say about one rider. The privileged
/// half — client, address, certificate — arrives only for an admin.
pub fn request_user_details(server_id: String, session: u32) -> anyhow::Result<()> {
    send_command(server_id, SessionCommand::RequestUserDetails(session))
}

/// Removes a rider and bars them from returning. Needs Ban on the root channel.
pub fn ban_user(server_id: String, session: u32, reason: String) -> anyhow::Result<()> {
    send_command(server_id, SessionCommand::BanUser { session, reason })
}

/// Moves somebody else into a channel. Needs Move.
pub fn move_user(server_id: String, session: u32, channel_id: u32) -> anyhow::Result<()> {
    send_command(
        server_id,
        SessionCommand::MoveUser {
            session,
            channel_id,
        },
    )
}

/// Asks for the server's ban list; it arrives as `AppEvent::Bans`.
pub fn request_bans(server_id: String) -> anyhow::Result<()> {
    send_command(server_id, SessionCommand::RequestBans)
}

/// Replaces the server's ban list with these entries.
///
/// Takes the `raw` strings from [`UiBan`], unchanged. **Whatever is left out is
/// lifted** — the protocol has no way to remove one ban — so the caller sends
/// every ban that is to remain, and an entry that cannot be read back is kept
/// rather than dropped, since dropping it would lift a ban nobody asked to lift.
pub fn set_bans(server_id: String, bans: Vec<String>) -> anyhow::Result<()> {
    let mut list = Vec::with_capacity(bans.len());
    for raw in &bans {
        match serde_json::from_str(raw) {
            Ok(entry) => list.push(entry),
            Err(e) => {
                anyhow::bail!("a ban could not be read back, so none were changed: {e}")
            }
        }
    }
    send_command(server_id, SessionCommand::SetBans(list))
}

/// Sets the picture shown beside our own name on this server; empty clears it.
///
/// One picture per rider, kept on the device, and sent to each server as it
/// connects — Mumble has no identity that spans servers, so every one of them
/// stores its own copy.
pub fn set_avatar(server_id: String, image: Vec<u8>) -> anyhow::Result<()> {
    send_command(server_id, SessionCommand::SetAvatar(image))
}

/// Sets the note shown beside our own name on this server, or clears it.
pub fn set_comment(server_id: String, text: String) -> anyhow::Result<()> {
    send_command(server_id, SessionCommand::SetComment(text))
}

pub fn set_self_mute(server_id: String, muted: bool) -> anyhow::Result<()> {
    send_command(server_id, SessionCommand::SetSelfMute(muted))
}

/// Tells a server this rider has turned their own sound off.
///
/// **The other riders have no other way to know.** Deafening is local — the
/// decoder simply stops — so without this the channel keeps talking to
/// somebody who cannot hear a word of it, which is the one thing worth saying
/// about a rider who is not listening.
pub fn set_self_deaf(server_id: String, deaf: bool) -> anyhow::Result<()> {
    send_command(server_id, SessionCommand::SetSelfDeaf(deaf))
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

    /// What the voice encoder is aiming for, in bits per second.
    pub voice_bitrate_bps: u32,
    /// The tightest bandwidth allowance among the connected servers, or 0 if
    /// none of them set one. A server enforces this by dropping voice.
    pub bandwidth_cap_bps: u32,
    /// Whether that allowance, rather than this app's own choice, is deciding
    /// the bitrate.
    pub bitrate_capped: bool,
    /// Whether the allowance is too low for usable voice at all.
    pub bitrate_below_floor: bool,

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
    let budget = shared.bitrate_budget();
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
        // Read as one budget rather than as separate numbers, so the flags and
        // the bitrate can never disagree about the same moment.
        voice_bitrate_bps: shared.encoder_bitrate_bps(),
        bandwidth_cap_bps: budget.cap_bps.unwrap_or(0),
        bitrate_capped: budget.capped,
        bitrate_below_floor: budget.below_floor,
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
    /// How much quieter the microphone is than what the speaker played, in dB,
    /// measured before our own canceller touches it.
    ///
    /// **The number that says whether the platform is cancelling underneath
    /// us.** A phone's speaker and microphone are inches apart, so 0 to 20 dB
    /// is an ordinary phone and beyond about 40 nothing acoustic explains it.
    /// Android's `VOICE_COMMUNICATION` capture preset switches the device's own
    /// echo cancellation on, and no API reports that reliably — pre-processing
    /// inside the audio HAL is invisible to the effects framework.
    ///
    /// It understates the loss rather than overstating it, so a high reading is
    /// trustworthy and a low one may only mean somebody was talking. The clean
    /// way to read it is to play the test tone and stay quiet.
    ///
    /// `None` until the far end has made a sound loud enough to measure
    /// against; `erl_blocks` says how much it rests on, because a confident
    /// number from four blocks is the failure mode here.
    pub erl_db: Option<f32>,
    pub erl_blocks: u32,
    /// The onset SNR the `Auto` profile was chosen from, in dB — how far the
    /// rider's voice stood above their own background over the first second of
    /// the last phrase. `None` until somebody speaks, and always `None` when
    /// the profile was set by hand.
    ///
    /// Shown next to the profile because it is the evidence for it. A rider on
    /// a motorway reads about 13; in a quiet room, forty-odd.
    pub auto_snr_db: Option<f32>,
    /// The SNRs the profile bands are divided at, in dB: below the first is
    /// `Helmet`, below the second `Standard`, above it `Light`.
    ///
    /// Sent every poll so the gauge cannot draw a boundary the chain has moved.
    /// The restoring bell that puts back what the enhancer took.
    ///
    /// `restore_gain_db` is how much it is lifting right now and
    /// `restore_centre_hz` where — both zero whenever the chain is not hearing
    /// speech, which is most of the time. The two `_ms` figures are what the
    /// peak search and the filter cost per block on this device.
    ///
    /// **Not in the stage timings**, because the stage list tiles a block
    /// exactly and these run inside suppression.
    pub restore_gain_db: f32,
    pub restore_centre_hz: f32,
    pub restore_peak_ms: f32,
    pub restore_filter_ms: f32,
    /// The bell's width, and the most it is ever allowed to lift.
    ///
    /// Constants on the Rust side, sent every poll rather than written into the
    /// painter: a curve drawn from a stale copy of `Q` is the wrong curve and
    /// looks exactly like the right one.
    pub restore_q: f32,
    pub restore_max_db: f32,
    pub auto_snr_helmet_below_db: f32,
    pub auto_snr_standard_below_db: f32,
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
    pub live_dots_disabled: bool,
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
        erl_db: c.erl_db,
        erl_blocks: c.erl_blocks,
        auto_snr_db: c.auto_snr_db,
        restore_gain_db: c.restore_gain_db,
        restore_centre_hz: c.restore_centre_hz,
        restore_peak_ms: c.restore_peak_ms,
        restore_filter_ms: c.restore_filter_ms,
        restore_q: c.restore_q,
        restore_max_db: c.restore_max_db,
        auto_snr_helmet_below_db: c.auto_snr_helmet_below_db,
        auto_snr_standard_below_db: c.auto_snr_standard_below_db,
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
    /// Motion readings storage could not keep up with.
    ///
    /// Shown for the same reason, and it matters more here: a gap nobody
    /// counted in the motion track looks exactly like a stretch of road where
    /// nothing happened, which is the reading a tap detector would then be
    /// scored against.
    pub dropped_motion: u64,
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
            dropped_motion: 0,
            directory: String::new(),
        };
    };
    let s = app.shared.diagnostic_recording_state();
    UiRecordingState {
        active: s.active,
        dropped_blocks: s.dropped_blocks,
        dropped_motion: s.dropped_motion,
        directory: s.directory,
    }
}

/// Hands one reading of the phone's own motion to the recorder.
///
/// **Recorded whether or not tap detection is switched on.** Measuring *false*
/// positives needs rides with no taps in them, so the negative corpus can only
/// be gathered while the feature is off — tying this to the detector would make
/// the corpus that matters most impossible to collect.
///
/// `platform_ns` is the platform's own stamp in its own epoch, kept as given
/// rather than converted: nanoseconds since boot on Android, seconds since boot
/// on iOS, and neither is the audio clock. It is written beside an arrival
/// stamp taken here, so the delay between them can be *measured* later instead
/// of assumed. Ignored when nothing is recording, so the platform may push
/// without asking first.
#[frb(sync)]
pub fn push_motion(
    platform_ns: u64,
    accel: Vec<f32>,
    gravity: Vec<f32>,
    rotation: Vec<f32>,
) -> anyhow::Result<()> {
    let three = |v: &Vec<f32>| -> [f32; 3] {
        [
            v.first().copied().unwrap_or(0.0),
            v.get(1).copied().unwrap_or(0.0),
            v.get(2).copied().unwrap_or(0.0),
        ]
    };
    app()?.shared.push_motion(mumbleway_core::audio::record::MotionSample {
        platform_ns,
        arrival_us: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_micros() as u64)
            .unwrap_or(0),
        accel: three(&accel),
        gravity: three(&gravity),
        rotation: three(&rotation),
    });
    Ok(())
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

/// Asks Android's capture path for the telephony preset, on the next open.
///
/// Stops another app capturing alongside us — from Android 10 the loser of a
/// contest for the microphone is handed silence, and only this preset and
/// `CAMCORDER` are privacy sensitive. It also switches on the device's own
/// echo cancellation, noise suppression and gain control on most phones, which
/// this chain already does for itself, so it is a trade rather than a win. The
/// echo-returned figure in the diagnostics panel is how to see which.
///
/// Takes effect when the input device is next opened, not immediately.
#[frb(sync)]
pub fn set_voice_communication(on: bool) -> anyhow::Result<()> {
    app()?.shared.set_voice_communication(on);
    Ok(())
}

/// Tells the recorder which microphone the platform has us on.
///
/// **A number, not a name.** The decision log is a line per 10 ms block, so a
/// string here would be the largest column in the file for something that
/// changes a handful of times a session; one digit and a comma is two bytes a
/// row against the 960 the audio costs beside it.
///
/// See `mumbleway_core::audio::record::Recorded::route` for what the numbers
/// mean. That table is the wire format: the platform code mirrors it, and the
/// numbers cannot be renumbered without changing the meaning of every
/// recording already sitting on somebody's phone.
///
/// Called when the session comes up and again whenever the route changes under
/// it, because a headset connected mid-ride is a different microphone from the
/// one the recording started on.
#[frb(sync)]
pub fn set_audio_route(code: u8) -> anyhow::Result<()> {
    app()?.shared.set_route(code);
    Ok(())
}

/// Plays a tone on the output device, to check the speaker choice.
#[frb(sync)]
pub fn play_test_tone(millis: u32) -> anyhow::Result<()> {
    app()?.shared.play_test_tone(millis);
    Ok(())
}

/// Opens or closes the capture half of the device pair.
///
/// `false` is the listening state of `docs/CAPTURE_ON_DEMAND.md`: output alive
/// so the group and whatever music is playing come through at full bandwidth,
/// and no input stream at all — which is what lets a Bluetooth headset fall
/// back off the hands-free profile.
///
/// **Call this with the platform session, not instead of it.** The profile is
/// the platform's to choose; this is the engine's half, and the two have to
/// agree or the streams are rebuilt against a device that is no longer there.
///
/// **Blocks until the streams are back**, like [`set_audio_active`] and for the
/// same reason: the answer is the point. Both streams are rebuilt on every
/// transition, because hands-free reports 8 or 16 kHz where A2DP reports 44.1
/// or 48 and nothing here asks the platform for a rate — so "capture is live"
/// is only true once the device has answered, and a cue fired before that would
/// be telling a rider to speak into a stream that does not exist yet.
///
/// Returns at once when nothing needed rebuilding.
pub fn set_capture_wanted(on: bool) -> anyhow::Result<()> {
    let app = app()?;
    if app.shared.set_capture_wanted(on) {
        app.shared
            .await_open(std::time::Duration::from_secs(10))
            .map_err(|e| anyhow::anyhow!(e))?;
    }
    Ok(())
}

/// One beat of the countdown while the hands-free profile is negotiated.
///
/// Called repeatedly for as long as the wait lasts — see
/// [`AudioCue::CaptureWaiting`] for why it is a beat rather than a pattern.
#[frb(sync)]
pub fn play_capture_waiting_cue() -> anyhow::Result<()> {
    app()?.shared.play_cue(AudioCue::CaptureWaiting);
    Ok(())
}

/// The countdown resolving: capture is live, the rider may speak.
///
/// **Only once input is genuinely live**, which is the whole value of it. Fired
/// on the request instead, it would tell a rider to speak into a microphone
/// that does not exist yet and cost them a sentence.
#[frb(sync)]
pub fn play_capture_live_cue() -> anyhow::Result<()> {
    app()?.shared.play_cue(AudioCue::CaptureLive);
    Ok(())
}

/// Capture released: the microphone is off.
///
/// **Only once it is confirmed closed, and never on the request.** The failure
/// modes are asymmetric — an early live cue costs a sentence, an early stop cue
/// puts a curse on the channel — so if the transition cannot be confirmed, play
/// nothing at all. Silence leaves a rider cautious; a false all-clear does the
/// opposite.
#[frb(sync)]
pub fn play_capture_stopped_cue() -> anyhow::Result<()> {
    app()?.shared.play_cue(AudioCue::CaptureStopped);
    Ok(())
}

/// Says on connecting that capture is off until the rider taps, and how often.
///
/// Required rather than decorative: in tap mode capture starts off, so without
/// it the first thing a rider does is talk into a microphone that is not there.
#[frb(sync)]
pub fn play_tap_armed_cue(taps: u8) -> anyhow::Result<()> {
    app()?.shared.play_tap_armed_cue(taps);
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

/// Asks the server to register this account permanently.
///
/// Registration ties the name to the certificate this app already keeps, so it
/// stays yours between visits and the server can put it in groups. Most servers
/// gate it behind the SelfRegister permission; where it is withheld the server
/// answers with a permission-denied message, which arrives the same way a
/// refused kick does rather than as an error from this call.
///
/// Returning `Ok` therefore means the request was sent, not that it worked —
/// the same contract as every other permission-gated action here.
pub fn register_self(server_id: String) -> anyhow::Result<()> {
    send_command(server_id, SessionCommand::RegisterSelf)
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
            // The route does travel: an export is the rider's own list coming
            // back to them on another device, and a server they can only reach
            // through a proxy is unusable without it.
            proxy: c.proxy_chain.into_iter().next().map(|p| {
                mumbleway_core::session::profile::build_proxy_text(&p.into_spec(), true)
            }),
        })
        .collect();

    serde_json::to_string_pretty(&entries)
        .map_err(|e| anyhow::anyhow!("could not build the export: {e}"))
}

/// Converts the Dart-facing config into the core's profile type.
///
/// **Every field, in one place.** `add_server` once did this by hand and copied
/// three of them; the fields it forgot were simply absent from the session, with
/// nothing to say so. Anything added to `ServerConfig` belongs here and nowhere
/// else.
fn config_to_profile(c: ServerConfig) -> ServerProfile {
    let mut p = ServerProfile::new(c.name, c.host, c.port, c.username);
    p.password = c.password;
    p.cert_fingerprint = c.cert_fingerprint;
    p.auto_join_channel = c.default_channel;
    p.access_tokens = c.access_tokens;
    p.proxy_chain = c.proxy_chain.into_iter().map(ServerProxy::into_spec).collect();
    if !c.id.trim().is_empty() {
        p.id = c.id;
    }
    p
}

/// A `mumble-proxy://` link for one proxy, to share as a link or a QR code.
///
/// `web` wraps it in the same https page a server invitation uses, which is
/// what survives a messenger.
pub fn build_proxy_link(
    proxy: ServerProxy,
    include_credentials: bool,
    web: bool,
) -> anyhow::Result<String> {
    let spec = proxy.into_spec();
    Ok(if web {
        mumbleway_core::session::profile::build_proxy_web_url(&spec, include_credentials)
    } else {
        mumbleway_core::session::profile::build_proxy_url(&spec, include_credentials)
    })
}

/// Reads a `mumble-proxy://` link, or the https wrapper around one.
///
/// `None` for anything else — including a server link, which looks alike and
/// means something entirely different.
pub fn parse_proxy_link(text: String) -> Option<ServerProxy> {
    mumbleway_core::session::profile::parse_proxy_url(&text).map(ServerProxy::from_spec)
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
            // An imported profile carries none: a token is a secret the
            // sharer did not put in the link, and inventing one would be
            // claiming an invitation said something it did not.
            access_tokens: Vec::new(),
            // A proxy, on the other hand, may well be in the link — a server
            // that can only be reached through one is not much of an
            // invitation without it. What arrives is shown before it is used:
            // adopting somebody else's route silently is the thing to avoid,
            // not carrying it.
            proxy_chain: p.proxy_chain.into_iter().map(ServerProxy::from_spec).collect(),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slot_map() -> Arc<Mutex<HashMap<String, u16>>> {
        Arc::new(Mutex::new(HashMap::new()))
    }

    /// Everything a saved server carries must reach the session.
    ///
    /// **The access tokens are why this test exists.** They were saved, shown
    /// in the menu, sent to a *live* session when the rider typed one — and
    /// dropped on the floor when a session was registered at startup, because
    /// the registration built its profile by hand and copied three fields. The
    /// symptom was a rider who held the word that opens a channel, could see it
    /// listed, and was refused entry after every restart, with the client
    /// offering no account of why.
    #[test]
    fn the_conversion_to_a_profile_keeps_every_field() {
        let config = ServerConfig {
            id: "chosen-id".into(),
            name: "Rig".into(),
            host: "example.test".into(),
            port: 64739,
            username: "rider".into(),
            password: Some("secret".into()),
            cert_fingerprint: Some("ab:cd".into()),
            default_channel: Some("Garage".into()),
            access_tokens: vec!["vip".into(), "rideboss".into()],
        };

        let p = config_to_profile(config);
        assert_eq!(
            p.id, "chosen-id",
            "a caller-chosen id is what makes duplicates possible"
        );
        assert_eq!(p.name, "Rig");
        assert_eq!(p.host, "example.test");
        assert_eq!(p.port, 64739);
        assert_eq!(p.username, "rider");
        assert_eq!(p.password.as_deref(), Some("secret"));
        assert_eq!(p.cert_fingerprint.as_deref(), Some("ab:cd"));
        assert_eq!(p.auto_join_channel.as_deref(), Some("Garage"));
        assert_eq!(
            p.access_tokens,
            vec!["vip".to_string(), "rideboss".to_string()],
            "the tokens have to be on the profile, or the handshake never asks"
        );
    }

    /// An empty id still derives one, which is what keeps old entries working.
    #[test]
    fn a_config_without_an_id_gets_the_usual_one() {
        let config = ServerConfig {
            id: "   ".into(),
            name: "Rig".into(),
            host: "example.test".into(),
            port: 64739,
            username: "rider".into(),
            password: None,
            cert_fingerprint: None,
            default_channel: None,
            access_tokens: Vec::new(),
        };
        assert_eq!(config_to_profile(config).id, "example.test:64739");
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
