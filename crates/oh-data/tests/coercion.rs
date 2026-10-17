//! 29l1 — the vendored jsoncpp fork's cross-type coercions
//! (`jsoncpp.inl:1177-1322`): `asBool` accepts int/uint/real, `asInt` /
//! `asUInt` accept bool and real (with the fork's range asserts),
//! `asFloat` accepts bool, `asString` accepts bool/int/uint/real. A
//! hand-edited `config.json` with these types loads in the C++ and must
//! load in the port (the pre-fix `oh_data::json::as_*` hard-failed on
//! every non-matching type).

use oh_data::json::{self, Obj};

fn parse_ok(src: &str) -> Obj {
    json::parse(src).expect("valid JSON")
}

fn val<'a>(obj: &'a Obj, key: &str) -> &'a Obj {
    json::get_missing_as_null(obj, key)
}

#[test]
fn as_bool_coerces_numbers_by_the_non_zero_test() {
    // C++ `asBool` (jsoncpp.inl:1310-1322): int/uint/real → non-zero test.
    let obj = parse_ok(
        r#"{"a": 1, "b": 0, "c": 2.5, "d": -0.5, "e": -1, "f": 0.0, "g": true, "h": false, "i": null}"#,
    );
    assert!(json::as_bool(val(&obj, "a")).unwrap(), "1");
    assert!(!json::as_bool(val(&obj, "b")).unwrap(), "0");
    assert!(json::as_bool(val(&obj, "c")).unwrap(), "2.5");
    assert!(json::as_bool(val(&obj, "d")).unwrap(), "-0.5");
    assert!(json::as_bool(val(&obj, "e")).unwrap(), "-1");
    assert!(!json::as_bool(val(&obj, "f")).unwrap(), "0.0");
    assert!(json::as_bool(val(&obj, "g")).unwrap(), "true");
    assert!(!json::as_bool(val(&obj, "h")).unwrap(), "false");
    assert!(!json::as_bool(val(&obj, "i")).unwrap(), "null");
    // a string is NOT coerced to bool (the fork throws).
    let obj = parse_ok(r#"{"s": "yes"}"#);
    assert_eq!(
        json::as_bool(val(&obj, "s")).unwrap_err().message(),
        "Value is not convertible to bool."
    );
}

#[test]
fn as_int_coerces_bool_and_enforces_the_fork_range_asserts() {
    // C++ `asInt` (jsoncpp.inl:1191-1210): bool → 1/0; real truncated
    // toward zero within [minInt, maxInt]; the fork's JSON_ASSERT_MESSAGE
    // range texts for int64/uint64/real values.
    let obj = parse_ok(
        r#"{"t": true, "f": false, "r": 2.7, "n": -2.7, "ok": 2147483647, "okreal": 2147483647.0,
             "big": 3000000000, "bigu": 9999999999999999999, "bigreal": 3000000000.0, "negreal": -3000000000.0}"#,
    );
    assert_eq!(json::as_int(val(&obj, "t")).unwrap(), 1, "bool true");
    assert_eq!(json::as_int(val(&obj, "f")).unwrap(), 0, "bool false");
    assert_eq!(
        json::as_int(val(&obj, "r")).unwrap(),
        2,
        "2.7 truncates toward zero"
    );
    assert_eq!(
        json::as_int(val(&obj, "n")).unwrap(),
        -2,
        "-2.7 truncates toward zero"
    );
    assert_eq!(json::as_int(val(&obj, "ok")).unwrap(), 2147483647);
    assert_eq!(json::as_int(val(&obj, "okreal")).unwrap(), 2147483647);
    assert_eq!(
        json::as_int(val(&obj, "big")).unwrap_err().message(),
        "LargestInt out of int range"
    );
    assert_eq!(
        json::as_int(val(&obj, "bigu")).unwrap_err().message(),
        "LargestUInt out of int range"
    );
    assert_eq!(
        json::as_int(val(&obj, "bigreal")).unwrap_err().message(),
        "double out of int range"
    );
    assert_eq!(
        json::as_int(val(&obj, "negreal")).unwrap_err().message(),
        "double out of int range"
    );
    // a string is NOT coerced to int (the fork throws).
    let obj = parse_ok(r#"{"s": "5"}"#);
    assert_eq!(
        json::as_int(val(&obj, "s")).unwrap_err().message(),
        "Value is not convertible to int."
    );
}

#[test]
fn as_uint_coerces_bool_and_enforces_the_fork_range_asserts() {
    // C++ `asUInt` (jsoncpp.inl:1211-1229): bool → 1/0; int within
    // [0, maxUInt]; real within [0, 4294967295.0] truncated toward zero.
    let obj = parse_ok(
        r#"{"t": true, "f": false, "neg": -1, "negreal": -0.5, "ok": 4294967295, "okreal": 4294967295.0,
             "over": 4294967296, "overreal": 4294967295.5, "big": 9999999999999999999}"#,
    );
    assert_eq!(json::as_uint(val(&obj, "t")).unwrap(), 1, "bool true");
    assert_eq!(json::as_uint(val(&obj, "f")).unwrap(), 0, "bool false");
    assert_eq!(
        json::as_uint(val(&obj, "neg")).unwrap_err().message(),
        "LargestInt out of UInt range"
    );
    assert_eq!(
        json::as_uint(val(&obj, "negreal")).unwrap_err().message(),
        "double out of UInt range"
    );
    assert_eq!(json::as_uint(val(&obj, "ok")).unwrap(), 4294967295);
    assert_eq!(json::as_uint(val(&obj, "okreal")).unwrap(), 4294967295);
    assert_eq!(
        json::as_uint(val(&obj, "over")).unwrap_err().message(),
        "LargestInt out of UInt range"
    );
    assert_eq!(
        json::as_uint(val(&obj, "overreal")).unwrap_err().message(),
        "double out of UInt range"
    );
    assert_eq!(
        json::as_uint(val(&obj, "big")).unwrap_err().message(),
        "LargestUInt out of UInt range"
    );
    // a string is NOT coerced to UInt (the fork throws).
    let obj = parse_ok(r#"{"s": "5"}"#);
    assert_eq!(
        json::as_uint(val(&obj, "s")).unwrap_err().message(),
        "Value is not convertible to UInt."
    );
}

#[test]
fn as_float_coerces_bool() {
    // C++ `asFloat` / `asDouble` (jsoncpp.inl:1297-1309 / :1284-1295): bool → 1.0/0.0.
    let obj = parse_ok(r#"{"t": true, "f": false, "n": 5, "r": 2.5, "s": "5.0"}"#);
    assert_eq!(json::as_float(val(&obj, "t")).unwrap(), 1.0, "bool true");
    assert_eq!(json::as_float(val(&obj, "f")).unwrap(), 0.0, "bool false");
    assert_eq!(
        json::as_float(val(&obj, "n")).unwrap(),
        5.0,
        "integral accepted"
    );
    assert_eq!(json::as_float(val(&obj, "r")).unwrap(), 2.5);
    assert_eq!(
        json::as_float(val(&obj, "s")).unwrap_err().message(),
        "Value is not convertible to float."
    );
}

#[test]
fn as_str_coerces_bool_and_numbers_the_fork_way() {
    // C++ `asString` (jsoncpp.inl:1177-1190): bool → "true"/"false";
    // int/uint → decimal; real → `valueToString(double)` (`%#.16g` +
    // trailing-zero trim — the fork keeps ONE trailing zero after the
    // last significant digit: 2.5 → "2.50", 100.0 → "100.0", 1e-5 →
    // "1.000000000000000e-05" — verified against the fork standalone);
    // object/array → the fork's distinct
    // "Type is not convertible to std::string" message.
    let obj = parse_ok(
        r#"{"t": true, "f": false, "i": -42, "u": 42, "r": 2.5, "r100": 100.0, "sci": 0.00001, "n": null, "s": "hello", "o": {}}"#,
    );
    assert_eq!(json::as_str(val(&obj, "t")).unwrap(), "true");
    assert_eq!(json::as_str(val(&obj, "f")).unwrap(), "false");
    assert_eq!(json::as_str(val(&obj, "i")).unwrap(), "-42");
    assert_eq!(json::as_str(val(&obj, "u")).unwrap(), "42");
    assert_eq!(
        json::as_str(val(&obj, "r")).unwrap(),
        "2.50",
        "the fork's valueToString(double) — the %#.16g trim keeps one zero"
    );
    assert_eq!(json::as_str(val(&obj, "r100")).unwrap(), "100.0");
    assert_eq!(
        json::as_str(val(&obj, "sci")).unwrap(),
        "1.000000000000000e-05",
        "the fork's %#.16g scientific form"
    );
    assert_eq!(json::as_str(val(&obj, "n")).unwrap(), "");
    assert_eq!(json::as_str(val(&obj, "s")).unwrap(), "hello");
    assert_eq!(
        json::as_str(val(&obj, "o")).unwrap_err().message(),
        "Type is not convertible to std::string"
    );
}

#[test]
fn linked_values_pick_up_the_coerced_types() {
    // The `LinkedValue` converters are where the 101 config keys read:
    // `unsigned int` → `asUInt`, `std::string` → `asString`, `float` →
    // `asFloat`, `bool` → `asBool`, `int` → `asInt`
    // (`SSVUtilsJson/Utils/BasicConverters.hpp:61-106`).
    use oh_data::linked_value::JsonConvertible;

    let obj = parse_ok(r#"{"w": 1, "n": true, "s": 123, "v": true, "i": 2.9, "u": -1.5}"#);
    assert_eq!(
        <u32 as JsonConvertible>::from_obj(val(&obj, "w")).unwrap(),
        1,
        "uint key written as a number-1 (the audit's `\"vsync\": 1` shape)"
    );
    assert_eq!(
        <u32 as JsonConvertible>::from_obj(val(&obj, "n")).unwrap(),
        1,
        "uint key written as a bool"
    );
    assert_eq!(
        <String as JsonConvertible>::from_obj(val(&obj, "s")).unwrap(),
        "123",
        "string key written as a number"
    );
    assert!(
        (<f32 as JsonConvertible>::from_obj(val(&obj, "v")).unwrap() - 1.0_f32).abs()
            < f32::EPSILON
    );
    assert_eq!(
        <i32 as JsonConvertible>::from_obj(val(&obj, "i")).unwrap(),
        2,
        "int key written as a real truncates"
    );
    assert!(
        <u32 as JsonConvertible>::from_obj(val(&obj, "u")).is_err(),
        "a negative real is out of the UInt range (the fork throws)"
    );
}
