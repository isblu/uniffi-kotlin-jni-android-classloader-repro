/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::{ffi::CStr, sync::OnceLock};

use anyhow::{ensure, Result};
use jni_sys::*;

use crate::JniString;

struct AppClassLoader {
    object: jobject,
    load_class: jmethodID,
}

// SAFETY: object is a global reference retained for the JVM lifetime; method IDs are thread independent.
unsafe impl Send for AppClassLoader {}
// SAFETY: JNI calls use the calling thread's JNIEnv and never mutate the cached handles.
unsafe impl Sync for AppClassLoader {}

static APP_CLASS_LOADER: OnceLock<AppClassLoader> = OnceLock::new();

/// Capture the bridge's loader from a Kotlin-to-Rust native call before starting native work.
///
/// # Safety
/// env must belong to the current thread, with a Scaffolding native method on its Java stack.
pub(crate) unsafe fn initialize_app_class_loader(env: *mut JNIEnv) -> Result<()> {
    if APP_CLASS_LOADER.get().is_some() {
        return Ok(());
    }
    // SAFETY: env is current; all class names, signatures and JNI argument types match the Java APIs.
    unsafe {
        let bridge = ((**env).v1_4.FindClass)(env, c"uniffi/Scaffolding".as_ptr());
        ensure!(!bridge.is_null(), "UniFFI bridge class not found");
        let class = ((**env).v1_4.GetObjectClass)(env, bridge);
        ensure!(!class.is_null(), "UniFFI bridge class metadata not found");
        let get_loader = ((**env).v1_4.GetMethodID)(
            env,
            class,
            c"getClassLoader".as_ptr(),
            c"()Ljava/lang/ClassLoader;".as_ptr(),
        );
        ensure!(!get_loader.is_null(), "Class.getClassLoader not found");
        let loader = ((**env).v1_4.CallObjectMethodA)(env, bridge, get_loader, std::ptr::null());
        ensure!(!loader.is_null(), "UniFFI bridge class loader not found");
        let loader_class = ((**env).v1_4.GetObjectClass)(env, loader);
        ensure!(
            !loader_class.is_null(),
            "UniFFI class loader metadata not found"
        );
        let load_class = ((**env).v1_4.GetMethodID)(
            env,
            loader_class,
            c"loadClass".as_ptr(),
            c"(Ljava/lang/String;)Ljava/lang/Class;".as_ptr(),
        );
        ensure!(!load_class.is_null(), "ClassLoader.loadClass not found");
        let object = ((**env).v1_4.NewGlobalRef)(env, loader);
        ensure!(!object.is_null(), "Could not retain UniFFI class loader");
        if let Err(unused) = APP_CLASS_LOADER.set(AppClassLoader { object, load_class }) {
            ((**env).v1_4.DeleteGlobalRef)(env, unused.object);
        }
        for reference in [loader_class, loader, class, bridge] {
            ((**env).v1_4.DeleteLocalRef)(env, reference);
        }
    }
    Ok(())
}

/// Resolve generated classes through their app loader, including on attached native threads.
///
/// # Safety
/// env must belong to the current thread. The returned class is a local reference.
pub(crate) unsafe fn find_class(env: *mut JNIEnv, name: &CStr) -> jclass {
    // SAFETY: names are valid MUTF-8 class names; the cached loader is global and the method signature matches.
    unsafe {
        if name.to_bytes().starts_with(b"uniffi/") {
            if let Some(loader) = APP_CLASS_LOADER.get() {
                let dotted = name.to_string_lossy().replace('/', ".");
                let name = JniString::from(dotted).into_jstring(env);
                if name.is_null() {
                    return std::ptr::null_mut();
                }
                let class = ((**env).v1_4.CallObjectMethodA)(
                    env,
                    loader.object,
                    loader.load_class,
                    [jvalue { l: name }].as_ptr(),
                );
                ((**env).v1_4.DeleteLocalRef)(env, name);
                return class;
            }
        }
        ((**env).v1_4.FindClass)(env, name.as_ptr())
    }
}
