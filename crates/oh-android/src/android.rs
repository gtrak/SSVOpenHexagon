//! The `android_main` native-activity entry (D-026).
//!
//! The android-activity glue spawns a dedicated main-loop thread and calls
//! this with a reference-counted `AndroidApp` handle; the entry hands it to
//! `oh_app::android::run`, which drives the shared windowed pipeline
//! (assets → config → the winit android event loop → `WindowedClient` →
//! `boot` → the winit loop) and returns when the window closes (the
//! activity's `TerminateWindow` — `run_app` returns, the thread ends, the
//! C++'s `mainClient` `return 0` equivalent).
//!
//! `#[no_mangle]` + the default (Rust) ABI: the glue's C `android_main`
//! wrapper links this Rust symbol directly (android-activity 0.6's entry
//! contract — its lib docs: "an exported, unmangled, `\"Rust\"` ABI
//! function with the signature `fn android_main(app: AndroidApp)`").

use winit::platform::android::activity::AndroidApp;

#[no_mangle]
pub fn android_main(app: AndroidApp) {
    oh_app::android::run(app);
}
