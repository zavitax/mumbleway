//! Recognising other MumbleWay clients on a server.
//!
//! Mumble has no way for one client to ask what another is running. The server
//! knows — every client sends a `Version` at handshake — but it only hands that
//! on to somebody holding **Ban on the root channel** (`msgUserStats` in the
//! server's `Messages.cpp`), so for an ordinary rider it is simply not there.
//!
//! So MumbleWay clients tell each other, over `PluginDataTransmission`. That
//! message is the one piece of the protocol any client may address to any other
//! without a permission, and the server treats it well for this: it **stamps
//! the sender's session itself** rather than trusting the client, so a hello
//! cannot claim to come from somebody else; and a client that does not
//! recognise the data ID ignores it, so riders on the official client never see
//! any of this.
//!
//! This is also the foundation for anything MumbleWay-specific that clients
//! exchange later. Everything under [`DATA_ID_PREFIX`] is ours, and [`Peer`]
//! carries a protocol number and a capability list so a later exchange can be
//! offered only to peers that say they understand it.
//!
//! # The handshake
//!
//! ```text
//!   joining client                          everybody already there
//!   ──────────────                          ───────────────────────
//!   hello { reply: false }  ── one message, every session ──▶
//!                           ◀── hello { reply: true }  (MumbleWay clients only)
//! ```
//!
//! A newcomer announces itself once, to every present session in a **single**
//! message. Each MumbleWay client that hears it records the sender and owes it a
//! reply. A reply is recorded and never answered, which is the whole of what
//! stops two clients from bouncing hellos between them for ever.
//!
//! Nobody needs to watch for joins: whoever arrives later announces itself, and
//! everybody present replies. So both halves of every pair learn about each
//! other whichever of them came first.
//!
//! # Saying it again
//!
//! The announcement is repeated every [`RETRY_INTERVAL`], up to
//! [`MAX_ANNOUNCEMENTS`] times, to whichever present sessions have not answered
//! — see [`Peers::unheard`] and [`Announcer`].
//!
//! **Not for packet loss.** This is the control channel: TLS over TCP, so a
//! hello that goes astray is retransmitted by TCP and cannot simply vanish.
//! What it covers is the server's own rate limiter, which drops a plugin
//! message when the sender's bucket is empty, logs it on the server and tells
//! the sender **nothing** — the one way a hello is lost silently. Five seconds
//! is far longer than a bucket refilling at four a second needs.
//!
//! **And it has to stop.** Hearing nothing back is the ordinary case, not a
//! fault: on a server where nobody else runs MumbleWay there is no reply to be
//! had, ever. Retrying until somebody answers would therefore mean announcing
//! to every session on that server every five seconds for the length of the
//! ride. So the count is bounded, and it also stops early once everybody
//! present has answered.
//!
//! Only the sessions that have not answered are told again, rather than
//! everybody: it covers the case of one reply among several going missing, and
//! it keeps the repetition away from people who have already answered.
//!
//! # Limits the server imposes, and how this stays inside them
//!
//! Checked against the server source rather than assumed:
//!
//! - `data` at most **1000 bytes**, `dataID` at most **100** (`MumbleConstants.h`).
//!   A hello is well under a hundred.
//! - **4 messages a second, bursts of 15**, per sender, by default, and a server
//!   may lower it. The limit is charged **per message**, not per recipient, and
//!   one message may name many receivers. So the announcement is one message
//!   however crowded the server, and replies are gathered up and sent together
//!   once a second — see [`Peers::take_owed`] — rather than one per hello, which
//!   a server restart dropping thirty MumbleWay riders back in at once would
//!   otherwise turn into thirty messages from every one of them.
//! - **The server must be 1.4.0 or later.** Older servers predate the message,
//!   and the official client refuses to send it to them
//!   (`API_v_1_x_x.cpp`); this does the same through
//!   [`server_supports_plugin_data`], rather than trusting an old server to
//!   ignore a type it has never heard of.
//!
//! # What a hello discloses
//!
//! The app's name, its version, the handshake protocol number and a capability
//! list — nothing about the device, the operating system or the rider. Today,
//! only a server admin can see which client somebody runs; this tells every
//! MumbleWay user on the same server. The privacy policy says so.
//!
//! # Trust
//!
//! The *sender* is authentic, because the server stamps it. The *contents* are
//! whatever that client chose to say. A hostile client can claim to be
//! MumbleWay, and nothing here can stop it. That is fine for a badge in a
//! roster, and it is the line any later exchange has to be designed around: a
//! peer's capabilities are a hint about what it will understand, never a reason
//! to trust what it sends.

use std::collections::{BTreeSet, HashMap};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

/// Every data ID MumbleWay sends starts with this. Anything else belongs to
/// somebody's plugin and is not ours to read.
pub const DATA_ID_PREFIX: &str = "mumbleway/";

/// The handshake's data ID.
pub const DATA_ID_HELLO: &str = "mumbleway/hello";

/// The application name a hello must carry to count.
pub const APP_NAME: &str = "MumbleWay";

/// Version of the handshake itself, not of the app. Raise it when the meaning
/// of a hello changes; add a capability instead when a feature is added.
pub const PROTOCOL: u32 = 1;

/// What this build tells peers it understands.
///
/// A name goes here when an exchange is added, and that exchange is offered
/// only to peers whose hello listed the same name — so it never reaches a
/// client that would not know what to do with it, and never needs a change to
/// the handshake to be introduced.
pub const CAPABILITIES: &[&str] = &[CAP_REMOTE_MUTE, CAP_MUTE_NOTICE];

/// This client will act on a request to mute or unmute its microphone — see
/// [`DATA_ID_MUTE`].
pub const CAP_REMOTE_MUTE: &str = "remote-mute";

/// This client says when it has silenced a peer for itself, and understands
/// being told — see [`DATA_ID_MUTED_YOU`].
pub const CAP_MUTE_NOTICE: &str = "mute-notice";

/// Asks a peer to turn its own microphone off or on.
///
/// **What it is for.** A rider whose noise cancellation has failed fills the
/// channel with wind, and on most servers nobody present holds the Mute
/// permission that would let them do anything but mute that rider for
/// themselves. The opposite case is as common: a rider muted, forgot, and is
/// talking to nobody. The server can fix neither — no one, admins included, may
/// change another user's *self*-mute (`msgUserState` refuses it outright) — so
/// it has to be the rider's own client that acts, at a peer's request.
///
/// It rides alongside the ordinary server mute rather than replacing it: the
/// roster's "Mute on server" sends both, so a rider with the permission gets the
/// real, binding mute and one without still gets the request.
pub const DATA_ID_MUTE: &str = "mumbleway/mute";

/// Tells a peer they have been silenced for this rider alone, or let back in.
///
/// **The one way of being inaudible that nobody can see.** A local mute drops
/// a rider's audio inside this app before it reaches the mixer: the server is
/// not told, nothing is in the roster, and from the other end it is
/// indistinguishable from being heard. So a rider carries on talking to
/// somebody who stopped listening half an hour ago, and there is nothing
/// anywhere that would tell them — unless the client that did it says so,
/// which is what this is.
///
/// It is a courtesy, not a permission: the sender gains nothing by sending it
/// and the receiver can do nothing about it but know. Sent only to peers whose
/// hello listed [`CAP_MUTE_NOTICE`], so nobody hears about a feature their
/// client cannot explain.
pub const DATA_ID_MUTED_YOU: &str = "mumbleway/muted-you";

/// How long after acting on one remote *mute* request the next one is ignored.
///
/// **This is what stops a peer holding someone muted.** Without it, a request
/// every second would re-mute a rider the moment they unmuted themselves. With
/// it, whoever is on the receiving end gets half a minute of their own
/// microphone after every remote mute — long enough to notice and undo it,
/// short enough that a genuine second request after the noise comes back is
/// not refused for long. Global across senders and servers, because a new
/// session is one reconnect away and a per-sender limit is therefore no limit.
///
/// **An unmute never waits for it**, and the reason is the sentence above:
/// this exists so that nobody can hold a rider muted, and refusing to hand the
/// microphone back is holding them muted. It shipped the other way and was
/// reported as what it looks like — somebody muted through the app, unmuted a
/// moment later, and the microphone stayed shut.
pub const REMOTE_MUTE_COOLDOWN: Duration = Duration::from_secs(30);

/// How long after sending a mute request a matching refusal from the server is
/// taken to be covered by it. See [`Peers::backup_covers`].
pub const BACKUP_WINDOW: Duration = Duration::from_secs(5);

/// How long to wait before announcing again.
///
/// Generous on purpose: the thing being worked around is a rate-limit bucket
/// that refills at four messages a second, so anything upwards of a second
/// would do, and a longer gap costs nothing when the total is bounded.
pub const RETRY_INTERVAL: Duration = Duration::from_secs(5);

/// How many announcements one connection may make in total — the first, and
/// three more.
///
/// **Bounded because silence is the ordinary answer.** On a server where nobody
/// else runs MumbleWay there is never a reply, so a rule of "repeat until
/// somebody answers" would announce to every session every five seconds for the
/// whole ride. Four covers a message the server dropped without covering that.
pub const MAX_ANNOUNCEMENTS: u8 = 4;

/// The server's cap on a plugin message's payload.
pub const MAX_DATA_LENGTH: usize = 1000;

/// The server's cap on a plugin message's data ID.
pub const MAX_DATA_ID_LENGTH: usize = 100;

/// Longest version string kept from a peer. It is shown in a tooltip; a peer
/// that sends a paragraph gets the first line's worth.
const MAX_VERSION_LENGTH: usize = 32;

/// Most capabilities kept from one peer, and the longest each may be.
const MAX_CAPS: usize = 32;
const MAX_CAP_LENGTH: usize = 32;

/// The hello as it goes on the wire.
///
/// JSON rather than a protobuf message of our own: nothing reads it but other
/// MumbleWay clients, and a new field must be ignorable by an older one. Unknown
/// keys are therefore **accepted and dropped** — do not add
/// `deny_unknown_fields`, or every client that predates a field refuses the
/// hello that carries it.
#[derive(Debug, Serialize, Deserialize)]
struct Wire {
    app: String,
    proto: u32,
    #[serde(default)]
    version: String,
    #[serde(default)]
    caps: Vec<String>,
    #[serde(default)]
    reply: bool,
}

/// A hello, as received.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hello {
    pub version: String,
    pub proto: u32,
    pub caps: Vec<String>,
    /// True for an answer to somebody's announcement, which must not itself be
    /// answered.
    pub reply: bool,
}

/// What is known about another session that has identified itself as MumbleWay.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Peer {
    pub version: String,
    pub proto: u32,
    pub caps: Vec<String>,
}

impl Peer {
    /// Whether this peer said it understands `cap`. A hint, never a credential —
    /// see the module notes on trust.
    pub fn supports(&self, cap: &str) -> bool {
        self.caps.iter().any(|c| c == cap)
    }
}

/// Encodes a hello. Always within [`MAX_DATA_LENGTH`] for any version this app
/// could carry.
pub fn encode_hello(version: &str, caps: &[&str], reply: bool) -> Vec<u8> {
    let wire = Wire {
        app: APP_NAME.to_string(),
        proto: PROTOCOL,
        version: clean(version, MAX_VERSION_LENGTH),
        caps: caps.iter().map(|c| clean(c, MAX_CAP_LENGTH)).collect(),
        reply,
    };
    serde_json::to_vec(&wire).unwrap_or_default()
}

/// Decodes a hello, or `None` for anything that is not one.
///
/// **Everything here arrived from another client**, so nothing is trusted: the
/// payload is length-checked before it is parsed, the name must match exactly,
/// and every string kept is stripped of control characters and cut to length
/// before it can reach a roster.
pub fn decode_hello(data: &[u8]) -> Option<Hello> {
    if data.is_empty() || data.len() > MAX_DATA_LENGTH {
        return None;
    }
    let wire: Wire = serde_json::from_slice(data).ok()?;
    if wire.app != APP_NAME {
        return None;
    }
    Some(Hello {
        version: clean(&wire.version, MAX_VERSION_LENGTH),
        proto: wire.proto,
        caps: wire
            .caps
            .iter()
            .take(MAX_CAPS)
            .map(|c| clean(c, MAX_CAP_LENGTH))
            .filter(|c| !c.is_empty())
            .collect(),
        reply: wire.reply,
    })
}

/// Drops control characters and surrounding whitespace, and cuts to `max`
/// characters — counted as characters, not bytes, so a Cyrillic version string
/// is not cut through the middle of a letter.
fn clean(s: &str, max: usize) -> String {
    s.chars()
        .filter(|c| !c.is_control())
        .collect::<String>()
        .trim()
        .chars()
        .take(max)
        .collect()
}

/// Whether the server relays `PluginDataTransmission`, which arrived in 1.4.0.
///
/// `v2` is the 1.5-era encoding and wins when present and non-zero; `v1` is the
/// legacy one. A server that sent neither is treated as too old — the
/// conservative reading, and the one that matches the official client.
pub fn server_supports_plugin_data(v1: Option<u32>, v2: Option<u64>) -> bool {
    let (major, minor) = match (v2.filter(|v| *v != 0), v1) {
        (Some(v), _) => (v >> 48, (v >> 32) & 0xFFFF),
        (None, Some(v)) => ((v >> 16) as u64, ((v >> 8) & 0xFF) as u64),
        (None, None) => return false,
    };
    (major, minor) >= (1, 4)
}

#[derive(Debug, Serialize, Deserialize)]
struct MuteWire {
    mute: bool,
}

/// Encodes a request to mute (`true`) or unmute (`false`).
pub fn encode_mute_request(mute: bool) -> Vec<u8> {
    serde_json::to_vec(&MuteWire { mute }).unwrap_or_default()
}

/// Decodes a mute request, or `None` for anything that is not one. The same
/// distrust as [`decode_hello`]: this arrived from another client.
pub fn decode_mute_request(data: &[u8]) -> Option<bool> {
    if data.is_empty() || data.len() > MAX_DATA_LENGTH {
        return None;
    }
    serde_json::from_slice::<MuteWire>(data)
        .ok()
        .map(|w| w.mute)
}

#[derive(Debug, Serialize, Deserialize)]
struct MutedYouWire {
    muted: bool,
}

/// Encodes "I have silenced you for myself" (`true`) or "you are back"
/// (`false`).
pub fn encode_muted_you(muted: bool) -> Vec<u8> {
    serde_json::to_vec(&MutedYouWire { muted }).unwrap_or_default()
}

/// Decodes that notice, or `None` for anything that is not one.
pub fn decode_muted_you(data: &[u8]) -> Option<bool> {
    if data.is_empty() || data.len() > MAX_DATA_LENGTH {
        return None;
    }
    serde_json::from_slice::<MutedYouWire>(data)
        .ok()
        .map(|w| w.muted)
}

/// What to do with a mute request that arrived from a peer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteMuteDecision {
    /// Change the microphone to the requested state.
    Apply,
    /// It is already in that state. Nothing to do, and no cue — a tone for a
    /// change that did not happen would teach the rider to ignore the tone.
    AlreadyThere,
    /// An unmute, and the rider has turned off being unmuted by others.
    UnmuteNotAllowed,
    /// Too soon after the last remote change; see [`REMOTE_MUTE_COOLDOWN`].
    CoolingDown,
}

/// Decides whether a remote mute request is acted on.
///
/// One per app rather than per connection, so the cooldown holds across every
/// server a rider is on at once.
#[derive(Debug, Default)]
pub struct RemoteMuteGuard {
    last: Option<Instant>,
}

impl RemoteMuteGuard {
    /// Decides, and records the decision if it is to act.
    ///
    /// **Only a mute that is acted on starts the cooldown, and only a mute
    /// waits for it.** An unmute hands the microphone back, which is the thing
    /// the cooldown exists to protect; delaying one would be the cooldown
    /// causing the fault it was written to prevent. A request that is ignored —
    /// already in that state, or an unmute the rider has refused — changes
    /// nothing, so it must not spend the half-minute either: otherwise a stream
    /// of refused unmutes would block the one mute request that was genuinely
    /// needed.
    pub fn decide(
        &mut self,
        mute: bool,
        currently_muted: bool,
        allow_unmute: bool,
        now: Instant,
    ) -> RemoteMuteDecision {
        if mute == currently_muted {
            return RemoteMuteDecision::AlreadyThere;
        }
        if !mute && !allow_unmute {
            return RemoteMuteDecision::UnmuteNotAllowed;
        }
        if mute
            && self
                .last
                .is_some_and(|t| now.saturating_duration_since(t) < REMOTE_MUTE_COOLDOWN)
        {
            return RemoteMuteDecision::CoolingDown;
        }
        if mute {
            self.last = Some(now);
        }
        RemoteMuteDecision::Apply
    }
}

/// How close together two reports of the same mute change have to be to count
/// as one change.
pub const MUTE_CUE_ECHO: Duration = Duration::from_secs(2);

/// Plays one cue for one change, when that change arrives by two routes.
///
/// A rider who *does* have permission to mute sends both the server mute and
/// the request (see `SetUserServerMute`), and both land: the server reports the
/// mute, then the request mutes the microphone. Two falling pairs a moment
/// apart is not two things happening — and in this cue vocabulary a figure
/// said twice means *urgent*, which would be exactly the wrong thing to hear.
#[derive(Debug, Default)]
pub struct MuteCueEcho {
    last: Option<(Instant, bool)>,
}

impl MuteCueEcho {
    /// Whether the cue for the microphone going `muted` should play, noting it
    /// if so.
    ///
    /// A suppressed one does not extend the window: it measures from the cue
    /// the rider actually heard.
    pub fn should_play(&mut self, muted: bool, now: Instant) -> bool {
        if let Some((at, was)) = self.last {
            if was == muted && now.saturating_duration_since(at) < MUTE_CUE_ECHO {
                return false;
            }
        }
        self.last = Some((now, muted));
        true
    }
}

/// Which sessions on this server run MumbleWay, and who is owed a reply.
///
/// Lives inside a connection's state, so a reconnect starts it empty — session
/// numbers are the server's, and they mean nothing on the next connection.
#[derive(Debug, Default)]
pub struct Peers {
    known: HashMap<u32, Peer>,
    owed: BTreeSet<u32>,
    /// Sessions a mute request went to, and when — see [`Peers::backup_covers`].
    backups: HashMap<u32, Instant>,
    /// Sessions that have said they are not listening to us.
    ///
    /// Their word for it, and nothing else could be: this is a decision taken
    /// inside somebody else's app, which neither the server nor we can observe.
    /// Worth exactly as much as the peer is honest — which is enough for a mark
    /// in a roster and would not be enough for anything that mattered.
    muted_us: BTreeSet<u32>,
}

impl Peers {
    /// Records a hello from `sender`.
    ///
    /// An announcement puts the sender on the list of sessions owed a reply; a
    /// reply does not, which is what ends the exchange after one round.
    pub fn on_hello(&mut self, sender: u32, hello: Hello) {
        if !hello.reply {
            self.owed.insert(sender);
        }
        self.known.insert(
            sender,
            Peer {
                version: hello.version,
                proto: hello.proto,
                caps: hello.caps,
            },
        );
    }

    /// Forgets a session that has left. Its number may be handed to somebody
    /// else, who is not known to run anything.
    pub fn forget(&mut self, session: u32) {
        self.known.remove(&session);
        self.owed.remove(&session);
        self.backups.remove(&session);
        self.muted_us.remove(&session);
    }

    /// Whether `session` said it understands being told about a local mute.
    pub fn accepts_mute_notice(&self, session: u32) -> bool {
        self.get(session)
            .is_some_and(|p| p.supports(CAP_MUTE_NOTICE))
    }

    /// Records that `session` has silenced us for themselves, or let us back.
    pub fn note_muted_us(&mut self, session: u32, muted: bool) {
        if muted {
            self.muted_us.insert(session);
        } else {
            self.muted_us.remove(&session);
        }
    }

    /// Whether `session` has told us they are not listening.
    pub fn has_muted_us(&self, session: u32) -> bool {
        self.muted_us.contains(&session)
    }

    /// Whether `session` said it will act on a mute request.
    pub fn accepts_remote_mute(&self, session: u32) -> bool {
        self.get(session)
            .is_some_and(|p| p.supports(CAP_REMOTE_MUTE))
    }

    /// Records that a mute request went to `session` alongside a server mute.
    pub fn note_backup(&mut self, session: u32, now: Instant) {
        self.backups.insert(session, now);
    }

    /// Whether a refusal from the server about `session` is covered by a mute
    /// request sent to it moments ago.
    ///
    /// **Without this the roster tells a lie at the moment it works.** A rider
    /// without the Mute permission taps "Mute on server": the server refuses,
    /// the request reaches the peer, the peer mutes itself — and the screen
    /// says "the server refused", about a mute that has just happened. Inside a
    /// few seconds of a request to that same session, the refusal is the
    /// expected half of a pair and not news.
    pub fn backup_covers(&self, session: u32, now: Instant) -> bool {
        self.backups
            .get(&session)
            .is_some_and(|t| now.saturating_duration_since(*t) < BACKUP_WINDOW)
    }

    /// Takes every session owed a reply, so all of them can be answered in a
    /// single message.
    pub fn take_owed(&mut self) -> Vec<u32> {
        std::mem::take(&mut self.owed).into_iter().collect()
    }

    pub fn get(&self, session: u32) -> Option<&Peer> {
        self.known.get(&session)
    }

    pub fn is_mumbleway(&self, session: u32) -> bool {
        self.known.contains_key(&session)
    }

    /// Of the sessions present, the ones that have not identified themselves —
    /// the only ones an announcement needs to reach again.
    ///
    /// Ourselves excluded: the server would relay it straight back.
    pub fn unheard(&self, present: &[u32], me: Option<u32>) -> Vec<u32> {
        present
            .iter()
            .copied()
            .filter(|s| Some(*s) != me && !self.known.contains_key(s))
            .collect()
    }
}

/// Decides when to announce, and when to give up announcing.
///
/// Starts armed, so the first announcement goes out on the first tick after the
/// session is up rather than needing a separate path of its own. Time is passed
/// in rather than read, so the stopping rules can be tested without waiting.
#[derive(Debug)]
pub struct Announcer {
    left: u8,
    next: Option<Instant>,
}

impl Announcer {
    /// Armed to announce at `now`.
    pub fn armed(now: Instant) -> Self {
        Self {
            left: MAX_ANNOUNCEMENTS,
            next: Some(now),
        }
    }

    /// Whether an announcement is due.
    pub fn is_due(&self, now: Instant) -> bool {
        self.next.is_some_and(|due| now >= due)
    }

    /// Records that one has just gone out, and schedules the next unless the
    /// allowance is spent.
    pub fn sent(&mut self, now: Instant) {
        self.left = self.left.saturating_sub(1);
        self.next = (self.left > 0).then(|| now + RETRY_INTERVAL);
    }

    /// Stops for good — everybody present has answered, so there is nobody left
    /// to ask, and repeating would be talking to people who already replied.
    pub fn stop(&mut self) {
        self.left = 0;
        self.next = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hello(version: &str, reply: bool) -> Hello {
        Hello {
            version: version.into(),
            proto: PROTOCOL,
            caps: vec![],
            reply,
        }
    }

    #[test]
    fn a_hello_survives_the_round_trip() {
        let bytes = encode_hello("1.0.1", &["loss-report"], false);
        let h = decode_hello(&bytes).expect("our own hello must decode");
        assert_eq!(h.version, "1.0.1");
        assert_eq!(h.proto, PROTOCOL);
        assert_eq!(h.caps, vec!["loss-report".to_string()]);
        assert!(!h.reply);
    }

    #[test]
    fn a_hello_fits_what_the_server_will_relay() {
        // The server drops a plugin message over these limits without telling
        // the sender, so a hello that grew past them would silently stop
        // detecting anybody.
        let bytes = encode_hello(&"9".repeat(500), &["c"; 64], true);
        assert!(bytes.len() <= MAX_DATA_LENGTH, "{} bytes", bytes.len());
        assert!(DATA_ID_HELLO.len() <= MAX_DATA_ID_LENGTH);
        assert!(DATA_ID_HELLO.starts_with(DATA_ID_PREFIX));
    }

    #[test]
    fn anything_that_is_not_a_mumbleway_hello_is_ignored() {
        assert_eq!(decode_hello(b""), None);
        assert_eq!(decode_hello(b"not json"), None);
        assert_eq!(decode_hello(br#"{"proto":1}"#), None, "no app name");
        assert_eq!(
            decode_hello(br#"{"app":"SomethingElse","proto":1}"#),
            None,
            "another app's data"
        );
        assert_eq!(
            decode_hello(br#"{"app":"mumbleway","proto":1}"#),
            None,
            "the name must match exactly"
        );
        // Over the server's own limit: something that cannot have come through
        // a real server is not worth parsing.
        let mut big = br#"{"app":"MumbleWay","proto":1,"version":""#.to_vec();
        big.extend(std::iter::repeat_n(b'x', MAX_DATA_LENGTH));
        big.extend(br#""}"#);
        assert_eq!(decode_hello(&big), None);
    }

    #[test]
    fn a_newer_client_is_still_understood() {
        // A field this build has never heard of must not stop it recognising a
        // later MumbleWay, or every release that adds one would make the two
        // versions invisible to each other.
        let h = decode_hello(
            br#"{"app":"MumbleWay","proto":7,"version":"3.0","caps":["x"],"future":{"a":1}}"#,
        )
        .expect("unknown keys are dropped, not refused");
        assert_eq!(h.proto, 7);
        assert_eq!(h.version, "3.0");
    }

    #[test]
    fn missing_optional_fields_take_their_defaults() {
        let h = decode_hello(br#"{"app":"MumbleWay","proto":1}"#).unwrap();
        assert_eq!(h.version, "");
        assert!(h.caps.is_empty());
        assert!(!h.reply, "an unmarked hello is an announcement");
    }

    #[test]
    fn what_a_peer_says_is_cleaned_before_it_reaches_the_roster() {
        let h = decode_hello(
            "{\"app\":\"MumbleWay\",\"proto\":1,\"version\":\"  1.0\\u0007\\n.1 extra text that goes on for far too long  \"}"
                .as_bytes(),
        )
        .unwrap();
        assert!(!h.version.chars().any(char::is_control), "{:?}", h.version);
        assert!(h.version.chars().count() <= MAX_VERSION_LENGTH);
        assert!(h.version.starts_with("1.0.1"));
    }

    #[test]
    fn a_version_is_cut_by_characters_not_bytes() {
        // Cutting a UTF-8 string at a byte count lands inside a letter and
        // panics; counting characters cannot.
        let long = "в".repeat(100);
        let h = decode_hello(
            format!(r#"{{"app":"MumbleWay","proto":1,"version":"{long}"}}"#).as_bytes(),
        )
        .unwrap();
        assert_eq!(h.version.chars().count(), MAX_VERSION_LENGTH);
    }

    #[test]
    fn capabilities_are_bounded_and_blank_ones_dropped() {
        let caps: Vec<String> = (0..100).map(|i| format!("cap{i}")).collect();
        let mut json = serde_json::json!({"app": "MumbleWay", "proto": 1, "caps": caps});
        json["caps"].as_array_mut().unwrap().insert(0, "   ".into());
        let h = decode_hello(serde_json::to_string(&json).unwrap().as_bytes()).unwrap();
        assert!(h.caps.len() <= MAX_CAPS);
        assert!(h.caps.iter().all(|c| !c.is_empty()));
    }

    #[test]
    fn plugin_data_needs_a_1_4_server() {
        use crate::proto::{version_v1, version_v2};
        assert!(server_supports_plugin_data(None, Some(version_v2(1, 4, 0))));
        assert!(server_supports_plugin_data(
            None,
            Some(version_v2(1, 5, 634))
        ));
        assert!(server_supports_plugin_data(None, Some(version_v2(2, 0, 0))));
        assert!(!server_supports_plugin_data(
            None,
            Some(version_v2(1, 3, 4))
        ));

        // v1 alone, as a pre-1.5 server sends it.
        assert!(server_supports_plugin_data(Some(version_v1(1, 4, 0)), None));
        assert!(!server_supports_plugin_data(
            Some(version_v1(1, 3, 5)),
            None
        ));

        // A zero v2 is not a version; fall back to v1 rather than calling it 0.0.
        // (v1 packs the patch into a byte, so a real 1.4.287 cannot be written in
        // it at all — which is the whole reason v2 exists, and why v2 wins.)
        assert!(server_supports_plugin_data(
            Some(version_v1(1, 4, 255)),
            Some(0)
        ));

        // Nothing to go on: assume the server is too old, as the official
        // client does.
        assert!(!server_supports_plugin_data(None, None));
    }

    #[test]
    fn an_announcement_is_owed_a_reply_and_a_reply_is_not() {
        // The one rule that stops two clients answering each other for ever.
        let mut p = Peers::default();
        p.on_hello(10, hello("1.0.1", false));
        p.on_hello(11, hello("1.0.2", true));

        assert!(p.is_mumbleway(10));
        assert!(p.is_mumbleway(11));
        assert_eq!(p.take_owed(), vec![10], "only the announcement is answered");
        assert!(p.take_owed().is_empty(), "and only once");
    }

    #[test]
    fn several_announcements_are_answered_together() {
        // Answered in one message rather than one each, which is what keeps a
        // crowd arriving at once under the server's burst limit.
        let mut p = Peers::default();
        for s in [30, 12, 20, 12] {
            p.on_hello(s, hello("1.0.1", false));
        }
        assert_eq!(p.take_owed(), vec![12, 20, 30], "deduplicated, in order");
    }

    #[test]
    fn a_session_that_leaves_is_forgotten_and_no_longer_owed() {
        let mut p = Peers::default();
        p.on_hello(5, hello("1.0.1", false));
        p.forget(5);
        assert!(!p.is_mumbleway(5));
        assert!(p.get(5).is_none());
        assert!(
            p.take_owed().is_empty(),
            "a reply to somebody who has gone is a reply to whoever gets the number next"
        );
    }

    #[test]
    fn a_later_hello_updates_what_is_known() {
        let mut p = Peers::default();
        p.on_hello(7, hello("1.0.1", true));
        p.on_hello(7, hello("1.0.2", true));
        assert_eq!(p.get(7).unwrap().version, "1.0.2");
    }

    #[test]
    fn only_the_sessions_that_have_not_answered_are_told_again() {
        let mut p = Peers::default();
        p.on_hello(20, hello("1.0.1", true));
        let present = [10, 20, 30, 99];

        // 20 answered, 99 is us, so 10 and 30 are what is left to ask.
        assert_eq!(p.unheard(&present, Some(99)), vec![10, 30]);

        p.on_hello(10, hello("1.0.1", true));
        p.on_hello(30, hello("1.0.1", true));
        assert!(
            p.unheard(&present, Some(99)).is_empty(),
            "everybody present has answered"
        );
    }

    #[test]
    fn we_never_announce_to_ourselves() {
        // The server would relay it straight back to us.
        let p = Peers::default();
        assert_eq!(p.unheard(&[7], Some(7)), Vec::<u32>::new());
        assert_eq!(p.unheard(&[7], None), vec![7], "before ServerSync names us");
    }

    #[test]
    fn the_announcement_goes_out_at_once_and_then_every_five_seconds() {
        let t0 = Instant::now();
        let mut a = Announcer::armed(t0);
        assert!(a.is_due(t0), "the first one waits for nothing");

        a.sent(t0);
        assert!(!a.is_due(t0 + Duration::from_secs(4)), "too soon");
        assert!(a.is_due(t0 + RETRY_INTERVAL));
    }

    #[test]
    fn announcing_stops_after_its_allowance() {
        // **The rule that keeps this from becoming a broadcast every five
        // seconds for the whole ride.** Hearing nothing is the ordinary case on
        // a server where nobody else runs MumbleWay, so "repeat until somebody
        // answers" would never stop.
        let mut now = Instant::now();
        let mut a = Announcer::armed(now);
        let mut sent = 0;
        for _ in 0..100 {
            if a.is_due(now) {
                a.sent(now);
                sent += 1;
            }
            now += RETRY_INTERVAL;
        }
        assert_eq!(sent, MAX_ANNOUNCEMENTS as usize);
        assert!(!a.is_due(now + Duration::from_secs(3600)));
    }

    #[test]
    fn announcing_stops_early_once_everybody_has_answered() {
        let now = Instant::now();
        let mut a = Announcer::armed(now);
        a.sent(now);
        assert!(a.is_due(now + RETRY_INTERVAL), "more attempts remain");

        a.stop();
        assert!(
            !a.is_due(now + RETRY_INTERVAL),
            "nobody left to ask, so nothing more to say"
        );
    }

    #[test]
    fn this_build_offers_the_mute_notice() {
        // Same reason as the one below: a capability missing from the hello is
        // a feature every peer declines to use, for ever and without a word.
        assert!(CAPABILITIES.contains(&CAP_MUTE_NOTICE));
        assert!(DATA_ID_MUTED_YOU.starts_with(DATA_ID_PREFIX));
        assert!(DATA_ID_MUTED_YOU.len() <= MAX_DATA_ID_LENGTH);
    }

    #[test]
    fn being_told_you_were_muted_survives_the_round_trip() {
        assert_eq!(decode_muted_you(&encode_muted_you(true)), Some(true));
        assert_eq!(decode_muted_you(&encode_muted_you(false)), Some(false));
        // Written by another client, so everything that is not one of ours is
        // dropped rather than guessed at.
        assert_eq!(decode_muted_you(b""), None);
        assert_eq!(decode_muted_you(b"{}"), None, "it says nothing");
        assert_eq!(decode_muted_you(b"go away"), None);
        assert_eq!(
            decode_muted_you(&vec![b'x'; MAX_DATA_LENGTH + 1]),
            None,
            "longer than the server will carry"
        );
    }

    #[test]
    fn who_has_us_muted_is_remembered_until_they_leave() {
        let mut peers = Peers::default();
        let hello = Hello {
            version: "1.0".into(),
            proto: PROTOCOL,
            caps: vec![CAP_MUTE_NOTICE.into()],
            reply: false,
        };
        peers.on_hello(7, hello);
        assert!(peers.accepts_mute_notice(7));
        assert!(!peers.has_muted_us(7), "nobody has said anything yet");

        peers.note_muted_us(7, true);
        assert!(peers.has_muted_us(7));
        peers.note_muted_us(7, false);
        assert!(!peers.has_muted_us(7), "they let us back in");

        // **A session number is the server's and it gets handed on.** Leaving
        // it behind would mark whoever inherits 7 as having muted us, which
        // they have never heard of.
        peers.note_muted_us(7, true);
        peers.forget(7);
        assert!(!peers.has_muted_us(7));
        assert!(!peers.accepts_mute_notice(7));
    }

    #[test]
    fn a_peer_without_the_capability_is_not_told() {
        let mut peers = Peers::default();
        peers.on_hello(
            9,
            Hello {
                version: "0.9".into(),
                proto: PROTOCOL,
                caps: vec![CAP_REMOTE_MUTE.into()],
                reply: false,
            },
        );
        assert!(
            !peers.accepts_mute_notice(9),
            "an older build would be told something it cannot explain"
        );
    }

    #[test]
    fn this_build_offers_remote_mute() {
        // Without it in the hello, no peer would ever send us a request, and
        // the feature would be present and unreachable.
        assert!(CAPABILITIES.contains(&CAP_REMOTE_MUTE));
        assert!(DATA_ID_MUTE.starts_with(DATA_ID_PREFIX));
        assert!(DATA_ID_MUTE.len() <= MAX_DATA_ID_LENGTH);
    }

    #[test]
    fn a_mute_request_survives_the_round_trip() {
        assert_eq!(decode_mute_request(&encode_mute_request(true)), Some(true));
        assert_eq!(
            decode_mute_request(&encode_mute_request(false)),
            Some(false)
        );
        assert_eq!(decode_mute_request(b""), None);
        assert_eq!(decode_mute_request(b"{}"), None, "no instruction in it");
        assert_eq!(decode_mute_request(b"yes please"), None);
    }

    #[test]
    fn only_a_peer_that_offered_it_is_sent_a_mute_request() {
        let mut p = Peers::default();
        p.on_hello(
            1,
            Hello {
                version: "1.0.2".into(),
                proto: 1,
                caps: vec![CAP_REMOTE_MUTE.into()],
                reply: true,
            },
        );
        p.on_hello(2, hello("1.0.1", true)); // MumbleWay, but before this feature
        assert!(p.accepts_remote_mute(1));
        assert!(
            !p.accepts_remote_mute(2),
            "would not know what to do with it"
        );
        assert!(
            !p.accepts_remote_mute(3),
            "not known to run MumbleWay at all"
        );
    }

    #[test]
    fn a_request_is_acted_on_once_and_then_the_rider_has_their_microphone() {
        // **The protection against being held muted.** A request every second
        // would otherwise re-mute somebody the moment they unmuted themselves.
        let t0 = Instant::now();
        let mut g = RemoteMuteGuard::default();
        assert_eq!(g.decide(true, false, true, t0), RemoteMuteDecision::Apply);

        // They unmute themselves; the next request, from anybody, waits.
        let soon = t0 + Duration::from_secs(10);
        assert_eq!(
            g.decide(true, false, true, soon),
            RemoteMuteDecision::CoolingDown
        );

        let later = t0 + REMOTE_MUTE_COOLDOWN;
        assert_eq!(
            g.decide(true, false, true, later),
            RemoteMuteDecision::Apply
        );
    }

    #[test]
    fn the_microphone_is_handed_back_without_waiting() {
        // The fault this was reported as: muted through the app, unmuted a
        // moment later by the same rider, and the microphone stayed shut for
        // half a minute — the cooldown refusing the one request that undoes
        // the thing it exists to prevent.
        let t0 = Instant::now();
        let mut g = RemoteMuteGuard::default();
        assert_eq!(g.decide(true, false, true, t0), RemoteMuteDecision::Apply);

        let moment = t0 + Duration::from_secs(2);
        assert_eq!(
            g.decide(false, true, true, moment),
            RemoteMuteDecision::Apply,
            "an unmute does not wait for the cooldown"
        );

        // And muting again still does: handing the microphone back is not a
        // way of buying another mute.
        assert_eq!(
            g.decide(true, false, true, t0 + Duration::from_secs(3)),
            RemoteMuteDecision::CoolingDown
        );
        assert_eq!(
            g.decide(true, false, true, t0 + REMOTE_MUTE_COOLDOWN),
            RemoteMuteDecision::Apply
        );
    }

    #[test]
    fn a_rider_who_refused_remote_unmute_is_never_unmuted() {
        let mut g = RemoteMuteGuard::default();
        assert_eq!(
            g.decide(false, true, false, Instant::now()),
            RemoteMuteDecision::UnmuteNotAllowed
        );
        // Muting is still allowed: the setting is about opening a microphone,
        // not about closing one.
        let mut g = RemoteMuteGuard::default();
        assert_eq!(
            g.decide(true, false, false, Instant::now()),
            RemoteMuteDecision::Apply
        );
    }

    #[test]
    fn a_request_that_changes_nothing_is_silent_and_costs_nothing() {
        let t0 = Instant::now();
        let mut g = RemoteMuteGuard::default();
        // Already muted: no change, so no cue — and no cooldown spent.
        assert_eq!(
            g.decide(true, true, true, t0),
            RemoteMuteDecision::AlreadyThere
        );
        assert_eq!(g.decide(true, false, true, t0), RemoteMuteDecision::Apply);
    }

    #[test]
    fn refused_unmutes_do_not_block_a_needed_mute() {
        // A peer sending unmutes that the rider has refused must not spend the
        // cooldown, or they could block the one mute request that mattered.
        let t0 = Instant::now();
        let mut g = RemoteMuteGuard::default();
        for s in 0..5 {
            let t = t0 + Duration::from_secs(s);
            assert_eq!(
                g.decide(false, true, false, t),
                RemoteMuteDecision::UnmuteNotAllowed
            );
        }
        // Unmuted by their own hand in the meantime, then asked to mute.
        assert_eq!(
            g.decide(true, false, false, t0 + Duration::from_secs(6)),
            RemoteMuteDecision::Apply
        );
    }

    #[test]
    fn one_change_by_two_routes_is_one_cue() {
        let t0 = Instant::now();
        let mut e = MuteCueEcho::default();
        assert!(e.should_play(true, t0), "the server's report");
        assert!(
            !e.should_play(true, t0 + Duration::from_millis(80)),
            "the request, right behind it, is the same change"
        );
        // Measured from the cue that was heard, not pushed on by the echo.
        assert!(e.should_play(true, t0 + MUTE_CUE_ECHO));
    }

    #[test]
    fn opposite_changes_are_both_heard() {
        // Muted and straight back: two real changes, however close.
        let t0 = Instant::now();
        let mut e = MuteCueEcho::default();
        assert!(e.should_play(true, t0));
        assert!(e.should_play(false, t0 + Duration::from_millis(100)));
        assert!(e.should_play(true, t0 + Duration::from_millis(200)));
    }

    #[test]
    fn a_refusal_right_after_a_request_is_covered_by_it() {
        let t0 = Instant::now();
        let mut p = Peers::default();
        p.note_backup(4, t0);
        assert!(p.backup_covers(4, t0 + Duration::from_secs(1)));
        assert!(
            !p.backup_covers(4, t0 + BACKUP_WINDOW),
            "too late to be the pair"
        );
        assert!(
            !p.backup_covers(5, t0),
            "a different person's refusal is news"
        );

        p.forget(4);
        assert!(
            !p.backup_covers(4, t0),
            "the number may belong to somebody else now"
        );
    }

    #[test]
    fn a_peer_reports_its_capabilities() {
        let mut p = Peers::default();
        p.on_hello(
            3,
            Hello {
                version: "2.0".into(),
                proto: 2,
                caps: vec!["loss-report".into()],
                reply: true,
            },
        );
        let peer = p.get(3).unwrap();
        assert!(peer.supports("loss-report"));
        assert!(!peer.supports("something-else"));
    }
}
