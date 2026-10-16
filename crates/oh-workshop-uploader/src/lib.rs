//! Port of `src/OHWorkshopUploader/main.cpp` (739 lines) — the workshop
//! uploader tool.
//!
//! # What the tool is (D-014: verified against source — the contract's
//! "argument parsing / pack packaging / metadata / `--help`" claims do not
//! match this file)
//!
//! The C++ `main` (main.cpp:711-737) takes NO arguments — the tool is a
//! purely interactive menu over stdin/stdout (create a new workshop item /
//! upload a user-supplied directory's contents / set a preview image /
//! exit). There is no `--help`, no pack reading, no zip/staging, no metadata
//! file building: option 1 uploads an EXISTING directory via
//! `SteamUGC()->SetItemContent` (main.cpp:407-426, 526-566), and the only
//! free-form text is the changelog note (main.cpp:553-560). All output text
//! is byte-pinned against the C++ in `tests/uploader.rs` (transcribed +
//! pinned — the C++ cannot be compiled without the Steam SDK; the task
//! allows "compile the C++ or transcribe + pin").
//!
//! # The Steam seam
//!
//! The Steam UGC upload sits behind [`steam::SteamUgc`] so everything else
//! is testable without Steam. This issue ships the trait + [`steam::NoSteam`]
//! (a `TODO(issue-20)` no-SDK impl — `init` fails, so the binary takes the
//! C++'s "Could not initialize Steam API, exiting..." path, main.cpp:716-719:
//! exactly the C++'s behavior on a machine without the Steam runtime); issue
//! 20 wires the real `SteamAPI_*`/`SteamUGC()` calls.
//!
//! # Documented deviations (D-014)
//!
//! - EOF: every C++ reader loops on a permanently-failed `std::cin`, so at
//!   EOF the C++ spins printing its prompt forever. The port prints the
//!   prompt once and aborts the run (exit 1, no `[CLI] Finished` line).
//! - The changelog-note loops (main.cpp:553-557, 597-601) are the same: one
//!   "Error reading changelog note, please try again" line, then abort.
//! - Non-UTF-8 path bytes: the C++ streams the raw bytes; the port's path
//!   echoes (`Read '...'`, the two `Failed to set ... path` lines) are
//!   lossy (U+FFFD substitution) — display-only; the bytes themselves are
//!   kept verbatim in the `Path` (on Linux) and in the validation checks.

mod cli;
mod input;
mod readers;
pub mod steam;

use std::io::{BufRead, Write};

use crate::steam::SteamUgc;

/// Port of the C++ `main` (main.cpp:711-737) + the `steam_helper` ctor/dtor
/// + `cli::make` + `cli::run`.
///
/// Returns the C++ `main` return value (0 / 1).
pub fn run<S: SteamUgc>(steam_sdk: &mut S, input: &mut dyn BufRead, out: &mut dyn Write) -> i32 {
    // The C++ `steam_helper steam;` (main.cpp:714) — the constructor runs
    // `initialize_steamworks` and prints the init lines.
    let mut helper = steam::SteamHelper::new(steam_sdk);
    helper.init(out);

    // main.cpp:716-719.
    if !helper.initialized() {
        let _ = writeln!(out, "[Main] Could not initialize Steam API, exiting...");
        helper.shutdown(out); // the C++ destructor (main.cpp:363-370)
        return 1;
    }

    // main.cpp:721-725: `cli::make(steam)` + `assert(opt_cli.has_value())`.
    // `make` returns `nullopt` only when the API is uninitialized (handled
    // above), so the CLI always constructs here.
    let mut cli = cli::Cli::new(&mut helper, input, out);
    let ok = cli.run(); // main.cpp:729-732

    helper.shutdown(out); // the C++ destructor
    i32::from(!ok)
}
