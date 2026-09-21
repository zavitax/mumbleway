//! Fitting the encoder inside the bandwidth a server allows.
//!
//! Every Mumble server hands each client a budget in bits per second —
//! `ServerSync.max_bandwidth` at the end of the handshake, and `ServerConfig`
//! again afterwards, which may arrive later if an admin changes it. It is not
//! advice. Murmur enforces it in `processMsg`:
//!
//! ```text
//! const std::size_t packetsize = 20 + 8 + 4 + audioData.payload.size();
//! if (!bw->addFrame(static_cast< int >(packetsize), iMaxBandwidth / 8)) {
//!     return;  // Suppress packet
//! }
//! ```
//!
//! **A client that sends more is silently dropped.** No refusal, no
//! disconnect: the control link stays up, pings keep answering, the rider's own
//! meter moves, and the channel hears nothing. Worse, the packet is decrypted
//! *before* that check, so it has already been counted as a good received
//! packet — the connection quality this app now shows for each rider reports a
//! clean link with no loss while the words are being thrown away.
//!
//! So the budget is arithmetic, and this module is that arithmetic.
//!
//! # What a packet costs
//!
//! The server charges 20 bytes of IP, 8 of UDP and 4 of crypto overhead per
//! packet, plus the Opus payload. At [`FRAME_SAMPLES`] per packet that is a
//! fixed toll before any audio: see [`overhead_bps`]. **The toll is per packet,
//! so it is set by the frame length, not by the bitrate** — which is why a very
//! low budget cannot be met by turning the bitrate down alone, and why
//! [`Budget::below_floor`] exists to say so rather than pretend.
//!
//! Matching the server's own accounting is deliberate, including the parts of
//! it that are slightly wrong — it counts a UDP header for tunnelled voice too,
//! and does not count this client's own 4-to-8 byte Mumble header. What is
//! enforced is the server's number, not the true one.
//!
//! # Why the margin
//!
//! Opus is told a *target* bitrate and produces packets that vary around it,
//! while the server's bucket is fed the real sizes. Aiming exactly at the
//! budget therefore spends it on average and overshoots half the time, and each
//! overshoot is a packet nobody hears. [`SAFETY`] keeps a little back.

use super::codec::{Quality, FRAME_SAMPLES, SAMPLE_RATE};

/// What the server charges for a packet before any audio: IP, UDP, crypto.
pub const PACKET_OVERHEAD_BYTES: u32 = 20 + 8 + 4;

/// The lowest bitrate worth handing Opus for speech.
///
/// Below this a voice is present but not worth listening to, and the honest
/// report is that the budget cannot be met — not a whisper of static.
pub const MIN_AUDIO_BPS: u32 = 8_000;

/// Share of the audio budget actually aimed for. See the note on the margin.
pub const SAFETY: f32 = 0.9;

/// Packets per second at this build's frame length.
pub const fn packets_per_second() -> u32 {
    SAMPLE_RATE / FRAME_SAMPLES as u32
}

/// The per-packet toll, as bits per second.
pub const fn overhead_bps() -> u32 {
    PACKET_OVERHEAD_BYTES * 8 * packets_per_second()
}

/// What the encoder should be set to, and whether the server decided it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Budget {
    /// Bitrate to hand the encoder.
    pub bitrate_bps: u32,
    /// The server's cap, when it gave one.
    pub cap_bps: Option<u32>,
    /// Whether the cap, rather than this app's own choice, decided the figure.
    pub capped: bool,
    /// Whether even [`MIN_AUDIO_BPS`] does not fit the cap.
    ///
    /// Nothing here can fix that: the per-packet toll is set by the frame
    /// length, which is fixed in this build. Voice will be dropped, and the
    /// rider deserves to be told rather than left wondering.
    pub below_floor: bool,
}

/// Works out what to send inside `cap_bps`, given what this app would choose.
pub fn fit(cap_bps: Option<u32>, chosen_bps: u32) -> Budget {
    let Some(cap) = cap_bps.filter(|c| *c > 0) else {
        return Budget {
            bitrate_bps: chosen_bps,
            cap_bps: None,
            capped: false,
            below_floor: false,
        };
    };

    // What is left for audio once the packets themselves are paid for.
    let for_audio = cap.saturating_sub(overhead_bps());
    let aimed = (for_audio as f32 * SAFETY) as u32;

    if aimed < MIN_AUDIO_BPS {
        return Budget {
            bitrate_bps: MIN_AUDIO_BPS,
            cap_bps: Some(cap),
            capped: true,
            below_floor: true,
        };
    }

    Budget {
        bitrate_bps: aimed.min(chosen_bps),
        cap_bps: Some(cap),
        capped: aimed < chosen_bps,
        below_floor: false,
    }
}

/// The strictest cap among the servers this rider is on.
///
/// **One encoder feeds every connection**, so a rider on two servers sends the
/// same packets to both and has to fit inside the smaller allowance. Being
/// generous to the looser server would make them inaudible on the tighter one.
pub fn tightest(caps: impl IntoIterator<Item = u32>) -> Option<u32> {
    caps.into_iter().filter(|c| *c > 0).min()
}

/// What this build would send if nothing capped it.
pub const fn preferred_bps() -> u32 {
    Quality::Balanced.bitrate() as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_toll_matches_this_builds_frame_length() {
        // 20 ms frames: fifty packets a second, 32 bytes of header each.
        assert_eq!(packets_per_second(), 50);
        assert_eq!(overhead_bps(), 12_800);
    }

    #[test]
    fn no_cap_means_what_this_app_chose() {
        let b = fit(None, 24_000);
        assert_eq!(b.bitrate_bps, 24_000);
        assert!(!b.capped);
        assert!(!b.below_floor);
        assert_eq!(b.cap_bps, None);
    }

    #[test]
    fn a_zero_cap_is_no_cap() {
        // Servers leave the field out rather than sending zero, but a zero
        // read as a limit would mute the app on a server that meant nothing.
        assert!(!fit(Some(0), 24_000).capped);
    }

    #[test]
    fn murmurs_default_leaves_this_app_alone() {
        // 72 kbit/s is the out-of-the-box figure, and Balanced plus the toll
        // is well inside it. A default server must not reduce anything.
        let b = fit(Some(72_000), 24_000);
        assert_eq!(b.bitrate_bps, 24_000);
        assert!(!b.capped);
    }

    #[test]
    fn a_tight_cap_decides_the_bitrate() {
        // 32 kbit/s: 12.8 goes on packet headers, leaving 19.2, and the margin
        // aims at 17.2 — below what this app wanted, so the cap decides.
        let b = fit(Some(32_000), 24_000);
        assert!(b.capped);
        assert_eq!(b.bitrate_bps, 17_280);
        assert!(!b.below_floor);
        assert!(
            b.bitrate_bps + overhead_bps() < 32_000,
            "and what goes on the wire has to fit, or the packets are dropped"
        );
    }

    #[test]
    fn the_margin_leaves_room_for_opus_to_vary() {
        // Opus is given a target and varies around it while the server counts
        // real sizes, so aiming at the whole budget overshoots half the time.
        let cap = 48_000;
        let b = fit(Some(cap), 40_000);
        let aimed_total = b.bitrate_bps + overhead_bps();
        assert!(aimed_total < cap);
        assert!(
            (cap - aimed_total) as f32 / cap as f32 > 0.03,
            "a margin too thin to absorb a burst is not a margin"
        );
    }

    #[test]
    fn a_budget_nothing_can_meet_says_so() {
        // 16 kbit/s cannot even pay the packet toll, let alone carry speech.
        // Turning the bitrate down further cannot fix it — the toll is per
        // packet and the frame length is fixed in this build — so the honest
        // answer is a flag, not a quieter lie.
        let b = fit(Some(16_000), 24_000);
        assert!(b.below_floor);
        assert!(b.capped);
        assert_eq!(b.bitrate_bps, MIN_AUDIO_BPS);
    }

    #[test]
    fn a_cap_below_the_toll_alone_is_still_reported() {
        let b = fit(Some(5_000), 24_000);
        assert!(b.below_floor);
        assert_eq!(b.cap_bps, Some(5_000));
    }

    #[test]
    fn a_generous_cap_does_not_raise_the_bitrate() {
        // The cap is a ceiling, never an instruction to spend more: this app
        // picks its rate for a moving vehicle, not for the link's comfort.
        let b = fit(Some(500_000), 24_000);
        assert_eq!(b.bitrate_bps, 24_000);
        assert!(!b.capped);
    }

    #[test]
    fn the_strictest_server_wins() {
        // One encoder, many servers. The looser server's allowance is no help
        // to the rider who is inaudible on the tighter one.
        assert_eq!(tightest([72_000, 32_000, 48_000]), Some(32_000));
        assert_eq!(
            tightest([0, 48_000]),
            Some(48_000),
            "an absent cap is not a limit"
        );
        assert_eq!(tightest(Vec::<u32>::new()), None);
        assert_eq!(tightest([0, 0]), None);
    }

    #[test]
    fn what_this_build_prefers_fits_a_default_server() {
        let b = fit(Some(72_000), preferred_bps());
        assert!(!b.capped, "or every default server would reduce quality");
    }
}
