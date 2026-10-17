//! Conversion rules, ported 1:1 from the C++ `_push` / `_read` overloads in
//! `include/SSVOpenHexagon/Utils/LuaWrapper.hpp` (mapping/lua-api.md §3).
//!
//! # Push (C++ value -> Lua value)
//!
//! | C++ type | Lua value | C++ rule |
//! |---|---|---|
//! | any integer (`int`, `uint`, `ull`, `size_t`, ...) | 64-bit integer | `lua_pushinteger` |
//! | any float (`float`, `double`) | number | `lua_pushnumber` (converted to `double`) |
//! | `bool` | boolean | `lua_pushboolean` |
//! | `std::string` | string | `lua_pushstring` |
//! | `std::vector` (array) | table | 1-based array table |
//!
//! # Read (Lua value -> C++ arg), per-type coercion (`_read`)
//!
//! Every read first throws `WrongTypeException` if the value is userdata
//! (full or light). Then, mirroring the Lua 5.1 / LuaJIT C API the wrapper is
//! built on:
//!
//! * int <- `lua_tointeger`: nil -> 0; integer -> itself; float -> truncated
//!   toward zero; bool -> 1/0; string -> `tonumber` (truncated), no number -> 0;
//!   anything else (table, function, thread) -> 0.
//! * float/double <- `lua_tonumber`: nil -> 0.0; integer/float -> number;
//!   bool -> 1.0/0.0; string -> `tonumber`, no number -> 0.0; anything else -> 0.0.
//! * bool <- `lua_toboolean != 0`: only `nil` and `false` are false; **every**
//!   other value (including `0` and `""`) is true.
//! * string <- `lua_tostring` (5.1 semantics, no `__tostring` metamethod):
//!   string -> itself; number -> Lua's own number formatting; bool ->
//!   "true"/"false". The C++ `nil`/other case (`lua_tostring` returns
//!   `nullptr`, then `std::string(nullptr)` — undefined behavior, mapping
//!   lua-api.md §8.4) is a *deliberate port decision*: fail with `WrongType`
//!   instead of the latent C++ crash. Lua strings that are not valid UTF-8
//!   (impossible in practice: `lua_pushstring` sources here are Rust strings)
//!   are read lossily (`to_string_lossy`).
//!
//! # `tonumber` parsing
//!
//! Lua 5.1 `tonumber` parses decimal via `strtod` (optional sign, decimal
//! point, `e`/`E` exponent, `inf`/`nan`) and additionally accepts
//! hexadecimal integers (`0x...`). Rust `f64::from_str` matches `strtod` for
//! decimal input; the hex form is handled explicitly. Underscore separators
//! are not accepted (the 5.1 grammar has none; the corpus never uses them in
//! function arguments).

use crate::error::LuaError;
use mlua::{MultiValue, Value};

/// Value convertible **to** Lua (push side). The state handle is needed for
/// string internment (`Lua::create_string`), matching C++ `lua_pushstring`.
/// Takes `&self` (not by value) deliberately: callers keep ownership, and
/// the C++ `writeVariable` copy semantics are preserved by pushing a copy.
#[allow(clippy::wrong_self_convention)]
pub trait IntoLuaValue: Sized {
    fn into_lua_value(&self, lua: &mlua::Lua) -> Result<Value, mlua::Error>;
}

/// Value convertible **from** Lua (read side), using the exact C++ `_read`
/// coercion rules. `read_variable` and registered-function argument
/// extraction use this.
pub trait FromLuaValue: Sized {
    fn from_lua_value(value: &Value) -> Result<Self, LuaError>;
}

/// The argument-conversion rules a registered function's call dispatch
/// uses: the C++ `FunctionToPush::operator()` call-frame rules (see
/// `LuaContext::add_lua_fn`) position the last-N argument window, and each
/// argument is then converted with this trait's `_read` coercion rules.
///
/// A registered function's return value(s), pushed the way C++ `_push` of
/// scalars and `std::tuple` does (tuples become Lua multiple returns).
pub trait PushValue: Sized {
    fn push_value(self, lua: &mlua::Lua) -> Result<MultiValue, mlua::Error>;
}

/// The return side of a registered-function closure
/// (`context::{LuaFn, LuaFnTry}`). Implemented for every `PushValue` (the
/// plain closure return) and for `Result<T, LuaError>` (fallible closures):
/// an `Err` becomes a Lua runtime error carrying the `LuaError`'s `Display`
/// text — the port of the C++ exception propagating out of the lambda to
/// the nearest `runLuaCode`/`runLuaFile` catch.
pub trait FnReturn: Sized {
    /// Push the closure's return value(s) as Lua return values.
    fn to_multi(self, lua: &mlua::Lua) -> Result<MultiValue, mlua::Error>;
}

impl<T: PushValue> FnReturn for T {
    fn to_multi(self, lua: &mlua::Lua) -> Result<MultiValue, mlua::Error> {
        self.push_value(lua)
    }
}

impl<T: PushValue> FnReturn for Result<T, LuaError> {
    fn to_multi(self, lua: &mlua::Lua) -> Result<MultiValue, mlua::Error> {
        self.map_err(|err| mlua::Error::RuntimeError(err.to_string()))?
            .push_value(lua)
    }
}

// ----------------------------------------------------------------------------
// Low-level `_read` coercions
// ----------------------------------------------------------------------------

/// C++ `lua_tonumber` semantics on a LuaJIT 5.1 value.
fn lua_tonumber(value: &Value) -> Option<f64> {
    match value {
        Value::Nil => None,
        Value::Integer(i) => Some(*i as f64),
        Value::Number(n) => Some(*n),
        Value::Boolean(b) => Some(if *b { 1.0 } else { 0.0 }),
        Value::String(s) => parse_lua_number(&s.as_bytes()),
        // table, function, thread, userdata, error, other: not a number
        _ => None,
    }
}

/// Lua 5.1 number parsing; see module docs.
fn parse_lua_number(bytes: &[u8]) -> Option<f64> {
    let text = std::str::from_utf8(bytes).ok()?;
    if let Ok(f) = text.parse::<f64>() {
        return Some(f);
    }
    // 5.1: "0x1A" (and signed variants) parse as hex integers.
    let stripped = text.strip_prefix(['+', '-']).unwrap_or(text);
    if let Some(hex) = stripped
        .strip_prefix("0x")
        .or_else(|| stripped.strip_prefix("0X"))
    {
        if let Ok(i) = i64::from_str_radix(hex, 16) {
            // f64 has no `From<i64>` (possible precision loss); the C cast
            // has the same behavior.
            return Some(i as f64);
        }
    }
    None
}

/// `lua_isuserdata` pre-check of every C++ `_read`: userdata arguments throw
/// `WrongTypeException` before the coercion happens.
fn reject_userdata(value: &Value) -> Result<(), LuaError> {
    if value.is_userdata() || value.is_light_userdata() {
        return Err(LuaError::WrongType);
    }
    Ok(())
}

/// C++ `lua_tointeger` (64-bit): nil -> 0, float truncated toward zero,
/// bool -> 1/0, string -> `tonumber` truncated, anything else -> 0.
///
/// `mlua::Integer` is `i64` on the LuaJIT backend and `i32` on the Luau
/// backend (Luau's `lua_Integer` is `int`); `i64::from` is a no-op on the
/// former and a widening cast on the latter (lane 030a).
#[allow(clippy::useless_conversion)]
pub(crate) fn read_i64(value: &Value) -> Result<i64, LuaError> {
    reject_userdata(value)?;
    Ok(match value {
        Value::Nil => 0,
        Value::Integer(i) => i64::from(*i),
        Value::Number(n) => *n as i64,
        Value::Boolean(b) => i64::from(*b),
        Value::String(_) => lua_tonumber(value).unwrap_or(0.0) as i64,
        _ => 0,
    })
}

/// C++ `lua_tonumber`: nil -> 0.0, bool -> 1.0/0.0, string -> `tonumber` or
/// 0.0, anything else -> 0.0.
pub(crate) fn read_f64(value: &Value) -> Result<f64, LuaError> {
    reject_userdata(value)?;
    Ok(match value {
        Value::Nil => 0.0,
        _ => lua_tonumber(value).unwrap_or(0.0),
    })
}

/// C++ `lua_toboolean != 0`: only `nil` and `false` read as `false`.
pub(crate) fn read_bool(value: &Value) -> Result<bool, LuaError> {
    reject_userdata(value)?;
    Ok(match value {
        Value::Nil => false,
        Value::Boolean(b) => *b,
        _ => true,
    })
}

/// C++ `lua_tostring` (5.1, no metamethods): strings/numbers/booleans
/// convert, everything else returned `nullptr` in C++ (UB crash). Port
/// decision: `WrongType` (mapping/lua-api.md §8.4).
///
/// Number spelling matches the C++ `lua_number2str` = C `%.14g`
/// (`1e20` -> `1e+20`, `0.5` -> `0.5`, `123.456` -> `123.456`), not the
/// LuaJIT `tostring()` builtin (which prints up to 17 significant digits).
pub(crate) fn read_string(value: &Value) -> Result<String, LuaError> {
    reject_userdata(value)?;
    match value {
        Value::String(s) => Ok(s.to_string_lossy()),
        Value::Integer(i) => Ok(i.to_string()),
        Value::Number(n) => Ok(lua_number2str(*n)),
        Value::Boolean(b) => Ok(b.to_string()),
        _ => Err(LuaError::WrongType),
    }
}

/// C `%.14g` — exactly what the 5.1 `lua_number2str`/`lua_tostring`
/// produce for a number in the C++ build (LUA_NUMBER_FMT = `"%.14g"`).
fn lua_number2str(n: f64) -> String {
    if !n.is_finite() {
        // C prints `inf`/`nan` (LuaJIT agrees for the C-API path).
        return if n.is_nan() {
            "nan".to_string()
        } else if n > 0.0 {
            "inf".to_string()
        } else {
            "-inf".to_string()
        };
    }
    if n == 0.0 {
        return if n.is_sign_negative() { "-0" } else { "0" }.to_string();
    }
    let neg = n < 0.0;
    let a = n.abs();
    // decimal exponent d = floor(log10(a)); correct the fp floor in both
    // directions (powers of ten are the only ambiguous boundary).
    let mut d = a.log10().floor() as i32;
    if a >= 10f64.powi(d + 1) {
        d += 1;
    }
    if a < 10f64.powi(d) {
        d -= 1;
    }
    let body = if !(-3..14).contains(&d) {
        // scientific form with 14 significant digits, trailing zeros trimmed,
        // exponent signed with at least two digits (C `e+20` / `e-05`).
        let s = format!("{a:.13e}");
        let (mant, _) = s.split_once('e').unwrap();
        let mant = mant.trim_end_matches('0').trim_end_matches('.');
        let exp = if d >= 0 {
            format!("e+{d:02}")
        } else {
            format!("e-{:02}", (-d).abs())
        };
        format!("{mant}{exp}")
    } else {
        // fixed form with 14 significant digits, trailing zeros trimmed.
        let decimals = (13 - d) as usize;
        let s = format!("{a:.dec$}", dec = decimals);
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    };
    if neg && !body.starts_with('-') {
        format!("-{body}")
    } else {
        body
    }
}

// ----------------------------------------------------------------------------
// `FromLuaValue` impls (the C++ `_read` dispatch per decayed type)
// ----------------------------------------------------------------------------

macro_rules! impl_from_lua_int {
    ($t:ty) => {
        impl FromLuaValue for $t {
            fn from_lua_value(value: &Value) -> Result<Self, LuaError> {
                // C++: `return T(lua_tointeger(_state, index));` — the 64-bit
                // value is cast to T.
                Ok(read_i64(value)? as $t)
            }
        }
    };
}
impl_from_lua_int!(i8);
impl_from_lua_int!(i16);
impl_from_lua_int!(i32);
impl_from_lua_int!(i64);
impl_from_lua_int!(isize);
impl_from_lua_int!(u8);
impl_from_lua_int!(u16);
impl_from_lua_int!(u32);
impl_from_lua_int!(u64);
impl_from_lua_int!(usize);

macro_rules! impl_from_lua_float {
    ($t:ty) => {
        impl FromLuaValue for $t {
            fn from_lua_value(value: &Value) -> Result<Self, LuaError> {
                // C++: `return T(lua_tonumber(_state, index));`
                Ok(read_f64(value)? as $t)
            }
        }
    };
}
impl_from_lua_float!(f32);
impl_from_lua_float!(f64);

impl FromLuaValue for bool {
    fn from_lua_value(value: &Value) -> Result<Self, LuaError> {
        read_bool(value)
    }
}

impl FromLuaValue for String {
    fn from_lua_value(value: &Value) -> Result<Self, LuaError> {
        read_string(value)
    }
}

/// Arrays (Lua 1-based tables) <- `Vec<T>` reads. NOTE: the C++ wrapper's
/// table `_read` is `throw std::logic_error{"Not implemented"}`; array reads
/// are a deliberate (documented) superset so later issues can read tables
/// back. The table's contiguous 1..n prefix is read; a `nil` slot ends the
/// array (C array semantics).
impl<T: FromLuaValue> FromLuaValue for Vec<T> {
    fn from_lua_value(value: &Value) -> Result<Self, LuaError> {
        let table = value.as_table().ok_or(LuaError::WrongType)?;
        let len = table.len().map_err(LuaError::from_mlua)?;
        let len = usize::try_from(len).unwrap_or(usize::MAX);
        let mut out = Vec::with_capacity(len);
        for i in 1..=len {
            match table.get::<Option<Value>>(i).map_err(LuaError::from_mlua)? {
                Some(item) => out.push(T::from_lua_value(&item)?),
                // nil inside the array part: stop (mirrors C array length).
                None => break,
            }
        }
        Ok(out)
    }
}

// ----------------------------------------------------------------------------
// `IntoLuaValue` impls (the C++ `_push` dispatch per type)
// ----------------------------------------------------------------------------

macro_rules! impl_into_lua_int {
    ($t:ty) => {
        impl IntoLuaValue for $t {
            fn into_lua_value(&self, _lua: &mlua::Lua) -> Result<Value, mlua::Error> {
                // C++: `lua_pushinteger(_state, nb)` — every integer type is
                // pushed as a 64-bit integer; values that do not fit the
                // backend's integer width saturate (the C++ cast is
                // implementation-defined there; on the Luau backend
                // `mlua::Integer` is `i32`, so `try_from` saturates at
                // `i32::MAX` — lane 030a characterises this in the integer
                // probe).
                Ok(Value::Integer(
                    mlua::Integer::try_from(*self).unwrap_or(mlua::Integer::MAX),
                ))
            }
        }
    };
}
// Unsigned values wider than the backend integer width wrap exactly like
// the C++ cast.
impl IntoLuaValue for u64 {
    fn into_lua_value(&self, _lua: &mlua::Lua) -> Result<Value, mlua::Error> {
        Ok(Value::Integer(*self as mlua::Integer))
    }
}
impl_into_lua_int!(i8);
impl_into_lua_int!(i16);
impl_into_lua_int!(i32);
impl_into_lua_int!(i64);
impl_into_lua_int!(isize);
impl_into_lua_int!(u8);
impl_into_lua_int!(u16);
impl_into_lua_int!(u32);
impl_into_lua_int!(usize);

macro_rules! impl_into_lua_float {
    ($t:ty) => {
        impl IntoLuaValue for $t {
            fn into_lua_value(&self, _lua: &mlua::Lua) -> Result<Value, mlua::Error> {
                // C++: `lua_pushnumber(_state, nb)` — float values are
                // converted to double on the way.
                Ok(Value::Number(mlua::Number::from(f64::from(*self))))
            }
        }
    };
}
impl_into_lua_float!(f32);
impl_into_lua_float!(f64);

impl IntoLuaValue for bool {
    fn into_lua_value(&self, _lua: &mlua::Lua) -> Result<Value, mlua::Error> {
        Ok(Value::Boolean(*self))
    }
}

impl IntoLuaValue for String {
    fn into_lua_value(&self, lua: &mlua::Lua) -> Result<Value, mlua::Error> {
        // C++: `lua_pushstring` interns the byte string in the state.
        Ok(Value::String(lua.create_string(self.as_bytes())?))
    }
}

impl IntoLuaValue for &str {
    fn into_lua_value(&self, lua: &mlua::Lua) -> Result<Value, mlua::Error> {
        Ok(Value::String(lua.create_string(self.as_bytes())?))
    }
}

impl IntoLuaValue for mlua::Value {
    fn into_lua_value(&self, _lua: &mlua::Lua) -> Result<Value, mlua::Error> {
        Ok(self.clone())
    }
}

impl IntoLuaValue for mlua::Function {
    fn into_lua_value(&self, _lua: &mlua::Lua) -> Result<Value, mlua::Error> {
        Ok(Value::Function(self.clone()))
    }
}

/// `std::vector<T>` / `Vec<T>` -> Lua 1-based array table (the C++ `Table` /
/// map pushes create a plain table; the game's arrays are contiguous).
impl<T: IntoLuaValue> IntoLuaValue for Vec<T> {
    fn into_lua_value(&self, lua: &mlua::Lua) -> Result<Value, mlua::Error> {
        let table = lua.create_table()?;
        for (i, item) in self.iter().enumerate() {
            table.set(i + 1, item.into_lua_value(lua)?)?;
        }
        Ok(Value::Table(table))
    }
}

// ----------------------------------------------------------------------------
// `PushValue` impls (return side of `add_lua_fn`)
// ----------------------------------------------------------------------------

impl PushValue for () {
    fn push_value(self, _lua: &mlua::Lua) -> Result<MultiValue, mlua::Error> {
        Ok(MultiValue::new())
    }
}

macro_rules! impl_push_scalar {
    ($t:ty) => {
        impl PushValue for $t {
            fn push_value(self, lua: &mlua::Lua) -> Result<MultiValue, mlua::Error> {
                Ok(MultiValue::from_vec(vec![IntoLuaValue::into_lua_value(
                    &self, lua,
                )?]))
            }
        }
    };
}
impl_push_scalar!(bool);
impl_push_scalar!(i8);
impl_push_scalar!(i16);
impl_push_scalar!(i32);
impl_push_scalar!(i64);
impl_push_scalar!(isize);
impl_push_scalar!(u8);
impl_push_scalar!(u16);
impl_push_scalar!(u32);
impl_push_scalar!(u64);
impl_push_scalar!(usize);
impl_push_scalar!(f32);
impl_push_scalar!(f64);

impl PushValue for String {
    fn push_value(self, lua: &mlua::Lua) -> Result<MultiValue, mlua::Error> {
        Ok(MultiValue::from_vec(vec![IntoLuaValue::into_lua_value(
            &self, lua,
        )?]))
    }
}

/// C++ `std::tuple<Ts...>` -> N Lua return values. The game's multi-returns
/// are 2-tuples (vertex pairs), 4-tuples (colors) and 8-tuples
/// (`cw_getVertexPos4`); arities 2..8 cover every registered API
/// (lua-api.md §1). Elements are pushed directly (C++ `_push` per element).
impl<A: IntoLuaValue, B: IntoLuaValue> PushValue for (A, B) {
    fn push_value(self, lua: &mlua::Lua) -> Result<MultiValue, mlua::Error> {
        Ok(MultiValue::from_vec(vec![
            A::into_lua_value(&self.0, lua)?,
            B::into_lua_value(&self.1, lua)?,
        ]))
    }
}
impl<A: IntoLuaValue, B: IntoLuaValue, C: IntoLuaValue> PushValue for (A, B, C) {
    fn push_value(self, lua: &mlua::Lua) -> Result<MultiValue, mlua::Error> {
        Ok(MultiValue::from_vec(vec![
            A::into_lua_value(&self.0, lua)?,
            B::into_lua_value(&self.1, lua)?,
            C::into_lua_value(&self.2, lua)?,
        ]))
    }
}
impl<A: IntoLuaValue, B: IntoLuaValue, C: IntoLuaValue, D: IntoLuaValue> PushValue
    for (A, B, C, D)
{
    fn push_value(self, lua: &mlua::Lua) -> Result<MultiValue, mlua::Error> {
        Ok(MultiValue::from_vec(vec![
            A::into_lua_value(&self.0, lua)?,
            B::into_lua_value(&self.1, lua)?,
            C::into_lua_value(&self.2, lua)?,
            D::into_lua_value(&self.3, lua)?,
        ]))
    }
}
impl<A: IntoLuaValue, B: IntoLuaValue, C: IntoLuaValue, D: IntoLuaValue, E: IntoLuaValue> PushValue
    for (A, B, C, D, E)
{
    fn push_value(self, lua: &mlua::Lua) -> Result<MultiValue, mlua::Error> {
        Ok(MultiValue::from_vec(vec![
            A::into_lua_value(&self.0, lua)?,
            B::into_lua_value(&self.1, lua)?,
            C::into_lua_value(&self.2, lua)?,
            D::into_lua_value(&self.3, lua)?,
            E::into_lua_value(&self.4, lua)?,
        ]))
    }
}
impl<
        A: IntoLuaValue,
        B: IntoLuaValue,
        C: IntoLuaValue,
        D: IntoLuaValue,
        E: IntoLuaValue,
        F: IntoLuaValue,
    > PushValue for (A, B, C, D, E, F)
{
    fn push_value(self, lua: &mlua::Lua) -> Result<MultiValue, mlua::Error> {
        Ok(MultiValue::from_vec(vec![
            A::into_lua_value(&self.0, lua)?,
            B::into_lua_value(&self.1, lua)?,
            C::into_lua_value(&self.2, lua)?,
            D::into_lua_value(&self.3, lua)?,
            E::into_lua_value(&self.4, lua)?,
            F::into_lua_value(&self.5, lua)?,
        ]))
    }
}
impl<
        A: IntoLuaValue,
        B: IntoLuaValue,
        C: IntoLuaValue,
        D: IntoLuaValue,
        E: IntoLuaValue,
        F: IntoLuaValue,
        G: IntoLuaValue,
    > PushValue for (A, B, C, D, E, F, G)
{
    fn push_value(self, lua: &mlua::Lua) -> Result<MultiValue, mlua::Error> {
        Ok(MultiValue::from_vec(vec![
            A::into_lua_value(&self.0, lua)?,
            B::into_lua_value(&self.1, lua)?,
            C::into_lua_value(&self.2, lua)?,
            D::into_lua_value(&self.3, lua)?,
            E::into_lua_value(&self.4, lua)?,
            F::into_lua_value(&self.5, lua)?,
            G::into_lua_value(&self.6, lua)?,
        ]))
    }
}
impl<
        A: IntoLuaValue,
        B: IntoLuaValue,
        C: IntoLuaValue,
        D: IntoLuaValue,
        E: IntoLuaValue,
        F: IntoLuaValue,
        G: IntoLuaValue,
        H: IntoLuaValue,
    > PushValue for (A, B, C, D, E, F, G, H)
{
    fn push_value(self, lua: &mlua::Lua) -> Result<MultiValue, mlua::Error> {
        Ok(MultiValue::from_vec(vec![
            A::into_lua_value(&self.0, lua)?,
            B::into_lua_value(&self.1, lua)?,
            C::into_lua_value(&self.2, lua)?,
            D::into_lua_value(&self.3, lua)?,
            E::into_lua_value(&self.4, lua)?,
            F::into_lua_value(&self.5, lua)?,
            G::into_lua_value(&self.6, lua)?,
            H::into_lua_value(&self.7, lua)?,
        ]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Known C `%.14g` spellings (LUA_NUMBER_FMT of the 5.1 C API).
    #[test]
    fn lua_number2str_matches_c_g14() {
        assert_eq!(lua_number2str(1e20), "1e+20");
        assert_eq!(lua_number2str(0.5), "0.5");
        assert_eq!(lua_number2str(-2.5), "-2.5");
        assert_eq!(lua_number2str(123.456), "123.456");
        assert_eq!(lua_number2str(0.0), "0");
        assert_eq!(lua_number2str(0.0012345), "0.0012345");
        assert_eq!(lua_number2str(0.0001), "1e-04");
        assert_eq!(lua_number2str(1234567.89), "1234567.89");
        assert_eq!(lua_number2str(42.0), "42");
        assert_eq!(lua_number2str(-0.0), "-0");
        assert_eq!(lua_number2str(f64::INFINITY), "inf");
        assert_eq!(lua_number2str(f64::NEG_INFINITY), "-inf");
        assert_eq!(lua_number2str(f64::NAN), "nan");
    }
}
