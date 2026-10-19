//! The Android `mainClient` (issue 25b, D-026) — the C++ `SSVOH_ANDROID`
//! build's equivalent over the SAME windowed pipeline as the desktop path:
//! the assets base dir (the app's internal data dir — the C++'s
//! chdir-to-executable-dir has no Android equivalent; the APK's `_RELEASE`
//! content is extracted there on first run, issue 25b SG4) → the
//! `mainClient` folders/config/assets pieces (`crate::client::main_client`'s
//! order, main.cpp:275-380) → the winit android event loop (the
//! `AndroidApp` from `android_main`, bound through
//! `EventLoopBuilderExtAndroid`) → `WindowedClient` (the desktop windowed
//! composition, `crate::windowed`) → `boot` → `run`.
//!
//! # The Android differences (every one documented, none silently faked)
//!
//! * **No SIGINT handler** — the desktop `client.install_sigint()` has no
//!   Android counterpart (the activity's lifecycle is the stop mechanism:
//!   the native activity's `TerminateWindow` closes the winit window → the
//!   loop-exit translation, `windowed.rs` module docs); the C++ Android
//!   build's windowed run is the same `GameWindow::run` body.
//! * **Steam** — the default (no-`steam`-feature) `NoSteam` backend,
//!   `crate::steam::steam_manager`: the C++'s `#ifndef SSVOH_ANDROID`
//!   build never links the SDK (Steam.cpp's Android no-SDK branch —
//!   `oh-app`'s feature-OFF default is that branch).
//! * **Discord** — the C++ Android arm (`Discord.cpp`: `_core{nullptr},
//!   _initialized{false}`, every method a logged `return false`): the
//!   port's `NoDiscordCore` manager, passed to `WindowedClient::build`
//!   (whose `None` arm would otherwise build the desktop IPC core).
//! * **CLI args** — the native activity carries none (the C++ Android
//!   build's `argv` is the loader's); `boot(None, None, None)` → the
//!   `gotoMenu` start (the C++ `mainClient`'s no-args arm, main.cpp:514-515
//!   "Start game as normal").
//! * **Window** — the activity's window (created by the platform, sized by
//!   the screen): `Window::create_android` records the CONFIG logical size
//!   (the C++'s logical `width`/`height` members) and takes the platform
//!   window + GL context on the first `resumed` (the EGL surface needs the
//!   activity's `ANativeWindow`, which only exists between `InitWindow` and
//!   `TerminateWindow` — `oh-render` module docs: the android GL note).
//!   The C++'s `main.cpp:305-332` config-setter half is applied by
//!   `create_android_window` (below) exactly as the desktop
//!   `create_window` does.
//! * **egui console** — desktop-only (D-026 plan: "the egui overlay is
//!   desktop-only"): on Android the lazy `IlcConsole::new` over the GLES
//!   context fails gracefully (the glow-0.17/`#version 330`-era egui
//!   shaders are the desktop GL surface) and the slot stays `None` — the
//!   game runs without the console overlay (the C++ Android build has no
//!   ImGui at all, `#ifndef SSVOH_ANDROID` in Imgui.cpp).
//! * **Audio** — the REAL `oh_audio::Audio` (the issue-025e replacement
//!   of the D-026-era `NoopAudio` / `None` stand-ins — the module doc
//!   here was stale): the SAME shared windowed composition as desktop
//!   (`WindowedClient::build`'s `oh_audio::Audio::new` over the cpal
//!   Android output + `set_sound_volume` / `set_music_volume` from the
//!   config, windowed.rs:2742-2754).
//! * **Engine logging** — issue 039 STEP 0: the `oh_game::log_sink`
//!   seam's Android sink (the `oh-web` browser-console sink's twin —
//!   issue 026): `run` installs an `__android_log_write` sink BEFORE any
//!   engine work, so the engine's `[channel]` lines (the C++
//!   `ssvu::lo` parity — `log()` / `host_error`) reach logcat under the
//!   `OpenHexagon` tag (`adb logcat -s OpenHexagon`). Without it the
//!   lines' stdout default never reaches logcat on Android (the device
//!   sessions' OCR navigation, issue 029k1's device-session note).
//! * **Soft input (the system IME)** — issue 025f (D-031): the C++'s
//!   Android text input came from SFML's own Android layer (the IME
//!   wiring lived in SFML, not in this repo) — the port's platform
//!   addition: the `ImeController` (the android-activity's
//!   `show_soft_input(true)` / `hide_soft_input(false)` on the menu's
//!   `is_entering_text` transition edge, once per transition — the
//!   `windowed::MenuSlotAdapter::refresh_input` docs). The character
//!   pipeline is the issue-025d chain (the `KeyboardInput.text` field →
//!   the entered-text sink → `TextEntered` → `enteredChars`).
//!
//! # Exit
//!
//! `run` returns when the winit loop ends (window closed — the activity's
//! `TerminateWindow`, or the engine stop's `window.close()`). The caller
//! (`android_main`) then returns: the C++'s `mainClient` `return 0` + the
//! "Finished" line (main.cpp:594-595).

use std::ffi::CString;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use ndk::asset::AssetManager;
use oh_config::Config;
use winit::platform::android::activity::AndroidApp;
use winit::platform::android::EventLoopBuilderExtAndroid;

use crate::lifecycle::{
    create_folder_if_non_existant, load_and_reapply, ConfigSaveGuard, DesktopModeSource,
    ProfileSaveGuard,
};
use crate::windowed::{SoftInput, WindowedClient};

/// issue 025f (D-031) — the Android `SoftInput` controller: the
/// android-activity `show_soft_input` / `hide_soft_input` over the
/// (cloned) `AndroidApp` (the `run` docs: the thread note). The C++'s
/// Android text input came from SFML's own Android layer (D-031) — the
/// port's platform addition: the game's `is_entering_text` transition
/// edge (the `windowed::MenuSlotAdapter::refresh_input` docs) drives
/// these calls; the characters then arrive the way they already do
/// (issue 025d: the `KeyboardInput.text` → entered-text sink →
/// `TextEntered` → `enteredChars` chain).
struct ImeController {
    app: AndroidApp,
}

impl SoftInput for ImeController {
    /// The task's explicit show (D-031's route 1): the android-activity
    /// `show_soft_input(true)` —
    /// `InputMethodManager.showSoftInput(decorView, SHOW_IMPLICIT)`.
    fn show(&self) {
        self.app.show_soft_input(true);
    }
    /// `hide_soft_input(false)` — the android-activity's
    /// `hideSoftInputFromWindow(token, 0)`: an unconditional hide (not
    /// only the implicitly shown IME) — the "keyboard must NOT appear in
    /// non-text states" half of the acceptance.
    fn hide(&self) {
        self.app.hide_soft_input(false);
    }
}

/// The marker file the content extraction writes (issue 25b SG4) — its
/// presence is the "the content is already present" skip check (a
/// crashed mid-extraction never wrote it, so the next start re-extracts
/// over the partial tree; a content UPDATE ships a new APK — clearing the
/// app data, or the marker's absence after a wipe, re-extracts).
const CONTENT_MARKER: &str = ".oh_content_extracted";
/// The packaging-generated file list (tools/package-android.sh staging):
/// one assets-relative path per line — the extraction's enumeration-free
/// source (module docs: why openDir is not used).
const FILELIST: &str = "oh.filelist"; // non-hidden: cargo-apk skips dotfiles (smoke finding)

/// The `_RELEASE`-content base dir for this run (the C++'s CWD).
///
/// SG4 (issue 25b) makes this the EXTRACTED content: the first run copies
/// the APK's `assets/` tree (the `_RELEASE` content — see
/// [`extract_content`]) into the app's internal data dir; later runs
/// detect the marker and skip the copy. The config/save writes land in
/// the same dir (Android has no CWD — module docs).
fn assets_base_dir(app: &AndroidApp) -> Result<PathBuf, String> {
    app.internal_data_path()
        .ok_or_else(|| "AndroidApp::internal_data_path is None (activity destroyed)".to_string())
}

/// Issue 25b SG4: extract the APK's `assets/` tree (the `_RELEASE` content
/// — cargo-apk's `assets` metadata places the `_RELEASE` entries at the
/// APK assets root) into `base_dir` (the app's internal data dir) on
/// first run, then write the marker; SKIP (the Ok no-op) when the marker
/// is already present (the spec's "skip extraction when the content is
/// already present").
///
/// This is the port-specific necessity the C++ Android build avoids
/// (SFML's `AssetManager` wrapper reads the APK on the fly): `oh-assets`
/// threads CWD-relative filesystem paths, so the content must land on
/// disk before the `config.json` load + the `Assets` scan in [`run`].
///
/// The walk: the packaging-generated `oh.filelist` (one assets path per
/// line, written by tools/package-android.sh into the staging assets
/// dir). NO `AAssetManager_openDir` enumeration: the on-device smoke
/// (emulator, API 34) caught `openDir("")` returning an EMPTY root
/// listing (the classic NDK root-enum quirk), which silently extracted 0
/// files. Each listed path is opened (`AAssetManager_open`, which is
/// reliable where openDir is not), read whole (streaming mode) and
/// written ATOMICALLY (`.oh_partial` + rename — a crash mid-extraction
/// never leaves a half-file at the real name; parents are created on
/// demand). Returns `Err` (the `[FATAL ERROR]` in [`run`] — the C++'s
/// `std::terminate` on missing content) when an open/read/write fails
/// OR when the filelist is missing/empty (never write the marker over a
/// zero-file extraction — the original smoke bug also wrote the marker
/// on the empty walk, permanently skipping a future good run).
fn extract_content(app: &AndroidApp, base_dir: &Path) -> Result<(), String> {
    if base_dir.join(CONTENT_MARKER).exists() {
        return Ok(());
    }
    let asset_manager = app.asset_manager();
    let filelist_asset = asset_manager
        .open(cstr(FILELIST)?.as_c_str())
        .ok_or_else(|| format!("the packaging-generated {FILELIST:?} is missing from the APK assets (stale/absent staging — rebuild via tools/package-android.sh)"))?;
    let mut filelist_bytes = Vec::new();
    std::io::Read::read_to_end(&mut { filelist_asset }, &mut filelist_bytes)
        .map_err(|e| format!("asset read({FILELIST:?}): {e}"))?;
    let filelist = std::str::from_utf8(&filelist_bytes)
        .map_err(|e| format!("{FILELIST:?} is not UTF-8: {e}"))?;
    let mut file_count = 0usize;
    for path in filelist.lines().filter(|l| !l.is_empty()) {
        extract_asset_file(&asset_manager, path, base_dir, &mut file_count)?;
    }
    if file_count == 0 {
        return Err(format!(
            "the {FILELIST:?} extraction copied 0 files — refusing to write the content marker (stale/absent APK assets)"
        ));
    }
    std::fs::write(base_dir.join(CONTENT_MARKER), b"ok")
        .map_err(|e| format!("write the content marker {CONTENT_MARKER:?}: {e}"))?;
    eprintln!(
        "[::mainClient] extracted {file_count} content files from the APK assets into {base_dir:?}"
    );
    Ok(())
}

/// One listed file `path` (from the packaging-generated `oh.filelist`, e.g.
/// `Assets/OpenSquare-Bold.ttf` or `Packs/.../level.json`): open, read
/// whole (streaming mode), write ATOMICALLY (module docs: the walk).
fn extract_asset_file(
    asset_manager: &AssetManager,
    path: &str,
    base_dir: &Path,
    file_count: &mut usize,
) -> Result<(), String> {
    let cpath = cstr(path)?;
    let mut asset = asset_manager
        .open(cpath.as_c_str())
        .ok_or_else(|| format!("AAssetManager_open({path:?}) failed"))?;
    let mut bytes = Vec::new();
    std::io::Read::read_to_end(&mut asset, &mut bytes)
        .map_err(|e| format!("asset read({path:?}): {e}"))?;
    let local = base_dir.join(path);
    if let Some(parent) = local.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("create_dir_all({parent:?}): {e}"))?;
    }
    let partial = local.with_extension("oh_partial");
    std::fs::write(&partial, &bytes).map_err(|e| format!("write({partial:?}): {e}"))?;
    std::fs::rename(&partial, &local)
        .map_err(|e| format!("rename({partial:?} -> {local:?}): {e}"))?;
    *file_count += 1;
    Ok(())
}

fn cstr(s: &str) -> Result<CString, String> {
    CString::new(s).map_err(|e| format!("CString({s:?}): {e}"))
}

/// `mainClient`'s CWD step on Android: point the process CWD at the assets
/// base dir so the CWD-relative paths the port threads explicitly
/// (`base_dir`) AND any CWD-relative fallback (`std::env::current_dir()`)
/// agree (the C++'s `ssvu::FileSystem` CWD model). `std::env::set_current_dir`
/// on the app's internal dir (writable, private).
fn chdir_to_base_dir(base_dir: &Path) -> Result<(), String> {
    std::env::set_current_dir(base_dir).map_err(|e| format!("set_current_dir({base_dir:?}): {e}"))
}

/// The C++ `mainClient`'s windowed pieces on Android: the activity's
/// `ANativeWindow`-backed winit window with the C++ `main.cpp:305-332`
/// config setters applied (the desktop `crate::windowed::create_window`
/// equivalent over the android event loop — the window + GL context
/// materialize on the first `resumed`, `oh-render` docs). `event_loop`
/// MOVES into the window (winit 0.30's `EventLoop` is not `Clone` — the
/// android loop is the `AndroidApp`-bound one, owned by the window for the
/// whole loop, exactly as the desktop `Window::create`'s loop is).
#[must_use]
fn create_android_window(
    config: &Config,
    event_loop: winit::event_loop::EventLoop<()>,
) -> oh_render::window::Window {
    let logical_w = config.get_width() as f32;
    let logical_h = config.get_height() as f32;
    let pixel_mult = config.get_pixel_multiplier() as f32;
    // C++ `recreateWindow` (GameWindow.hpp:89-93): the render window is
    // sized `width * pixelMult × height * pixelMult`; the logical members
    // stay logical. The android platform window is screen-sized (the
    // attributes are ignored by the platform — winit FIXME noted in
    // oh-render); the CONFIG logical size drives the logical members +
    // the touch-control half-width, the framebuffer size comes from the
    // native window on GL init (`oh-render` android note).
    let physical_w = (logical_w * pixel_mult) as u32;
    let physical_h = (logical_h * pixel_mult) as u32;
    // C++ `makeWindowTitle()` (main.cpp:139-143) — the activity has no
    // visible title (NoActionBar.Fullscreen theme); the title feeds the
    // window's bookkeeping only.
    let title = "Open Hexagon 2.1.7 - by Vittorio Romeo - https://vittorioromeo.info";

    let mut window = oh_render::window::Window::create_android(
        event_loop,
        physical_w.max(1),
        physical_h.max(1),
        title,
    );
    // Restore the logical size members (the C++ `setSize(logical)`).
    window.set_size(logical_w as u32, logical_h as u32);

    // C++ main.cpp:305-310 (the setters, in the C++ order). Antialiasing
    // (main.cpp:307) records the config value here (29w: the store half +
    // the `mustRecreate` mark — the `Window::set_antialiasing_level`'s
    // docs; the consumption is the documented android no-op, the
    // `recreate_window`'s per-platform reasons: the activity window has
    // no window-style to recreate, the C++ has no android build) and is
    // APPLIED at the first `resumed`'s GL materialization (`ensure_
    // android_gl` → `create_android_parts(el, self.antialiasing_level)` —
    // the C++ `recreateWindow`'s `ContextSettings` sample count, 050); a
    // runtime level change stays stored for the next activity resume.
    // Fullscreen / fps-limit keep their android-platform record halves
    // (the platform window owns the actual surface; vsync IS applied live
    // at the materialization — `parts.set_swap_interval`).
    window.set_pixel_mult(pixel_mult);
    window.set_fullscreen(config.get_fullscreen());
    window.set_antialiasing_level(config.get_antialiasing_level());
    window.set_vsync(config.get_vsync());
    window.set_fps_limited(config.get_limit_fps());
    window.set_max_fps(config.get_max_fps() as f32);

    // C++ main.cpp:312-328 — `Assets/icon.png` via `sf::Image` + `setIcon`
    // (the desktop `create_window`'s icon half; on the android platform
    // window the set is a no-op — the launcher icon is the APK manifest's
    // — but the load + the C++'s load-failure log line are mirrored).
    let icon_path = std::env::current_dir()
        .unwrap_or_else(|_| std::path::PathBuf::from("."))
        .join("Assets/icon.png");
    match oh_render::Image::load_png(&icon_path) {
        Ok(icon) => {
            window.set_icon(&icon);
        }
        Err(_) => {
            // C++ main.cpp:318 (the C++'s `::main` channel).
            println!("[::main] Failed to load icon image");
        }
    }

    window
}

/// The Android `mainClient` (module docs): the shared windowed pipeline
/// over the `AndroidApp` from `android_main`. Called ONCE per
/// `android_main` invocation (the activity's main-loop thread); returns
/// when the window closes (the loop-exit translation).
/// issue 039 STEP 0 — the engine-log sink's liblog write (the NDK
/// `log.h` `ANDROID_LOG_INFO` = 4; the `__android_log_write` non-format
/// form — the format-taking `__android_log_print` is variadic, which
/// stable Rust's FFI cannot call, so the fully-formatted line is
/// written whole). The tag is the spec's `adb logcat -s <tag>` handle.
const ANDROID_LOG_INFO: i32 = 4;
const LOG_TAG: &core::ffi::CStr = c"OpenHexagon";

#[allow(clippy::missing_safety_doc)] // the FFI's own docs are the NDK log.h
extern "C" {
    fn __android_log_write(
        prio: i32,
        tag: *const core::ffi::c_char,
        buf: *const core::ffi::c_char,
    ) -> i32;
}

fn android_log_write(line: &str) {
    // The NUL terminator (the liblog `const char*` contract): the
    // `CString` is the bounded allocation of the day (the engine's
    // lines are event-rate, not frame-rate).
    let buf = match std::ffi::CString::new(line) {
        Ok(b) => b,
        // An interior NUL cannot occur in a formatted `[{channel}] {message}`
        // (both halves are Rust `String`s; the C++'s own lines carry none).
        Err(_) => return,
    };
    unsafe {
        __android_log_write(ANDROID_LOG_INFO, LOG_TAG.as_ptr(), buf.as_ptr());
    }
}

pub fn run(app: AndroidApp) {
    // issue 039 STEP 0 — the engine-log sink (the `oh-web` boot's
    // browser-console sink installed "before `main` runs" — issue 026's
    // seam, `oh_game::log_sink::set_log_sink`): every engine `[channel]`
    // line (`log()` / `host_error` → `emit_log`) reaches logcat through
    // `__android_log_write` (the `ANDROID_LOG_INFO` half; the
    // composition's own `eprintln!` boot-failure lines already reach
    // logcat on the `RustStdoutStderr` tag). Installed FIRST — before
    // the base-dir / extraction / config / assets failures below, so the
    // engine's lines exist from the earliest possible frame.
    oh_game::log_sink::set_log_sink(|channel, message| {
        android_log_write(&format!("[{channel}] {message}"));
    });
    // The sink's install-confirmation line (the STEP 0 acceptance
    // evidence on `adb logcat -s OpenHexagon`: the engine's
    // `[channel]` lines — the C++ `ssvu::lo` parity's `log()` /
    // `host_error` + the event-rate ones, the replay's send / save
    // lines — route through this sink from here on; the
    // composition's own `eprintln!` diagnostics ride the
    // `RustStdoutStderr` tag, a second logcat-visible route).
    oh_game::log_sink::emit_log(
        "android",
        "engine boot: the OpenHexagon logcat sink is installed (issue 039 STEP 0)",
    );

    // C++ main.cpp:650-658 (the chdir-to-argv[0]-parent) + the Android
    // storage model (D-026): the base dir is the app's internal data dir
    // (the extracted `_RELEASE` content from SG4 on).
    let base_dir = match assets_base_dir(&app) {
        Ok(dir) => dir,
        Err(e) => {
            eprintln!("[::mainClient] {e}");
            return;
        }
    };
    // SG4 (issue 25b): the `_RELEASE` content must be ON DISK before the
    // `config.json` load + the `Assets` scan below (the module docs: the
    // port-specific necessity — the C++ reads the APK on the fly). The
    // first run extracts (the marker check skips later runs).
    if let Err(e) = extract_content(&app, &base_dir) {
        // The C++ would `std::terminate()` (missing/unreadable content).
        eprintln!("[FATAL ERROR] Asset extraction failed: {e}");
        return;
    }
    if let Err(e) = chdir_to_base_dir(&base_dir) {
        eprintln!("[::mainClient] {e}");
        return;
    }

    // C++ main.cpp:251 — the Steam manager (the no-SDK default: the
    // C++'s SSVOH_ANDROID no-SDK branch — module docs).
    let steam = crate::steam::steam_manager();

    // C++ main.cpp:275-276 — the folders (in the internal dir — the C++'s
    // CWD-relative `Profiles/` / `Replays/`).
    create_folder_if_non_existant(&base_dir, "Profiles/");
    create_folder_if_non_existant(&base_dir, "Replays/");

    // C++ main.cpp:279-283 — the config load + reapply (the base dir's
    // `config.json` — the extracted `_RELEASE/config.json` on SG4; the
    // C++'s `hg::Config::loadConfig(args)` with the empty android argv).
    // The `reapplyResolution`'s desktop-mode read uses the C++ zero-mode
    // fallback here: winit's monitor API (`primary_monitor`) exists only
    // INSIDE a running event loop, and the activity's loop is consumed by
    // the game's own `run` (a probe `run` here would consume it). Inert
    // in practice — both auto-resolution flags default false, and the
    // platform's `ANativeWindow` is screen-sized independent of the
    // config's resolution values (module docs: the Android storage
    // model) — so the C++'s android `getDesktopMode` read (the screen
    // size) has no port-side effect on the window.
    let mut config = Config::default();
    if let Err(e) = load_and_reapply(
        &mut config,
        &base_dir,
        &[],
        DesktopModeSource::Fixed(oh_render::VideoMode::new(0, 0)),
    ) {
        eprintln!("[::mainClient] config load failed: {e}");
        return;
    }
    let config = Arc::new(RwLock::new(config));

    // C++ main.cpp:285-291 — the config-save scope guard (fires on EVERY
    // exit below, the C++'s scope-guard semantics): the config writes land
    // in the internal dir (module docs: the Android storage model).
    let _config_save = ConfigSaveGuard::new(config.clone(), base_dir.join("config.json"));

    // C++ main.cpp:380 — the assets (`HGAssets{&steamManager, false}` —
    // the android windowed build is non-headless; `levelsOnly` = false;
    // the `&steamManager` half is the 29p1 workshop discovery — the
    // no-SDK default's manager is uninitialized, so the
    // `workshopCache.json` fallback is the live path there).
    let assets = match oh_assets::Assets::new_with_workshop(
        &base_dir,
        false,
        false,
        Some(steam.clone() as std::sync::Arc<dyn oh_assets::WorkshopPacks>),
    ) {
        Ok(a) => Arc::new(a),
        Err(e) => {
            // The C++ would `std::terminate()` (Assets.cpp:323-352).
            eprintln!("[FATAL ERROR] Asset loading failed: {}", e.reason);
            return;
        }
    };

    // C++ main.cpp:382-388 — the pSaveAll scope guard (issue 029c1): the
    // C++'s Android build is the `mainClient` path (windowed, non-headless)
    // with the same guard — the run's profile writes persist on exit.
    // LIFO: constructed after `_config_save` → fires before it (the C++'
    // drop order).
    let _profile_save = ProfileSaveGuard::new(assets.clone());

    // The winit android event loop (the `AndroidApp` from `android_main` —
    // MUST be created before any winit window / GL work; the platform
    // `ANativeWindow` exists only between `InitWindow` and
    // `TerminateWindow`, so the window + GL context materialize on the
    // first `resumed` — `oh-render`'s android GL note). winit 0.30's
    // builder: `EventLoop::<()>::with_user_event()` (+ the `&mut`-taking
    // `with_android_app` before `build`; `build`'s `Err` is the missing-
    // `AndroidApp` panic's Result equivalent — unreachable here, the app
    // was just bound above; the documented return instead of a panic).
    let mut builder: winit::event_loop::EventLoopBuilder<()> =
        winit::event_loop::EventLoop::<()>::with_user_event();
    // issue 025f (D-031) — the soft-input (system IME) controller (the
    // `ImeController` docs): the `AndroidApp` is `Clone` (an
    // `Arc`-backed handle), so the winit loop binds the original while
    // the controller keeps its own copy. The winit loop thread IS the
    // `android_main` thread (the android-activity entry contract — the
    // glue's dedicated main-loop thread), so the android-activity's JNI
    // `show/hideSoftInput` calls fired from the menu's `refresh_input`
    // (the text-entry transition edge) run on-thread.
    let soft_input: Arc<dyn SoftInput> = Arc::new(ImeController { app: app.clone() });
    builder.with_android_app(app);
    let event_loop = match builder.build() {
        Ok(el) => el,
        Err(e) => {
            eprintln!("[::mainClient] winit event loop build failed: {e}");
            return;
        }
    };

    // C++ main.cpp:297-332 (the window + config setters + timer — the
    // timer is installed inside `WindowedClient::build`, the desktop
    // order) + :413-463 (the engine + MenuGame + the four callback links)
    // + :471-537 (the boot dispatch) + :588-592 (`window->run()`).
    let window = create_android_window(&config.read().expect("config poisoned"), event_loop);
    match WindowedClient::build(
        window,
        config.clone(),
        assets,
        &base_dir,
        steam,
        // C++ main.cpp:264-269 — `discordManager`'s Android arm
        // (Discord.cpp: `_core{nullptr}, _initialized{false}` — the
        // `NoDiscordCore` manager is that arm: every method a logged no-op
        // (module docs: the Discord difference)).
        Some(
            Arc::new(oh_discord::DiscordManager::new(oh_discord::NoDiscordCore))
                as Arc<dyn oh_discord::Discord>,
        ),
        // issue 025f (D-031) — the soft-input (system IME) controller
        // (the `ImeController` docs): the android-activity's
        // `show_soft_input` / `hide_soft_input` on the menu's
        // text-entry transitions (the desktop arm's no-op, D-031).
        soft_input,
    ) {
        Ok(client) => {
            // C++ main.cpp:514-515 — "Start game as normal" (the android
            // activity carries no CLI args / drag&drop replay — module
            // docs: the CLI-args difference).
            client.boot(None, None, None);
            // C++ main.cpp:588-592 — `window->run()`: the winit android
            // loop (the shared `windowed::WindowedClient::run` frame body).
            client.run();
            // C++ main.cpp:594-595.
            println!("[::mainClient] Finished");
        }
        Err(e) => {
            // The C++ would `std::terminate()` on the font-load assert.
            eprintln!("[FATAL ERROR] Windowed boot failed: {e}");
        }
    }
}
