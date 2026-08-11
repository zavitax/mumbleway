pub mod api;
mod frb_generated;

// Android needs the JVM and the app Context handed to the audio backend before
// a stream can be played, and nothing in a Flutter process does that on its own.
#[cfg(target_os = "android")]
mod android_context;
