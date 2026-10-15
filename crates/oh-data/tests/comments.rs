//! Comment-tolerance test — pins the parser property:
//!
//! **the reader accepts `//` and `/* */` comments in every token position
//! (before the root, between members, after a comma, at end of line after
//! a value, inside arrays, consecutive comments) and REJECTS trailing
//! commas** — exactly the tolerance of the pinned JsonCpp fork
//! (mapping §1.6; empirical C++ results quoted there).

use oh_data::json;

fn parse_ok(src: &str) -> json::Obj {
    json::parse(src).unwrap_or_else(|e| panic!("expected parse of {src:?}, got {e:?}"))
}

#[test]
fn line_comments_in_every_position_are_accepted() {
    // Property: `//` comments parse wherever the C++ fork skips comment
    // tokens. This test FAILS if comment stripping is removed (serde_json
    // would reject the `/` characters).
    let obj = parse_ok(
        "// leading comment before the root\n\
         {\n\
         // comment before the first member\n\
         \"a\": 1, // comment after a value, then comma\n\
         // comment on its own line between members\n\
         \"b\": [ // comment after array open\n\
         1, // one\n\
         2 // two\n\
         ],\n\
         \"c\": { \"d\": true } // comment after the last value, before }\n\
         } // comment after the root",
    );
    assert_eq!(
        obj,
        serde_json::json!({"a": 1, "b": [1, 2], "c": {"d": true}})
    );
}

#[test]
fn block_comments_are_accepted_and_require_closing() {
    let obj = parse_ok("/* root */ { /* c */ \"a\" : 1, \"b\" : /* x */ 2 }");
    assert_eq!(obj, serde_json::json!({"a": 1, "b": 2}));

    // The fork's `readObject` reads the `:` DIRECTLY after the key string
    // (no comment skip): a comment between key and colon fails with
    // "Missing ':' after object member name" — pinned here.
    let e = json::parse(r#"{ "a" /* inline */ : 1 }"#).unwrap_err();
    assert_eq!(e.message(), "Missing ':' after object member name");

    // consecutive comments between tokens
    let obj = parse_ok("{ /* one */ /* two */ \"a\" : 1 }");
    assert_eq!(obj, serde_json::json!({"a": 1}));

    // newlines inside block comments keep line numbers stable
    let obj = parse_ok("{\n/* c1\n   c2 */\n\"a\" : 1\n}");
    assert_eq!(obj, serde_json::json!({"a": 1}));

    // unterminated block comment -> error (fork: readCStyleComment fails)
    assert!(json::parse("{ \"a\" : 1, /* never closed").is_err());
}

#[test]
fn comments_are_not_accepted_inside_string_literals() {
    // The `//` is part of the string value, not a comment.
    let obj = parse_ok(r#"{"url": "https://example.com//x"}"#);
    assert_eq!(
        obj.get("url").and_then(serde_json::Value::as_str),
        Some("https://example.com//x")
    );
    // A `/*` inside a string is also data.
    let obj = parse_ok(r#"{"s": "a /* not a comment"}"#);
    assert_eq!(
        obj.get("s").and_then(serde_json::Value::as_str),
        Some("a /* not a comment")
    );
}

#[test]
fn trailing_commas_are_rejected_like_the_cpp_fork() {
    // Mapping §1.6: the fork does NOT accept trailing commas (objects nor
    // arrays); the C++ errors were:
    //   {"a": 1, "b": 2,}   -> "Missing '}' or object member name"
    //   {"arr": [1, 2, 3,]} -> "Syntax error: value, object or array expected."
    let err = json::parse(r#"{ "a": 1, "b": 2, }"#).unwrap_err();
    assert!(
        !err.formatted_messages().starts_with("  "),
        "formatted message must start with '* Line'"
    );
    assert!(json::parse(r#"{ "arr": [1, 2, 3,] }"#).is_err());
    assert!(json::parse("{\n  \"x\": [1, 2,],\n}").is_err());
}

#[test]
fn missing_key_reads_as_null_singleton() {
    // Mapping §1.4: getExtr<T> without a default on a missing key returns
    // the zero value (null singleton conversion), not an error.
    let obj = parse_ok("{}");
    assert_eq!(
        json::as_int(json::get_missing_as_null(&obj, "nope")).unwrap(),
        0
    );
    assert_eq!(
        json::as_float(json::get_missing_as_null(&obj, "nope")).unwrap(),
        0.0
    );
    assert!(!json::as_bool(json::get_missing_as_null(&obj, "nope")).unwrap());
    assert_eq!(
        json::as_str(json::get_missing_as_null(&obj, "nope")).unwrap(),
        ""
    );
    assert_eq!(
        json::extr_vec_f32(json::get_missing_as_null(&obj, "nope")).unwrap(),
        Vec::<f32>::new()
    );
}

#[test]
fn type_mismatch_is_an_error_like_the_fork_throw() {
    // C++: asInt() on a string throws "Value is not convertible to int."
    let obj = parse_ok(r#"{"a": "one"}"#);
    let e = json::as_int(json::get_missing_as_null(&obj, "a")).unwrap_err();
    assert_eq!(e.message(), "Value is not convertible to int.");
    let e = json::as_float(json::get_missing_as_null(&obj, "a")).unwrap_err();
    assert_eq!(e.message(), "Value is not convertible to float.");
    // integral values ARE convertible to float (fork isDouble = real||int)
    let obj = parse_ok(r#"{"n": 5}"#);
    assert_eq!(
        json::as_float(json::get_missing_as_null(&obj, "n")).unwrap(),
        5.0
    );
}

#[test]
fn parse_error_message_has_jsoncpp_shape() {
    let e = json::parse("{ \"a\": }").unwrap_err();
    let formatted = e.formatted_messages();
    assert!(formatted.starts_with("* Line 1, Column "), "{formatted:?}");
    assert!(formatted.contains("  "), "{formatted:?}");
    assert!(formatted.ends_with('\n'), "{formatted:?}");
}

#[test]
fn real_release_files_with_comments_parse() {
    // These files actually contain `//` comments in the shipped tree.
    for rel in [
        "Packs/artwork/Styles/pointless.json",
        "Packs/cube/Music/callMeKatla.json",
        "Packs/workshopexample/pack.json",
        "Packs/workshopexample/Levels/examplelevel.json",
    ] {
        let (obj, err) = json::read_file_with_errors(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../_RELEASE")
                .join(rel),
        );
        assert!(err.is_empty(), "{rel}: {err}");
        assert!(json::is_obj(&obj));
    }
}
