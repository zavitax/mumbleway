//! Mumble's bespoke variable-length integer codec.
//!
//! This is *not* protobuf varint encoding. Mumble uses a big-endian, prefix-tagged
//! scheme (see `PacketDataStream` in the upstream C++ client) for the session id,
//! sequence number and Opus frame headers inside legacy UDP audio packets.
//!
//! ```text
//! 0xxxxxxx                    7-bit positive
//! 10xxxxxx + 1 byte          14-bit positive
//! 110xxxxx + 2 bytes         21-bit positive
//! 1110xxxx + 3 bytes         28-bit positive
//! 111100__ + 4 bytes         32-bit positive
//! 111101__ + 8 bytes         64-bit positive
//! 111110__ + varint          negative, recursively encoded
//! 111111xx                   negative, -1 to -4 inline
//! ```

use crate::error::{CoreError, Result};

/// Appends `value` to `out` using Mumble's varint encoding.
pub fn encode(out: &mut Vec<u8>, value: u64) {
    let mut i = value;

    // Negative numbers whose bit pattern fits in 32 bits get the "inverted" treatment,
    // mirroring the upstream check `(i & 0x8000000000000000) && (~i < 0x100000000)`.
    if (i & 0x8000_0000_0000_0000) != 0 && (!i) < 0x1_0000_0000 {
        i = !i;
        if i <= 0x3 {
            // Shortcut for -1 through -4.
            out.push(0xFC | i as u8);
            return;
        }
        out.push(0xF8);
    }

    if i < 0x80 {
        out.push(i as u8);
    } else if i < 0x4000 {
        out.push(((i >> 8) as u8) | 0x80);
        out.push(i as u8);
    } else if i < 0x20_0000 {
        out.push(((i >> 16) as u8) | 0xC0);
        out.push((i >> 8) as u8);
        out.push(i as u8);
    } else if i < 0x1000_0000 {
        out.push(((i >> 24) as u8) | 0xE0);
        out.push((i >> 16) as u8);
        out.push((i >> 8) as u8);
        out.push(i as u8);
    } else if i < 0x1_0000_0000 {
        out.push(0xF0);
        out.extend_from_slice(&(i as u32).to_be_bytes());
    } else {
        out.push(0xF4);
        out.extend_from_slice(&i.to_be_bytes());
    }
}

/// A cursor that reads Mumble varints and raw bytes from a packet body.
pub struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    pub fn position(&self) -> usize {
        self.pos
    }

    pub fn remaining(&self) -> usize {
        self.buf.len().saturating_sub(self.pos)
    }

    pub fn is_empty(&self) -> bool {
        self.remaining() == 0
    }

    fn byte(&mut self) -> Result<u64> {
        let b = *self
            .buf
            .get(self.pos)
            .ok_or(CoreError::Protocol("varint truncated"))?;
        self.pos += 1;
        Ok(b as u64)
    }

    /// Reads the next varint.
    pub fn varint(&mut self) -> Result<u64> {
        let v = self.byte()?;

        // Order matters: the 0xF0 family must be tested before the shorter prefixes,
        // because 0xF_ also satisfies the 0xE0 and 0xC0 masks.
        if v & 0x80 == 0x00 {
            Ok(v & 0x7F)
        } else if v & 0xC0 == 0x80 {
            Ok((v & 0x3F) << 8 | self.byte()?)
        } else if v & 0xF0 == 0xF0 {
            match v & 0xFC {
                0xF0 => {
                    let mut n = 0u64;
                    for _ in 0..4 {
                        n = n << 8 | self.byte()?;
                    }
                    Ok(n)
                }
                0xF4 => {
                    let mut n = 0u64;
                    for _ in 0..8 {
                        n = n << 8 | self.byte()?;
                    }
                    Ok(n)
                }
                0xF8 => {
                    let inner = self.varint()?;
                    Ok(!inner)
                }
                0xFC => Ok(!(v & 0x03)),
                _ => Err(CoreError::Protocol("invalid varint prefix")),
            }
        } else if v & 0xF0 == 0xE0 {
            let mut n = v & 0x0F;
            for _ in 0..3 {
                n = n << 8 | self.byte()?;
            }
            Ok(n)
        } else if v & 0xE0 == 0xC0 {
            let mut n = v & 0x1F;
            for _ in 0..2 {
                n = n << 8 | self.byte()?;
            }
            Ok(n)
        } else {
            Err(CoreError::Protocol("invalid varint prefix"))
        }
    }

    /// Reads exactly `n` bytes.
    pub fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        if self.remaining() < n {
            return Err(CoreError::Protocol("packet truncated"));
        }
        let s = &self.buf[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }

    /// Reads a big-endian `f32`, used for positional audio.
    pub fn f32_be(&mut self) -> Result<f32> {
        let b = self.take(4)?;
        Ok(f32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }

    /// Returns everything not yet consumed without advancing.
    pub fn rest(&self) -> &'a [u8] {
        &self.buf[self.pos..]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip(v: u64) {
        let mut buf = Vec::new();
        encode(&mut buf, v);
        let mut r = Reader::new(&buf);
        let got = r.varint().expect("decode");
        assert_eq!(got, v, "roundtrip failed for {v:#x} (encoded {buf:02x?})");
        assert!(r.is_empty(), "leftover bytes for {v:#x}");
    }

    #[test]
    fn roundtrips_each_width() {
        // One value inside every encoding bucket, plus the boundaries.
        for v in [
            0u64,
            1,
            0x7F,
            0x80,
            0x3FFF,
            0x4000,
            0x1F_FFFF,
            0x20_0000,
            0x0FFF_FFFF,
            0x1000_0000,
            0xFFFF_FFFF,
            0x1_0000_0000,
            u64::MAX / 3,
        ] {
            roundtrip(v);
        }
    }

    #[test]
    fn encodes_expected_byte_widths() {
        let mut b = Vec::new();
        encode(&mut b, 0x7F);
        assert_eq!(b, vec![0x7F]);

        b.clear();
        encode(&mut b, 0x80);
        assert_eq!(b, vec![0x80, 0x80]);

        b.clear();
        encode(&mut b, 0x4000);
        assert_eq!(b, vec![0xC0, 0x40, 0x00]);

        b.clear();
        encode(&mut b, 0x1000_0000);
        assert_eq!(b, vec![0xF0, 0x10, 0x00, 0x00, 0x00]);
    }

    #[test]
    fn small_negatives_use_inline_form() {
        // -1 .. -4 collapse into a single byte.
        for (v, expect) in [
            (u64::MAX, 0xFCu8),   // -1
            (u64::MAX - 1, 0xFD), // -2
            (u64::MAX - 2, 0xFE), // -3
            (u64::MAX - 3, 0xFF), // -4
        ] {
            let mut b = Vec::new();
            encode(&mut b, v);
            assert_eq!(b, vec![expect], "for {v:#x}");
            assert_eq!(Reader::new(&b).varint().unwrap(), v);
        }
    }

    #[test]
    fn larger_negatives_use_recursive_form() {
        // -5 no longer fits the inline form, so it becomes 0xF8 + varint(4).
        let v = u64::MAX - 4;
        let mut b = Vec::new();
        encode(&mut b, v);
        assert_eq!(b, vec![0xF8, 0x04]);
        assert_eq!(Reader::new(&b).varint().unwrap(), v);
    }

    #[test]
    fn truncated_input_errors_rather_than_panicking() {
        assert!(Reader::new(&[0x80]).varint().is_err());
        assert!(Reader::new(&[0xF0, 0x01]).varint().is_err());
        assert!(Reader::new(&[]).varint().is_err());
    }
}
