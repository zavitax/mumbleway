//! Networking: TLS control channel, UDP voice, and their framing.

pub mod audio_packet;
pub mod control;
pub mod frame;
pub mod ping;
pub mod tls;
pub mod voice;

pub use audio_packet::VoicePacket;
pub use tls::{Identity, TrustPolicy};
