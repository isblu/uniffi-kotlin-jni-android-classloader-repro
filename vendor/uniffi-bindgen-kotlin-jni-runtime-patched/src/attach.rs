/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::ffi::c_void;

use jni_sys::*;

/// Attach the current thread to the JVM and run a closure
///
/// This is used to implement callback interfaces. Threads attached here are detached
/// after the closure returns or unwinds; existing attachments remain intact.
///
/// The closure must not return any JNI objects,
/// since their lifetime ends before this function returns.
///
/// # Safety
///
/// jvm must point to a valid JavaVM
pub unsafe fn attach_current_thread<F, T>(jvm: *mut JavaVM, f: F) -> T
where
    F: FnOnce(*mut JNIEnv) -> T,
{
    let mut env: *mut JNIEnv = std::ptr::null_mut();
    // SAFETY: jvm is valid; GetEnv writes the current thread's JNI environment.
    let status =
        unsafe { ((**jvm).v1_2.GetEnv)(jvm, std::ptr::from_mut(&mut env).cast(), JNI_VERSION_1_2) };
    let attached = match status {
        JNI_OK => false,
        JNI_EDETACHED => {
            // SAFETY: jvm is valid; the output points to writable environment storage.
            let result = unsafe {
                ((**jvm).v1_2.AttachCurrentThread)(
                    jvm,
                    std::ptr::from_mut(&mut env).cast::<*mut c_void>(),
                    std::ptr::null_mut(),
                )
            };
            assert_eq!(result, JNI_OK, "AttachCurrentThread failed");
            true
        }
        _ => panic!("GetEnv failed: {status}"),
    };
    let _attachment = Attachment { jvm, attached };
    // SAFETY: env is valid for this attached thread.
    assert_eq!(
        unsafe { ((**env).v1_2.PushLocalFrame)(env, 16) },
        0,
        "PushLocalFrame failed"
    );
    let _frame = LocalFrame(env);
    f(env)
}

struct Attachment {
    jvm: *mut JavaVM,
    attached: bool,
}

impl Drop for Attachment {
    fn drop(&mut self) {
        if self.attached {
            // SAFETY: this guard owns the attachment created on this thread, after all local frames close.
            unsafe {
                ((**self.jvm).v1_2.DetachCurrentThread)(self.jvm);
            }
        }
    }
}

struct LocalFrame(*mut JNIEnv);

impl Drop for LocalFrame {
    fn drop(&mut self) {
        // SAFETY: this guard owns a successfully pushed local frame on the current thread.
        unsafe {
            ((**self.0).v1_2.PopLocalFrame)(self.0, std::ptr::null_mut());
        }
    }
}
