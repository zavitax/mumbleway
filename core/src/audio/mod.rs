//! Audio capture, processing, coding and playback.

pub mod aec;
pub mod codec;
pub mod dehiss;
pub mod denoise;
pub mod feedback;
pub mod dsp;
pub mod engine;
pub mod jitter;
pub mod resample;

pub use codec::{Quality, VoiceDecoder, VoiceEncoder};
pub use denoise::{CaptureProcessor, NoiseProfile};
pub use engine::{AudioConfig, AudioCue, AudioEngine, AudioShared, TransmitMode};
pub use jitter::SpeakerBuffer;
pub use resample::Resampler;
