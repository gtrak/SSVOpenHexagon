//! `open-hexagon` — the `oh-app` binary (issue 15 step (b)).
//!
//! C++ `main`'s dispatch (main.cpp:604-686: parseArgs → printLuaDocs →
//! server → client); see `oh_app::dispatch::run` for the ported pieces and
//! the documented deviations (no libsodium — issue 16; no SIGINT handler /
//! no `log.txt` save / no chdir-to-executable — step (c) and the base-dir
//! docs in `oh_app::client`).
//!
//! The exit code is the C++ `main` return value reduced to its low byte —
//! POSIX status semantics, so the C++ `return -1` (argc < 1) becomes 255
//! exactly as in C++. Returning from `main` (rather than
//! `std::process::exit`) keeps the runtime's stdout flush on the way out,
//! so the headless branch's `Player died.\nFinal time: ...` line is never
//! lost to buffering when stdout is a pipe.
//!
//! Emscripten (sub-goal 4A): the binary still COMPILES for
//! `wasm32-unknown-emscripten`, but its entry is a placeholder — the real
//! web client (the windowed composition over the emscripten `oh-render`
//! backend + the content/Lua paths) lands in the `oh-web` crate
//! (sub-goal 4B). This stub only proves `oh-app` builds for the target.
#[cfg(not(target_os = "emscripten"))]
fn main() -> std::process::ExitCode {
    let code = oh_app::dispatch::run(&std::env::args().collect::<Vec<_>>());
    // `code & 0xff`: the C++ return values are -1 / 0 / 1; the low byte is
    // the POSIX exit status (C++ `return -1` → 255).
    std::process::ExitCode::from((code & 0xff) as u8)
}

#[cfg(target_os = "emscripten")]
fn main() {
    // Sub-goal 4A: `oh-app` compiles for `wasm32-unknown-emscripten` (the
    // windowed / server / egui-console paths are gated off, `Cargo.toml`
    // + `lib.rs`). The real web client — the windowed composition over the
    // emscripten `oh-render` backend plus the content + Lua paths — lands
    // in the `oh-web` crate (sub-goal 4B).
    println!("oh-web entry lands in sub-goal 4B (the `oh-web` crate)");
}
