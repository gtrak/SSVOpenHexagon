//! C++ `main` (main.cpp:604-686) — the mode dispatch, plus the not-yet-
//! ported server stub. Issue 15 step (b).

use std::path::PathBuf;

use crate::args::parse_args;
use crate::client::main_client;
use crate::print_lua_docs::main_print_lua_docs;

/// C++ `mainServer()` (main.cpp:197-236) — NOT ported yet (issue 18 owns
/// the `HexagonServer` port: the C++ resolves the server IP, binds
/// `server_port` / `server_control_port`, and serves the level whitelist).
///
/// Documented deviation (recorded until issue 18 lands): the C++ RUNS A
/// SERVER here and returns 0 when it is done; the port must not silently
/// succeed in its place, so it prints an explicit not-ported message to
/// stderr and exits 1.
pub fn main_server() -> i32 {
    eprintln!("Server mode is not yet ported (issue 18)");
    1
}

/// C++ `main(argc, argv)` (main.cpp:604-686) — the ported pieces, in the
/// C++ order:
///
/// 1. `argc < 1` fatal (`:605-609`) — Rust `std::env::args()` always
///    yields `argv[0]`, so this is unreachable but ported for verbatimness
///    (the C++ `return -1` → POSIX exit status 255);
/// 2. (libsodium init `:615-621` — issue 16, not ported yet: no sodium use
///    exists in the step-(b) tree);
/// 3. (the SIGINT handler `:627-637` — step (c), with the window);
/// 4. (the `log.txt` save scope guard `:643-650` — the port's logging is
///    stdout-`[channel]` lines, there is no log file to save; documented);
/// 5. (the chdir to `argv[0]`'s parent `:656-658` — see `main_client`'s
///    `base_dir` docs: NOT ported, the base dir is the process CWD);
/// 6. `parseArgs` (`:660-661`);
/// 7. the dispatch order (`:666-686`): `printLuaDocs` → `server` →
///    `mainClient` (the C++ `SSVOH_ASSERT(!printLuaDocs)` /
///    `SSVOH_ASSERT(!server)` at `:684-685` hold by construction here).
///
/// Returns the C++ `main`'s return value (`-1`, `0`, or `1`); the binary
/// entrypoint converts it to the POSIX exit status (low byte, so `-1` →
/// 255 exactly as the C++).
pub fn run(argv: &[String]) -> i32 {
    if argv.is_empty() {
        // C++ main.cpp:605-609 (unreachable in Rust — `std::env::args()`
        // always yields `argv[0]`; kept for the verbatim port).
        eprintln!("Fatal error: no executable specified");
        return -1;
    }

    let parsed = parse_args(argv);

    // Print Lua docs mode (C++ main.cpp:666-669).
    if parsed.print_lua_docs {
        let base_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        return main_print_lua_docs(&base_dir);
    }

    // Server mode (C++ main.cpp:674-677).
    if parsed.server {
        return main_server();
    }

    // Client mode (C++ main.cpp:682-686).
    let base_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    main_client(
        parsed.headless,
        &parsed.args,
        parsed.cli_level_name.as_deref(),
        parsed.cli_level_pack.as_deref(),
        &base_dir,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// C++ `main`'s `argc < 1` branch (main.cpp:605-609): the C++
    /// `return -1` (POSIX exit status 255 via the binary's low-byte
    /// conversion). Unreachable in Rust, but the ported check pins it.
    #[test]
    fn empty_argv_is_the_c_fatal_error() {
        assert_eq!(run(&[]), -1);
    }

    /// The dispatch order (main.cpp:666-686): `-server` short-circuits
    /// BEFORE the client (and before any filesystem work) with the
    /// documented not-ported exit.
    #[test]
    fn server_mode_is_the_documented_not_ported_stub() {
        assert_eq!(run(&["open-hexagon".to_string(), "-server".to_string()]), 1);
    }
}
