//! MumbleWay core.
//!
//! A self-contained Mumble client engine: wire protocol, UDP crypto, audio
//! capture/playback with noise suppression, and a reconnecting session manager.
//! The Flutter UI talks to this through `flutter_rust_bridge`.

pub mod audio;
pub mod crypto;
pub mod diag;
pub mod error;
pub mod net;
pub mod proto;
pub mod session;
pub mod varint;

pub use error::{CoreError, DisconnectReason, Result};
