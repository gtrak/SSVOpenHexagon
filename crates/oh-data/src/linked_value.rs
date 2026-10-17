//! Port of `include/SSVOpenHexagon/SSVUtilsJson/LinkedValue/LinkedValue.hpp`
//! + `src/SSVOpenHexagon/SSVUtilsJson/LinkedValue.cpp`.
//!
//! `ssvuj::LinkedValue<T>`: a named value bound to a JSON key, with a
//! compile-time default. Every config value (issue 04) is one of these.
//!
//! C++ semantics (mapping §1.9):
//! - `syncFrom(obj)`: `value = getExtr(obj, name, defValue)` — the key
//!   absent → the default; present → converted (type mismatch throws in
//!   C++; here: `Err`);
//! - `syncTo(obj)`: `arch(obj, name, value)`;
//! - `resetToDefault()`: `value = defValue`.
//!
//! The C++ class is a template with explicit instantiations for `bool`,
//! `int`, `float`, `unsigned int`, `unsigned short`, `std::string`,
//! `std::vector<std::string>` and `ssvs::Input::Trigger`; the port uses a
//! `JsonConvertible` trait instead (the Trigger converter is issue 04's).

use crate::json::{self, JsonError, Obj};

/// JSON conversion bound for `LinkedValue<T>` (the C++ `Converter<T>`).
pub trait JsonConvertible {
    /// C++ `Converter<T>::fromObj` — convert a JSON value (null → the
    /// zero value; wrong type → `Err`).
    fn from_obj(obj: &Obj) -> Result<Self, JsonError>
    where
        Self: Sized;
    /// C++ `Converter<T>::toObj` — archive as a JSON value.
    fn to_obj(&self) -> Obj;
}

impl JsonConvertible for bool {
    fn from_obj(obj: &Obj) -> Result<Self, JsonError> {
        json::as_bool(obj)
    }
    fn to_obj(&self) -> Obj {
        json::get_arch_bool(*self)
    }
}

impl JsonConvertible for i32 {
    fn from_obj(obj: &Obj) -> Result<Self, JsonError> {
        // C++ `int` → `asInt()` (the fork's 32-bit range asserts now live
        // in `as_int`, so the narrowing cast is infallible).
        Ok(json::as_int(obj)? as i32)
    }
    fn to_obj(&self) -> Obj {
        json::get_arch_int(*self as i64)
    }
}

impl JsonConvertible for u32 {
    fn from_obj(obj: &Obj) -> Result<Self, JsonError> {
        // C++ `unsigned int` → `asUInt()` (null → 0, bool → 1/0, the
        // 32-bit range asserts).
        json::as_uint(obj)
    }
    fn to_obj(&self) -> Obj {
        json::get_arch_int(*self as i64)
    }
}

impl JsonConvertible for u16 {
    fn from_obj(obj: &Obj) -> Result<Self, JsonError> {
        // C++ `unsigned short` → `mValue = mObj.asUInt()` — the 32-bit
        // `asUInt` read, then the implicit narrowing to `unsigned short`.
        Ok(json::as_uint(obj)? as u16)
    }
    fn to_obj(&self) -> Obj {
        json::get_arch_int(*self as i64)
    }
}

/// C++ `float` archives as a JSON **double** (hence `9.45f` is stored as
/// `9.449999809265137` in the shipped `config.json`).
impl JsonConvertible for f32 {
    fn from_obj(obj: &Obj) -> Result<Self, JsonError> {
        Ok(json::as_float(obj)? as f32)
    }
    fn to_obj(&self) -> Obj {
        json::get_arch_f32(*self)
    }
}

impl JsonConvertible for String {
    fn from_obj(obj: &Obj) -> Result<Self, JsonError> {
        // C++ `std::string` → `asString()` (the fork's cross-type
        // coercions: bool/int/uint/real → string, jsoncpp.inl:1177-1190).
        json::as_str(obj)
    }
    fn to_obj(&self) -> Obj {
        json::get_arch_str(self)
    }
}

impl JsonConvertible for Vec<String> {
    fn from_obj(obj: &Obj) -> Result<Self, JsonError> {
        json::extr_vec_str(obj)
    }
    fn to_obj(&self) -> Obj {
        json::get_arch_vec_str(self)
    }
}

/// C++ `ssvuj::LinkedValue<T>`.
#[derive(Clone)]
pub struct LinkedValue<T: JsonConvertible> {
    name: &'static str,
    value: T,
    def_value: T,
}

impl<T: JsonConvertible + Clone> LinkedValue<T> {
    /// C++ `LinkedValue(name, defaultValue)`.
    #[must_use = "mirrors the C++ [[nodiscard]]"]
    pub fn new(name: &'static str, m_default: T) -> Self {
        Self {
            name,
            value: m_default.clone(),
            def_value: m_default,
        }
    }

    /// C++ `syncFrom(const Obj&)` — `getExtr(obj, name, defValue)`.
    pub fn sync_from(&mut self, m_obj: &Obj) -> Result<(), JsonError> {
        if json::has(m_obj, self.name) {
            let v = json::get_missing_as_null(m_obj, self.name);
            self.value = T::from_obj(v)?;
        } else {
            self.value = self.def_value.clone();
        }
        Ok(())
    }

    /// C++ `syncTo(Obj&) const` — `arch(obj, name, value)`.
    pub fn sync_to(&self, m_obj: &mut Obj) {
        json::arch(m_obj, self.name, self.value.to_obj());
    }

    /// C++ `resetToDefault()`.
    pub fn reset_to_default(&mut self) {
        self.value = self.def_value.clone();
    }

    /// C++ `getDefault()`.
    #[must_use = "mirrors the C++ [[nodiscard]]"]
    pub fn get_default(&self) -> &T {
        &self.def_value
    }

    /// C++ implicit conversion to `const T&`.
    #[must_use = "mirrors the C++ [[nodiscard]]"]
    pub fn get(&self) -> &T {
        &self.value
    }

    /// C++ implicit conversion to `T&` (the `LinkedValue(name()).getCombos()`
    /// mutation path, e.g. `Config::getTrigger`).
    pub fn get_mut(&mut self) -> &mut T {
        &mut self.value
    }

    /// C++ `operator=` from `T`.
    pub fn set(&mut self, value: T) {
        self.value = value;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sync_from_missing_key_keeps_default() {
        let mut lv = LinkedValue::new("official", true);
        let obj: Obj = serde_json::from_str(r#"{}"#).unwrap();
        lv.sync_from(&obj).unwrap();
        assert!(*lv.get());

        let mut lv = LinkedValue::new("sound_volume", 100.0f32);
        lv.sync_from(&obj).unwrap();
        assert_eq!(*lv.get(), 100.0f32);
    }

    #[test]
    fn sync_from_present_key_overrides_and_sync_to_writes() {
        let mut lv = LinkedValue::new("sound_volume", 100.0f32);
        let obj: Obj = serde_json::from_str(r#"{"sound_volume": 55.5}"#).unwrap();
        lv.sync_from(&obj).unwrap();
        assert_eq!(*lv.get(), 55.5f32);

        let mut root = Obj::Object(serde_json::Map::new());
        lv.sync_to(&mut root);
        assert_eq!(
            root.get("sound_volume").and_then(serde_json::Value::as_f64),
            Some(55.5f32 as f64)
        );
        lv.reset_to_default();
        assert_eq!(*lv.get(), 100.0f32);
    }

    #[test]
    fn type_mismatch_is_an_error() {
        let mut lv = LinkedValue::new("official", true);
        let obj: Obj = serde_json::from_str(r#"{"official": "yes"}"#).unwrap();
        assert!(lv.sync_from(&obj).is_err());
        assert!(*lv.get());
    }
}
