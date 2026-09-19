//! The server's ban list: reading it, and writing it back.
//!
//! Banning is the one moderation action with no undo inside the action itself.
//! A kick is over the moment it happens and the rider may reconnect; a ban
//! stands until somebody removes it, and removing it means holding the list.
//!
//! # The list is replaced, not edited
//!
//! Mumble has no "remove one ban" message. A client asks for the list with
//! `BanList { query: true }`, and **writes it back whole** — the server clears
//! what it has and takes what arrived. So lifting one ban means sending every
//! other ban back unchanged, and anything dropped on the way is lifted too.
//!
//! Two consequences shape everything here. Entries are carried verbatim,
//! including fields this app never shows, because a field quietly lost on the
//! way through would be a ban silently rewritten. And the list is re-read after
//! writing it, since between the read and the write another admin may have
//! banned somebody whose entry this client never had.

use serde::{Deserialize, Serialize};

/// One ban, as the server holds it.
///
/// Every field the protocol defines, whether or not it is shown: this is
/// round-tripped back to the server, and dropping a field would rewrite the
/// ban.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BanEntry {
    /// The banned address, 16 bytes — IPv6, or IPv4 mapped into it.
    pub address: Vec<u8>,
    /// How much of the address the ban covers.
    pub mask: u32,
    /// Who it was, for identification only. Changing it bans nobody else.
    pub name: String,
    /// Their certificate's hash, which is the half that survives a new address.
    pub hash: String,
    pub reason: String,
    /// When it started, as the server's ISO date string.
    pub start: String,
    /// Seconds it lasts, or 0 for "until somebody lifts it".
    pub duration: u32,
}

impl BanEntry {
    /// Whether this ban is one an ordinary rider would call permanent.
    pub fn is_permanent(&self) -> bool {
        self.duration == 0
    }
}

/// The address a ban covers, as something a person can read.
///
/// **IPv4 addresses arrive mapped into IPv6** — twelve bytes of prefix and then
/// the four that anybody would recognise — and printing those as IPv6 would
/// give an admin `::ffff:5b2d:1f04` for an address their own firewall calls
/// `91.45.31.4`. The mask is shown only when it covers less than the whole
/// address, because `/32` after every entry is noise.
pub fn address_text(address: &[u8], mask: u32) -> String {
    const V4_MAPPED_PREFIX: [u8; 12] = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0xff, 0xff];

    if address.len() == 16 && address[..12] == V4_MAPPED_PREFIX {
        let v4 = &address[12..];
        let text = format!("{}.{}.{}.{}", v4[0], v4[1], v4[2], v4[3]);
        // A mapped IPv4 ban is expressed in the IPv6 mask the server keeps, so
        // its "whole address" is 128, and the IPv4 form of it is the last 32.
        return match mask {
            128 => text,
            m if m >= 96 => format!("{text}/{}", m - 96),
            m => format!("{text}/{m}"),
        };
    }

    if address.len() == 16 {
        let groups: Vec<String> = address
            .chunks(2)
            .map(|c| format!("{:x}", u16::from_be_bytes([c[0], c[1]])))
            .collect();
        let text = groups.join(":");
        return if mask == 128 {
            text
        } else {
            format!("{text}/{mask}")
        };
    }

    // Not a shape the protocol defines. Shown as bytes rather than dropped:
    // an admin looking at a ban they cannot read is better served than one
    // looking at a list with an entry missing.
    let hex: String = address.iter().map(|b| format!("{b:02x}")).collect();
    format!("{hex}/{mask}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mapped(a: u8, b: u8, c: u8, d: u8) -> Vec<u8> {
        let mut v = vec![0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0xff, 0xff];
        v.extend_from_slice(&[a, b, c, d]);
        v
    }

    #[test]
    fn a_v4_address_reads_as_one() {
        // The server keeps it mapped into IPv6; an admin knows it as four
        // numbers, and that is what their firewall says too.
        assert_eq!(address_text(&mapped(91, 45, 31, 4), 128), "91.45.31.4");
    }

    #[test]
    fn a_v4_range_is_shown_in_v4_terms() {
        // /120 of an IPv6 address is /24 of the IPv4 inside it. Printing the
        // 120 would be arithmetically true and read as a mistake.
        assert_eq!(address_text(&mapped(91, 45, 31, 0), 120), "91.45.31.0/24");
    }

    #[test]
    fn a_whole_address_carries_no_mask() {
        assert!(!address_text(&mapped(10, 0, 0, 1), 128).contains('/'));
    }

    #[test]
    fn a_v6_address_is_left_as_one() {
        let mut addr = vec![0x20, 0x01, 0x0d, 0xb8];
        addr.extend_from_slice(&[0; 11]);
        addr.push(1);
        assert_eq!(address_text(&addr, 128), "2001:db8:0:0:0:0:0:1");
        assert_eq!(address_text(&addr, 64), "2001:db8:0:0:0:0:0:1/64");
    }

    #[test]
    fn something_the_protocol_does_not_define_is_still_shown() {
        // A list with an entry missing is worse than one with an entry an
        // admin has to squint at — and it would be a ban they cannot lift.
        assert_eq!(address_text(&[1, 2, 3], 8), "010203/8");
    }

    #[test]
    fn a_ban_with_no_duration_is_the_permanent_kind() {
        let ban = BanEntry {
            address: mapped(10, 0, 0, 1),
            mask: 128,
            name: "someone".into(),
            hash: String::new(),
            reason: "noise".into(),
            start: "2026-09-19T10:00:00".into(),
            duration: 0,
        };
        assert!(ban.is_permanent());
        assert!(!BanEntry {
            duration: 3600,
            ..ban
        }
        .is_permanent());
    }
}
