//! Audio capture, processing, coding and playback.

pub mod aec;
pub mod codec;
pub mod denoise;
pub mod dsp;
pub mod engine;
pub mod jitter;
pub mod resample;

pub use codec::{Quality, VoiceDecoder, VoiceEncoder};
pub use denoise::{CaptureProcessor, NoiseProfile};
pub use engine::{AudioConfig, AudioCue, AudioEngine, AudioShared, TransmitMode};
pub use jitter::SpeakerBuffer;
pub use resample::Resampler;
