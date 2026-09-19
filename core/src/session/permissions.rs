//! What this rider is allowed to do here, asked rather than found out.
//!
//! Every moderation action in Mumble is permission-gated, and the client learns
//! it was not allowed by being refused *after* trying: the server answers with
//! `PermissionDenied` and the action silently did not happen. That is a poor way
//! to run an interface — the rider taps "Mute on server", nothing changes, and a
//! message arrives to explain it — and it is the reason the remote mute request
//! needed a backup at all.
//!
//! `PermissionQuery` asks in advance. The client sends one with a `channel_id`
//! and the server replies with a bitmask of what this rider may do there, which
//! is enough to grey out what would be refused before it is reached for.
//!
//! # Two channels matter, not one
//!
//! Most permissions are per channel: muting, moving, speaking, writing. **Kick,
//! Ban, Register and SelfRegister are root-channel permissions** — the server
//! reads them from channel 0 whatever channel the rider is standing in — so
//! answering "may I kick this person" means having asked about the root as well
//! as about here. Both are queried, and [`Rights`] is the two answers put
//! together.
//!
//! # It is a hint, never the authority
//!
//! The server decides, and it decides again at the moment of the action. An ACL
//! can change between the answer and the tap, a cached answer can be stale, and
//! `flush` exists precisely because the server sometimes says *forget what I
//! told you*. So this greys out what is known to be refused and never stands in
//! for the refusal path: anything that gets through is still sent, and a
//! `PermissionDenied` coming back is still shown.
//!
//! The bit values are Mumble's `ChanACL::Perm`, taken from the server's own
//! `ACL.h` rather than inferred.

/// Change the channel: rename it, move it, edit its description.
pub const WRITE: u32 = 0x01;
/// See through the channel to what is below it.
pub const TRAVERSE: u32 = 0x02;
/// Join the channel.
pub const ENTER: u32 = 0x04;
/// Be heard in it.
pub const SPEAK: u32 = 0x08;
/// Mute and deafen other people, for everyone.
pub const MUTE_DEAFEN: u32 = 0x10;
/// Move other people between channels.
pub const MOVE: u32 = 0x20;
/// Create a sub-channel.
pub const MAKE_CHANNEL: u32 = 0x40;
/// Link channels together.
pub const LINK_CHANNEL: u32 = 0x80;
/// Whisper into the channel.
pub const WHISPER: u32 = 0x100;
/// Write to the channel's text chat.
pub const TEXT_MESSAGE: u32 = 0x200;
/// Create a temporary sub-channel.
pub const MAKE_TEMP_CHANNEL: u32 = 0x400;
/// Listen to the channel without joining it.
pub const LISTEN: u32 = 0x800;

/// Remove somebody from the server. **Root channel only.**
pub const KICK: u32 = 0x10000;
/// Remove and bar them. **Root channel only.**
pub const BAN: u32 = 0x20000;
/// Register somebody else. **Root channel only.**
pub const REGISTER: u32 = 0x40000;
/// Register yourself. **Root channel only.**
pub const SELF_REGISTER: u32 = 0x80000;
/// Clear somebody's comment or avatar. **Root channel only.**
pub const RESET_USER_CONTENT: u32 = 0x100000;

/// The server's marker that the answer came from its cache. Not a permission,
/// and masked off before anything here reads the bits.
pub const CACHED: u32 = 0x8000000;

/// The channel every root-only permission is read from.
pub const ROOT_CHANNEL: u32 = 0;

/// What the rider may do, here and on this server.
///
/// Every field is "the server said yes". A field is false when the server said
/// no **and** when it has not been asked yet, which is why [`Rights::known`]
/// exists: an interface that greys out every moderation action during the second
/// before the first reply arrives looks broken, so the caller can tell "not
/// allowed" from "no answer yet".
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Rights {
    /// Whether an answer has arrived at all.
    pub known: bool,
    /// Be heard in the current channel.
    pub speak: bool,
    /// Mute and deafen others in the current channel, for everyone.
    pub mute_deafen: bool,
    /// Move others out of the current channel.
    pub move_users: bool,
    /// Write to the current channel's chat.
    pub text: bool,
    /// Whisper into the current channel.
    pub whisper: bool,
    /// Create a sub-channel here.
    pub make_channel: bool,
    /// Rename or re-describe the current channel.
    pub write: bool,
    /// Remove somebody from the server.
    pub kick: bool,
    /// Remove and bar somebody, and read or edit the ban list.
    pub ban: bool,
    /// Register somebody else.
    pub register_others: bool,
    /// Register ourselves.
    pub self_register: bool,
}

impl Rights {
    /// Reads the two masks the server sent: the root channel's, and the one for
    /// the channel the rider is standing in.
    ///
    /// Either may be missing — the answers arrive separately, and a rider who
    /// has just moved has asked about the new channel but not heard back.
    pub fn from_masks(root: Option<u32>, here: Option<u32>) -> Self {
        let root_bits = root.unwrap_or(0) & !CACHED;
        let here_bits = here.unwrap_or(0) & !CACHED;
        let has = |bits: u32, perm: u32| bits & perm != 0;
        Self {
            known: root.is_some() || here.is_some(),
            speak: has(here_bits, SPEAK),
            mute_deafen: has(here_bits, MUTE_DEAFEN),
            move_users: has(here_bits, MOVE),
            text: has(here_bits, TEXT_MESSAGE),
            whisper: has(here_bits, WHISPER),
            make_channel: has(here_bits, MAKE_CHANNEL),
            write: has(here_bits, WRITE),
            // Root only, whatever channel the rider is in.
            kick: has(root_bits, KICK),
            ban: has(root_bits, BAN),
            register_others: has(root_bits, REGISTER),
            self_register: has(root_bits, SELF_REGISTER),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_asked_is_not_the_same_as_nothing_allowed() {
        let none = Rights::from_masks(None, None);
        assert!(
            !none.known,
            "an interface must be able to wait rather than grey out"
        );
        assert!(!none.kick);

        let answered = Rights::from_masks(Some(0), Some(0));
        assert!(answered.known, "a server that said no has still answered");
        assert!(!answered.kick);
    }

    #[test]
    fn root_only_permissions_are_read_from_the_root() {
        // A rider may be an admin of the whole server while standing in a
        // channel that grants them nothing, and the other way round.
        let r = Rights::from_masks(Some(KICK | BAN), Some(SPEAK));
        assert!(r.kick);
        assert!(r.ban);
        assert!(r.speak);
        assert!(!r.mute_deafen);

        // The same bits in the wrong mask say nothing.
        let r = Rights::from_masks(Some(SPEAK), Some(KICK | BAN));
        assert!(!r.kick);
        assert!(!r.ban);
    }

    #[test]
    fn the_cache_marker_is_not_a_permission() {
        // The server sets it on an answer it served from its own cache. Left
        // in, it would be indistinguishable from a permission bit nobody has
        // heard of.
        let r = Rights::from_masks(Some(CACHED | KICK), Some(CACHED | SPEAK));
        assert!(r.kick);
        assert!(r.speak);
        assert!(!r.mute_deafen);
        assert!(r.known);
    }

    #[test]
    fn an_ordinary_rider_may_speak_and_nothing_else() {
        // What a default Murmur grants everybody in a channel.
        let r = Rights::from_masks(
            Some(TRAVERSE | ENTER | SPEAK | WHISPER | TEXT_MESSAGE),
            Some(TRAVERSE | ENTER | SPEAK | WHISPER | TEXT_MESSAGE),
        );
        assert!(r.speak);
        assert!(r.text);
        assert!(r.whisper);
        assert!(
            !r.mute_deafen,
            "this is the case the remote mute request is for"
        );
        assert!(!r.kick);
        assert!(!r.ban);
        assert!(!r.self_register);
    }
}
