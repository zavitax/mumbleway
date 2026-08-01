//! UDP transport encryption.
//!
//! Mumble 1.2-1.4 servers use OCB2-AES128 ([`ocb2`]). Mumble 1.5 added an
//! AES-256-GCM mode; until a server negotiates it we always speak OCB2, which
//! every deployed server understands.

pub mod ocb2;

pub use ocb2::{CryptState, CryptStats};
