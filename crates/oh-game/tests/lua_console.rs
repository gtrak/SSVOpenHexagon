//! Issue 014 step (a): the headless Lua console — engine-side tests over the
//! REAL `oh-lua` composition (D-013 two-phase build, `tests/common`).
//!
//! # The print-docs pair (D-023/D-024)
//!
//! `mainPrintLuaDocs` (`main.cpp:167-189`) is mirrored over a `Vec<u8>`
//! writer: `initLuaAndPrintDocs` = `init_lua` (the 285 registrations, whose
//! `LuaMetadataProxy` drops record the entries in feature-on builds) +
//! `print_docs`.
//!
//! * **Default mode** (the C++ flag OFF — the shipped build): the store
//!   stays empty, so the output is the 11 category headers and nothing
//!   else, and `?u_rndReal` logs the C++ `"UNKNOWN FUNCTION"` fallback.
//! * **Feature-on mode** (`cargo test -p oh-game --features
//!   produce-lua-metadata`): the entries are recorded, so the output carries
//!   `* **\`...\`**` entry lines and the lookup resolves.
//!
//! (The store is process-global; within each binary every composition sees
//! the same mode, so the assertions hold regardless of test order.)

mod common;

use std::sync::Arc;

use common::{compose, release_dir};
use oh_game::lua_console::PREFIX_HEADERS;
use oh_game::seams::ScriptHost;

fn load_release() -> Arc<oh_assets::Assets> {
    Arc::new(
        oh_assets::Assets::new(Arc::new(oh_vfs::RealFs::new(release_dir())), true, false)
            .expect("_RELEASE loads headless"),
    )
}

/// Walks the output over the 11 category headers, in order (panics on the
/// first missing/out-of-order one).
fn assert_headers_in_order(text: &str) {
    let mut position = 0;
    for header in PREFIX_HEADERS {
        let rest = text
            .get(position..)
            .unwrap_or_else(|| panic!("output shorter than the headers: {text}"));
        let rel = rest
            .find(header)
            .unwrap_or_else(|| panic!("header missing or out of order: {header}"));
        position += rel + header.len();
    }
}

#[cfg(not(feature = "produce-lua-metadata"))]
#[test]
fn print_lua_docs_default_mode_is_headers_only() {
    let assets = load_release();
    let c = compose(assets, release_dir(), false);

    let mut out = Vec::new();
    c.game
        .init_lua_and_print_docs_to(&mut out)
        .expect("print_docs");
    let text = String::from_utf8(out).expect("utf8");

    // The 11 category headers, in order...
    assert_headers_in_order(&text);
    // ...and ZERO entry lines (the flag-OFF build records nothing — D-023).
    assert!(
        !text.contains("* **`"),
        "default mode must print no entry lines:\n{text}"
    );

    // The `?fn` command is inert the same way (C++ empty-store fallback).
    c.game.post_update_imgui_lua_console_command("?u_rndReal");
    let log = c.game.state_handle().lock().ilc_cmd_log.clone();
    assert_eq!(
        log,
        vec![
            "# ?u_rndReal\n".to_string(),
            "[?]: UNKNOWN FUNCTION".to_string()
        ]
    );
}

#[cfg(feature = "produce-lua-metadata")]
#[test]
fn print_lua_docs_feature_on_mode_has_entries_and_resolves_lookups() {
    let assets = load_release();
    let c = compose(assets, release_dir(), false);

    let mut out = Vec::new();
    c.game
        .init_lua_and_print_docs_to(&mut out)
        .expect("print_docs");
    let text = String::from_utf8(out).expect("utf8");

    // The 11 category headers, in order...
    assert_headers_in_order(&text);
    // ...and entry lines (the flag-ON build: `init_lua`'s 285 registrations
    // recorded their entries).
    assert!(
        text.contains("* **`"),
        "feature-on mode must print entry lines"
    );

    // A `u_rndReal`-style lookup RESOLVES (not the fallback).
    c.game.post_update_imgui_lua_console_command("?u_rndReal");
    let log = c.game.state_handle().lock().ilc_cmd_log.clone();
    let last = log.last().expect("the ?fn log line");
    assert!(last.starts_with("[?]: "), "unexpected ?fn line: {last:?}");
    assert!(
        !last.contains("UNKNOWN FUNCTION"),
        "the lookup must resolve in feature-on mode: {last:?}"
    );
    // The found-entry format ends with the C++ `"\n\n"`.
    assert!(
        last.ends_with("\n\n"),
        "resolved docs keep their trailing newlines: {last:?}"
    );
}

/// The tracked-results loop over the REAL `u_impl_addTrackedResult` lambda
/// (both feature modes — the tracking machinery is independent of the
/// metadata store): the C++ `HGUpdate.cpp:1584-1621` flow — clear the
/// results, execute each snippet (the lambda pushes into
/// `ilcLuaTrackedResults`), on the first error log the exact line, erase the
/// pair, and stop.
#[test]
fn tracked_results_gathered_via_the_real_lambda() {
    let assets = load_release();
    let c = compose(assets, release_dir(), false);

    // The C++ lambda lives in `HexagonGame::initLua` (HGScripting.cpp:57);
    // without registering the API, `u_impl_addTrackedResult` is nil, the
    // snippet errors, and the pair is erased.
    c.host.init_lua();

    // Define the tracked global through the console command's fallback
    // (`u_log(myTracked = "42")` is a syntax error → the inner catch retries
    // the bare `myTracked = "42"\n`, which executes).
    c.game
        .post_update_imgui_lua_console_command("myTracked = \"42\"");

    // Track it (C++ Track-`InputText` Enter: LR-trim the buffer, record the
    // snippet + name, clear the buffer).
    c.game.ilc_set_track_buffer("  myTracked ");
    c.game.ilc_track();
    assert_eq!(
        c.game.ilc_lua_tracked(),
        vec!["u_impl_addTrackedResult(myTracked)\n".to_string()]
    );
    assert_eq!(
        c.game.ilc_lua_tracked_names(),
        vec!["myTracked".to_string()]
    );
    assert_eq!(c.game.ilc_cmd_buffer(), "");

    // The loop: the real lambda pushes the value into
    // `ilcLuaTrackedResults` (the cleared vec is re-gathered).
    c.game.ilc_update_tracked_results();
    {
        let shared = c.game.state_handle().lock();
        assert_eq!(shared.ilc_lua_tracked_results, vec!["42".to_string()]);
        assert!(
            shared.ilc_cmd_log.iter().all(|l| !l.contains("[error]")),
            "no error expected: {:?}",
            shared.ilc_cmd_log
        );
    }

    // Track a missing global: the snippet errors (nil → String), the C++
    // error line is logged (the snippet text carries its own trailing
    // newline → the line ends `\n\n`), the pair is erased, the loop stops.
    c.game.ilc_set_track_buffer("noSuchGlobal");
    c.game.ilc_track();
    c.game.ilc_update_tracked_results();
    {
        let shared = c.game.state_handle().lock();
        // Only the surviving snippet's result remains (the cleared vec was
        // re-gathered by snippet 1; snippet 2 never pushed).
        assert_eq!(shared.ilc_lua_tracked_results, vec!["42".to_string()]);
        let error_lines: Vec<&String> = shared
            .ilc_cmd_log
            .iter()
            .filter(|l| l.starts_with("[error]: error '"))
            .collect();
        assert_eq!(error_lines.len(), 1, "log: {:?}", shared.ilc_cmd_log);
        assert!(
            error_lines[0].ends_with(" while tracking u_impl_addTrackedResult(noSuchGlobal)\n\n"),
            "C++ error-line shape: {:?}",
            error_lines[0]
        );
        // The failed pair is erased; the good pair survives.
        assert_eq!(
            c.game.ilc_lua_tracked(),
            vec!["u_impl_addTrackedResult(myTracked)\n".to_string()]
        );
        assert_eq!(
            c.game.ilc_lua_tracked_names(),
            vec!["myTracked".to_string()]
        );
    }

    // "Untrack All" (C++ button) clears both lists.
    c.game.ilc_untrack_all();
    assert!(c.game.ilc_lua_tracked().is_empty());
    assert!(c.game.ilc_lua_tracked_names().is_empty());
}
