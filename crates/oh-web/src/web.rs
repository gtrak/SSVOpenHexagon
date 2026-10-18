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
//!    `EXIT_RUNTIME=0` default), so the menu keeps driving;
//! 4. durable user data (issue 048 / F34 / D-039) — the WRITEABLE half of
//!    the base dir lands in IndexedDB, not MEMFS: the page's glue
//!    (`site/index.html`'s `ohWebPersistBoot`) mounts emscripten IDBFS at
//!    `/userdata` (the root `.cargo/config.toml`'s `-lidbfs.js` +
//!    `-sEXPORTED_RUNTIME_METHODS=FS,IDBFS`), restores the persisted user
//!    data (`FS.syncfs(true, ...)` — the load direction) and re-points the
//!    three writable paths (`config.json`, `Profiles/`, `Replays/`) at the
//!    mount with one `FS.symlink` each, BEFORE `main_client` runs (see
//!    `boot`'s gate). The read-only `_RELEASE` content stays in MEMFS —
//!    IDBFS is never mounted over `/RELEASE` (a persisted copy would serve
//!    a STALE `_RELEASE` after the next deployment).
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

use js_sys::{Function, Reflect};
use wasm_bindgen::closure::Closure;
use wasm_bindgen::prelude::wasm_bindgen;
use wasm_bindgen::{JsCast, JsValue};
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
/// issue 048 (F34 / D-039) — the one-shot latch for the persistence gate's
/// callback: the glue invokes the callback exactly once (on the `FS.syncfs`
/// load's success OR failure), but the callback is a `dyn Fn` (wasm-bindgen
/// has no `FnOnce` JS closures — JS cannot enforce single invocation), so
/// the once-semantics live here: the composition boots AT MOST ONCE, even
/// if the callback were ever invoked twice.
static BOOT_GATE_FIRED: AtomicBool = AtomicBool::new(false);

/// The `#[wasm_bindgen]` probe (the 26c.1/26c.2 `status()` shape): the
/// boot state + the composition's exit code + the rAF tick count (lane
/// 026) + the current representative state (issue 32 — the rAF loop's
/// per-frame store: the menu's C++ `States` name or `"game"`; the
/// browser check's step-by-step observable: boot → name entry → menu
/// → level start). The supervisor's browser check calls it after the
/// module loads.
#[wasm_bindgen]
pub fn status() -> String {
    #[cfg(target_os = "emscripten")]
    let ticks = oh_app::windowed::web_tick_count();
    #[cfg(not(target_os = "emscripten"))]
    let ticks = 0;
    #[cfg(target_os = "emscripten")]
    let state = oh_app::windowed::web_state();
    #[cfg(not(target_os = "emscripten"))]
    let state = "";
    format!(
        "booted={};exit_code={};ticks={ticks};state={state}",
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

    // issue 048 (F34 / D-039) — the durable-user-data gate (D-036's
    // "MEMFS preload, NOT a VFS refactor" decision, with the WRITABLE
    // paths re-pointed at IndexedDB): IndexedDB is ASYNCHRONOUS, so the
    // persisted user data (the `/userdata` IDBFS mount — the composition's
    // `config.json` / `Profiles/` / `Replays/` through the page's
    // symlinks) must be in place BEFORE the composition reads them. The
    // page's glue (`site/index.html`'s `ohWebPersistBoot`) mounts the
    // IDBFS, restores `/userdata` (the `FS.syncfs` LOAD direction), seeds
    // the writable-path symlinks, and invokes this callback exactly once
    // when the load settles. A load FAILURE degrades to the session-only
    // MEMFS (the C++'s "no config" / fresh-`Profiles/` state — the game
    // still boots; the C++ has no durable storage either, this is a
    // platform addition, not a transcription). The composition therefore
    // runs FROM THE CALLBACK, not from `main` (which returns here; the
    // runtime stays alive — `EXIT_RUNTIME=0` — until the callback fires,
    // milliseconds later). A missing glue (a non-issue-048 page loading
    // this wasm) boots synchronously, exactly as pre-048.
    let done = Closure::new(run_boot_gate_once);
    // `done` is `'static` (the log sink + the composition are process-global),
    // so the ownership transfers to the JS GC (`into_js_value` inside
    // `persist_boot` — the glue invokes the callback exactly once, AFTER
    // `main` has returned, when the async IndexedDB load settles).
    match persist_boot(done) {
        Ok(()) => log(
            "oh-web: awaiting the IndexedDB load (the page's FS.syncfs) before the composition boots",
        ),
        Err(why) => {
            log(&format!(
                "oh-web: the persist-boot glue is unavailable ({why}) - booting on session-only MEMFS"
            ));
            boot_composition();
        }
    }
}

/// The composition itself (pre-048 this was the tail of `boot`; now it is
/// invoked from the persistence gate's callback): the C++ `mainClient`
/// port with `headless = false` — `RealFs` rooted at the preloaded
/// `_RELEASE` tree (the WRITABLE paths — `config.json`, `Profiles/`,
/// `Replays/` — re-pointed at the `/userdata` IDBFS mount by the gate),
/// config + assets load, the windowed client (the emscripten window over
/// `<canvas id="c">` + engine + MenuGame), the boot dispatch to the menu,
/// and the rAF loop. It RETURNS after scheduling the first frame (the
/// browser owns the loop — D-029); the loop's self-owning closure keeps
/// the engine + menu + the heap-pinned Lua context alive across `main`
/// returning.
fn boot_composition() {
    let code = oh_app::client::main_client(false, &[], None, None, Path::new(BASE_DIR));
    EXIT_CODE.store(code, Ordering::SeqCst);
    log(&format!(
        "oh-web: main_client returned {code} (the rAF loop keeps driving the menu)"
    ));
}

/// The persistence gate's callback body (the one-shot latch's consumer):
/// the composition boots AT MOST ONCE (see `BOOT_GATE_FIRED`).
fn run_boot_gate_once() {
    if BOOT_GATE_FIRED.swap(true, Ordering::SeqCst) {
        log("oh-web: the persistence gate fired twice - second invocation ignored (one composition per page)");
        return;
    }
    log("oh-web: persisted user data ready (IDBFS /userdata) - booting the composition");
    boot_composition();
}

/// issue 048 (F34 / D-039) — the page glue's entry (`site/index.html`'s
/// `ohWebPersistBoot` — defined in the page's module script BEFORE the
/// module factory is called, so it exists by the time `main` runs):
/// mounts the IDBFS, runs the `FS.syncfs` load, seeds the symlinks, then
/// invokes the callback exactly once (on load success OR failure). `Err`
/// = the glue is absent / not a function / threw at setup (a
/// non-issue-048 page) — the caller boots without the gate (the pre-048
/// behaviour).
fn persist_boot(done: Closure<dyn Fn()>) -> Result<(), String> {
    let global = js_sys::global();
    let glue = Reflect::get(&global, &JsValue::from_str("ohWebPersistBoot"))
        .map_err(|e| format!("Reflect::get(ohWebPersistBoot): {e:?}"))?
        .dyn_into::<Function>()
        .map_err(|_| "ohWebPersistBoot is not a function".to_string())?;
    // The callback: the glue's `done()` invocation fires `boot_composition`
    // (the `into_js_value` transfer below is what keeps it alive across
    // the async load — see `into_js_value`'s docs).
    let js_done = done.into_js_value();
    glue.call1(&JsValue::UNDEFINED, &js_done)
        .map(|_| ())
        .map_err(|e| format!("ohWebPersistBoot threw: {e:?}"))
}

/// issue 048 (F34 / D-039) — the web session's exit-save (the C++
/// `mainClient`'s exit guards — the `pSaveAll` + `saveConfig` pair —
/// re-fired at the session's REAL end). On web the C++'s guards drop at
/// composition (D-029's rAF loop returns — see oh-app `windowed`'s
/// `web_session_save` docs), so without this an in-session config change
/// (the menu's options) would never reach `config.json`. The page's
/// `pagehide` / `visibilitychange` glue (site/index.html) calls this
/// through the wasm-bindgen export, then flushes the IDBFS mount to
/// IndexedDB.
#[wasm_bindgen]
pub fn save_session() {
    oh_app::windowed::web_session_save();
}
