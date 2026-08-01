//! TCP control-channel framing.
//!
//! Every control packet is a 6-byte big-endian header followed by a body:
//!
//! ```text
//! u16  message type   (see MessageType)
//! u32  payload length
//! ..   payload        (protobuf, or a raw UDP packet for UdpTunnel)
//! ```

use crate::error::{CoreError, Result};
use crate::proto::MessageType;

pub const HEADER_LEN: usize = 6;

/// Upper bound on a control payload. Mumble's own limit is 8 MiB; anything larger
/// is treated as a desync or a hostile server rather than allocated.
pub const MAX_PAYLOAD: usize = 8 * 1024 * 1024;

/// Serialises a header + payload into a single buffer ready for the socket.
pub fn encode(msg_type: u16, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(HEADER_LEN + payload.len());
    out.extend_from_slice(&msg_type.to_be_bytes());
    out.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    out.extend_from_slice(payload);
    out
}

/// Serialises a protobuf control message.
pub fn encode_proto<M: prost::Message>(msg_type: MessageType, msg: &M) -> Vec<u8> {
    let mut body = Vec::with_capacity(msg.encoded_len());
    // Encoding into a Vec only fails on capacity, which we just reserved.
    msg.encode(&mut body).expect("protobuf encode into Vec");
    encode(msg_type as u16, &body)
}

/// Wraps a raw UDP audio packet for tunnelling over the TLS control channel.
pub fn encode_tunnel(udp_packet: &[u8]) -> Vec<u8> {
    encode(MessageType::UdpTunnel as u16, udp_packet)
}

/// A parsed control header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Header {
    pub msg_type: u16,
    pub length: usize,
}

impl Header {
    pub fn parse(buf: &[u8; HEADER_LEN]) -> Result<Self> {
        let msg_type = u16::from_be_bytes([buf[0], buf[1]]);
        let length = u32::from_be_bytes([buf[2], buf[3], buf[4], buf[5]]) as usize;
        if length > MAX_PAYLOAD {
            return Err(CoreError::Protocol("control payload exceeds maximum size"));
        }
        Ok(Self { msg_type, length })
    }

    pub fn kind(&self) -> Option<MessageType> {
        MessageType::from_u16(self.msg_type)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_roundtrips_big_endian() {
        let framed = encode(MessageType::Ping as u16, &[1, 2, 3, 4]);
        assert_eq!(&framed[..HEADER_LEN], &[0, 3, 0, 0, 0, 4]);

        let hdr = Header::parse(framed[..HEADER_LEN].try_into().unwrap()).unwrap();
        assert_eq!(hdr.msg_type, 3);
        assert_eq!(hdr.length, 4);
        assert_eq!(hdr.kind(), Some(MessageType::Ping));
        assert_eq!(&framed[HEADER_LEN..], &[1, 2, 3, 4]);
    }

    #[test]
    fn empty_payload_is_valid() {
        let framed = encode(MessageType::Version as u16, &[]);
        assert_eq!(framed, vec![0, 0, 0, 0, 0, 0]);
        let hdr = Header::parse(framed[..].try_into().unwrap()).unwrap();
        assert_eq!(hdr.length, 0);
    }

    #[test]
    fn absurd_length_is_rejected_before_allocation() {
        let hdr = Header::parse(&[0, 9, 0xFF, 0xFF, 0xFF, 0xFF]);
        assert!(hdr.is_err(), "a 4 GiB payload claim must be rejected");
    }

    #[test]
    fn unknown_message_types_parse_but_do_not_map() {
        let hdr = Header::parse(&[0xFF, 0xFF, 0, 0, 0, 0]).unwrap();
        assert_eq!(hdr.kind(), None);
    }
}
