pub mod api;
mod frb_generated;

// What this process costs, per platform. Not under `api` because nothing here
// crosses the bridge: `api::mumbleway` calls it and puts the two numbers into
// `UiDiagnostics`.
mod usage;

// Android needs the JVM and the app Context handed to the audio backend before
// a stream can be played, and nothing in a Flutter process does that on its own.
#[cfg(target_os = "android")]
mod android_context;
