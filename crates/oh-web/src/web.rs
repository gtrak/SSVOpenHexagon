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
//! Progress is logged to the browser console. NOTE (lane 026): the
//! composition's Rust `println!` / `eprintln!` lines do NOT reach the
//! browser console in this emscripten glue (the fd 1/2 writes fall into
//! the default `/dev/stdout -> /dev/tty` null path and are swallowed), so
//! `boot()` installs an `oh_game::log_sink` hook that routes the engine's
//! `[channel]` lines (the C++ `ssvu::lo` parity lines) to
//! `console.log`; `status()` (the `#[wasm_bindgen]` export) reports the
//! boot state + the `main_client` exit code.

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
    #[cfg(target_os = "emscripten")]
    let ticks = oh_app::windowed::web_tick_count();
    #[cfg(not(target_os = "emscripten"))]
    let ticks = 0;
    format!(
        "booted={};exit_code={};ticks={ticks}",
        BOOTED.load(Ordering::SeqCst),
        EXIT_CODE.load(Ordering::SeqCst)
    )
}

/// The browser console (the web build's log sink — the 26c.1 evidence
/// shape: progress lines as `console.log`).
fn log(msg: &str) {
    console::log_1(&JsValue::from_str(msg));
}

/// The boot (called from `main` AFTER `__wasm_call_ctors()`).
pub fn boot() {
    // The engine-log sink (lane 026): the composition's `[channel]` lines
    // (oh-game's `log()` — the C++ `ssvu::lo` parity) go to the browser
    // console. Rust stdio is a black hole in this glue (see the module
    // docs), so without this hook the engine's lines are invisible.
    oh_game::log_sink::set_log_sink(|channel, message| {
        log(&format!("[{channel}] {message}"));
    });

    // The single-init guard (the 26c.1/26c.2 "main ran twice" artifact):
    // only the FIRST boot does the work — a re-entered `main` no-ops.
    if BOOTED.swap(true, Ordering::SeqCst) {
        log("oh-web: main re-entered - second init skipped (single-init guard)");
        return;
    }
    log("oh-web: boot started (after __wasm_call_ctors)");

    // No profile seed: the W6 workaround (seed a default profile into
    // MEMFS) is OBSOLETE — 029c1 landed the real profile system
    // (`Assets::profile_create` writes `Profiles/<name>.json` + reloads
    // the store; `ProfileSaveGuard`'s exit save), and on web `std::fs`
    // is emscripten MEMFS, so the REAL first-run path works in-browser:
    // a fresh user boots to the C++'s own PROFILE CREATION state and a
    // created profile persists for the session (C++ parity — the seeded
    // boot skipped that state, a deviation from the C++ first run).
    // Session-only persistence is the web prototype's storage limit
    // (MEMFS is re-preloaded from `oh_web.data` on every page load);
    // durable storage is out of scope (issue 32's territory).

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
