//! Generated Mumble protobuf schemas.
//!
//! `Mumble.proto` (package `MumbleProto`) carries the TCP control channel.
//! `MumbleUDP.proto` (package `MumbleUDP`) is the protobuf UDP format introduced
//! in Mumble 1.5; older servers use the legacy hand-rolled UDP framing instead.

pub mod mumble {
    include!(concat!(env!("OUT_DIR"), "/mumble_proto.rs"));
}

pub mod udp {
    include!(concat!(env!("OUT_DIR"), "/mumble_udp.rs"));
}

/// TCP control-channel message type numbers.
///
/// Every control packet is framed as a 6-byte big-endian header
/// (`u16` type, `u32` payload length) followed by the protobuf body — except
/// [`MessageType::UdpTunnel`], whose payload is a raw UDP audio packet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum MessageType {
    Version = 0,
    UdpTunnel = 1,
    Authenticate = 2,
    Ping = 3,
    Reject = 4,
    ServerSync = 5,
    ChannelRemove = 6,
    ChannelState = 7,
    UserRemove = 8,
    UserState = 9,
    BanList = 10,
    TextMessage = 11,
    PermissionDenied = 12,
    Acl = 13,
    QueryUsers = 14,
    CryptSetup = 15,
    ContextActionModify = 16,
    ContextAction = 17,
    UserList = 18,
    VoiceTarget = 19,
    PermissionQuery = 20,
    CodecVersion = 21,
    UserStats = 22,
    RequestBlob = 23,
    ServerConfig = 24,
    SuggestConfig = 25,
}

impl MessageType {
    pub fn from_u16(v: u16) -> Option<Self> {
        use MessageType::*;
        Some(match v {
            0 => Version,
            1 => UdpTunnel,
            2 => Authenticate,
            3 => Ping,
            4 => Reject,
            5 => ServerSync,
            6 => ChannelRemove,
            7 => ChannelState,
            8 => UserRemove,
            9 => UserState,
            10 => BanList,
            11 => TextMessage,
            12 => PermissionDenied,
            13 => Acl,
            14 => QueryUsers,
            15 => CryptSetup,
            16 => ContextActionModify,
            17 => ContextAction,
            18 => UserList,
            19 => VoiceTarget,
            20 => PermissionQuery,
            21 => CodecVersion,
            22 => UserStats,
            23 => RequestBlob,
            24 => ServerConfig,
            25 => SuggestConfig,
            _ => return None,
        })
    }
}

/// Packs a Mumble version into the legacy `version_v1` u32 form (major.minor.patch
/// as `0xMMmmpp`). Versions past 1.4 should also set `version_v2`.
pub const fn version_v1(major: u16, minor: u8, patch: u8) -> u32 {
    ((major as u32) << 16) | ((minor as u32) << 8) | (patch as u32)
}

/// Packs a Mumble version into the 1.5+ `version_v2` u64 form
/// (`major` in bits 48-63, `minor` in 32-47, `patch` in 16-31).
pub const fn version_v2(major: u16, minor: u16, patch: u16) -> u64 {
    ((major as u64) << 48) | ((minor as u64) << 32) | ((patch as u64) << 16)
}
