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
    /// Access tokens to present at the handshake.
    ///
    /// **A token is a password that is spelled as a group name.** A server
    /// grants permissions to a group, and anybody who presents a token with
    /// that group's name is treated as a member of it — which is how a
    /// password-protected channel works in Mumble, there being no such thing
    /// as a channel password.
    ///
    /// Kept with the server rather than typed each time, because a rider who
    /// has to retype one at a junction does not have it.
    #[serde(default)]
    pub access_tokens: Vec<String>,
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
            access_tokens: Vec::new(),
        }
    }
}

impl Default for ServerProfile {
    fn default() -> Self {
        Self::new("", "", 64738, "")
    }
}

/// What this server will take, in bytes. Zero means it set no limit.
///
/// Both are enforced by refusal rather than by truncation: a comment or a
/// message over the limit comes back as `TextTooLong` and nothing changes, so
/// a client that does not know the limit looks to its rider like a client that
/// ignored them. A picture over `image_message_length` is refused with the
/// same code, which is the server's own confusion and not this client's.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerLimits {
    pub message_length: u32,
    pub image_message_length: u32,
}

impl ServerLimits {
    /// Cuts text to what this server accepts, by characters rather than bytes.
    ///
    /// **The limit is in bytes and the cut is in characters**, which is the
    /// conservative direction: a Russian note cut to the byte would split a
    /// character and arrive as mojibake, so this takes whole characters until
    /// the bytes fit.
    pub fn fit_text(&self, text: &str) -> String {
        let max = self.message_length as usize;
        if max == 0 || text.len() <= max {
            return text.to_string();
        }
        let mut out = String::with_capacity(max);
        for c in text.chars() {
            if out.len() + c.len_utf8() > max {
                break;
            }
            out.push(c);
        }
        out
    }

    /// Whether an image of `bytes` fits.
    pub fn image_fits(&self, bytes: usize) -> bool {
        self.image_message_length == 0 || bytes <= self.image_message_length as usize
    }
}

/// What a server will tell an admin about one user.
///
/// Every field is optional on the wire and most are withheld from an ordinary
/// rider — the client version, the address and the certificate are handed over
/// only to somebody holding Ban on the root channel, or to the user themselves.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserDetails {
    pub session: u32,
    /// The client they run, as it describes itself.
    pub release: String,
    pub os: String,
    pub os_version: String,
    /// Where they connected from. Empty when the server withheld it.
    pub address: String,
    /// Whether their certificate is one a certificate authority vouches for,
    /// rather than the self-signed kind every client makes for itself.
    pub strong_certificate: bool,
    /// How long they have been connected, and how long since they did
    /// anything, in seconds.
    pub online_secs: u32,
    pub idle_secs: u32,
}

/// One rule in a channel's access list.
///
/// A rule names either a registered user or a group, and carries two bitmasks:
/// what it grants and what it denies. The bits are `permissions::*`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AclRule {
    /// Whether it applies to this channel.
    pub apply_here: bool,
    /// Whether it applies to channels under it.
    pub apply_subs: bool,
    /// Whether it came from a parent channel. **Inherited rules cannot be
    /// edited here**: they belong to the channel that defines them, and a
    /// server drops them from any list written back.
    pub inherited: bool,
    /// The registered user it names, if it names one.
    pub user_id: Option<u32>,
    /// The group it names, if it names one.
    pub group: Option<String>,
    pub grant: u32,
    pub deny: u32,
}

/// A named group of users on a channel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AclGroup {
    pub name: String,
    /// Whether the group itself came from a parent channel.
    pub inherited: bool,
    /// Whether it takes its members from the parent as well.
    pub inherit: bool,
    /// Whether channels under this one may inherit it.
    pub inheritable: bool,
    /// Members added here, by registered user id.
    pub add: Vec<u32>,
    /// Members removed here, when the group is inherited.
    pub remove: Vec<u32>,
    /// Members that came from the parent.
    pub inherited_members: Vec<u32>,
}

/// A channel's whole access list, as the server states it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChannelAcl {
    pub channel_id: u32,
    /// Whether this channel takes its parent's rules as well as its own.
    pub inherit_acls: bool,
    pub groups: Vec<AclGroup>,
    pub rules: Vec<AclRule>,
}

/// Somebody the server has an account for, whether or not they are here.
///
/// Registration is what makes a name belong to a person: an unregistered name
/// is free for anybody to take once its holder disconnects, and no ACL can
/// name them. The list is an admin's view of who the server knows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegisteredUser {
    pub user_id: u32,
    pub name: String,
    /// The server's own date string, or empty if it never said.
    pub last_seen: String,
    /// The channel they were last in.
    pub last_channel: u32,
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
    /// Silenced by the server itself, because this rider lacks Speak
    /// permission in the channel they are in.
    ///
    /// **The third way to be inaudible, and the only one nobody chose.** The
    /// server sets it from the ACL on entering a channel and then discards
    /// every voice packet, with no refusal and no disconnection — so a rider
    /// whose client ignores this flag talks into nothing while their own meter
    /// moves, because the meter is measured before the wire.
    #[serde(default)]
    pub suppress: bool,
    /// Whether the server treats this rider as a priority speaker.
    ///
    /// Everyone else is ducked while they talk, which is worth showing: it
    /// explains why a channel goes quiet when one person starts, and it is not
    /// otherwise visible from anything the roster shows.
    #[serde(default)]
    pub priority_speaker: bool,
    /// The note this rider hung beside their own name, as plain text.
    ///
    /// Empty when they have none, and stripped of the markup Mumble's own
    /// client writes it in — see [`crate::session::notes`].
    #[serde(default)]
    pub comment: String,
    /// The account the server knows this rider by, if they have one.
    ///
    /// **Presence is registration.** The server sends it only for a rider with
    /// an account, so `None` means unregistered and `Some(0)` is SuperUser,
    /// whose id really is zero. Worth having beyond the yes/no: removing a
    /// registration is done by id, and the roster is where the rider doing it
    /// is standing.
    #[serde(default)]
    pub user_id: Option<u32>,
    /// Whether this rider has told us they have silenced us for themselves.
    ///
    /// **The mirror of [`UserInfo::local_mute`], and the only way it is ever
    /// visible.** A local mute is invisible by design — the server is not
    /// told — so being on the receiving end of one looks exactly like being
    /// heard. Their client says so over `PluginDataTransmission`, which makes
    /// this the one field here that rests on another client's honesty rather
    /// than on the server's word. Fine for a mark in a roster; it is load
    /// bearing for nothing.
    #[serde(default)]
    pub muted_you: bool,
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
        } else if self.suppress {
            // After the mute states: those are somebody's decision about this
            // person, and this is the channel's rule about everybody in it.
            "suppressed"
        } else if self.talking {
            "talking"
        } else {
            "silent"
        }
    }

    /// Whether we can hear this user at all right now.
    pub fn is_audible(&self) -> bool {
        !self.local_mute && !self.mute && !self.self_mute && !self.suppress
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
    /// The channels this client is listening to without having joined them.
    Listening(Vec<u32>),
    /// What this server will accept, in bytes: see [`ServerLimits`].
    Limits(ServerLimits),
    /// The bandwidth this server allows each client, in bits per second.
    ///
    /// **Not advice.** A server enforces it by dropping voice packets without
    /// telling anybody, so a client that ignores it goes inaudible with every
    /// indicator healthy. See `audio::bandwidth`.
    BandwidthCap(u32),
    /// Menu entries this server has registered, whenever the set changes.
    ///
    /// The whole set rather than the one that changed: it is small, and a menu
    /// built from a running total drifts out of step with the server in ways
    /// nobody notices until an entry does the wrong thing.
    ContextActions(Vec<crate::session::context_actions::ContextAction>),
    /// The server's ban list, in answer to asking for it.
    Bans(Vec<crate::session::bans::BanEntry>),
    /// Everybody the server has an account for.
    Registered(Vec<RegisteredUser>),
    /// One channel's access list, in answer to asking for it.
    Acl(ChannelAcl),
    /// Names for registered user ids, in answer to asking.
    UserNames(Vec<(u32, String)>),
    /// Everything the server will say about one user, for an admin.
    UserDetails(UserDetails),
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
    /// This client's own voice has been silenced by the server, or allowed
    /// again — because of where it is standing, not because anybody acted.
    ///
    /// Reported separately from the roster for the same reason as
    /// [`SessionEvent::SelfModerated`]: it happens *to* the rider, it is the
    /// explanation for a silence they cannot otherwise account for, and they
    /// are not looking at the screen.
    SelfSuppressed(bool),
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
    /// Remove a user and bar them from coming back. Requires Ban on the root
    /// channel, which is a stronger permission than Kick.
    ///
    /// The server bans the address *and* the certificate by default, which is
    /// what makes it survive a new connection.
    BanUser {
        session: u32,
        reason: String,
    },
    /// Move somebody else into a channel. Requires Move.
    MoveUser {
        session: u32,
        channel_id: u32,
    },
    /// Picks one of the menu entries this server registered.
    ///
    /// The identifier goes back exactly as it arrived; what happens next is
    /// entirely the server's business, and it may be nothing at all.
    TriggerContextAction {
        action: String,
        session: Option<u32>,
        channel_id: Option<u32>,
    },
    /// Makes a channel under `parent`. Requires MakeChannel there, or
    /// MakeTempChannel for a temporary one.
    ///
    /// **A temporary channel disappears when the last person leaves it**, which
    /// is what a group wants for one ride and not what they want for their
    /// club's room. The two are different permissions on the server for the
    /// same reason.
    CreateChannel {
        parent: u32,
        name: String,
        description: String,
        temporary: bool,
    },
    /// Renames a channel or re-describes it. Requires Write on that channel.
    EditChannel {
        channel_id: u32,
        name: Option<String>,
        description: Option<String>,
    },
    /// Removes a channel and everything under it. Requires Write on it.
    RemoveChannel(u32),
    /// Asks for one channel's access list. Requires Write on that channel.
    RequestAcl(u32),
    /// Replaces a channel's access list.
    ///
    /// **Written whole, like the ban list**, and for the same reason: the
    /// protocol has no way to change one rule. Inherited rules are dropped by
    /// the server rather than stored again, so sending them back is harmless
    /// and leaving them out is not a deletion.
    SetAcl(ChannelAcl),
    /// Asks the server for the names behind registered user ids.
    ///
    /// An access list names users by id, and an id is not something to show
    /// anybody. This is how the names are found.
    QueryUserNames(Vec<u32>),
    /// Asks for the list of registered users. Requires Register on the root
    /// channel.
    RequestRegistered,
    /// Removes registrations by user id. Requires Register on the root channel.
    ///
    /// **The protocol has no "unregister" message.** Removing somebody is a
    /// `UserList` containing them with no name, which is the same shape as the
    /// ban list: a field left out is a decision rather than an omission.
    UnregisterUsers(Vec<u32>),
    /// Registers somebody else who is connected. Requires Register on the root
    /// channel; `RegisterSelf` is the one that needs only SelfRegister.
    RegisterUser(u32),
    /// Grants or takes away priority speaker. Requires Write on the root
    /// channel — the server treats it as an administrative change to the user.
    SetPrioritySpeaker {
        session: u32,
        priority: bool,
    },
    /// Clears somebody's comment and picture. Requires ResetUserContent on the
    /// root channel.
    ResetUserContent {
        session: u32,
        comment: bool,
        texture: bool,
    },
    /// Asks for everything the server will say about one user, including the
    /// client they run and the address they came from — which it hands over
    /// only to an admin, and only when the request does not say `stats_only`.
    RequestUserDetails(u32),
    /// Ask for the server's ban list. Requires Ban on the root channel.
    RequestBans,
    /// Replace the server's ban list with this one.
    ///
    /// **There is no "remove one ban" in the protocol.** Lifting a ban means
    /// sending every other ban back unchanged, so this carries the whole list
    /// and anything left out of it is lifted. See [`crate::session::bans`].
    SetBans(Vec<crate::session::bans::BanEntry>),
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
    /// Replaces the access tokens, taking effect at once.
    ///
    /// The server re-reads them on an `Authenticate` sent after the handshake —
    /// it handles that case before the check that would otherwise refuse a
    /// second one — and it re-evaluates every channel's permissions against
    /// the new set, so a channel that was closed can open without reconnecting.
    SetAccessTokens(Vec<String>),
    /// Starts and stops listening to channels without joining them.
    ///
    /// **Hearing a channel is not being in it.** A rider stays where they are,
    /// and their voice still goes to their own channel; what changes is what
    /// reaches their ears. Needs the Listen permission on each channel, and a
    /// server may cap how many listeners a channel takes or how many channels
    /// one rider may listen to — both arrive as refusals.
    SetListening {
        add: Vec<u32>,
        remove: Vec<u32>,
    },
    /// Sets the picture shown beside our own name, or clears it with an empty
    /// one.
    ///
    /// **One picture for the rider, not one per server.** It is kept on the
    /// device and sent to each server as it connects, because Mumble has no
    /// notion of an identity that spans servers — every one of them stores its
    /// own copy against its own account.
    SetAvatar(Vec<u8>),
    /// Channel to join automatically on every future connect. `None` clears it.
    SetDefaultChannel(Option<String>),
    /// Push-to-talk / voice-activation gate.
    SetTransmitting(bool),
    /// Accept a changed server certificate and re-pin it.
    AcceptCertificate,
    Shutdown,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rider() -> UserInfo {
        UserInfo {
            session: 1,
            name: "Anna".into(),
            channel_id: 0,
            mute: false,
            deaf: false,
            self_mute: false,
            self_deaf: false,
            talking: false,
            local_mute: false,
            suppress: false,
            mumbleway: None,
            quality: None,
            user_id: None,
            muted_you: false,
            comment: String::new(),
            priority_speaker: false,
        }
    }

    #[test]
    fn a_suppressed_rider_is_not_merely_quiet() {
        // The fault this guards against is a silent one: a rider the server
        // has silenced reading as "silent", which is what somebody who simply
        // is not talking reads as.
        let mut u = rider();
        assert_eq!(u.status_label(), "silent");
        u.suppress = true;
        assert_eq!(u.status_label(), "suppressed");
    }

    #[test]
    fn a_suppressed_rider_cannot_be_heard() {
        // Anything reasoning about who is audible has to count this, or it
        // believes a rider whose every packet is discarded can be heard.
        let mut u = rider();
        assert!(u.is_audible());
        u.suppress = true;
        assert!(!u.is_audible());
    }

    #[test]
    fn a_deliberate_mute_is_named_before_the_channels_rule() {
        // Both can be true at once. "Muted" is somebody's decision about this
        // person and is the more useful word; suppressed is the rule that
        // applies to everybody standing here.
        let mut u = rider();
        u.suppress = true;
        u.mute = true;
        assert_eq!(u.status_label(), "muted");
    }

    #[test]
    fn losing_hearing_still_outranks_everything() {
        let mut u = rider();
        u.suppress = true;
        u.self_deaf = true;
        assert_eq!(u.status_label(), "deafened");
    }
}

#[cfg(test)]
mod limit_tests {
    use super::*;

    #[test]
    fn no_limit_means_no_cut() {
        let l = ServerLimits::default();
        assert_eq!(l.fit_text("anything at all"), "anything at all");
        assert!(l.image_fits(10_000_000));
    }

    #[test]
    fn text_is_cut_to_what_the_server_takes() {
        let l = ServerLimits {
            message_length: 5,
            image_message_length: 0,
        };
        assert_eq!(l.fit_text("abcdefgh"), "abcde");
        assert_eq!(l.fit_text("abc"), "abc");
    }

    #[test]
    fn a_cut_never_splits_a_character() {
        // The server counts bytes; a Russian note cut to the byte would arrive
        // as mojibake, so whole characters go until the bytes fit.
        let l = ServerLimits {
            message_length: 5,
            image_message_length: 0,
        };
        let cut = l.fit_text("ямба");
        assert!(cut.len() <= 5);
        assert_eq!(cut, "ям", "and not two and a half characters");
    }

    #[test]
    fn an_image_the_server_will_refuse_is_known_before_sending() {
        let l = ServerLimits {
            message_length: 0,
            image_message_length: 128,
        };
        assert!(l.image_fits(128));
        assert!(!l.image_fits(129));
    }
}
