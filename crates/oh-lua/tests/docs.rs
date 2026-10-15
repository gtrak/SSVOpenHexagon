//! Issue 014 step (a) — the DISCRIMINATION pair, feature-on half
//! (D-023/D-024): with `produce-lua-metadata` ON (the C++ flag-ON build),
//! the `LuaMetadataProxy` drop records, so the same registration that the
//! default-mode binary proves records nothing here yields populated
//! entries: the name list is non-empty, `get_docs_for_function` resolves
//! with the C++ `"{ret} {name}({args}):\n{docs}\n\n"` format, and
//! `print_docs` carries the `* **` entry lines under the 11 headers.
//!
//! Dedicated test binary (feature-on builds only; in the default build every
//! test here is cfg-excluded, leaving an empty binary): the store
//! assertions are race-free because this is the only writer in the process.

#[cfg(feature = "produce-lua-metadata")]
mod feature_on {
    use oh_lua::docs::{get_all_function_names, get_docs_for_function, print_docs};
    use oh_lua::get_metadata;
    use oh_lua::{add_lua_fn, FnMeta, LuaContext, PREFIX_HEADERS};

    #[test]
    fn feature_on_mode_populates_entries() {
        {
            let mut meta = get_metadata();
            meta.clear();
        }
        let lua = LuaContext::new();
        {
            let mut proxy = add_lua_fn!(
                &lua,
                "u_docsA",
                FnMeta {
                    ret: "int",
                    arg_types: &["int", "int"],
                },
                |lower: i32, upper: i32| {
                    let _ = (lower, upper);
                    0
                }
            )
            .unwrap();
            proxy.arg("lower").arg("upper");
            proxy.doc("Return a random integer in `[$0; $1]`.");
        }
        {
            let mut proxy = add_lua_fn!(
                &lua,
                "a_docsB",
                FnMeta {
                    ret: "void",
                    arg_types: &["std::string"],
                },
                |id: String| {
                    let _ = id;
                }
            )
            .unwrap();
            proxy.arg("id");
            proxy.doc("Play sound `$0`.");
        }

        // Populated name list (both registrations, nothing else).
        let names = get_all_function_names();
        assert!(names.iter().any(|n| n == "u_docsA"), "u_docsA: {names:?}");
        assert!(names.iter().any(|n| n == "a_docsB"), "a_docsB: {names:?}");
        assert_eq!(names.len(), 2, "exactly the two registrations: {names:?}");

        // The C++ `getDocsForFunction` format, byte-exact.
        assert_eq!(
            get_docs_for_function("u_docsA"),
            "int u_docsA(int lower, int upper):\nReturn a random integer in \
             `[lower; upper]`.\n\n"
        );
        assert_eq!(
            get_docs_for_function("a_docsB"),
            "void a_docsB(std::string id):\nPlay sound `id`.\n\n"
        );
        // The fallback survives alongside real entries.
        assert_eq!(get_docs_for_function("u_rndReal"), "UNKNOWN FUNCTION");

        // print_docs: the 11 headers + the two `* **` entry lines.
        let mut out = Vec::new();
        print_docs(&mut out).unwrap();
        let text = String::from_utf8(out).unwrap();
        for header in PREFIX_HEADERS {
            assert!(text.contains(header), "header missing: {header}");
        }
        assert!(
            text.contains(
                "* **`int u_docsA(int lower, int upper)`**: \
                 Return a random integer in `[lower; upper]`.\n\n"
            ),
            "u_docsA entry line missing:\n{text}"
        );
        assert!(
            text.contains(
                "* **`void a_docsB(std::string id)`**: Play sound `id`.\n\n"
            ),
            "a_docsB entry line missing:\n{text}"
        );
    }
}
