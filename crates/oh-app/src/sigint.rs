//! 29o1 — the C++'s two non-windowed SIGINT handlers (the windowed stop is
//! `windowed.rs::install_sigint` / `sigint_stop`):
//!
//! * the **headless** (no game window) handler — C++ `main.cpp:627-634`:
//!   `ssvu::lo("::main") << "Caught signal '" << s << "' without game window
//!   open, exiting...\n"` + `std::exit(1)`. The C++ installs it in `main`
//!   BEFORE any dispatch (the default) and the windowed handler REPLACES it
//!   (`main.cpp:340-350`). A headless Ctrl-C thus logs + exits **1** (not
//!   the default 130).
//! * the **server** handler — C++ `HexagonServer.cpp:1389-1390`:
//!   `std::printf("Caught signal %d\n", s)` + `globalListener.close()` +
//!   `globalRunning = false`.
//!
//! **Structural deviation (documented):** the C++'s `std::signal` REPLACES
//! the prior handler for SIGINT (one active at a time); `signal-hook` CHAINS
//! (every registered action for a signal fires, in registration order). So a
//! "default headless + windowed/server replacement" would fire the headless
//! `exit(1)` FIRST (registered first) and never the windowed/server handler.
//! The port therefore installs PER MODE (each mode registers only its own
//! handler — `install_headless` in the headless client path + the
//! printLuaDocs path, `install_server` in the server path, `windowed.rs::
//! install_sigint` in the windowed path). The observable behavior (which
//! handler is active per mode) is identical to the C++.
//!
//! **Safe-port note (the C++'s `TODO (P2): UB`):** the C++ handlers ran
//! `ssvu::lo` / `std::printf` (I/O) + `std::exit` / non-atomic state in the
//! signal context (UB). The port keeps the signal context to an async-
//! signal-safe `write(2)` of a PRE-BUILT line (no allocation / no `std`) +
//! either a raw `_exit(1)` (headless — `signal_hook::low_level::exit`) or a
//! single atomic store (server — the `globalRunning = false`; the C++'s
//! `globalListener.close()` is deferred to the `run` loop's exit / the
//! server's drop). The pre-built line's signal number is the registered
//! SIGINT constant (the handler is SIGINT-only).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// C++ `main.cpp:627-634` — the headless handler's log line:
/// `ssvu::lo("::main") << "Caught signal '" << s << "' without game window
/// open, exiting...`. The port's `ssvu::lo("::main")` channel prefix →
/// `[::main]`; the single-quoted signal number is the C++'s `<< s <<`. Pure
/// (the handler's body — testable without a signal).
pub fn headless_line(sig: i32) -> String {
    format!("[::main] Caught signal '{sig}' without game window open, exiting...")
}

/// C++ `HexagonServer.cpp:1389-1390` — the server handler's print:
/// `std::printf("Caught signal %d\n", s)` — to stdout, NO `[::main]`
/// channel prefix (the C++'s `std::printf`, not `ssvu::lo`), no quotes.
/// Pure.
pub fn server_line(sig: i32) -> String {
    format!("Caught signal {sig}")
}

/// The async-signal-safe stdout write (the handlers' log / print half — the
/// ONLY I/O the port runs in the signal context; `std::println!` /
/// `eprintln!` are NOT async-signal-safe — they take the stdout lock, the
/// `TODO (P2): UB` the C++ comment marks). `bytes` is a pre-built line (no
/// formatting in the signal context). The partial-write / error is ignored
/// (the C++'s `ssvu::lo` / `std::printf` do not check either; a signal-
/// context retry loop is not async-signal-safe here).
/// The stdout file descriptor, per target. POSIX `STDOUT_FILENO` — which
/// libc ALSO defines on `windows-gnu` (MinGW's unistd, value 1) but NOT on
/// `windows-msvc` (the MSVC CRT has no POSIX unistd constants; its stdout
/// handle number is 1 — stdio.h's `_IOUT`). A bare `libc::STDOUT_FILENO`
/// therefore fails to compile only under MSVC (E0425) — invisible to the
/// host gate AND to the x86_64-pc-windows-gnu cross-check, which is how it
/// survived to issue 044's windows-msvc CI leg (run 39088375527).
#[cfg(target_env = "msvc")]
const STDOUT_FD: libc::c_int = 1; // the MSVC CRT's `_IOUT` (stdout)
#[cfg(not(target_env = "msvc"))]
const STDOUT_FD: libc::c_int = libc::STDOUT_FILENO;

fn write_stdout(bytes: &[u8]) {
    unsafe {
        // The count argument of `write(2)` is `size_t` (usize) on Unix but
        // `c_uint` (u32) on the Windows targets (libc declares `write`
        // differently per platform — a `usize` count is an E0308 there,
        // found by the Windows cross-build, which the host gate never
        // compiles). The pre-built line is far shorter than either bound.
        // On Windows: clamp THEN cast — never `try_into().unwrap()` (a
        // signal handler must not unwind) and never a partial-write retry
        // loop (it could live-lock in the signal context; a single
        // truncated write mirrors the C++'s single-call behavior).
        #[cfg(windows)]
        let count: libc::c_uint = bytes.len().min(libc::c_uint::MAX as usize) as libc::c_uint;
        #[cfg(not(windows))]
        let count = bytes.len();
        // SAFETY: `write(2)` to `STDOUT_FD` with a valid user buffer is
        // async-signal-safe (POSIX); the buffer outlives the call.
        let _ = libc::write(STDOUT_FD, bytes.as_ptr() as *const libc::c_void, count);
    }
}

/// Install the headless (no-window) SIGINT handler: on SIGINT, log
/// [`headless_line`] + the C++'s `std::exit(1)` (`signal_hook::low_level::
/// exit` — the raw async-signal-safe `_exit(1)`). The handler is armed for
/// the process lifetime (the returned `SigId` is discarded — `signal-hook`
/// keeps the action registered until `unregister` / process exit; the C++
/// `std::signal` likewise persists). Callers: the headless client path
/// (`client.rs`) and the printLuaDocs path (`dispatch.rs` — the C++'s
/// default handler is active there too).
pub fn install_headless() {
    let line = headless_line(signal_hook::consts::signal::SIGINT);
    let mut bytes = line.into_bytes();
    bytes.push(b'\n'); // the C++'s trailing `\n`
    let _ = unsafe {
        // SAFETY: the action is async-signal-safe (a `write(2)` of a
        // pre-built buffer + the raw `_exit(1)` — no allocation, no `std`,
        // no lock); the contract `signal_hook_registry::register` requires.
        signal_hook::low_level::register(signal_hook::consts::signal::SIGINT, move || {
            write_stdout(&bytes);
            signal_hook::low_level::exit(1); // the C++ `std::exit(1)`
        })
    };
}

/// Install the server SIGINT handler: on SIGINT, print [`server_line`] (the
/// C++'s `std::printf`) + set the stop flag (the C++'s `globalRunning =
/// false` — the atomic store is async-signal-safe; the C++'s
/// `globalListener.close()` is deferred to the `run` loop's exit / the
/// server's drop, the safe port). `stop` is the `main_server`'s stop flag
/// the `server.run` loop head observes.
pub fn install_server(stop: Arc<AtomicBool>) {
    let line = server_line(signal_hook::consts::signal::SIGINT);
    let mut bytes = line.into_bytes();
    bytes.push(b'\n'); // the C++ `std::printf`'s trailing `\n`
    let _ = unsafe {
        // SAFETY: the action is async-signal-safe (a `write(2)` of a
        // pre-built buffer + one atomic store — no allocation, no `std`, no
        // lock); the contract `signal_hook_registry::register` requires.
        signal_hook::low_level::register(signal_hook::consts::signal::SIGINT, move || {
            write_stdout(&bytes);
            stop.store(true, Ordering::SeqCst);
        })
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The headless handler's log line (main.cpp:627-634) — the exact C++
    /// `ssvu::lo("::main")` channel-prefixed text for SIGINT (2).
    #[test]
    fn headless_line_is_the_c_log_line() {
        assert_eq!(
            headless_line(2),
            "[::main] Caught signal '2' without game window open, exiting..."
        );
    }

    /// The server handler's print (HexagonServer.cpp:1389-1390) — the exact
    /// C++ `std::printf("Caught signal %d\n", s)` text for SIGINT (2): no
    /// `[::main]` prefix, no quotes.
    #[test]
    fn server_line_is_the_c_printf_line() {
        assert_eq!(server_line(2), "Caught signal 2");
    }
}
