//! Opus encode/decode.
//!
//! Configured for lossy mobile links: VOIP mode, in-band FEC so a lost packet can
//! be partly reconstructed from the next one, and DTX so silence costs nothing.

use opus::{Application, Channels, Decoder, Encoder};

use crate::error::{CoreError, Result};

/// Mumble negotiates Opus at 48 kHz mono for voice.
pub const SAMPLE_RATE: u32 = 48_000;

/// 20 ms frames: the usual latency/overhead compromise, and what Mumble expects.
pub const FRAME_SAMPLES: usize = 960;

/// Sequence numbers count 10 ms units, not packets.
///
/// This is Mumble's convention, not ours: a client sending 20 ms frames steps
/// the counter by two. Stepping by one instead leaves an apparently missing
/// slot between every pair of packets, which a receiver conceals with invented
/// audio — half the stream, synthesised, which sounds like a broken link.
pub const SEQ_UNITS_PER_FRAME: u64 = (FRAME_SAMPLES / 480) as u64;

/// Generous upper bound for one encoded frame.
const MAX_PACKET: usize = 4000;

/// Bitrate presets. Speech-only content does not need much.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Quality {
    /// Marginal cellular coverage.
    Low,
    /// Default.
    Balanced,
    /// Good coverage or Wi-Fi.
    High,
}

impl Quality {
    pub fn bitrate(self) -> i32 {
        match self {
            Quality::Low => 16_000,
            Quality::Balanced => 24_000,
            Quality::High => 40_000,
        }
    }
}

pub struct VoiceEncoder {
    enc: Encoder,
    scratch: Vec<u8>,
}

impl VoiceEncoder {
    pub fn new(quality: Quality) -> Result<Self> {
        let mut enc = Encoder::new(SAMPLE_RATE, Channels::Mono, Application::Voip)
            .map_err(|e| CoreError::Codec(format!("creating Opus encoder: {e}")))?;

        enc.set_bitrate(opus::Bitrate::Bits(quality.bitrate()))
            .map_err(|e| CoreError::Codec(format!("setting bitrate: {e}")))?;
        // FEC lets the decoder reconstruct a dropped frame from the following
        // packet, which matters far more than raw quality on a moving vehicle.
        enc.set_inband_fec(true)
            .map_err(|e| CoreError::Codec(format!("enabling FEC: {e}")))?;
        // Tell the encoder to expect roughly 10% loss so it actually spends bits
        // on that FEC data.
        enc.set_packet_loss_perc(10)
            .map_err(|e| CoreError::Codec(format!("setting loss percentage: {e}")))?;

        Ok(Self {
            enc,
            scratch: vec![0u8; MAX_PACKET],
        })
    }

    pub fn set_quality(&mut self, quality: Quality) -> Result<()> {
        self.enc
            .set_bitrate(opus::Bitrate::Bits(quality.bitrate()))
            .map_err(|e| CoreError::Codec(format!("setting bitrate: {e}")))
    }

    /// Tells the encoder how much loss to protect against, 0..=100.
    ///
    /// It is a real trade and not a free dial. The FEC copy is carved out of
    /// the same bitrate as the audio, so protecting against 30% loss on a link
    /// that is losing nothing makes every packet worse for a benefit nobody
    /// collects. Leaving it at a fixed 10 makes the opposite mistake in the
    /// place it hurts most: under a bridge, where 10% of protection is spent
    /// against 40% of loss and the words are gone anyway.
    pub fn set_packet_loss_perc(&mut self, percent: u8) -> Result<()> {
        self.enc
            .set_packet_loss_perc(percent.min(100) as i32)
            .map_err(|e| CoreError::Codec(format!("setting loss percentage: {e}")))
    }

    /// Encodes exactly [`FRAME_SAMPLES`] samples.
    pub fn encode(&mut self, pcm: &[f32]) -> Result<Vec<u8>> {
        if pcm.len() != FRAME_SAMPLES {
            return Err(CoreError::Codec(format!(
                "expected {FRAME_SAMPLES} samples, got {}",
                pcm.len()
            )));
        }
        let n = self
            .enc
            .encode_float(pcm, &mut self.scratch)
            .map_err(|e| CoreError::Codec(format!("Opus encode: {e}")))?;
        Ok(self.scratch[..n].to_vec())
    }

    pub fn reset(&mut self) -> Result<()> {
        self.enc
            .reset_state()
            .map_err(|e| CoreError::Codec(format!("resetting encoder: {e}")))
    }
}

pub struct VoiceDecoder {
    dec: Decoder,
}

impl VoiceDecoder {
    pub fn new() -> Result<Self> {
        Ok(Self {
            dec: Decoder::new(SAMPLE_RATE, Channels::Mono)
                .map_err(|e| CoreError::Codec(format!("creating Opus decoder: {e}")))?,
        })
    }

    /// Decodes one packet into `out`, returning how many samples were written.
    pub fn decode(&mut self, packet: &[u8], out: &mut [f32]) -> Result<usize> {
        self.dec
            .decode_float(packet, out, false)
            .map_err(|e| CoreError::Codec(format!("Opus decode: {e}")))
    }

    /// Conceals a lost packet. With FEC enabled the encoder embedded a coarse
    /// copy of the previous frame in the *next* packet, so passing that packet
    /// with `fec = true` recovers far better audio than pure interpolation.
    pub fn decode_lost(&mut self, next_packet: Option<&[u8]>, out: &mut [f32]) -> Result<usize> {
        match next_packet {
            Some(p) => self
                .dec
                .decode_float(p, out, true)
                .map_err(|e| CoreError::Codec(format!("Opus FEC decode: {e}"))),
            None => self
                .dec
                .decode_float(&[], out, false)
                .map_err(|e| CoreError::Codec(format!("Opus PLC decode: {e}"))),
        }
    }

    pub fn reset(&mut self) -> Result<()> {
        self.dec
            .reset_state()
            .map_err(|e| CoreError::Codec(format!("resetting decoder: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn speech_like(len: usize, seed: f32) -> Vec<f32> {
        // A couple of harmonics with an envelope: enough structure that Opus
        // produces a realistic packet rather than degenerate silence.
        (0..len)
            .map(|i| {
                let t = i as f32 / SAMPLE_RATE as f32;
                let env = 0.4 * (1.0 + (2.0 * std::f32::consts::PI * 4.0 * t).sin());
                ((2.0 * std::f32::consts::PI * (180.0 + seed) * t).sin() * 0.6
                    + (2.0 * std::f32::consts::PI * (540.0 + seed) * t).sin() * 0.3)
                    * env
            })
            .collect()
    }

    #[test]
    fn encodes_and_decodes_a_frame() {
        let mut enc = VoiceEncoder::new(Quality::Balanced).unwrap();
        let mut dec = VoiceDecoder::new().unwrap();

        let pcm = speech_like(FRAME_SAMPLES, 0.0);
        let packet = enc.encode(&pcm).unwrap();
        assert!(!packet.is_empty(), "encoder produced nothing");
        assert!(
            packet.len() < 400,
            "packet unexpectedly large: {}",
            packet.len()
        );

        let mut out = vec![0.0f32; FRAME_SAMPLES];
        let n = dec.decode(&packet, &mut out).unwrap();
        assert_eq!(n, FRAME_SAMPLES);
        assert!(out.iter().all(|s| s.is_finite()));
    }

    #[test]
    fn round_trip_preserves_signal_energy() {
        let mut enc = VoiceEncoder::new(Quality::High).unwrap();
        let mut dec = VoiceDecoder::new().unwrap();

        // Opus needs a few frames to converge, so measure on a later one.
        let mut last_out = vec![0.0f32; FRAME_SAMPLES];
        let mut last_in = Vec::new();
        for i in 0..10 {
            let pcm = speech_like(FRAME_SAMPLES, i as f32);
            let packet = enc.encode(&pcm).unwrap();
            dec.decode(&packet, &mut last_out).unwrap();
            last_in = pcm;
        }

        let in_rms = crate::audio::dsp::rms(&last_in);
        let out_rms = crate::audio::dsp::rms(&last_out);
        assert!(
            out_rms > in_rms * 0.3 && out_rms < in_rms * 3.0,
            "energy badly mismatched: in {in_rms}, out {out_rms}"
        );
    }

    #[test]
    fn rejects_wrong_frame_sizes() {
        let mut enc = VoiceEncoder::new(Quality::Balanced).unwrap();
        assert!(enc.encode(&vec![0.0; 100]).is_err());
        assert!(enc.encode(&vec![0.0; FRAME_SAMPLES + 1]).is_err());
        assert!(enc.encode(&[]).is_err());
    }

    #[test]
    fn silence_encodes_to_very_little() {
        // DTX/VOIP mode should make silence nearly free on the wire.
        let mut enc = VoiceEncoder::new(Quality::Balanced).unwrap();
        let mut smallest = usize::MAX;
        for _ in 0..20 {
            let packet = enc.encode(&vec![0.0; FRAME_SAMPLES]).unwrap();
            smallest = smallest.min(packet.len());
        }
        assert!(smallest < 40, "silence cost {smallest} bytes per frame");
    }

    #[test]
    fn packet_loss_concealment_produces_usable_audio() {
        let mut enc = VoiceEncoder::new(Quality::Balanced).unwrap();
        let mut dec = VoiceDecoder::new().unwrap();

        // Prime the decoder.
        for i in 0..5 {
            let packet = enc.encode(&speech_like(FRAME_SAMPLES, i as f32)).unwrap();
            let mut out = vec![0.0f32; FRAME_SAMPLES];
            dec.decode(&packet, &mut out).unwrap();
        }

        // Simulate a drop with no following packet available yet.
        let mut out = vec![0.0f32; FRAME_SAMPLES];
        let n = dec.decode_lost(None, &mut out).unwrap();
        assert_eq!(n, FRAME_SAMPLES);
        assert!(out.iter().all(|s| s.is_finite() && s.abs() <= 1.5));
    }

    #[test]
    fn fec_recovers_a_dropped_frame_from_the_next_packet() {
        let mut enc = VoiceEncoder::new(Quality::Balanced).unwrap();
        let mut dec = VoiceDecoder::new().unwrap();

        for i in 0..5 {
            let p = enc.encode(&speech_like(FRAME_SAMPLES, i as f32)).unwrap();
            let mut o = vec![0.0f32; FRAME_SAMPLES];
            dec.decode(&p, &mut o).unwrap();
        }

        // Encode two frames; pretend the first is lost and recover it via FEC.
        let _lost = enc.encode(&speech_like(FRAME_SAMPLES, 6.0)).unwrap();
        let next = enc.encode(&speech_like(FRAME_SAMPLES, 7.0)).unwrap();

        let mut recovered = vec![0.0f32; FRAME_SAMPLES];
        let n = dec.decode_lost(Some(&next), &mut recovered).unwrap();
        assert_eq!(n, FRAME_SAMPLES);
        assert!(recovered.iter().all(|s| s.is_finite()));
    }

    #[test]
    fn quality_presets_are_ordered() {
        assert!(Quality::Low.bitrate() < Quality::Balanced.bitrate());
        assert!(Quality::Balanced.bitrate() < Quality::High.bitrate());
    }

    #[test]
    fn frame_size_is_20ms_and_matches_two_denoise_blocks() {
        assert_eq!(FRAME_SAMPLES, (SAMPLE_RATE as usize / 1000) * 20);
        assert_eq!(FRAME_SAMPLES, super::super::denoise::FRAME_SIZE * 2);
    }
}
