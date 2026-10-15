//! Issue 014 step (a) — the DISCRIMINATION pair, default-mode half
//! (D-023/D-024): with `produce-lua-metadata` OFF (the shipped C++ build's
//! `if(FALSE)`), the metadata store has ZERO entries, so:
//!
//! * `print_docs` prints exactly the 11 category headers (byte-exact,
//!   nothing else),
//! * `get_all_function_names()` is empty,
//! * `get_docs_for_function` returns `"UNKNOWN FUNCTION"` for every input.
//!
//! This is a dedicated test binary: in default mode nothing records into the
//! process-global store, so the global assertions below are race-free here
//! (no other test in this process can write to it).

use oh_lua::docs::{get_all_function_names, get_docs_for_function, print_docs};
use oh_lua::get_metadata;
use oh_lua::PREFIX_HEADERS;

#[cfg(not(feature = "produce-lua-metadata"))]
#[test]
fn default_mode_prints_eleven_headers_with_zero_entries() {
    {
        let mut meta = get_metadata();
        meta.clear();
    }

    let mut out = Vec::new();
    print_docs(&mut out).unwrap();
    let text = String::from_utf8(out).unwrap();

    // Exactly the 11 header blocks — byte-exact full output.
    let expected: String = PREFIX_HEADERS
        .iter()
        .map(|header| format!("\n{header}\n\n"))
        .collect();
    assert_eq!(
        text, expected,
        "default-mode print_docs must be the 11 category headers and nothing \
         else (the C++ flag-off build records no entries)"
    );

    // Zero `* **` entry lines (re-derived from the byte-exact check above,
    // stated on its own for the spec's discrimination requirement).
    assert!(!text.contains("* **`"), "no entry lines in default mode");
    assert_eq!(
        text.matches("## ").count(),
        11,
        "exactly 11 category headers"
    );

    // Empty name list; every lookup is "UNKNOWN FUNCTION".
    assert!(get_all_function_names().is_empty());
    assert_eq!(get_docs_for_function("u_rndReal"), "UNKNOWN FUNCTION");
    assert_eq!(get_docs_for_function("u_log"), "UNKNOWN FUNCTION");
    assert_eq!(get_docs_for_function("u_clear"), "UNKNOWN FUNCTION");
    assert_eq!(get_docs_for_function(""), "UNKNOWN FUNCTION");
    assert_eq!(
        get_docs_for_function("nonexistent_fn_xyz"),
        "UNKNOWN FUNCTION"
    );
}
