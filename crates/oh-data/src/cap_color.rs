//! Port of `include/SSVOpenHexagon/Data/CapColor.hpp` +
//! `src/SSVOpenHexagon/Data/CapColor.cpp`.

use oh_math::hg::color::Color;

use crate::color_data::ColorData;
use crate::json::{self, JsonError, Obj};

/// C++ `CapColor` =
/// `vittorioromeo::tinyvariant<Main, MainDarkened, ByIndex, ColorData>` —
/// the same four alternatives in the same order.
#[derive(Debug, Clone, PartialEq)]
pub enum CapColor {
    /// C++ `CapColorMode::Main{}`.
    Main,
    /// C++ `CapColorMode::MainDarkened{}`.
    MainDarkened,
    /// C++ `CapColorMode::ByIndex{int _index}`.
    ByIndex { index: i32 },
    /// C++ alternative `ColorData`.
    ColorData(ColorData),
}

/// C++ `hg::parseCapColor(obj)` — exact mode precedence (CapColor.cpp):
///
/// 1. JSON string `"main"` → `Main`; `"main_darkened"` → `MainDarkened`;
///    any other string falls through;
/// 2. JSON object → `legacy` (bool, default **true**):
///    - `true` → `ByIndex { index }`, `index = getExtr<int>(obj, "index", 0)`;
///    - `false` → `ColorData` built with `main` hardwired to `false`
///      (unlike `ColorData(mRoot)`, which reads `"main"`), reading
///      `"dynamic"` (false), `"dynamic_offset"` (false),
///      `"dynamic_darkness"` (1.f), `"hue_shift"` (0.f), `"offset"` (0.f),
///      `"value"` (White) and `"pulse"` (`pulse_from_json`).
/// 3. Fallback (null / number / array / unknown string) → `ByIndex { 0 }`.
///
/// The C++ function is `noexcept` but its `getExtr` calls can throw; the
/// port makes the mismatch surface as `Err` instead (mapping §1.4).
#[must_use = "mirrors the C++ [[nodiscard]]"]
pub fn parse_cap_color(obj: &Obj) -> Result<CapColor, JsonError> {
    if json::is_obj_type_str(obj) {
        let str = json::as_str(obj)?;
        if str == "main" {
            return Ok(CapColor::Main);
        }
        if str == "main_darkened" {
            return Ok(CapColor::MainDarkened);
        }
    }

    if json::is_obj(obj) {
        let legacy = json::extr_bool(obj, "legacy", true)?;
        if legacy {
            let index = json::extr_int(obj, "index", 0)? as i32;
            return Ok(CapColor::ByIndex { index });
        }
        return Ok(CapColor::ColorData(ColorData::new(
            false,
            json::extr_bool(obj, "dynamic", false)?,
            json::extr_bool(obj, "dynamic_offset", false)?,
            json::extr_float(obj, "dynamic_darkness", 1.0)? as f32,
            json::extr_float(obj, "hue_shift", 0.0)? as f32,
            json::extr_float(obj, "offset", 0.0)? as f32,
            if json::has(obj, "value") {
                json::extr_color(json::get_missing_as_null(obj, "value"))?
            } else {
                Color::WHITE
            },
            crate::color_data::pulse_from_json(obj)?,
        )));
    }

    // Fallback case:
    Ok(CapColor::ByIndex { index: 0 })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color_data::PulseColor;

    #[test]
    fn string_modes() {
        let main: Obj = serde_json::from_str(r#""main""#).unwrap();
        assert_eq!(parse_cap_color(&main).unwrap(), CapColor::Main);
        let dark: Obj = serde_json::from_str(r#""main_darkened""#).unwrap();
        assert_eq!(parse_cap_color(&dark).unwrap(), CapColor::MainDarkened);
        let other: Obj = serde_json::from_str(r#""bogus""#).unwrap();
        assert_eq!(
            parse_cap_color(&other).unwrap(),
            CapColor::ByIndex { index: 0 }
        );
    }

    #[test]
    fn legacy_object_uses_index() {
        let obj: Obj = serde_json::from_str(r#"{"index": 3}"#).unwrap();
        assert_eq!(
            parse_cap_color(&obj).unwrap(),
            CapColor::ByIndex { index: 3 }
        );
        // `legacy` default is true even when other color keys are present.
        let obj: Obj = serde_json::from_str(r#"{"dynamic": true, "value": [1, 2, 3, 4]}"#).unwrap();
        assert_eq!(
            parse_cap_color(&obj).unwrap(),
            CapColor::ByIndex { index: 0 }
        );
    }

    #[test]
    fn non_legacy_object_builds_color_data() {
        let obj: Obj = serde_json::from_str(
            r#"{"legacy": false, "dynamic": true, "dynamic_offset": true,
                "dynamic_darkness": 2.5, "hue_shift": 10, "offset": 0.5,
                "value": [9, 8, 7, 6], "pulse": [1, 2, 3, 4]}"#,
        )
        .unwrap();
        match parse_cap_color(&obj).unwrap() {
            CapColor::ColorData(cd) => {
                assert!(!cd.main);
                assert!(cd.dynamic);
                assert!(cd.dynamic_offset);
                assert_eq!(cd.dynamic_darkness, 2.5f32);
                assert_eq!(cd.hue_shift, 10.0f32);
                assert_eq!(cd.offset, 0.5f32);
                assert_eq!(cd.color, Color::new(9, 8, 7, 6));
                assert_eq!(cd.pulse, PulseColor::new(1, 2, 3, 4));
            }
            other => panic!("expected ColorData, got {other:?}"),
        }
    }
}
