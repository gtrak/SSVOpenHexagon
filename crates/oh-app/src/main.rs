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

/// W2: the emscripten entry is the `oh-web` crate (W5) — the real web
/// entry point. This binary is a stub on emscripten (the `oh-app` lib
/// compiles for emscripten; the `open-hexagon` bin is desktop-only).
#[cfg(target_os = "emscripten")]
fn main() {
    eprintln!("web entry is the oh-web crate (W5)");
}

#[cfg(not(target_os = "emscripten"))]
fn main() -> std::process::ExitCode {
    let code = oh_app::dispatch::run(&std::env::args().collect::<Vec<_>>());
    // `code & 0xff`: the C++ return values are -1 / 0 / 1; the low byte is
    // the POSIX exit status (C++ `return -1` → 255).
    std::process::ExitCode::from((code & 0xff) as u8)
}
