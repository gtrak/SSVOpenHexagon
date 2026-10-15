# lib-serde-json — serde_json (Rust)

Pinned version: **serde_json 1.0.154** (`Cargo.lock:158-161`, checksum `e7e9cc8b1b85264074fbcc02a88680c4096b1e47df8f739dceb03bf482f04bd6`; dep `zmij 1.0.23` at `Cargo.lock:201-204`). Read from checked-out source:

`~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/serde_json-1.0.154/src/`

Declared as `serde_json = "1"` in root `Cargo.toml:28` (workspace dep), consumed by `crates/oh-data` as `serde_json = { workspace = true }` (`crates/oh-data/Cargo.toml:11`). **Default features only** — no `preserve_order`, so `Map` is `BTreeMap`-backed (`src/map.rs:29-30,37-38`).

Companion skill: `.agents/skills/lib-jsoncpp.md` (the C++ baseline this crate emulates via `crates/oh-data/src/json.rs`).

## What `oh-data` actually uses

`crates/oh-data/src/json.rs:28` `use serde_json::{Number, Value as SerdeValue};` — everything in oh-data's JSON layer funnels to:

- `Obj = SerdeValue` (`json.rs:35`)
- `serde_json::from_str` on the comment-stripped text (`json.rs:84-86`)
- `Value::get` (string keys) and `Vec::get` for array indices (`json.rs:248-266`)
- `Value::{is_object, is_array, is_boolean, is_string, is_number}` (`json.rs:286-323`)
- `Number::{as_i64, as_u64, as_f64, is_f64}` (`json.rs:324,340-351,363-366`)
- `Number::from(i)` (`json.rs:510`) and `Number::from_f64(f as f64)` (`json.rs:518`)
- serde's `Display` for error text only (`json.rs:86-100`)

The **write path does NOT use serde_json's serializer**: `write_to_string` (`json.rs:589-595`) re-implements the C++ fork's `StyledStreamWriter` byte-for-byte (incl. the `%#.16g`+trim float format, `json.rs:733-817`). So serde's own float formatting is only a parity risk if someone calls `serde_json::to_string`/`Value::to_string` — see "Float formatting".

## `Value` (`src/value/mod.rs:116-150`)

```rust
pub enum Value {
    Null,
    Bool(bool),
    Number(Number),
    String(String),
    Array(Vec<Value>),
    Object(Map<String, Value>),   // default Map = BTreeMap (byte-sorted keys)
}
```

- Key order: **byte-sorted** via BTreeMap — matches the C++ fork's `std::map<CZString, Value>` member order (see `lib-jsoncpp.md`); iteration order of objects in any writer is deterministic.
- Duplicate keys: last one wins (BTreeMap insert), same as the C++ fork.
- `Value::get<I: Index>` (`src/value/mod.rs:305`): `Index<&str>` → `None` for missing key or non-object (`src/value/index.rs:84-90`); `Index<usize>` → `Vec::get`, `None` for out-of-range or non-array (`src/value/index.rs:55-60`). The C++ "missing key/index → null singleton" behavior is emulated on top (`json.rs:247-270` `get_or_null`/`get_missing_as_null`/`get_at_or_null`); serde's own `value["missing"]` `Index` impl returns `Value::Null` directly, which matches the C++ const `operator[]` null-singleton reads.
- `is_null/is_boolean/is_number/is_string/is_array/is_object` + `as_*` accessors are plain type tests (`mod.rs:348-520`).

## `from_str` (`src/de.rs:2709-2714`)

```rust
pub fn from_str<'a, T>(s: &'a str) -> Result<T> where T: de::Deserialize<'a>
{ from_trait(read::StrRead::new(s)) }
```

Strictness (all enforced by the `read`/`de` layers, none are configurable):

- **No comments**: `/` starts no token; a `//` or `/*` in a comment position fails with `"expected value"`. (That's why `oh-data` runs `strip_comments` first — `json.rs:86`, fn at `:113`.)
- **Trailing comma** in object or array → `ErrorCode::TrailingComma` → `"trailing comma"` (`src/error.rs:381`). Matches the C++ fork's rejection; only the message differs (surfaced verbatim at `json.rs:95-98`).
- **Trailing non-whitespace after the root value** → `ErrorCode::TrailingCharacters` → `"trailing characters"` (`src/error.rs:303,382`). The C++ fork **accepts** this (no end-of-input check — `lib-jsoncpp.md`, "Trailing-comma behavior").
- Whitespace: `tab`, `newline`, CR, space (`src/read.rs`); the C++ fork allows exactly the same four.
- Error positions: `Error::line()`/`column()` are 1-based, relative to the string passed to `from_str` (the comment-stripped text — newlines are preserved by the strip, so they stay aligned with the original; `json.rs:111-112`).
- **Non-finite numbers**: any token that lexes as a non-finite float (e.g. `1e999`) reaches `visit_f64` → `Number::from_f64` → `None` → `de::Error::custom("not a JSON number")` (`src/number.rs:440-443`). The C++ fork accepts such tokens into a `realValue` (`sscanf("%lf")` → inf) — see `lib-jsoncpp.md`.
- Integer overflow: out-of-u64 integer digits fall back to `f64` (lexical), stored as `Number` `Float` — the C++ fork's equivalent path falls back to double via `decodeNumber` too.

## `Number` f64/f32 handling (`src/number.rs`)

Internal representation, `src/number.rs:28-34`:

```rust
enum N {
    PosInt(u64),   // 0..=u64::MAX
    NegInt(i64),   // always < 0
    Float(f64),    // always finite
}
```

Parsing: small integers → `PosInt`/`NegInt`; anything with fraction/exponent or overflow → `Float` (`visit_f64`, `number.rs:440`).

Used accessors:

- `is_f64()` (`number.rs:113`): true **only** for `Float` (integers report false). `oh-data` uses this as the "is it a C++ `realValue`" test (`json.rs:324`).
- `as_i64()` (`number.rs:132`): `PosInt` → `Some` iff `≤ i64::MAX`; `NegInt` → `Some`; `Float` → `None`.
- `as_u64()` (`number.rs:151`): `PosInt` → `Some`; else `None`.
- `as_f64()` (`number.rs:162`): **all** variants → `Some(f64)` (never `None` for finite numbers) — this is what makes "a JSON integer is also a valid C++ `realValue` conversion" (`isDouble()` includes integrals) work in `json.rs:336-366`.

Construction:

- `Number::from_f64(f)` (`number.rs:183-196`): `f.is_finite()` → `Some(N::Float(f))`, else `None` (NaN/±inf rejected).
- f32: serde has **no public `from_f32`**; `N::from_f32` is `pub(crate)` (`number.rs:323-335`), used by the f32 serializer (`f as f64`). `oh-data` does the same widening explicitly: `Number::from_f64(f as f64)` (`json.rs:518`), falling back to `Number::from(0)` for non-finite (C++ would write `inf`/`nan` — theoretical divergence, `lib-jsoncpp.md`).
- `Value::from(f64/f32)` go through the same check (`src/value/from.rs:30,47`) — non-finite becomes `Value::Null` there, a footgun if used.

## Float formatting (parity-relevant)

serde_json's own writer formats floats with the **zmij** crate — shortest-round-trip representation (`src/ser.rs:1695,1716-1724` `zmij::Buffer::new().format_finite(value)`; `write_f32` formats the f32 itself). Examples: `100.5` → `"100.5"`, `8.0` → `"8.0"`.

That is **NOT** what the C++ baseline produces: the fork's `valueToString(double)` is `sprintf("%#.16g")` + trailing-zero trim → `100.5` → `"100.50"`, `8.0` → `"8.0"`, 16 significant digits otherwise (see `lib-jsoncpp.md`, "Writer"). Therefore:

- `oh-data` never serializes through serde_json for game-facing output — `write_to_string` re-implements `%#.16g`+trim (`json.rs:733-817`) exactly.
- **Do not** swap in `serde_json::to_string`/`to_writer` for `oh_data::json::write_to_string` — shortest-form floats (`"100.5"` instead of `"100.50"`) would break byte-for-byte parity of written config/level files.
- Read-side float values are unaffected: both paths end in `f64`, and `as_f64` on a parsed `Float` is the exact parsed value.

## Where serde_json 1.0.154 behavior differs from the JsonCpp fork

| Aspect | serde_json | JsonCpp fork | oh-data mitigation |
|---|---|---|---|
| Comments `//`, `/* */` | rejected (`"expected value"`) | accepted (except between key and `:`); see `lib-jsoncpp.md` | `strip_comments` pre-pass, `json.rs:113-201` |
| Trailing comma | rejected, `"trailing comma"` (`error.rs:381`) | rejected, but with `"Missing '}' or object member name"` / `"Syntax error: value, object or array expected."` | rejection kept; wording differs (documented, `json.rs:95-98`) |
| Trailing garbage after root | rejected, `"trailing characters"` (`error.rs:382`) | **accepted** (no end check, `jsoncpp.inl:130,145-146`) | Rust stricter — divergence |
| `1e999` (non-finite real) | error `"not a JSON number"` (`number.rs:440-443`) | accepted as `inf` in a `realValue` | Rust stricter — divergence |
| Integer storage | `PosInt(u64)`/`NegInt(i64)`/`Float(f64)` | `intValue`/`uintValue` (i64/u64)/`realValue` (f64); negative magnitude overflow → double (`jsoncpp.inl:453-458`) | `as_int`/`as_float` re-implement fork semantics, `json.rs:336-370` |
| `NaN`/`inf` as written values | not representable (`from_f64` → `None`) | representable; `%#.16g` writes `"inf"`/`"nan"` | `json.rs:518` falls back to `0` — theoretical divergence |
| Float text in output | shortest round-trip (zmij) | `%#.16g` + zero trim | custom writer, `json.rs:589-817` |
| Object key order | BTreeMap, byte-sorted (default features) | `std::map<CZString, Value>`, byte-sorted | identical |
| Duplicate keys | last wins | last wins | identical |
