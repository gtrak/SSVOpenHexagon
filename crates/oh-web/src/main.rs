//! `oh-web` — the emscripten (web) entry crate (issue 26c, W0 skeleton).
//!
//! The `wasm32-unknown-emscripten` **binary** that boots the web platform
//! skeleton: get-or-create the page's `<canvas id="c">`, create a WebGL2
//! context through the `emscripten_webgl_*` C API, clear it a colour, and
//! expose a `#[wasm_bindgen] status()` probe. `oh-render`'s emscripten backend
//! (the real GL backend over `glow`) is W1 and the real engine composition is
//! W2 — this W0 skeleton proves the target, the build recipe, and the
//! wasm-bindgen loader end-to-end.
//!
//! **Build**: `cargo build -p oh-web --target wasm32-unknown-emscripten`
//! (the emsdk env recipe; the target's rustflags live in the root
//! `.cargo/config.toml`'s `[target.wasm32-unknown-emscripten]` table, the
//! verbatim 26c.1/26c.2 recipe). It emits `oh-web.js` (MODULARIZE +
//! EXPORT_ES6 — default-exports the Module factory) + `oh_web.wasm`; the page
//! is `site/index.html` (serve the folder with the two artifacts next to it).
//!
//! On every OTHER target this crate is a no-op binary (the desktop gate's
//! `cargo build --workspace` compiles it; nothing runs).

#[cfg(target_os = "emscripten")]
mod web;

// The 26c.2 trap fix (MANDATORY — D-029 / lib-web-stack.md §26c.2): with
// `-sWASM_BINDGEN` the wasm-bindgen glue registers Rust `main` via
// `addOnInit`, and emscripten's `initRuntime()` runs the onInit callbacks
// **BEFORE** `wasmExports['__wasm_call_ctors']()`. So Rust `main` — and every
// C++ consumer after it (Luau's statics, W1+) — would run BEFORE the C++
// static initializers (a zero-initialized `blockSize` → a silent
// divide-by-zero wasm trap in `newclasspage`) unless we call them explicitly.
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

    // The boot (the W0 web platform skeleton — `web.rs`).
    #[cfg(target_os = "emscripten")]
    web::boot();

    // The desktop-gate no-op (this crate's real target is
    // wasm32-unknown-emscripten; the native build exists only so
    // `cargo build --workspace` stays green).
    #[cfg(not(target_os = "emscripten"))]
    eprintln!(
        "oh-web: the emscripten (web) entry crate (issue 26c W0, D-029) \
         — this no-op main exists only so the desktop gate (cargo build \
         --workspace) compiles the crate; build for wasm32-unknown-emscripten \
         and load site/index.html in a browser to run it."
    );
}
