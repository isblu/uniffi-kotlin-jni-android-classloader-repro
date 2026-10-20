//! Minimal repro: a callback-interface method invoked for the first time on a Rust-spawned thread.
//!
//! The generated JNI scaffolding refers to `::uniffi_bindgen_kotlin_jni_runtime`. With the
//! `patched` feature we alias the vendored (fixed) copy of the runtime to that name.
#[cfg(all(feature = "patched", feature = "upstream"))]
compile_error!("enable exactly one of `upstream` (default) or `patched`");
#[cfg(feature = "patched")]
extern crate runtime_patched as uniffi_bindgen_kotlin_jni_runtime;

uniffi::setup_scaffolding!();

include!(concat!(env!("OUT_DIR"), "/uniffi_bindgen_kotlin_jni.uniffi.rs"));

use std::panic::{catch_unwind, AssertUnwindSafe};

#[uniffi::export(callback_interface)]
pub trait Listener: Send + Sync {
    fn on_event(&self, n: u32);
}

/// Which runtime this library was built with.
#[uniffi::export]
pub fn runtime_flavor() -> String {
    if cfg!(feature = "patched") {
        "patched (app class loader cached, PR #74 runtime)".into()
    } else {
        "upstream (unpatched uniffi-bindgen-kotlin-jni-runtime @ c31f4bc)".into()
    }
}

/// Calls `listener.on_event(0)` on the calling (Kotlin main) thread.
#[uniffi::export]
pub fn fire_on_calling_thread(listener: Box<dyn Listener>) -> String {
    listener.on_event(0);
    report("calling thread: on_event(0) returned OK".into())
}

/// Calls `listener.on_event(1)` from a fresh `std::thread::spawn` thread and joins it.
///
/// The panic (if any) is caught and returned so it is visible on screen and in logcat; on Android
/// Rust panic messages otherwise go to stderr, which is discarded. The listener is only borrowed
/// inside `catch_unwind` and is dropped back on the calling thread, so the Kotlin-side
/// `callbackInterfaceFree` lookup does not also run on the spawned thread.
#[uniffi::export]
pub fn fire_from_rust_thread(listener: Box<dyn Listener>) -> String {
    let handle = std::thread::Builder::new()
        .name("rust-spawned".into())
        .spawn(move || {
            let result = catch_unwind(AssertUnwindSafe(|| listener.on_event(1)));
            (listener, result)
        })
        .expect("spawn");
    let msg = match handle.join() {
        Ok((_listener, Ok(()))) => "rust thread: on_event(1) returned OK".to_string(),
        Ok((_listener, Err(p))) => format!("rust thread: PANIC: {}", panic_msg(&p)),
        Err(p) => format!("rust thread: thread died: {}", panic_msg(&p)),
    };
    report(msg)
}

fn panic_msg(p: &Box<dyn std::any::Any + Send>) -> String {
    p.downcast_ref::<String>()
        .cloned()
        .or_else(|| p.downcast_ref::<&str>().map(|s| s.to_string()))
        .unwrap_or_else(|| "<non-string panic>".into())
}

fn report(msg: String) -> String {
    #[cfg(target_os = "android")]
    {
        #[link(name = "log")]
        extern "C" {
            fn __android_log_write(prio: i32, tag: *const u8, text: *const u8) -> i32;
        }
        let text = std::ffi::CString::new(format!("[rust] {msg}")).unwrap_or_default();
        // 4 = ANDROID_LOG_INFO
        unsafe { __android_log_write(4, b"Repro\0".as_ptr(), text.as_ptr().cast()) };
    }
    msg
}
