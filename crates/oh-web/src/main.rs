//! `oh-web` — the emscripten (web) entry crate (issue 26c, DECISIONS.md
//! D-029): the `wasm32-unknown-emscripten` **binary** that boots the REAL
//! engine composition in a browser — `oh_app::client::main_client` (the
//! C++ `mainClient` port) with `headless = false` over the `_RELEASE`
//! tree preloaded into emscripten MEMFS (sub-goal 5B): content (config /
//! assets / packs through `oh_vfs::RealFs`), the `oh-render` emscripten
//! window (the page's `<canvas id="c">` + the WebGL2 context via the
//! `emscripten_webgl_*` C API + the DOM event listeners + the
//! `request_animation_frame` loop — the winit event loop's web
//! replacement), the engine + MenuGame (boot to the menu — the first
//! representative state), and a `#[wasm_bindgen] status()` probe.
//!
//! **Build**: `cargo build -p oh-web --target wasm32-unknown-emscripten`
//! (the emsdk env recipe — `.agents/skills/lib-web-stack.md` §26c.1; the
//! target's rustflags live in the root `.cargo/config.toml`'s
//! `[target.wasm32-unknown-emscripten]` table, the verbatim 26c.1/26c.2
//! recipe + the sub-goal 5B `--preload-file=_RELEASE@/RELEASE` /
//! `-sFORCE_FILESYSTEM=1` content path — build from the repo root so the
//! relative preload path resolves). It emits `oh-web.js` (MODULARIZE +
//! EXPORT_ES6 — default-exports the Module factory; the preloaded
//! `_RELEASE` tree is embedded in it) + `oh_web.wasm` (emcc names the
//! wasm after the object file — underscore; the loader's `locateFile`
//! looks for exactly that name); the page is `site/index.html` (serve the
//! folder with the two artifacts next to it — see that file's header
//! comment).
//!
//! On every OTHER target this crate is a no-op binary (the desktop gate's
//! `cargo build --workspace` compiles it; nothing runs).

#[cfg(target_os = "emscripten")]
mod web;

// The 26c.2 trap fix (MANDATORY — D-029 / lib-web-stack.md §26c.2): with
// `-sWASM_BINDGEN` the wasm-bindgen glue registers Rust `main` via
// `addOnInit`, and emscripten's `initRuntime()` runs the onInit callbacks
// **BEFORE** `wasmExports['__wasm_call_ctors']()`. So Rust `main` — and
// every C++ consumer after it (Luau's `kSizeClassConfig`, sub-goal 2+) —
// would run BEFORE the C++ static initializers (a zero-initialized
// `blockSize` → a silent divide-by-zero wasm trap in `newclasspage`)
// unless we call them explicitly.
#[cfg(target_os = "emscripten")]
extern "C" {
    fn __wasm_call_ctors();
}

fn main() {
    // The FIRST statement of `main()` on the emscripten target (the 26c.2
    // trap fix — the C++ static initializers MUST run before any Rust code
    // touches C++ state; see the `__wasm_call_ctors` docs above).
    #[cfg(target_os = "emscripten")]
    unsafe {
        __wasm_call_ctors()
    };

    // The boot (the real composition — `oh_app::client::main_client` over
    // the preloaded `_RELEASE` MEMFS tree, sub-goal 5B — `web.rs`).
    #[cfg(target_os = "emscripten")]
    web::boot();

    // The desktop-gate no-op (this crate's real target is
    // wasm32-unknown-emscripten; the native build exists only so
    // `cargo build --workspace` stays green).
    #[cfg(not(target_os = "emscripten"))]
    eprintln!(
        "oh-web: the emscripten (web) entry crate (issue 26c sub-goal 4b, D-029) \
         — this no-op main exists only so the desktop gate (cargo build \
         --workspace) compiles the crate; build for wasm32-unknown-emscripten \
         and load site/index.html in a browser to run it."
    );
}
