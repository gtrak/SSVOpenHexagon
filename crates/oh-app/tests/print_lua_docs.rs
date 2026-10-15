//! Issue 15 step (b): the `-printLuaDocs` mode (C++ `mainPrintLuaDocs`,
//! main.cpp:167-189) over the REAL `_RELEASE`.
//!
//! The C++ writes to `std::cout`; the port takes a writer
//! (`oh_app::print_lua_docs::print_lua_docs`), so this test drives the
//! same code the binary entrypoint uses and asserts the exact bytes.
//!
//! # Build-mode adaptation (D-023)
//!
//! The docs store's state decides the tail of the output: in the DEFAULT
//! build nothing is ever recorded (11 headers, zero entries), while the
//! feature-ON build records `init_lua`'s registrations. This test's own
//! `print_lua_docs` call runs `init_lua`, so the store it observes is
//! exactly the one its output was produced from: empty ⇒ the byte-exact
//! default-mode text; populated ⇒ the entry lines must be present.
//! (The store itself is pinned mode-for-mode by `oh-lua`'s
//! `docs.rs` test pair — which is why this adaptation cannot mask a
//! store regression.)

mod common;

use common::release_dir;

/// Binary-level smoke (the in-process test above covers the core bytes):
/// the `-printLuaDocs` dispatch in the real `open-hexagon` binary — exit
/// 0 and the Finished line. Runs against the read-only `_RELEASE` (this
/// mode writes nothing to the data root: no folders, no config save).
#[test]
fn print_lua_docs_binary_exits_0_and_prints_the_finished_line() {
    let exe = std::env::var("CARGO_BIN_EXE_open-hexagon").expect("open-hexagon is built");
    let out = std::process::Command::new(exe)
        .current_dir(release_dir())
        .arg("-printLuaDocs")
        .output()
        .expect("spawn open-hexagon");
    assert_eq!(
        out.status.code(),
        Some(0),
        "C++ mainPrintLuaDocs returns 0; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.ends_with("[::mainPrintLuaDocs] Finished\n"),
        "the C++ Finished line closes the binary output; stdout tail: {}",
        &stdout[stdout.len().saturating_sub(80)..]
    );
}

#[test]
fn print_lua_docs_output_matches_the_build_mode() {
    let mut out = Vec::new();
    let code = oh_app::print_lua_docs::print_lua_docs(&release_dir(), &mut out).expect("print");
    assert_eq!(code, 0, "C++ mainPrintLuaDocs returns 0");

    let stdout = String::from_utf8(out).expect("utf-8");

    // Always (both modes): the 11 category headers (the C++ `prefixHeaders`
    // array, verbatim prefixes) in order, five leading newlines, and the
    // Finished log line as the very last line.
    assert!(
        stdout.starts_with("\n\n\n\n\n"),
        "C++ `std::cout << \"\\n\\n\\n\\n\\n\"` first"
    );
    let mut position = 5;
    for header in [
        "## Utility Functions (u_)",
        "## Audio Functions (a_)",
        "## Main Timeline Functions (t_)",
        "## Event Timeline Functions (e_)",
        "## Level Functions (l_)",
        "## Style Functions (s_)",
        "## Wall Functions (w_)",
        "## Custom Wall Functions (cw_)",
        "## Custom Timeline Functions (ct_)",
        "## Shader Functions (shdr_)",
        "## Miscellaneous Functions",
    ] {
        let rel = stdout[position..]
            .find(header)
            .unwrap_or_else(|| panic!("header missing or out of order: {header}"));
        position += rel + header.len();
    }
    assert!(
        stdout.ends_with("[::mainPrintLuaDocs] Finished\n"),
        "the C++ Finished log line closes the output"
    );

    // The D-023 store state decides the rest: the store is empty ONLY in
    // the default build (nothing records there); this test's own
    // `init_lua` call populated it in the feature-ON build.
    let populated = !oh_lua::docs::get_all_function_names().is_empty();
    if populated {
        // Feature-ON build (the C++ SSVOH_PRODUCE_LUA_METADATA build): the
        // 285 registered API globals carry their entry lines.
        assert!(
            stdout.contains("* **`"),
            "feature-on: the entry lines are present"
        );
        assert!(
            stdout.contains("u_log") && stdout.contains("l_setSides"),
            "feature-on: the registered API globals are documented"
        );
    } else {
        // Default build (the shipped C++ `if(FALSE)` build): byte-exact —
        // five newlines + the 11 headers (the empty store's `print_docs`
        // is the reference) + five newlines + the Finished line, and ZERO
        // entry lines.
        let mut headers_only = Vec::new();
        oh_lua::docs::print_docs(&mut headers_only).expect("headers");
        let headers_only = String::from_utf8(headers_only).expect("utf-8");
        let expected = format!("\n\n\n\n\n{headers_only}\n\n\n\n\n[::mainPrintLuaDocs] Finished\n");
        assert_eq!(
            stdout, expected,
            "default mode (D-023): 11 headers, zero entries — byte-exact"
        );
        assert!(
            !stdout.contains("* **`"),
            "default mode (D-023): zero entry lines"
        );
    }
}
