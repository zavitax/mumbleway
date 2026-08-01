//! Legacy (pre-1.5) UDP voice packet codec.
//!
//! The first byte splits into a 3-bit codec/type field and a 5-bit target:
//!
//! ```text
//! 100ttttt   Opus voice data, target t
//! 001xxxxx   ping
//! ```
//!
//! Client to server: `header | sequence:varint | payload | [position]`
//! Server to client: `header | session:varint | sequence:varint | payload | [position]`
//!
//! For Opus the payload is a single varint whose low 13 bits are the frame length
//! and whose `0x2000` bit marks the end of a transmission, followed by the frame.

use crate::error::{CoreError, Result};
use crate::varint;

pub const TYPE_CELT_ALPHA: u8 = 0;
pub const TYPE_PING: u8 = 1;
pub const TYPE_SPEEX: u8 = 2;
pub const TYPE_CELT_BETA: u8 = 3;
pub const TYPE_OPUS: u8 = 4;

/// Normal channel speech.
pub const TARGET_NORMAL: u8 = 0;
/// Server-side loopback, useful for the audio self-test.
pub const TARGET_LOOPBACK: u8 = 31;

/// Marks the final packet of a transmission within the Opus length varint.
const OPUS_TERMINATOR_BIT: u64 = 0x2000;
const OPUS_LENGTH_MASK: u64 = 0x1FFF;

/// Why the server says we are hearing this audio (1.5 `context`, legacy target).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioContext {
    Normal,
    Shout,
    Whisper,
    Listener,
}

/// A decoded voice packet.
#[derive(Debug, Clone, PartialEq)]
pub struct VoicePacket {
    /// Sender's session id. Only present on packets received from the server.
    pub session: Option<u32>,
    /// Target (outgoing) or context (incoming).
    pub target: u8,
    /// Position of the first contained frame in the sender's stream.
    pub sequence: u64,
    /// Raw Opus frame.
    pub opus: Vec<u8>,
    /// Set on the last packet of a transmission.
    pub terminator: bool,
    /// Optional positional-audio coordinates.
    pub position: Option<[f32; 3]>,
}

impl VoicePacket {
    /// Builds an outgoing normal-speech packet.
    pub fn speech(sequence: u64, opus: Vec<u8>, terminator: bool) -> Self {
        Self {
            session: None,
            target: TARGET_NORMAL,
            sequence,
            opus,
            terminator,
            position: None,
        }
    }

    /// Encodes in client-to-server form (no session id).
    pub fn encode_outgoing(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.opus.len() + 16);
        out.push((TYPE_OPUS << 5) | (self.target & 0x1F));
        varint::encode(&mut out, self.sequence);

        let mut len_field = self.opus.len() as u64 & OPUS_LENGTH_MASK;
        if self.terminator {
            len_field |= OPUS_TERMINATOR_BIT;
        }
        varint::encode(&mut out, len_field);
        out.extend_from_slice(&self.opus);

        if let Some(p) = self.position {
            for c in p {
                out.extend_from_slice(&c.to_be_bytes());
            }
        }
        out
    }

    /// Decodes a server-to-client packet (session id present).
    pub fn decode_incoming(buf: &[u8]) -> Result<Self> {
        let header = *buf.first().ok_or(CoreError::Protocol("empty UDP packet"))?;
        let ptype = header >> 5;
        let target = header & 0x1F;

        if ptype != TYPE_OPUS {
            // CELT and Speex are long dead; we advertise Opus-only during
            // authentication, so anything else is a misbehaving server.
            return Err(CoreError::Protocol("non-Opus voice packet received"));
        }

        let mut r = varint::Reader::new(&buf[1..]);
        let session = r.varint()? as u32;
        let sequence = r.varint()?;

        let len_field = r.varint()?;
        let terminator = len_field & OPUS_TERMINATOR_BIT != 0;
        let len = (len_field & OPUS_LENGTH_MASK) as usize;
        let opus = r.take(len)?.to_vec();

        // Positional data is optional and only present when the sender enabled it.
        let position = if r.remaining() >= 12 {
            Some([r.f32_be()?, r.f32_be()?, r.f32_be()?])
        } else {
            None
        };

        Ok(Self {
            session: Some(session),
            target,
            sequence,
            opus,
            terminator,
            position,
        })
    }

    pub fn context(&self) -> AudioContext {
        match self.target {
            0 => AudioContext::Normal,
            1 => AudioContext::Shout,
            2 => AudioContext::Whisper,
            _ => AudioContext::Listener,
        }
    }
}

/// Builds a legacy UDP ping: type 1, then the timestamp as a varint.
pub fn encode_ping(timestamp: u64) -> Vec<u8> {
    let mut out = Vec::with_capacity(9);
    out.push(TYPE_PING << 5);
    varint::encode(&mut out, timestamp);
    out
}

/// Parses a ping echo, returning the timestamp we originally sent.
pub fn decode_ping(buf: &[u8]) -> Result<u64> {
    let header = *buf.first().ok_or(CoreError::Protocol("empty UDP packet"))?;
    if header >> 5 != TYPE_PING {
        return Err(CoreError::Protocol("not a ping packet"));
    }
    varint::Reader::new(&buf[1..]).varint()
}

/// True if the packet is a ping rather than voice.
pub fn is_ping(buf: &[u8]) -> bool {
    matches!(buf.first(), Some(b) if b >> 5 == TYPE_PING)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Re-encodes an outgoing packet in incoming form so we can exercise the decoder.
    fn to_incoming(p: &VoicePacket, session: u32) -> Vec<u8> {
        let mut out = Vec::new();
        out.push((TYPE_OPUS << 5) | (p.target & 0x1F));
        varint::encode(&mut out, session as u64);
        varint::encode(&mut out, p.sequence);
        let mut lf = p.opus.len() as u64 & OPUS_LENGTH_MASK;
        if p.terminator {
            lf |= OPUS_TERMINATOR_BIT;
        }
        varint::encode(&mut out, lf);
        out.extend_from_slice(&p.opus);
        if let Some(pos) = p.position {
            for c in pos {
                out.extend_from_slice(&c.to_be_bytes());
            }
        }
        out
    }

    #[test]
    fn outgoing_header_encodes_type_and_target() {
        let p = VoicePacket::speech(1, vec![0xAA, 0xBB], false);
        let bytes = p.encode_outgoing();
        assert_eq!(bytes[0], 0b100_00000, "Opus type in the top three bits");

        let mut whisper = VoicePacket::speech(1, vec![0xAA], false);
        whisper.target = 7;
        assert_eq!(whisper.encode_outgoing()[0], 0b100_00111);

        let mut loopback = VoicePacket::speech(1, vec![0xAA], false);
        loopback.target = TARGET_LOOPBACK;
        assert_eq!(loopback.encode_outgoing()[0], 0b100_11111);
    }

    #[test]
    fn roundtrips_through_the_incoming_form() {
        let p = VoicePacket::speech(12345, vec![1, 2, 3, 4, 5], true);
        let wire = to_incoming(&p, 42);
        let got = VoicePacket::decode_incoming(&wire).unwrap();

        assert_eq!(got.session, Some(42));
        assert_eq!(got.sequence, 12345);
        assert_eq!(got.opus, vec![1, 2, 3, 4, 5]);
        assert!(got.terminator);
        assert_eq!(got.position, None);
    }

    #[test]
    fn carries_positional_data_when_present() {
        let mut p = VoicePacket::speech(7, vec![9; 20], false);
        p.position = Some([1.5, -2.25, 3.0]);
        let got = VoicePacket::decode_incoming(&to_incoming(&p, 3)).unwrap();
        assert_eq!(got.position, Some([1.5, -2.25, 3.0]));
        assert_eq!(got.opus.len(), 20);
    }

    #[test]
    fn terminator_bit_survives_and_does_not_corrupt_length() {
        // A frame long enough to need a multi-byte varint, with the terminator set.
        let opus = vec![0x5A; 300];
        let p = VoicePacket::speech(1, opus.clone(), true);
        let got = VoicePacket::decode_incoming(&to_incoming(&p, 1)).unwrap();
        assert!(got.terminator);
        assert_eq!(got.opus, opus);
    }

    #[test]
    fn ping_roundtrips() {
        let wire = encode_ping(0x0123_4567_89AB);
        assert!(is_ping(&wire));
        assert_eq!(decode_ping(&wire).unwrap(), 0x0123_4567_89AB);
    }

    #[test]
    fn voice_packets_are_not_mistaken_for_pings() {
        let wire = VoicePacket::speech(1, vec![1, 2, 3], false).encode_outgoing();
        assert!(!is_ping(&wire));
        assert!(decode_ping(&wire).is_err());
    }

    #[test]
    fn truncated_and_malformed_packets_error_cleanly() {
        assert!(VoicePacket::decode_incoming(&[]).is_err());
        // Claims 500 bytes of Opus but supplies none.
        let mut wire = vec![TYPE_OPUS << 5];
        varint::encode(&mut wire, 1); // session
        varint::encode(&mut wire, 1); // sequence
        varint::encode(&mut wire, 500); // length
        assert!(VoicePacket::decode_incoming(&wire).is_err());

        // A CELT packet must be refused rather than misparsed.
        assert!(VoicePacket::decode_incoming(&[TYPE_CELT_ALPHA << 5, 1, 1, 1]).is_err());
    }
}
