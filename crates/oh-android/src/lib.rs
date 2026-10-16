//! `oh-android` — the Android (aarch64) entry crate (issue 25b, D-026).
//!
//! The thin, target-gated glue between the Android runtime and the shared
//! windowed pipeline:
//!
//! * the `android_main(app: AndroidApp)` native-activity entry (D-026:
//!   `native-activity`, not `game-activity` — the smaller surface, no Java
//!   game framework) — android-activity 0.6 through Winit's re-export
//!   (`winit::platform::android::activity::AndroidApp`), exactly the
//!   signature the glue's C wrapper expects (`fn android_main(app)`);
//! * the `[package.metadata.android]` cargo-apk section (issue 25b SG6 —
//!   package name, the aarch64-only target, the `_RELEASE` content as APK
//!   assets, the SDK bounds, the fullscreen `NativeActivity`);
//! * the cdylib name: the `liboh_android.so` cargo-apk ships into the
//!   APK's `lib/arm64-v8a/` (the `android.app.lib_name` meta-data cargo-apk
//!   writes is this lib name).
//!
//! Everything the game does on Android — the assets extraction + chdir, the
//! config load, the `WindowedClient` composition (boot dispatch, MenuGame,
//! HexagonGame), the winit/GLES event loop, the touch input — lives in
//! `oh_app::android` so the desktop and Android paths share ONE windowed
//! pipeline (no copy-paste fork of the game loop; this crate only hands the
//! `AndroidApp` to it).
//!
//! On every non-Android target the crate builds to an empty cdylib (the
//! entry symbol + the winit android modules are `target_os = "android"`-
//! gated), so the desktop gate (`cargo build/clippy/test --workspace`)
//! never sees it.

#[cfg(target_os = "android")]
mod android;
