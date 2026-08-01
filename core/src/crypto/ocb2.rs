//! OCB2-AES128 as implemented by Mumble, including the counter-cryptanalysis
//! mitigation for the XEX* attack described in <https://eprint.iacr.org/2019/311>.
//!
//! This is a faithful port of upstream `src/crypto/CryptStateOCB2.cpp`. Wire
//! compatibility with Mumble 1.2/1.3/1.4 servers depends on matching it exactly,
//! so the structure deliberately mirrors the C++ rather than being idiomatic.
//!
//! Packet layout produced by [`CryptState::encrypt`]:
//!
//! ```text
//! byte 0      low byte of the encrypt IV
//! bytes 1..4  first three bytes of the OCB tag
//! bytes 4..   ciphertext (same length as the plaintext)
//! ```

use aes::cipher::{BlockDecrypt, BlockEncrypt, KeyInit};
use aes::Aes128;

use crate::error::{CoreError, Result};

pub const BLOCK_SIZE: usize = 16;
pub const KEY_SIZE: usize = 16;
/// Bytes of framing that [`CryptState::encrypt`] prepends to the ciphertext.
pub const OVERHEAD: usize = 4;

type Block = [u8; BLOCK_SIZE];

/// Multiply by x in GF(2^128) — upstream's `S2` macro over big-endian subblocks.
#[inline]
fn s2(b: &mut Block) {
    let v = u128::from_be_bytes(*b);
    let carry = v >> 127;
    let mut r = v << 1;
    if carry != 0 {
        r ^= 0x87;
    }
    *b = r.to_be_bytes();
}

/// Multiply by (x + 1) — upstream's `S3`, i.e. `v ^ s2(v)`.
#[inline]
fn s3(b: &mut Block) {
    let v = u128::from_be_bytes(*b);
    let carry = v >> 127;
    let mut r = v << 1;
    if carry != 0 {
        r ^= 0x87;
    }
    *b = (v ^ r).to_be_bytes();
}

#[inline]
fn xor_into(dst: &mut Block, a: &Block, b: &Block) {
    for i in 0..BLOCK_SIZE {
        dst[i] = a[i] ^ b[i];
    }
}

#[inline]
fn xor_assign(dst: &mut Block, a: &Block) {
    for i in 0..BLOCK_SIZE {
        dst[i] ^= a[i];
    }
}

/// Packet accounting the server asks for in `Ping` messages.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CryptStats {
    pub good: u32,
    pub late: u32,
    pub lost: u32,
    pub resync: u32,
}

/// Mumble's OCB2 cipher state, including replay/reorder tracking.
pub struct CryptState {
    cipher: Aes128,
    raw_key: [u8; KEY_SIZE],
    encrypt_iv: Block,
    decrypt_iv: Block,
    /// Maps `decrypt_iv[0]` to the `decrypt_iv[1]` last accepted for it, which is
    /// how upstream rejects replays inside the reorder window.
    decrypt_history: [u8; 256],
    stats: CryptStats,
}

impl CryptState {
    /// Builds a state from the `CryptSetup` message fields.
    pub fn new(key: &[u8], encrypt_iv: &[u8], decrypt_iv: &[u8]) -> Result<Self> {
        if key.len() != KEY_SIZE || encrypt_iv.len() != BLOCK_SIZE || decrypt_iv.len() != BLOCK_SIZE
        {
            return Err(CoreError::Crypto("CryptSetup had wrong key or IV length"));
        }
        let mut raw_key = [0u8; KEY_SIZE];
        raw_key.copy_from_slice(key);
        let mut e = [0u8; BLOCK_SIZE];
        let mut d = [0u8; BLOCK_SIZE];
        e.copy_from_slice(encrypt_iv);
        d.copy_from_slice(decrypt_iv);

        Ok(Self {
            cipher: Aes128::new_from_slice(&raw_key)
                .map_err(|_| CoreError::Crypto("invalid AES key"))?,
            raw_key,
            encrypt_iv: e,
            decrypt_iv: d,
            decrypt_history: [0u8; 256],
            stats: CryptStats::default(),
        })
    }

    pub fn stats(&self) -> CryptStats {
        self.stats
    }

    pub fn key(&self) -> &[u8; KEY_SIZE] {
        &self.raw_key
    }

    pub fn encrypt_iv(&self) -> &Block {
        &self.encrypt_iv
    }

    pub fn decrypt_iv(&self) -> &Block {
        &self.decrypt_iv
    }

    /// Replaces the decrypt IV after a server-driven nonce resync.
    pub fn set_decrypt_iv(&mut self, iv: &[u8]) -> Result<()> {
        if iv.len() != BLOCK_SIZE {
            return Err(CoreError::Crypto("resync IV had wrong length"));
        }
        self.decrypt_iv.copy_from_slice(iv);
        self.stats.resync += 1;
        Ok(())
    }

    #[inline]
    fn aes_encrypt(&self, block: &Block) -> Block {
        let mut b = aes::cipher::generic_array::GenericArray::clone_from_slice(block);
        self.cipher.encrypt_block(&mut b);
        let mut out = [0u8; BLOCK_SIZE];
        out.copy_from_slice(&b);
        out
    }

    #[inline]
    fn aes_decrypt(&self, block: &Block) -> Block {
        let mut b = aes::cipher::generic_array::GenericArray::clone_from_slice(block);
        self.cipher.decrypt_block(&mut b);
        let mut out = [0u8; BLOCK_SIZE];
        out.copy_from_slice(&b);
        out
    }

    /// Encrypts `plain`, returning `[iv_byte][tag0..3][ciphertext]`.
    pub fn encrypt(&mut self, plain: &[u8]) -> Result<Vec<u8>> {
        // Bump the IV (little-endian carry across the whole block).
        for i in 0..BLOCK_SIZE {
            self.encrypt_iv[i] = self.encrypt_iv[i].wrapping_add(1);
            if self.encrypt_iv[i] != 0 {
                break;
            }
        }

        let mut out = vec![0u8; OVERHEAD + plain.len()];
        let iv = self.encrypt_iv;
        let tag = self.ocb_encrypt(plain, &mut out[OVERHEAD..], &iv)?;

        out[0] = self.encrypt_iv[0];
        out[1] = tag[0];
        out[2] = tag[1];
        out[3] = tag[2];
        Ok(out)
    }

    /// Decrypts a packet produced by [`CryptState::encrypt`], handling reordering,
    /// wraparound and replay rejection exactly as upstream does.
    pub fn decrypt(&mut self, source: &[u8]) -> Result<Vec<u8>> {
        if source.len() < OVERHEAD {
            return Err(CoreError::Crypto("UDP packet too short to be encrypted"));
        }
        let plain_length = source.len() - OVERHEAD;
        let ivbyte = source[0];
        let saveiv = self.decrypt_iv;
        let mut restore = false;
        let mut late: i32 = 0;
        let mut lost: i32 = 0;

        if self.decrypt_iv[0].wrapping_add(1) == ivbyte {
            // Arrived in the expected order.
            if ivbyte > self.decrypt_iv[0] {
                self.decrypt_iv[0] = ivbyte;
            } else if ivbyte < self.decrypt_iv[0] {
                // Wrapped past 0xFF, so carry into the upper bytes.
                self.decrypt_iv[0] = ivbyte;
                for i in 1..BLOCK_SIZE {
                    self.decrypt_iv[i] = self.decrypt_iv[i].wrapping_add(1);
                    if self.decrypt_iv[i] != 0 {
                        break;
                    }
                }
            } else {
                return Err(CoreError::Crypto("nonce did not advance"));
            }
        } else {
            // Out of order, or a repeat.
            let mut diff = ivbyte as i32 - self.decrypt_iv[0] as i32;
            if diff > 128 {
                diff -= 256;
            } else if diff < -128 {
                diff += 256;
            }

            if ivbyte < self.decrypt_iv[0] && diff > -30 && diff < 0 {
                // Late packet, no wraparound.
                late = 1;
                lost = -1;
                self.decrypt_iv[0] = ivbyte;
                restore = true;
            } else if ivbyte > self.decrypt_iv[0] && diff > -30 && diff < 0 {
                // Late packet from the previous wrap (e.g. 0xFF arriving after 0x02).
                late = 1;
                lost = -1;
                self.decrypt_iv[0] = ivbyte;
                for i in 1..BLOCK_SIZE {
                    let old = self.decrypt_iv[i];
                    self.decrypt_iv[i] = old.wrapping_sub(1);
                    if old != 0 {
                        break;
                    }
                }
                restore = true;
            } else if ivbyte > self.decrypt_iv[0] && diff > 0 {
                // Dropped a few packets.
                lost = ivbyte as i32 - self.decrypt_iv[0] as i32 - 1;
                self.decrypt_iv[0] = ivbyte;
            } else if ivbyte < self.decrypt_iv[0] && diff > 0 {
                // Dropped a few packets and wrapped around.
                lost = 256 - self.decrypt_iv[0] as i32 + ivbyte as i32 - 1;
                self.decrypt_iv[0] = ivbyte;
                for i in 1..BLOCK_SIZE {
                    self.decrypt_iv[i] = self.decrypt_iv[i].wrapping_add(1);
                    if self.decrypt_iv[i] != 0 {
                        break;
                    }
                }
            } else {
                return Err(CoreError::Crypto("nonce outside the acceptable window"));
            }

            if self.decrypt_history[self.decrypt_iv[0] as usize] == self.decrypt_iv[1] {
                self.decrypt_iv = saveiv;
                return Err(CoreError::Crypto("replayed packet"));
            }
        }

        let mut plain = vec![0u8; plain_length];
        let iv = self.decrypt_iv;
        let result = self.ocb_decrypt(&source[OVERHEAD..], &mut plain, &iv);

        match result {
            Ok(tag) if tag[..3] == source[1..4] => {
                self.decrypt_history[self.decrypt_iv[0] as usize] = self.decrypt_iv[1];
                if restore {
                    self.decrypt_iv = saveiv;
                }
                self.stats.good += 1;
                if late > 0 {
                    self.stats.late = self.stats.late.saturating_add(late as u32);
                } else if self.stats.late as i32 > late.abs() {
                    self.stats.late -= late.unsigned_abs();
                }
                if lost > 0 {
                    self.stats.lost = self.stats.lost.saturating_add(lost as u32);
                } else if self.stats.lost as i32 > lost.abs() {
                    self.stats.lost -= lost.unsigned_abs();
                }
                Ok(plain)
            }
            _ => {
                self.decrypt_iv = saveiv;
                Err(CoreError::Crypto("authentication tag mismatch"))
            }
        }
    }

    /// Core OCB2 encryption. Returns the full 16-byte tag.
    ///
    /// `encrypted` must be exactly `plain.len()` bytes.
    fn ocb_encrypt(&self, plain: &[u8], encrypted: &mut [u8], nonce: &Block) -> Result<Block> {
        debug_assert_eq!(plain.len(), encrypted.len());

        let mut delta = self.aes_encrypt(nonce);
        let mut checksum = [0u8; BLOCK_SIZE];
        let mut tmp = [0u8; BLOCK_SIZE];

        let mut off = 0usize;
        let mut len = plain.len();

        while len > BLOCK_SIZE {
            let block: Block = plain[off..off + BLOCK_SIZE].try_into().unwrap();

            // Counter-cryptanalysis (eprint 2019/311 §9): the attack needs the
            // second-to-last block to be all zeroes except the final byte. Digital
            // silence produces such blocks in bulk, so rather than refusing to send
            // we perturb a bit that the tag accounts for, leaving audio unaffected.
            let mut flip_a_bit = false;
            if len - BLOCK_SIZE <= BLOCK_SIZE {
                let sum = block[..BLOCK_SIZE - 1].iter().fold(0u8, |a, b| a | b);
                if sum == 0 {
                    flip_a_bit = true;
                }
            }

            s2(&mut delta);
            xor_into(&mut tmp, &delta, &block);
            if flip_a_bit {
                tmp[0] ^= 1;
            }
            tmp = self.aes_encrypt(&tmp);
            let mut ct = [0u8; BLOCK_SIZE];
            xor_into(&mut ct, &delta, &tmp);
            encrypted[off..off + BLOCK_SIZE].copy_from_slice(&ct);
            xor_assign(&mut checksum, &block);
            if flip_a_bit {
                checksum[0] ^= 1;
            }

            len -= BLOCK_SIZE;
            off += BLOCK_SIZE;
        }

        // Final (possibly partial) block.
        s2(&mut delta);
        let mut lenblock = [0u8; BLOCK_SIZE];
        lenblock[BLOCK_SIZE - 8..].copy_from_slice(&((len * 8) as u64).to_be_bytes());
        xor_into(&mut tmp, &lenblock, &delta);
        let pad = self.aes_encrypt(&tmp);

        // tmp = plain[..len] || pad[len..]
        tmp[..len].copy_from_slice(&plain[off..off + len]);
        tmp[len..].copy_from_slice(&pad[len..]);
        xor_assign(&mut checksum, &tmp);
        let mut ct = [0u8; BLOCK_SIZE];
        xor_into(&mut ct, &pad, &tmp);
        encrypted[off..off + len].copy_from_slice(&ct[..len]);

        s3(&mut delta);
        xor_into(&mut tmp, &delta, &checksum);
        Ok(self.aes_encrypt(&tmp))
    }

    /// Core OCB2 decryption. Returns the computed tag, or an error if the
    /// XEX* attack signature is detected.
    fn ocb_decrypt(&self, encrypted: &[u8], plain: &mut [u8], nonce: &Block) -> Result<Block> {
        debug_assert_eq!(plain.len(), encrypted.len());

        let mut delta = self.aes_encrypt(nonce);
        let mut checksum = [0u8; BLOCK_SIZE];
        let mut tmp = [0u8; BLOCK_SIZE];

        let mut off = 0usize;
        let mut len = encrypted.len();

        while len > BLOCK_SIZE {
            let block: Block = encrypted[off..off + BLOCK_SIZE].try_into().unwrap();
            s2(&mut delta);
            xor_into(&mut tmp, &delta, &block);
            tmp = self.aes_decrypt(&tmp);
            let mut pt = [0u8; BLOCK_SIZE];
            xor_into(&mut pt, &delta, &tmp);
            plain[off..off + BLOCK_SIZE].copy_from_slice(&pt);
            xor_assign(&mut checksum, &pt);

            len -= BLOCK_SIZE;
            off += BLOCK_SIZE;
        }

        s2(&mut delta);
        let mut lenblock = [0u8; BLOCK_SIZE];
        lenblock[BLOCK_SIZE - 8..].copy_from_slice(&((len * 8) as u64).to_be_bytes());
        xor_into(&mut tmp, &lenblock, &delta);
        let pad = self.aes_encrypt(&tmp);

        tmp = [0u8; BLOCK_SIZE];
        tmp[..len].copy_from_slice(&encrypted[off..off + len]);
        xor_assign(&mut tmp, &pad);
        xor_assign(&mut checksum, &tmp);
        plain[off..off + len].copy_from_slice(&tmp[..len]);

        // Counter-cryptanalysis: reject the crafted final block that would let an
        // attacker forge a tag. `len` only ever alters the last byte, so compare the
        // leading 15.
        let attack_detected = tmp[..BLOCK_SIZE - 1] == delta[..BLOCK_SIZE - 1];

        s3(&mut delta);
        let mut t = [0u8; BLOCK_SIZE];
        xor_into(&mut t, &delta, &checksum);
        let tag = self.aes_encrypt(&t);

        if attack_detected {
            return Err(CoreError::Crypto("OCB2 XEX* attack signature detected"));
        }
        Ok(tag)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pair() -> (CryptState, CryptState) {
        let key = [0x11u8; KEY_SIZE];
        let civ = [0x22u8; BLOCK_SIZE]; // client encrypt / server decrypt
        let siv = [0x33u8; BLOCK_SIZE]; // server encrypt / client decrypt

        // Two peers: A encrypts with civ and decrypts with siv; B is the mirror.
        let a = CryptState::new(&key, &civ, &siv).unwrap();
        let b = CryptState::new(&key, &siv, &civ).unwrap();
        (a, b)
    }

    #[test]
    fn gf_doubling_matches_reference_vectors() {
        // s2 of 1 is 2; s2 of a top-bit-set value reduces by 0x87.
        let mut b = 1u128.to_be_bytes();
        s2(&mut b);
        assert_eq!(u128::from_be_bytes(b), 2);

        let mut b = (1u128 << 127).to_be_bytes();
        s2(&mut b);
        assert_eq!(u128::from_be_bytes(b), 0x87);

        // s3(v) == v ^ s2(v)
        let v = 0x0123_4567_89ab_cdef_0123_4567_89ab_cdefu128;
        let mut x = v.to_be_bytes();
        s2(&mut x);
        let doubled = u128::from_be_bytes(x);
        let mut y = v.to_be_bytes();
        s3(&mut y);
        assert_eq!(u128::from_be_bytes(y), v ^ doubled);
    }

    #[test]
    fn roundtrips_every_length_across_the_block_boundary() {
        // Cover partial, exact and multi-block payloads.
        for len in [1usize, 15, 16, 17, 31, 32, 33, 64, 100, 160, 511] {
            let (mut a, mut b) = pair();
            let plain: Vec<u8> = (0..len).map(|i| (i * 7 + 3) as u8).collect();
            let ct = a.encrypt(&plain).unwrap();
            assert_eq!(ct.len(), len + OVERHEAD, "len {len}");
            let pt = b.decrypt(&ct).unwrap();
            assert_eq!(pt, plain, "roundtrip failed at len {len}");
        }
    }

    #[test]
    fn many_sequential_packets_stay_in_sync() {
        let (mut a, mut b) = pair();
        for i in 0..1000u32 {
            let plain = format!("packet number {i}").into_bytes();
            let ct = a.encrypt(&plain).unwrap();
            let pt = b.decrypt(&ct).unwrap();
            assert_eq!(pt, plain);
        }
        assert_eq!(b.stats().good, 1000);
        assert_eq!(b.stats().lost, 0);
    }

    #[test]
    fn survives_iv_wraparound() {
        // Push well past a 256-packet boundary so the upper IV bytes must carry.
        let (mut a, mut b) = pair();
        for i in 0..600u32 {
            let plain = i.to_be_bytes().to_vec();
            let ct = a.encrypt(&plain).unwrap();
            assert_eq!(b.decrypt(&ct).unwrap(), plain, "failed at packet {i}");
        }
    }

    #[test]
    fn tolerates_reordering_and_counts_late_packets() {
        let (mut a, mut b) = pair();
        let mut packets = Vec::new();
        for i in 0..6u8 {
            packets.push(a.encrypt(&[i; 40]).unwrap());
        }
        // Deliver 0,1, then 3 (skipping 2), then the late 2, then 4,5.
        b.decrypt(&packets[0]).unwrap();
        b.decrypt(&packets[1]).unwrap();
        b.decrypt(&packets[3]).unwrap();
        assert_eq!(b.decrypt(&packets[2]).unwrap(), vec![2u8; 40]);
        b.decrypt(&packets[4]).unwrap();
        b.decrypt(&packets[5]).unwrap();
        assert!(b.stats().late >= 1);
    }

    #[test]
    fn rejects_replayed_packets() {
        let (mut a, mut b) = pair();
        let p1 = a.encrypt(&[1u8; 32]).unwrap();
        let p2 = a.encrypt(&[2u8; 32]).unwrap();
        let p3 = a.encrypt(&[3u8; 32]).unwrap();
        b.decrypt(&p1).unwrap();
        b.decrypt(&p2).unwrap();
        b.decrypt(&p3).unwrap();
        // Replaying an already-seen packet must fail rather than being accepted.
        assert!(b.decrypt(&p2).is_err(), "replay was accepted");
    }

    #[test]
    fn rejects_tampered_ciphertext_and_tag() {
        let (mut a, mut b) = pair();
        let mut ct = a.encrypt(&[0xAAu8; 48]).unwrap();
        ct[10] ^= 0x01; // flip a ciphertext bit
        assert!(b.decrypt(&ct).is_err(), "tampered ciphertext was accepted");

        let (mut a, mut b) = pair();
        let mut ct = a.encrypt(&[0xAAu8; 48]).unwrap();
        ct[1] ^= 0x01; // flip a tag bit
        assert!(b.decrypt(&ct).is_err(), "tampered tag was accepted");
    }

    #[test]
    fn rejects_short_packets_without_panicking() {
        let (_, mut b) = pair();
        for bad in [vec![], vec![0u8], vec![0u8; 3]] {
            assert!(b.decrypt(&bad).is_err());
        }
    }

    #[test]
    fn digital_silence_authenticates_with_the_documented_single_bit_perturbation() {
        // All-zero payloads are exactly the pattern that trips the XEX* mitigation.
        // Upstream cannot modify the caller's const plaintext, so it perturbs the
        // block *inside* the cipher: the receiver legitimately observes
        // `block[0] ^ 1` for the second-to-last block. This is upstream's designed
        // behaviour (chosen over refusing to transmit, because silence generates
        // these blocks constantly), so we must reproduce it bit-for-bit to stay
        // wire-compatible. What matters is that the tag still validates and the
        // payload is otherwise intact.
        let (mut a, mut b) = pair();
        for len in [32usize, 33, 48, 64] {
            let plain = vec![0u8; len];
            let ct = a.encrypt(&plain).unwrap();
            let got = b.decrypt(&ct).expect("silence must authenticate");
            assert_eq!(got.len(), len, "length changed at len {len}");

            let differing: Vec<usize> = (0..len).filter(|&i| got[i] != plain[i]).collect();
            assert!(
                differing.len() <= 1,
                "expected at most one perturbed byte at len {len}, got {differing:?}"
            );
            if let Some(&i) = differing.first() {
                // Only the low bit may differ, and only at the start of the
                // second-to-last 16-byte block.
                assert_eq!(
                    got[i] ^ plain[i],
                    1,
                    "perturbation was not a single low bit"
                );
                let second_to_last_block_start = ((len - 1) / BLOCK_SIZE - 1) * BLOCK_SIZE;
                assert_eq!(
                    i, second_to_last_block_start,
                    "perturbation landed at byte {i}, not the expected block start"
                );
            }
        }
    }

    #[test]
    fn non_silent_audio_is_never_perturbed() {
        // The mitigation must not touch ordinary payloads.
        let (mut a, mut b) = pair();
        for len in [32usize, 48, 64, 160] {
            let plain: Vec<u8> = (0..len).map(|i| (i as u8) | 0x40).collect();
            let ct = a.encrypt(&plain).unwrap();
            assert_eq!(
                b.decrypt(&ct).unwrap(),
                plain,
                "perturbed a normal payload at len {len}"
            );
        }
    }

    #[test]
    fn failed_decrypt_leaves_state_usable() {
        let (mut a, mut b) = pair();
        let good1 = a.encrypt(&[7u8; 24]).unwrap();
        b.decrypt(&good1).unwrap();

        let mut bad = a.encrypt(&[8u8; 24]).unwrap();
        bad[5] ^= 0xFF;
        assert!(b.decrypt(&bad).is_err());

        // The IV must have been rolled back, so the next genuine packet still works.
        let good2 = a.encrypt(&[9u8; 24]).unwrap();
        // Packet index 2 was consumed by the corrupted frame, so 3 arrives as "lost 1".
        assert_eq!(b.decrypt(&good2).unwrap(), vec![9u8; 24]);
    }
}
