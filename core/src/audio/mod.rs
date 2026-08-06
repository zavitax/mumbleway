//! Audio capture, processing, coding and playback.

pub mod aec;
pub mod codec;
pub mod dehiss;
pub mod denoise;
pub mod dsp;
pub mod engine;
pub mod feedback;
pub mod jitter;
pub mod pitch;
pub mod quality;
pub mod resample;
pub mod spectrum;
pub mod stretch;
pub mod testsig;

pub use codec::{Quality, VoiceDecoder, VoiceEncoder};
pub use denoise::{CaptureProcessor, NoiseProfile};
pub use engine::{AudioConfig, AudioCue, AudioEngine, AudioShared, ChainStatus, TransmitMode};
pub use jitter::{SpeakerBuffer, DEFAULT_TARGET_FRAMES, MAX_TARGET_FRAMES, MIN_TARGET_FRAMES};
pub use resample::Resampler;
pub use spectrum::{SpectrumAnalyser, SpectrumFrame, BANDS, TAPS, TAP_PRE_GATE, TAP_RAW, TAP_SENT};
