//! Rust analogue of the game's `ssvuj` JSON layer
//! (`include/SSVOpenHexagon/SSVUtilsJson/**`, `Global/UtilsJson.*`).
//!
//! Backend: `serde_json` (see `.agents/skills/lib-serde-json.md`), in front of
//! which runs a comment-stripping pass that reproduces the pinned JsonCpp
//! fork's reader tolerance EXACTLY (see `.agents/skills/lib-jsoncpp.md` and
//! mapping doc §1.6):
//!
//! - `//` line comments: consumed up to (but NOT including) `\r`/`\n`,
//!   accepted in every token position (before root, between members, after a
//!   comma, end-of-line after a value, inside arrays).
//! - `/* ... */` block comments: accepted in every token position; the
//!   closing `*/` is required (unclosed → error).
//! - Comments are never accepted inside string literals.
//! - **Trailing commas are NOT accepted** (objects or arrays) — rejected,
//!   matching the C++ baseline (error strings differ; documented in the
//!   skill files).
//!
//! C++ semantics preserved for value access (mapping §1.4) — including
//! the fork's cross-type COERCIONS (`jsoncpp.inl:1177-1322`):
//! - a missing key / missing array index reads as the JSON null singleton:
//!   `as_int(null) = 0`, `as_float(null) = 0.0`, `as_bool(null) = false`,
//!   `as_str(null) = ""`, `size(null) = 0`.
//! - `as_bool` accepts int/uint/real (non-zero → true, `asBool`
//!   @jsoncpp.inl:1310-1322); `as_int` / `as_uint` accept bool (→ 1/0) and
//!   real (truncated toward zero, in-range checks, `asInt` @:1191-1210 /
//!   `asUInt` @:1211-1229); `as_float` accepts bool (→ 1.0/0.0,
//!   `asFloat` @:1297-1309 / `asDouble` @:1284-1295); `as_str` accepts bool ("true"/
//!   "false") and numbers (decimal strings, reals via the fork's
//!   `valueToString(double)` = `%#.16g` + trailing-zero trim,
//!   `asString` @:1177-1190).
//! - a value with a type the fork does NOT coerce (a string where a
//!   number is expected, an object/array where a scalar is) is a
//!   `JsonError` — the Rust analogue of the `std::runtime_error`
//!   (`"Value is not convertible to int."` et al.) the fork's
//!   `JSON_ASSERT_MESSAGE` throws (only the per-pack asset-load path in
//!   C++ catches it, `Assets.cpp`; see mapping §1.4 / §10.3).

use serde_json::{Number, Value as SerdeValue};

/// JSON object (C++ `ssvuj::Obj` = `Json::Value`).
///
/// `serde_json::Value` keeps object members in a `BTreeMap<String, _>` by
/// default — i.e. byte-sorted key order, the same order the C++ fork's
/// `std::map<CZString, Value>` member storage gives (mapping §1.1).
pub type Obj = SerdeValue;

/// JSON read error. `formatted_messages()` reproduces the fork's
/// `Reader::getFormattedErrorMessages()` line shape:
/// `* Line L, Column C\n  <message>\n` (line/column are 1-based, relative to
/// the ORIGINAL text — the comment strip preserves newlines).
#[derive(Debug, Clone, PartialEq)]
pub struct JsonError {
    line: u32,
    column: u32,
    message: String,
}

impl JsonError {
    fn new(line: u32, column: u32, message: String) -> Self {
        Self {
            line,
            column,
            message,
        }
    }

    /// C++ `Reader::getFormattedErrorMessages()`.
    #[must_use = "mirrors the C++ [[nodiscard]]"]
    pub fn formatted_messages(&self) -> String {
        format!(
            "* Line {}, Column {}\n  {}\n",
            self.line, self.column, self.message
        )
    }

    #[must_use = "mirrors the C++ [[nodiscard]]"]
    pub fn line(&self) -> u32 {
        self.line
    }

    #[must_use = "mirrors the C++ [[nodiscard]]"]
    pub fn column(&self) -> u32 {
        self.column
    }

    #[must_use = "mirrors the C++ [[nodiscard]]"]
    pub fn message(&self) -> &str {
        &self.message
    }
}

/// C++ `ssvuj::Impl::tryParse` / `readFromString`: parse `src` with the
/// default `Reader` (comments allowed, `collectComments = false`).
pub fn parse(src: &str) -> Result<Obj, JsonError> {
    let stripped = strip_comments(src)?;
    serde_json::from_str(&stripped).map_err(|e| {
        let (line, column) = if e.line() == 0 {
            (1u32, 1u32)
        } else {
            (e.line() as u32, e.column() as u32)
        };
        // Note: the fork reports its own wording for e.g. trailing commas
        // ("Missing '}' or object member name" / "Syntax error: value,
        // object or array expected."); rejection behavior matches, the
        // wording does not (documented in lib-jsoncpp.md).
        JsonError::new(line, column, e.to_string())
    })
}

/// Strip `//` and `/* */` comments exactly as the fork's `Reader` tolerates
/// them (mapping §1.6). Comment spans are replaced by spaces (newlines
/// inside block comments are kept) so line/column of later parse errors
/// still point at the original text.
///
/// One deliberate rejection beyond plain "not JSON": a comment between an
/// object key string and its `:` — the fork's `readObject` reads the
/// `tokenMemberSeparator` DIRECTLY after `decodeString` (no comment skip),
/// so `{ "a" /*c*/ : 1 }` fails with
/// `Missing ':' after object member name` in C++. Comments everywhere else
/// (before root/key/value, between members, after a comma, before `]`/`}`)
/// are accepted, matching `skipCommentTokens` / the `readObject` /
/// `readArray` finalize loops.
fn strip_comments(src: &str) -> Result<String, JsonError> {
    let bytes = src.as_bytes();
    let mut out = String::with_capacity(src.len());
    let mut i = 0usize;
    let mut last_sig: u8 = 0; // last non-whitespace char written
    while i < bytes.len() {
        match bytes[i] {
            b' ' | b'\t' | b'\r' | b'\n' => {
                out.push(bytes[i] as char);
                i += 1;
            }
            b'"' => {
                // String literal: copy verbatim, honoring escapes (a `\"`
                // does not end the string).
                out.push('"');
                i += 1;
                while i < bytes.len() {
                    let c = bytes[i];
                    if c == b'\\' {
                        out.push(c as char);
                        i += 1;
                        if i < bytes.len() {
                            out.push(bytes[i] as char);
                            i += 1;
                        } else {
                            break;
                        }
                        continue;
                    }
                    out.push(c as char);
                    i += 1;
                    if c == b'"' {
                        last_sig = b'"';
                        break;
                    }
                }
            }
            b'/' if i + 1 < bytes.len() && (bytes[i + 1] == b'/' || bytes[i + 1] == b'*') => {
                // A comment is only a legal token where the fork's
                // `skipCommentTokens`/finalize loops run. The one spot it
                // does NOT run is between an object key and its `:`.
                if last_sig == b'"' && next_significant_after_comment(bytes, i) == Some(b':') {
                    let line = src[..i].lines().count() as u32;
                    return Err(JsonError::new(
                        line.max(1),
                        1,
                        "Missing ':' after object member name".to_string(),
                    ));
                }
                let close = skip_comment_span(bytes, i)?;
                for &b in &bytes[i..close] {
                    // keep newlines, blank the rest
                    out.push(if b == b'\n' { '\n' } else { ' ' });
                }
                i = close;
            }
            c => {
                out.push(c as char);
                last_sig = c;
                i += 1;
            }
        }
    }
    Ok(out)
}

/// End offset (exclusive) of the comment starting at `i` (`//` or `/*`).
/// Unterminated block comment → `Err` (fork: `readCStyleComment` fails).
fn skip_comment_span(bytes: &[u8], i: usize) -> Result<usize, JsonError> {
    if bytes[i + 1] == b'/' {
        let mut j = i + 2;
        while j < bytes.len() && bytes[j] != b'\n' && bytes[j] != b'\r' {
            j += 1;
        }
        Ok(j)
    } else {
        let mut j = i + 2;
        while j + 1 < bytes.len() {
            if bytes[j] == b'*' && bytes[j + 1] == b'/' {
                return Ok(j + 2);
            }
            j += 1;
        }
        Err(JsonError::new(
            bytes[..j].iter().filter(|&&b| b == b'\n').count() as u32,
            1,
            "Syntax error: unterminated /* comment".to_string(),
        ))
    }
}

/// The first non-whitespace, non-comment byte at/after `i` (the comment at
/// `i` itself included), or `None` at EOF.
fn next_significant_after_comment(bytes: &[u8], i: usize) -> Option<u8> {
    let mut i = i;
    loop {
        match skip_comment_span(bytes, i) {
            Err(_) => return None, // malformed comment; parser will report
            Ok(end) => i = end,
        }
        while i < bytes.len() && matches!(bytes[i], b' ' | b'\t' | b'\r' | b'\n') {
            i += 1;
        }
        if i >= bytes.len() {
            return None;
        }
        if bytes[i] == b'/' && i + 1 < bytes.len() && (bytes[i + 1] == b'/' || bytes[i + 1] == b'*')
        {
            continue;
        }
        return Some(bytes[i]);
    }
}

impl std::fmt::Display for JsonError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for JsonError {}

// ---------------------------------------------------------------------------
// Value access (C++ `ssvuj::Main.hpp`)
// ---------------------------------------------------------------------------

/// C++ `getObj(mObj)` — the value itself.
#[must_use = "mirrors the C++ [[nodiscard]]"]
pub fn get(obj: &Obj) -> &Obj {
    obj
}

/// C++ `getObj(mObj, key)` — `None` when the key is absent (Rust analogue of
/// the const `operator[]` missing-key → null singleton split into an Option).
#[must_use = "mirrors the C++ [[nodiscard]]"]
pub fn get_or_null<'a>(obj: &'a Obj, key: &str) -> Option<&'a Obj> {
    obj.get(key)
}

/// C++ const `operator[]` missing-key behavior: reads the null singleton.
#[must_use = "mirrors the C++ [[nodiscard]]"]
pub fn get_missing_as_null<'a>(obj: &'a Obj, key: &str) -> &'a Obj {
    obj.get(key).unwrap_or(&SerdeValue::Null)
}

/// C++ `getObj(mArray, idx)` (const): missing index / non-array → null.
#[must_use = "mirrors the C++ [[nodiscard]]"]
pub fn get_at_or_null(obj: &Obj, idx: usize) -> &Obj {
    match obj {
        SerdeValue::Array(items) => items.get(idx).unwrap_or(&SerdeValue::Null),
        _ => &SerdeValue::Null,
    }
}

/// C++ `hasObj(mObj, key)` = `obj.isObject() && obj.isMember(key)`.
#[must_use = "mirrors the C++ [[nodiscard]]"]
pub fn has(obj: &Obj, key: &str) -> bool {
    matches!(obj, SerdeValue::Object(m) if m.contains_key(key))
}

/// C++ `hasObj(mArray, idx)` = `arr.isArray() && idx < arr.size()`.
#[must_use = "mirrors the C++ [[nodiscard]]"]
pub fn has_at(obj: &Obj, idx: usize) -> bool {
    matches!(obj, SerdeValue::Array(items) if idx < items.len())
}

/// C++ `getObjSize(obj)` = `obj.size()`: 0 for null / scalars.
#[must_use = "mirrors the C++ [[nodiscard]]"]
pub fn size(obj: &Obj) -> usize {
    match obj {
        SerdeValue::Array(items) => items.len(),
        SerdeValue::Object(items) => items.len(),
        _ => 0,
    }
}

/// C++ `isObj(obj)` = `obj.isObject()`.
#[must_use = "mirrors the C++ [[nodiscard]]"]
pub fn is_obj(obj: &Obj) -> bool {
    obj.is_object()
}

/// C++ `isObjArray(obj)` = `obj.isArray()`.
#[must_use = "mirrors the C++ [[nodiscard]]"]
pub fn is_obj_array(obj: &Obj) -> bool {
    obj.is_array()
}

/// C++ `Impl::isObjType<T>` for the types the game uses (mapping §1.5):
/// float/double → `isDouble()` = real **or integral**.
#[must_use = "mirrors the C++ [[nodiscard]]"]
pub fn is_obj_type_int(obj: &Obj) -> bool {
    matches!(obj, SerdeValue::Number(_)) && !is_real(obj)
}

#[must_use = "mirrors the C++ [[nodiscard]]"]
pub fn is_obj_type_float(obj: &Obj) -> bool {
    obj.is_number()
}

#[must_use = "mirrors the C++ [[nodiscard]]"]
pub fn is_obj_type_bool(obj: &Obj) -> bool {
    obj.is_boolean()
}

#[must_use = "mirrors the C++ [[nodiscard]]"]
pub fn is_obj_type_str(obj: &Obj) -> bool {
    obj.is_string()
}

fn is_real(obj: &Obj) -> bool {
    matches!(obj, SerdeValue::Number(n) if n.is_f64())
}

// ---------------------------------------------------------------------------
// Typed conversion (C++ `BasicConverters.hpp`; null → zero value, wrong type
// → error, exactly as the fork's `as*` JSON_ASSERT throws, mapping §1.4)
// ---------------------------------------------------------------------------

/// C++ `asInt()` (jsoncpp.inl:1191-1210) with the fork's int64 range
/// asserts: null → 0; int64 within the i32 range (else "LargestInt out of
/// int range"), uint64 ≤ i32::MAX (else "LargestUInt out of int range"),
/// real within [i32::MIN, i32::MAX] truncated toward zero (else "double
/// out of int range"); bool → 1/0; string/object/array → `JsonError`
/// (`"Value is not convertible to int."`).
#[must_use = "mirrors the C++ [[nodiscard]]"]
pub fn as_int(obj: &Obj) -> Result<i64, JsonError> {
    match obj {
        SerdeValue::Null => Ok(0),
        SerdeValue::Number(n) => {
            if let Some(i) = n.as_i64() {
                if !(i32::MIN as i64..=i32::MAX as i64).contains(&i) {
                    return Err(JsonError::new(
                        1,
                        1,
                        "LargestInt out of int range".to_string(),
                    ));
                }
                return Ok(i);
            }
            if let Some(u) = n.as_u64() {
                // fork `asInt`'s uint arm: `isInt()` iff the uint ≤ max Int
                if u > i32::MAX as u64 {
                    return Err(JsonError::new(
                        1,
                        1,
                        "LargestUInt out of int range".to_string(),
                    ));
                }
                return Ok(u as i64);
            }
            let f = n.as_f64().unwrap_or(0.0);
            // fork `asInt`'s real arm: `InRange(real, minInt, maxInt)`
            // (NaN / ±inf fail the range test, exactly as in C++).
            if !(f >= i32::MIN as f64 && f <= i32::MAX as f64) {
                return Err(JsonError::new(1, 1, "double out of int range".to_string()));
            }
            Ok(f as i64)
        }
        SerdeValue::Bool(b) => Ok(i64::from(*b)),
        other => Err(type_error(other, "int")),
    }
}

/// C++ `asUInt()` (jsoncpp.inl:1211-1229): null → 0; int64 within
/// [0, u32::MAX] (else "LargestInt out of UInt range"), uint64 ≤ u32::MAX
/// (else "LargestUInt out of UInt range"), real within [0, 4294967295.0]
/// truncated toward zero (else "double out of UInt range"); bool → 1/0;
/// string/object/array → `JsonError` (`"Value is not convertible to
/// UInt.").
#[must_use = "mirrors the C++ [[nodiscard]]"]
pub fn as_uint(obj: &Obj) -> Result<u32, JsonError> {
    match obj {
        SerdeValue::Null => Ok(0),
        SerdeValue::Number(n) => {
            if let Some(i) = n.as_i64() {
                if !(0..=u32::MAX as i64).contains(&i) {
                    return Err(JsonError::new(
                        1,
                        1,
                        "LargestInt out of UInt range".to_string(),
                    ));
                }
                return Ok(i as u32);
            }
            if let Some(u) = n.as_u64() {
                if u > u32::MAX as u64 {
                    return Err(JsonError::new(
                        1,
                        1,
                        "LargestUInt out of UInt range".to_string(),
                    ));
                }
                return Ok(u as u32);
            }
            let f = n.as_f64().unwrap_or(0.0);
            // fork `asUInt`'s real arm: `InRange(real, 0, maxUInt)` —
            // `maxUInt` (4294967295) is exactly representable as a double.
            if !(f >= 0.0 && f <= u32::MAX as f64) {
                return Err(JsonError::new(1, 1, "double out of UInt range".to_string()));
            }
            Ok(f as u32)
        }
        SerdeValue::Bool(b) => Ok(u32::from(*b)),
        _ => Err(JsonError::new(
            1,
            1,
            "Value is not convertible to UInt.".to_string(),
        )),
    }
}

/// C++ `asFloat()` (jsoncpp.inl:1297-1309; float converters go through
/// `asDouble()`, :1284-1295, in the fork): null → 0.0; integrals and reals accepted
/// (`isDouble()` includes integrals, mapping §1.5); bool → 1.0/0.0;
/// string/object/array → `JsonError` (`"Value is not convertible to
/// float.").
#[must_use = "mirrors the C++ [[nodiscard]]"]
pub fn as_float(obj: &Obj) -> Result<f64, JsonError> {
    match obj {
        SerdeValue::Null => Ok(0.0),
        SerdeValue::Number(n) => Ok(n.as_f64().unwrap_or(0.0)),
        SerdeValue::Bool(b) => Ok(f64::from(*b)),
        other => Err(type_error(other, "float")),
    }
}

/// C++ `asBool()` (jsoncpp.inl:1310-1322): null → false; bool accepted;
/// int/uint/real coerced by non-zero test; string/object/array →
/// `JsonError` (`"Value is not convertible to bool.").
#[must_use = "mirrors the C++ [[nodiscard]]"]
pub fn as_bool(obj: &Obj) -> Result<bool, JsonError> {
    match obj {
        SerdeValue::Null => Ok(false),
        SerdeValue::Bool(b) => Ok(*b),
        SerdeValue::Number(n) => {
            let non_zero = if let Some(i) = n.as_i64() {
                i != 0
            } else if let Some(u) = n.as_u64() {
                u != 0
            } else {
                n.as_f64().unwrap_or(0.0) != 0.0
            };
            Ok(non_zero)
        }
        other => Err(type_error(other, "bool")),
    }
}

/// C++ `asString()` (jsoncpp.inl:1177-1190) — the fork returns an owned
/// `std::string`, so this returns one too: null → ""; string accepted;
/// bool → "true"/"false"; int64/uint64 → decimal strings; real → the
/// fork's `valueToString(double)` (`%#.16g` + trailing-zero trim — the
/// writer's [`double_to_string`]); object/array → `JsonError` (`"Type is
/// not convertible to std::string"` — the fork's distinct message).
#[must_use = "mirrors the C++ [[nodiscard]]"]
pub fn as_str(obj: &Obj) -> Result<String, JsonError> {
    match obj {
        SerdeValue::Null => Ok(String::new()),
        SerdeValue::String(s) => Ok(s.clone()),
        SerdeValue::Bool(b) => Ok(if *b { "true" } else { "false" }.to_string()),
        SerdeValue::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(i.to_string())
            } else if let Some(u) = n.as_u64() {
                Ok(u.to_string())
            } else {
                Ok(double_to_string(n.as_f64().unwrap_or(0.0)))
            }
        }
        _ => Err(JsonError::new(
            1,
            1,
            "Type is not convertible to std::string".to_string(),
        )),
    }
}

fn type_error(obj: &Obj, to: &str) -> JsonError {
    // The fork's JSON_ASSERT_MESSAGE text: "Value is not convertible to X."
    // (X = the target type name: int / float / bool / string …)
    let _ = obj;
    JsonError::new(1, 1, format!("Value is not convertible to {to}."))
}

// ---------------------------------------------------------------------------
// `getExtr<T>` convenience layer (C++ `Main.hpp`)
// ---------------------------------------------------------------------------

/// C++ `getExtr<std::string>(obj, key, default)`: `has ? asString :`
/// default (the fork's `asString` coerctions apply to the present value).
#[must_use = "mirrors the C++ [[nodiscard]]"]
pub fn extr_str(obj: &Obj, key: &str, default: &str) -> Result<String, JsonError> {
    if has(obj, key) {
        as_str(get_missing_as_null(obj, key))
    } else {
        Ok(default.to_string())
    }
}

#[must_use = "mirrors the C++ [[nodiscard]]"]
pub fn extr_bool(obj: &Obj, key: &str, default: bool) -> Result<bool, JsonError> {
    if has(obj, key) {
        as_bool(get_missing_as_null(obj, key))
    } else {
        Ok(default)
    }
}

#[must_use = "mirrors the C++ [[nodiscard]]"]
pub fn extr_int(obj: &Obj, key: &str, default: i64) -> Result<i64, JsonError> {
    if has(obj, key) {
        as_int(get_missing_as_null(obj, key))
    } else {
        Ok(default)
    }
}

#[must_use = "mirrors the C++ [[nodiscard]]"]
pub fn extr_float(obj: &Obj, key: &str, default: f64) -> Result<f64, JsonError> {
    if has(obj, key) {
        as_float(get_missing_as_null(obj, key))
    } else {
        Ok(default)
    }
}

/// C++ `getExtr<std::vector<float>>`: `resize(size); for i: extr(i)` — a
/// missing index yields null → 0.0; a non-array yields size 0 → empty.
#[must_use = "mirrors the C++ [[nodiscard]]"]
pub fn extr_vec_f32(obj: &Obj) -> Result<Vec<f32>, JsonError> {
    let mut v = vec![0.0f32; size(obj)];
    for (i, v) in v.iter_mut().enumerate() {
        *v = as_float(get_at_or_null(obj, i))? as f32;
    }
    Ok(v)
}

#[must_use = "mirrors the C++ [[nodiscard]]"]
pub fn extr_vec_str(obj: &Obj) -> Result<Vec<String>, JsonError> {
    let mut v = vec![String::new(); size(obj)];
    for (i, v) in v.iter_mut().enumerate() {
        *v = as_str(get_at_or_null(obj, i))?;
    }
    Ok(v)
}

/// C++ `getExtr<std::unordered_map<std::string, float>>` — object members →
/// (key, float value); a missing/null value → 0.0.
#[must_use = "mirrors the C++ [[nodiscard]]"]
pub fn extr_map_str_f32(obj: &Obj) -> Result<std::collections::BTreeMap<String, f32>, JsonError> {
    let mut m = std::collections::BTreeMap::new();
    if let SerdeValue::Object(map) = obj {
        for (k, v) in map {
            m.insert(k.clone(), as_float(v)? as f32);
        }
    }
    Ok(m)
}

/// C++ `Converter<sf::Color>` (`Global/UtilsJson.cpp`): a 4-element integer
/// array `[r, g, b, a]`, each converted `unsigned char(asInt())`.
#[must_use = "mirrors the C++ [[nodiscard]]"]
pub fn extr_color(obj: &Obj) -> Result<oh_math::hg::color::Color, JsonError> {
    let raw: [i64; 4] = (0..4)
        .map(|i| as_int(get_at_or_null(obj, i)))
        .collect::<Result<Vec<_>, _>>()
        .map(|v| v.try_into().unwrap())?;
    Ok(oh_math::hg::color::Color {
        r: raw[0].rem_euclid(256) as u8,
        g: raw[1].rem_euclid(256) as u8,
        b: raw[2].rem_euclid(256) as u8,
        a: raw[3].rem_euclid(256) as u8,
    })
}

/// C++ `ssvuj::arch(obj, key, value)` — write a Rust value into an object.
pub fn arch(obj: &mut Obj, key: &str, value: Obj) {
    if let SerdeValue::Object(map) = obj {
        map.insert(key.to_string(), value);
    } else {
        *obj = SerdeValue::Object([(key.to_string(), value)].into_iter().collect());
    }
}

/// C++ `ssvuj::Obj{}` — a default-constructed EMPTY object (the fork's
/// `ObjImpl` = `VecMap` — a sorted map, default-empty). `pCreate` archives
/// it for the fresh profile's `scores` / `favorites` (Assets.cpp:1553-1557),
/// and the writer emits it as `{}` — the on-disk shape `pCreate`'s own
/// reload then reads back through `loadProfileFromJson`.
#[must_use = "mirrors the C++ [[nodiscard]]"]
pub fn empty_obj() -> Obj {
    SerdeValue::Object(serde_json::Map::new())
}

/// C++ `getArch(value)` for scalars: build a fresh Obj.
#[must_use = "mirrors the C++ [[nodiscard]]"]
pub fn get_arch_str(s: &str) -> Obj {
    SerdeValue::String(s.to_string())
}

#[must_use = "mirrors the C++ [[nodiscard]]"]
pub fn get_arch_bool(b: bool) -> Obj {
    SerdeValue::Bool(b)
}

#[must_use = "mirrors the C++ [[nodiscard]]"]
pub fn get_arch_int(i: i64) -> Obj {
    Number::from(i).into()
}

/// C++ float arch: stored as a JSON double (C++ `float` → `double` on
/// archive, which is why the shipped `config.json` holds
/// `"player_speed" : 9.449999809265137`).
#[must_use = "mirrors the C++ [[nodiscard]]"]
pub fn get_arch_f32(f: f32) -> Obj {
    SerdeValue::Number(Number::from_f64(f as f64).unwrap_or_else(|| Number::from(0)))
}

#[must_use = "mirrors the C++ [[nodiscard]]"]
pub fn get_arch_vec_str(items: &[String]) -> Obj {
    SerdeValue::Array(
        items
            .iter()
            .map(|s| SerdeValue::String(s.clone()))
            .collect(),
    )
}

// ---------------------------------------------------------------------------
// `Reader` file IO (C++ `Io.hpp`)
// ---------------------------------------------------------------------------

/// C++ `getFromFileWithErrors(path)`: parse a whole file; on failure the
/// error is `""` if the reader produced no message, else
/// `formatted_messages() + " in file " + fileName`.
#[must_use = "mirrors the C++ [[nodiscard]]"]
pub fn read_file_with_errors(path: &std::path::Path) -> (Obj, String) {
    match std::fs::read_to_string(path) {
        Err(_) => (
            SerdeValue::Null,
            format!(
                " in file {}",
                path.file_name().unwrap_or_default().to_string_lossy()
            ),
        ),
        Ok(contents) => match parse(&contents) {
            Ok(obj) => (obj, String::new()),
            Err(e) => {
                // C++ `tryParse` (Io.hpp:27-33): a parse failure logs
                // `ssvuj::logReadError` with the formatted messages and a
                // `From: [<contents>]` tail (the channel prefix applies
                // once; the second line carries no prefix — the C++ log
                // call's embedded newline).
                println!("[ssvuj::logReadError] {}", e.formatted_messages());
                println!("From: [{contents}]");
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                (
                    SerdeValue::Null,
                    format!("{} in file {}", e.formatted_messages(), name),
                )
            }
        },
    }
}

// ---------------------------------------------------------------------------
// Writer (C++ `StyledStreamWriter`, default `"\t"` indentation, mapping §1.1)
// ---------------------------------------------------------------------------

/// C++ `ssvuj::getWriteToString(obj)` — the fork's `StyledStreamWriter`:
///
/// - indentation: one `"\t"` per depth level;
/// - **every non-empty object/array starts on its own line** (`\n` + the
///   value's own indent, via `writeWithIndent` — including the root, which
///   is why the shipped `config.json` starts with a blank line); content is
///   indented one level deeper;
/// - objects: `"key" : value`, one member per line, keys in byte-sorted
///   order (fork member storage = `std::map`), empty object → inline `{}`;
/// - arrays: single-line `[ a, b, c ]` **on the current line** iff
///   `size*3 < 74` AND no child is a non-empty array/object AND
///   `4 + (size-1)*2 + sum(childLengths) < 74`; otherwise multi-line
///   (empty array → `[]`);
/// - strings escaped the fork's way (`valueToQuotedString`): `\"` `\\`
///   `\b` `\f` `\n` `\r` `\t`, other control chars `0x01..=0x1F` as
///   `\uXXXX` (UPPERCASE hex, 4 digits); `0x7F` is NOT escaped in this
///   fork;
/// - floats: `%#.16g` then the fork's trailing-zero trim (so `8.0f` →
///   `"8.0"`, `100.5f` → `"100.50"`, `9.45f` → `"9.449999809265137"`);
/// - a trailing `\n` after the root.
#[must_use = "mirrors the C++ [[nodiscard]]"]
pub fn write_to_string(root: &Obj) -> String {
    let mut s = String::new();
    write_value(&mut s, root, 0);
    s.push('\n');
    s
}

/// C++ `StyledStreamWriter::writeValue` + `writeObjectValue` +
/// `writeArrayValue` (fork `jsoncpp.inl`).
fn write_value(out: &mut String, value: &Obj, depth: usize) {
    match value {
        SerdeValue::Object(map) if map.is_empty() => out.push_str("{}"),
        SerdeValue::Array(items) if items.is_empty() => out.push_str("[]"),
        SerdeValue::Array(items) => {
            if is_multiline_array(items) {
                write_with_indent(out, depth, "[");
                let mut first = true;
                for item in items {
                    if !first {
                        out.push(',');
                    }
                    first = false;
                    let mut child = String::new();
                    write_value(&mut child, item, depth + 1);
                    write_with_indent(out, depth + 1, &child);
                }
                write_with_indent(out, depth, "]");
            } else {
                // C++ `document_ += "[ "`, items joined by `", "`, then
                // `" ]"` — on the CURRENT line.
                out.push_str("[ ");
                let mut first = true;
                for item in items {
                    if !first {
                        out.push_str(", ");
                    }
                    first = false;
                    let mut child = String::new();
                    write_value(&mut child, item, depth + 1);
                    out.push_str(&child);
                }
                out.push_str(" ]");
            }
        }
        SerdeValue::Object(map) => {
            write_with_indent(out, depth, "{");
            let mut first = true;
            for (key, child) in map {
                if !first {
                    out.push(',');
                }
                first = false;
                write_with_indent(out, depth + 1, &quote_string(key));
                out.push_str(" : ");
                write_value(out, child, depth + 1);
            }
            write_with_indent(out, depth, "}");
        }
        other => out.push_str(&scalar_repr(other)),
    }
}

fn scalar_repr(value: &Obj) -> String {
    match value {
        SerdeValue::Null => "null".to_string(),
        SerdeValue::Bool(b) => if *b { "true" } else { "false" }.to_string(),
        SerdeValue::Number(n) => {
            if let Some(i) = n.as_i64() {
                i.to_string()
            } else if let Some(u) = n.as_u64() {
                u.to_string()
            } else if let Some(f) = n.as_f64() {
                double_to_string(f)
            } else {
                "null".to_string()
            }
        }
        SerdeValue::String(s) => quote_string(s),
        _ => "null".to_string(),
    }
}

/// C++ `writeWithIndent`: `\n` + the current indent string + the value
/// (this is why even the ROOT object begins with a newline in
/// `write(root)` — the shipped `config.json`'s first line is blank).
fn write_with_indent(out: &mut String, depth: usize, s: &str) {
    out.push('\n');
    for _ in 0..depth {
        out.push('\t');
    }
    out.push_str(s);
}

/// C++ `StyledStreamWriter::isMultineArray` (fork source, verbatim rules):
/// multiline iff `size*3 >= 74`, or any child is a **non-empty**
/// array/object (`(child.isArray() || child.isObject()) &&
/// child.size() > 0` — empty containers stay inline as `[]`/`{}`), or
/// `lineLength = 4 + (size-1)*2 + sum(childStringLens) >= 74`.
fn is_multiline_array(items: &[Obj]) -> bool {
    let size = items.len() as i64;
    let mut is_multi_line = size * 3 >= 74;
    for item in items {
        if !is_multi_line {
            is_multi_line = matches!(item, Obj::Array(a) if !a.is_empty())
                || matches!(item, Obj::Object(o) if !o.is_empty());
        }
    }
    if !is_multi_line {
        let mut line_length: i64 = 4 + (size - 1) * 2;
        for item in items {
            let mut child = String::new();
            write_value(&mut child, item, 0);
            line_length += child.len() as i64;
        }
        is_multi_line = is_multi_line || line_length >= 74;
    }
    is_multi_line
}

/// C++ `valueToQuotedString` (fork `jsoncpp.inl`): escapes `\"` `\\` `\b`
/// `\f` `\n` `\r` `\t`; other control chars (`0x01..=0x1F`) as `\uXXXX`
/// uppercase-hex; `0x7F` and above pass through.
fn quote_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0C}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) > 0 && (c as u32) <= 0x1F => {
                out.push_str(&format!("\\u{:04X}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// C++ fork `valueToString(double)`: `sprintf("%#.16g")` + the fork's
/// trailing-zero trim (walk back over `'0'`; if a `'.'` is found keep one
/// char past the last non-zero digit; else return unchanged).
pub(crate) fn double_to_string(value: f64) -> String {
    let formatted = format_value_16g(value);
    trim_fork_trailing_zeros(&formatted)
}

/// `%#.16g` emulation: 16 significant digits, scientific form when the
/// decimal exponent is `< -4` or `>= 16` (C++ `%g` rule at precision 16),
/// with `#` semantics (no trailing-zero stripping — the fork trims after).
fn format_value_16g(value: f64) -> String {
    let s = format!("{:.15e}", value); // 1 + 15 significant digits
                                       // Parse "[-]d[.ddddddddddddddd]e[±]ddd"
    let (mant, exp_str) = s.split_once('e').expect("e present");
    let exp: i64 = exp_str.parse().expect("int exp");
    let sign = if value.is_sign_negative() { "-" } else { "" };
    let digits: String = mant.chars().filter(|c| c != &'.').collect();
    debug_assert_eq!(digits.len(), 16);

    if !(-4..16).contains(&exp) {
        // C++ %g scientific: d[.ddd]e±xx (>=2 exponent digits, no leading 0)
        let mant_s = if digits.len() == 1 {
            digits.clone()
        } else {
            format!("{}.{}", &digits[..1], &digits[1..])
        };
        let e_abs = exp.abs();
        let e_str = if e_abs >= 10 {
            e_abs.to_string()
        } else {
            format!("0{e_abs}")
        };
        format!(
            "{sign}{mant_s}e{}{}",
            if exp < 0 { '-' } else { '+' },
            e_str
        )
    } else if exp >= 0 {
        // fixed point, 16 significant digits: integer part = exp+1 digits
        let int_part = if (exp as usize) + 1 >= digits.len() {
            format!(
                "{}{}",
                digits,
                "0".repeat((exp as usize) + 1 - digits.len())
            )
        } else {
            digits[..(exp as usize) + 1].to_string()
        };
        let frac_part = &digits[(exp as usize) + 1..];
        if frac_part.is_empty() {
            format!("{sign}{int_part}")
        } else {
            format!("{sign}{int_part}.{frac_part}")
        }
    } else {
        // -4 <= exp < 0: "0." + (-exp-1) zeros + all 16 digits
        let zeros = "0".repeat((-exp - 1) as usize);
        format!("{sign}0.{zeros}{digits}")
    }
}

/// The fork's exact trim: if the last char is not `'0'` return unchanged;
/// else walk back over `'0'`s to the last non-zero char, then walk left:
/// digits continue, `'.'` truncates to `last_nonzero + 2` chars, anything
/// else returns unchanged.
fn trim_fork_trailing_zeros(s: &str) -> String {
    let bytes = s.as_bytes();
    if bytes.last() != Some(&b'0') {
        return s.to_string();
    }
    let mut ch: isize = bytes.len() as isize - 1;
    while ch > 0 && bytes[ch as usize] == b'0' {
        ch -= 1;
    }
    let last_nonzero = ch;
    loop {
        match bytes[ch as usize] {
            b'0'..=b'9' => ch -= 1,
            b'.' => return s[..last_nonzero as usize + 2].to_string(),
            _ => return s.to_string(),
        }
        if ch < 0 {
            return s.to_string();
        }
    }
}
