//! The emscripten (web) boot (issue 26c sub-goal W5, D-029) — called from
//! `main` AFTER `__wasm_call_ctors()` (the 26c.2 trap fix): the REAL
//! composition — `oh_app::client::main_client` (the C++ `mainClient` port)
//! with `headless = false` over the preloaded `_RELEASE` tree:
//!
//! 1. the content — the `_RELEASE` tree preloaded into emscripten MEMFS at
//!    `/RELEASE` (the root `.cargo/config.toml`'s
//!    `--preload-file=_RELEASE@/RELEASE` + `-sFORCE_FILESYSTEM=1`);
//!    `main_client` reads it through `std::fs` (the emscripten libc) —
//!    `config.json`, `Packs/`, `Assets/`, the fonts;
//! 2. the windowed client — the emscripten `oh-render` `Window` over the
//!    page's `<canvas id="c">` (the WebGL2 context via the
//!    `emscripten_webgl_*` C API + the DOM event listeners), the engine +
//!    MenuGame + the four callback links, the boot dispatch (`gotoMenu` —
//!    the menu is the first representative state);
//! 3. the `request_animation_frame` loop (`oh-render`'s web
//!    `Window::run` — the winit event loop's web replacement):
//!    `main_client` RETURNS after scheduling the first frame (D-029: the
//!    browser owns the loop) — the loop's self-owning closure keeps the
//!    engine / menu / the heap-pinned Lua context alive across `main`
//!    returning (the emscripten runtime stays alive after `main` —
//!    `EXIT_RUNTIME=0` default), so the menu keeps driving.
//!
//! Progress is logged to the browser console (the composition's own
//! `println!` / `eprintln!` lines land there through the emscripten
//! stdout/stderr redirect), and `status()` (the `#[wasm_bindgen]` export)
//! reports the boot state + the `main_client` exit code.

use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};

use wasm_bindgen::prelude::wasm_bindgen;
use wasm_bindgen::JsValue;
use web_sys::console;

/// The `_RELEASE` tree's MEMFS mount point (the root
/// `.cargo/config.toml`'s `--preload-file=_RELEASE@/RELEASE`) — passed
/// verbatim as `main_client`'s `base_dir` (the C++'s CWD-relative content
/// root: `config.json`, `Packs/`, `Assets/`, ... live directly under it).
const BASE_DIR: &str = "/RELEASE";

/// The single-init guard (the 26c.1/26c.2 "main ran twice" artifact):
/// `true` once the boot has run — a re-entered `main` skips the second
/// init (one composition / one canvas / one WebGL2 context / one rAF loop).
static BOOTED: AtomicBool = AtomicBool::new(false);
/// The `main_client` return code (`-1` until it returns — the windowed
/// boot path returns `0` after `window->run()` schedules the rAF loop).
static EXIT_CODE: AtomicI32 = AtomicI32::new(-1);

/// The `#[wasm_bindgen]` probe (the 26c.1/26c.2 `status()` shape): the
/// boot state + the composition's exit code — the supervisor's browser
/// check calls it after the module loads.
#[wasm_bindgen]
pub fn status() -> String {
    format!(
        "booted={};exit_code={}",
        BOOTED.load(Ordering::SeqCst),
        EXIT_CODE.load(Ordering::SeqCst)
    )
}

/// The browser console (the web build's log sink — the 26c.1 evidence
/// shape: progress lines as `console.log`).
fn log(msg: &str) {
    console::log_1(&JsValue::from_str(msg));
}

/// PROTOTYPE WORKAROUND (tracker rows 29b6/29c1, QUEUED): seed ONE default
/// profile into the preloaded MEMFS tree (`/RELEASE/Profiles/default.json`)
/// so `main_client`'s local profile scan finds a profile and the boot
/// reaches the main menu instead of the blocking "PROFILE CREATION" screen
/// (master's profile/save system is a known no-op — `Assets::profile_create`
/// never writes, so `_RELEASE/Profiles/` is empty). The JSON keys mirror
/// `load_profile_from_json` (`crates/oh-data/src/load_from_json.rs`):
/// `name` (string), `scores` (map str→f32), `favorites` (array of
/// strings). A write failure is logged, never fatal (boot continues).
/// REMOVE THIS WHEN THE REAL PROFILE SYSTEM LANDS.
fn seed_default_profile() {
    let dir = format!("{BASE_DIR}/Profiles");
    let path = format!("{dir}/default.json");
    match std::fs::create_dir_all(&dir)
        .and_then(|()| std::fs::write(&path, r#"{"name":"DEFAULT","scores":{},"favorites":[]}"#))
    {
        Ok(()) => log(&format!(
            "oh-web: seeded default profile at {path} (29b6/29c1 workaround)"
        )),
        Err(e) => log(&format!(
            "oh-web: WARNING - failed to seed default profile at {path}: {e} (boot continues)"
        )),
    }
}

/// The boot (called from `main` AFTER `__wasm_call_ctors()`).
pub fn boot() {
    // The single-init guard (the 26c.1/26c.2 "main ran twice" artifact):
    // only the FIRST boot does the work — a re-entered `main` no-ops.
    if BOOTED.swap(true, Ordering::SeqCst) {
        log("oh-web: main re-entered - second init skipped (single-init guard)");
        return;
    }
    log("oh-web: boot started (after __wasm_call_ctors)");

    // PROTOTYPE WORKAROUND (tracker rows 29b6/29c1, QUEUED): master's
    // profile/save system is a known no-op (`Assets::profile_create` never
    // writes), so `_RELEASE/Profiles/` is empty and `main_client` would boot
    // into the BLOCKING "PROFILE CREATION" screen instead of the menu. Seed
    // ONE default profile into the preloaded MEMFS tree so the local profile
    // scan (`pack_scan::scan_single_by_ext` over `Profiles/*.json`) finds it
    // and the boot proceeds to the main menu. Remove this block when the
    // real profile system lands.
    seed_default_profile();

    // The real composition (sub-goal W5): the C++ `mainClient` port with
    // `headless = false` — `RealFs` rooted at the preloaded `_RELEASE`
    // tree, config + assets load, the windowed client (the emscripten
    // window over `<canvas id="c">` + engine + MenuGame), the boot
    // dispatch to the menu, and the rAF loop. It RETURNS after scheduling
    // the first frame (the browser owns the loop — D-029); the loop's
    // self-owning closure keeps the engine + menu + the heap-pinned Lua
    // context alive across `main` returning (the emscripten runtime stays
    // alive — `EXIT_RUNTIME=0` default).
    let code = oh_app::client::main_client(false, &[], None, None, Path::new(BASE_DIR));
    EXIT_CODE.store(code, Ordering::SeqCst);
    log(&format!(
        "oh-web: main_client returned {code} (the rAF loop keeps driving the menu)"
    ));
}
