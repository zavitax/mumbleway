//! Handing Android's JVM and app Context to the audio backend.
//!
//! cpal reaches the Java audio APIs through `ndk_context`, which is a pair of
//! raw pointers some earlier code is expected to have registered. In an app
//! built around `ndk-glue` that happens in the generated `main`. A Flutter app
//! has no such main: this crate is a library loaded into a process Java already
//! owns, so nothing registers anything and `ndk_context::android_context()`
//! panics the first time a stream is played.
//!
//! The panic surfaced as "audio device did not start", ten seconds after the
//! fact and on a different thread, which is why it survived so long: the thread
//! that failed was not the thread that reported, and the message it left went
//! to stderr, which Android discards.
//!
//! [`JNI_OnLoad`] is called by the runtime when `System.loadLibrary` maps this
//! library, which is the earliest moment a JVM pointer exists, and before any
//! Dart runs. The Context arrives separately from [`MainActivity`], because the
//! JVM alone is not enough — the backend needs an application Context to ask
//! for audio focus.

use std::ffi::c_void;

use jni::objects::{JClass, JObject};
use jni::sys::jint;
use jni::{JNIEnv, JavaVM};

/// Stashed until the Context arrives, since the two are registered together.
static mut JAVA_VM: *mut c_void = std::ptr::null_mut();

/// Called by the runtime as this library is loaded.
///
/// # Safety
/// Invoked by the JVM with a valid `JavaVM`; not callable from Rust.
#[no_mangle]
pub unsafe extern "C" fn JNI_OnLoad(vm: JavaVM, _reserved: *mut c_void) -> jint {
    JAVA_VM = vm.get_java_vm_pointer() as *mut c_void;
    // 1.6 is the floor every Android runtime supports and all this needs.
    jni::sys::JNI_VERSION_1_6
}

/// Registers the application Context, completing what the audio backend needs.
///
/// The reference is deliberately leaked into a global one and never released:
/// it lives exactly as long as the process, and `ndk_context` holds it as a raw
/// pointer with no way to learn that it has gone. Freeing it would hand the
/// audio backend a dangling reference at some unpredictable later moment.
///
/// # Safety
/// Called from Java with a live `Context`; not callable from Rust.
#[no_mangle]
pub unsafe extern "C" fn Java_com_mumbleway_mumbleway_MainActivity_nativeSetAndroidContext(
    env: JNIEnv,
    _class: JClass,
    context: JObject,
) {
    let Ok(global) = env.new_global_ref(context) else {
        // Nothing to be done from here, and panicking across the JNI boundary
        // during startup would take the app with it. Audio then fails with its
        // own message, which is at least the one the user can act on.
        return;
    };
    let raw = global.as_raw() as *mut c_void;
    std::mem::forget(global);

    if !JAVA_VM.is_null() {
        ndk_context::initialize_android_context(JAVA_VM, raw);
    }
}
