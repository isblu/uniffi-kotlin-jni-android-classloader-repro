# uniffi-bindgen-kotlin-jni: Android callback from a Rust thread fails with `Class not found: uniffi/UniffiKt`

Minimal Android repro for [`uniffi-bindgen-kotlin-jni`](https://github.com/mozilla/uniffi-rs/tree/c31f4bc025c7a31eae56b49107f50e6d38e76bc7/uniffi-bindgen-kotlin-jni)
at mozilla/uniffi-rs `main` @ [`c31f4bc`](https://github.com/mozilla/uniffi-rs/commit/c31f4bc025c7a31eae56b49107f50e6d38e76bc7)
(2026-10-16; still `main` HEAD on 2026-10-20).

**Bug:** the first time a callback-interface method is invoked on a Rust-spawned thread, the
generated scaffolding lazily calls JNI `FindClass("uniffi/UniffiKt")`. On Android, a native thread
attached with `AttachCurrentThread` resolves `FindClass` through the *system* class loader, which
cannot see app classes, so it fails and the runtime panics:

```
Class not found: uniffi/UniffiKt
```

If the same method has already been invoked once on a Java thread, the `OnceLock` cache is
populated and the Rust-thread call works. So whether this shows up depends on timing.
Desktop JVM tests don't catch it, because there the system class loader can see the app classes.

## Downloads (debug APKs, arm64-v8a)

See [Release v0.1](https://github.com/isblu/uniffi-kotlin-jni-android-classloader-repro/releases/tag/v0.1):

| APK | applicationId | Runtime |
|---|---|---|
| `uniffi-jni-repro-upstream-arm64-v8a-debug.apk` | `dev.isblu.uniffijnirepro` | unpatched upstream `uniffi-bindgen-kotlin-jni-runtime` (git, `c31f4bc`) |
| `uniffi-jni-repro-patched-arm64-v8a-debug.apk` | `dev.isblu.uniffijnirepro.patched` | same plus an app-class-loader fix ([`vendor/`](vendor/uniffi-bindgen-kotlin-jni-runtime-patched)) |

The two APKs have different application IDs, so you can install both at the same time.

## Steps

1. Cold-start **UniFFI JNI Repro** (force-stop it first if it was already running).
2. Tap **1) Fire from Rust thread first**.
3. Tap **2) Fire on main thread**, then tap **1** again.

Watch the screen, or run `adb logcat -s Repro`.

| Button | What it does |
|---|---|
| 1) Fire from Rust thread first | `fire_from_rust_thread(listener)`: `std::thread::spawn` calls `listener.on_event(1)`, joins, and reports OK or the caught panic message. |
| 2) Fire on main thread | `fire_on_calling_thread(listener)`: calls `listener.on_event(0)` on the calling (Android main) thread. |

### Expected

Every tap logs `Kotlin Listener.onEvent(n) ...` and `... returned OK`.

### Actual (upstream flavor), observed on an API 35 x86_64 emulator

```
I/Repro: started, runtime = upstream (unpatched uniffi-bindgen-kotlin-jni-runtime @ c31f4bc)
I/Repro: [rust] rust thread: PANIC: Class not found: uniffi/UniffiKt          <- tap 1 (first ever on_event)
I/Repro: Kotlin Listener.onEvent(0) on thread 'main'                          <- tap 2
I/Repro: [rust] calling thread: on_event(0) returned OK
I/Repro: Kotlin Listener.onEvent(1) on thread 'Thread-3'                      <- tap 1 again: now works
I/Repro: [rust] rust thread: on_event(1) returned OK
```

The repro catches the panic with `catch_unwind` so it can show the message: on Android, Rust panic
output goes to stderr, which is discarded. In a real app the panic ends up somewhere else: it unwinds
across the callback (or aborts with `panic = "abort"`). The `NoClassDefFoundError` that
`FindClass` raises stays pending on the attached thread, because upstream never detaches it.

### Patched flavor

Tapping **1** first logs `Kotlin Listener.onEvent(1) on thread 'Thread-2'` / `returned OK`.

## Where it happens (upstream @ c31f4bc)

- [`runtime/src/caching.rs#L337-L344`](https://github.com/mozilla/uniffi-rs/blob/c31f4bc025c7a31eae56b49107f50e6d38e76bc7/uniffi-bindgen-kotlin-jni/runtime/src/caching.rs#L337-L344):
  `CachedStaticMethod::get` does `FindClass` lazily inside `OnceLock::get_or_init` and panics with
  `Class not found: ...` on null.
- [`src/scaffolding/templates/callback_interface.rs#L62-L68`](https://github.com/mozilla/uniffi-rs/blob/c31f4bc025c7a31eae56b49107f50e6d38e76bc7/uniffi-bindgen-kotlin-jni/src/scaffolding/templates/callback_interface.rs#L62-L68):
  each sync callback method dispatcher declares `static METHOD: CachedStaticMethod = new(c"uniffi/UniffiKt", ...)`
  and resolves it on whatever thread calls it, after `attach_current_thread`.
  The same lazy pattern is used for async callback methods (`#L138-L151`), the callback free/`Drop` path (`#L178-L183`), and future continuations (`async.rs#L52`).
- [`runtime/src/attach.rs#L19-L44`](https://github.com/mozilla/uniffi-rs/blob/c31f4bc025c7a31eae56b49107f50e6d38e76bc7/uniffi-bindgen-kotlin-jni/runtime/src/attach.rs#L19-L44):
  `AttachCurrentThread` with no class-loader handling, and the thread is never detached.
- [`src/scaffolding/templates/jni_support.rs#L1-L8`](https://github.com/mozilla/uniffi-rs/blob/c31f4bc025c7a31eae56b49107f50e6d38e76bc7/uniffi-bindgen-kotlin-jni/src/scaffolding/templates/jni_support.rs#L1-L8):
  `JNI_OnLoad` only stores the `JavaVM*`. Nothing is pre-cached on a Java thread.

In this repo:

- [`rust/src/lib.rs#L45-L59`](rust/src/lib.rs#L45-L59): `fire_from_rust_thread` (the spawned-thread call is on L49).
- The generated scaffolding lands in `rust/target/**/out/uniffi_bindgen_kotlin_jni.uniffi.rs`.
  Look for `callback_interface_dispatch_11000000` and `c"uniffi/UniffiKt"`.

## The fix being tested (patched flavor)

[`vendor/uniffi-bindgen-kotlin-jni-runtime-patched`](vendor/uniffi-bindgen-kotlin-jni-runtime-patched) is the
upstream runtime plus [`src/class_loader.rs`](vendor/uniffi-bindgen-kotlin-jni-runtime-patched/src/class_loader.rs).
On the first Kotlin->Rust call (a Java thread), it caches a global ref to the app's `ClassLoader`.
After that, `uniffi/*` classes are resolved with `ClassLoader.loadClass` instead of `FindClass`, on any thread.
`attach.rs` also detaches threads that it attached.

The `patched` cargo feature aliases this crate to `uniffi_bindgen_kotlin_jni_runtime` (`rust/src/lib.rs#L7-L8`).
The generated scaffolding is unchanged.

## Upstream references

- mozilla/uniffi-rs#2672 "JNI library for Kotlin/Java/KMP". In
  [paxbun's comment (2025-09-29)](https://github.com/mozilla/uniffi-rs/issues/2672#issuecomment-3346721261),
  he warns that lazily resolving classes needs "some hacks ... when invoking callbacks in a native
  thread on Android, because on Android, even when attached, the default class loader of native threads
  has no access to user-defined classes".
- mozilla/uniffi-rs#2911: the PR that added `uniffi-bindgen-kotlin-jni`.
- Android JNI tips, [FindClass and native threads](https://developer.android.com/training/articles/perf-jni#faq_FindClass).

## Build

Requirements (same as the setup this was extracted from):

- Rust stable with `aarch64-linux-android` (plus `x86_64-linux-android` for the emulator).
- `cargo install cargo-ndk`.
- Android SDK with platform 37.2 and NDK `27.1.12297006`.
- JDK 17.

```sh
cd android
echo "sdk.dir=$ANDROID_HOME" > local.properties
./gradlew --max-workers=2 :app:assembleUpstreamDebug :app:assemblePatchedDebug
# APKs: app/build/outputs/apk/{upstream,patched}/debug/app-*-debug.apk
# Emulator: also build x86_64 with -PreproExtraAbis=x86_64
```

The Gradle build does three things:

- `generateUniffiKotlin` runs `cargo run --features cli --bin uniffi-bindgen -- bindings src:repro <out>`
  (`uniffi_parse_rs`, so no host library build is needed).
- `buildRustUpstream` / `buildRustPatched` run `cargo ndk -t arm64-v8a build --release [--no-default-features --features patched]`
  with 16 KB page-size link flags.
- `rust/build.rs` calls `uniffi_bindgen_kotlin_jni::generate_scaffolding()`.

`rust/uniffi.toml` sets `disable_java_cleaner = true`, because `SystemCleaner` needs API 33 and `minSdk` here is 28.
