//! Port of `src/SSVOpenHexagon/Core/LuaScripting.cpp:1613-1688` — the docs
//! readers over the process-global [`LuaMetadata`]: `printDocs` /
//! `getAllFunctionNames` / `getDocsForFunction` (the Lua console's `?fn`
//! command and tab-completion read exactly these, issue 014 step (a)).
//!
//! C++ `printDocs` writes to `std::cout`; the port takes a writer
//! (testability) with [`print_docs_to_stdout`] as the `std::cout` form (the
//! `-printLuaDocs` CLI entry lands with issue 15).
//!
//! In the default build (the C++ metadata flag OFF, D-023/D-024 — the cargo
//! feature `produce-lua-metadata`), the store has zero entries:
//! [`print_docs`] emits the 11 category headers and nothing else,
//! [`get_all_function_names`] is empty, and [`get_docs_for_function`]
//! returns `"UNKNOWN FUNCTION"` for every input.

use std::io::Write;

use crate::metadata::{get_metadata, PREFIX_HEADERS};

/// C++ `getAllFunctionNames` (LuaScripting.cpp:1632-1652): every registered
/// function name, category order 0..NUM_CATEGORIES, entry order within a
/// category.
///
/// (The C++ caches the result in a function-local `static` built on the
/// first call — which happens after `initLua()` in every real flow, so the
/// cache and a recompute agree there. The port recomputes per call:
/// identical while the store is stable, and it keeps the process-global
/// store testable without a stale cache.)
#[must_use]
pub fn get_all_function_names() -> Vec<String> {
    let lm = get_metadata();
    let mut result = Vec::new();
    for i in 0..lm.num_categories() {
        lm.for_fn_entries(i, |_ret, name, _args, _docs| {
            result.push(name.to_string());
        });
    }
    result
}

/// C++ `getDocsForFunction` (LuaScripting.cpp:1654-1688): the first matching
/// entry (category order, then entry order) formatted
/// `"{ret} {name}({args}):\n{docs}\n\n"`, else `"UNKNOWN FUNCTION"`.
#[must_use]
pub fn get_docs_for_function(fn_name: &str) -> String {
    let lm = get_metadata();
    let mut found = false;
    let mut oss = String::new();
    for i in 0..lm.num_categories() {
        lm.for_fn_entries(i, |ret, name, args, docs| {
            if found || name != fn_name {
                return; // basically a `continue`
            }
            found = true;
            oss.push_str(ret);
            oss.push(' ');
            oss.push_str(name);
            oss.push('(');
            oss.push_str(args);
            oss.push_str("):\n");
            oss.push_str(docs);
            oss.push_str("\n\n");
        });
    }
    if !found {
        return "UNKNOWN FUNCTION".to_string();
    }
    oss
}

/// C++ `printDocs` (LuaScripting.cpp:1613-1630) to a writer: per category
/// `"\n" + prefixHeaders[i] + "\n\n"`, then per entry `"* **`" + ret + " " +
/// name + "(" + args + ")`**: " + docs + `"\n\n"`.
pub fn print_docs(out: &mut impl Write) -> std::io::Result<()> {
    let lm = get_metadata();
    let mut error: Option<std::io::Error> = None;
    for i in 0..lm.num_categories() {
        if let Err(e) = write!(out, "\n{}\n\n", PREFIX_HEADERS[i]) {
            error = Some(e);
            break;
        }
        lm.for_fn_entries(i, |ret, name, args, docs| {
            if error.is_some() {
                return;
            }
            if let Err(e) = write!(out, "* **`{ret} {name}({args})`**: {docs}\n\n") {
                error = Some(e);
            }
        });
    }
    match error {
        Some(e) => Err(e),
        None => Ok(()),
    }
}

/// The C++ `std::cout` form (the `-printLuaDocs` entry, issue 15).
pub fn print_docs_to_stdout() {
    let mut stdout = std::io::stdout().lock();
    if let Err(e) = print_docs(&mut stdout) {
        // The C++ has no error path here (`std::cout` failbit); surface it
        // on stderr rather than failing silently.
        eprintln!("printDocs failed: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata::LuaMetadataProxy;

    /// D-023 default mode (feature off): nothing is ever recorded, so the
    /// docs are inert. This test only READS the store; the default-mode lib
    /// test binary has no other store writer, so the global assertions hold
    /// (the dedicated `tests/docs_default.rs` binary pins the byte-exact
    /// full output).
    #[cfg(not(feature = "produce-lua-metadata"))]
    #[test]
    fn default_mode_docs_are_inert() {
        // Zero entries -> the empty name list.
        assert!(get_all_function_names().is_empty());

        // Every lookup -> "UNKNOWN FUNCTION" (the shipped C++ behavior).
        assert_eq!(get_docs_for_function("u_rndReal"), "UNKNOWN FUNCTION");
        assert_eq!(get_docs_for_function("u_log"), "UNKNOWN FUNCTION");
        assert_eq!(get_docs_for_function("anything"), "UNKNOWN FUNCTION");

        // 11 category headers, in order, and no `* **` entry line anywhere.
        let mut out = Vec::new();
        print_docs(&mut out).unwrap();
        let text = String::from_utf8(out).unwrap();
        let mut position = 0;
        for header in PREFIX_HEADERS {
            let rel = text[position..]
                .find(header)
                .unwrap_or_else(|| panic!("header missing or out of order: {header}"));
            position += rel + header.len();
        }
        assert!(
            !text.contains("* **`"),
            "default-mode print_docs must contain zero entry lines: {text}"
        );
    }

    /// Feature-on mode (the C++ flag-ON build): the proxy drop records, so
    /// the docs resolve. Registered under a Miscellaneous-category name: the
    /// `metadata.rs` drop test in this same binary asserts on category 0's
    /// global count, so a `u_`-prefixed registration would race it.
    #[cfg(feature = "produce-lua-metadata")]
    #[test]
    fn feature_on_mode_populates_entries() {
        {
            let mut proxy =
                LuaMetadataProxy::new("z_docsProbe", "float", &["int", "int"]);
            proxy.arg("lower").arg("upper");
            proxy.doc("Return a random value in `[$0; $1]`.");
        }
        assert!(
            get_all_function_names().iter().any(|n| n == "z_docsProbe"),
            "registered name must appear in the list"
        );
        assert_eq!(
            get_docs_for_function("z_docsProbe"),
            // `resolve_arg_names` substitutes `$N` verbatim — the backticks
            // around the placeholder survive (matches the C++ parser).
            "float z_docsProbe(int lower, int upper):\n\
             Return a random value in `[lower; upper]`.\n\n"
        );
        assert_eq!(get_docs_for_function("no_such_fn"), "UNKNOWN FUNCTION");

        let mut out = Vec::new();
        print_docs(&mut out).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(
            text.contains(
                "* **`float z_docsProbe(int lower, int upper)`**: \
                 Return a random value in `[lower; upper]`.\n\n"
            ),
            "feature-on print_docs must carry the entry line: {text}"
        );
        for header in PREFIX_HEADERS {
            assert!(text.contains(header), "header missing: {header}");
        }
    }
}
