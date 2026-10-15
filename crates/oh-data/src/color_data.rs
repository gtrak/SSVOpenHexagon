//! Port of `include/SSVOpenHexagon/Data/ColorData.hpp` +
//! `src/SSVOpenHexagon/Data/ColorData.cpp`.

use oh_math::hg::color::Color;

use crate::json::{self, JsonError, Obj};

/// C++ `hg::PulseColor` — `int r, g, b, a` (NO in-struct defaults).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PulseColor {
    pub r: i32,
    pub g: i32,
    pub b: i32,
    pub a: i32,
}

impl PulseColor {
    #[must_use = "mirrors the C++ [[nodiscard]]"]
    pub const fn new(r: i32, g: i32, b: i32, a: i32) -> Self {
        Self { r, g, b, a }
    }
}

/// C++ `hg::ColorData` (field order = C++ declaration order).
///
/// C++ `ColorData() = default`: booleans/floats zero, `color = sf::Color{}`
/// (0,0,0,255), `pulse` = `{0,0,0,0}` (indeterminate in C++ — only
/// JSON-constructed instances are used; the port pins the zero value).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ColorData {
    pub main: bool,
    pub dynamic: bool,
    pub dynamic_offset: bool,
    pub dynamic_darkness: f32,
    pub hue_shift: f32,
    pub offset: f32,
    pub color: Color,
    pub pulse: PulseColor,
}

impl Default for ColorData {
    /// C++ `ColorData()` (see struct doc).
    fn default() -> Self {
        Self {
            main: false,
            dynamic: false,
            dynamic_offset: false,
            dynamic_darkness: 0.0,
            hue_shift: 0.0,
            offset: 0.0,
            color: Color::default(),
            pulse: PulseColor::new(0, 0, 0, 0),
        }
    }
}

impl ColorData {
    /// C++ 8-arg constructor (used by `parseCapColor`).
    // `8/7` args: the C++ signature is verbatim (D-002).
    #[allow(clippy::too_many_arguments)]
    #[must_use = "mirrors the C++ [[nodiscard]]"]
    pub const fn new(
        m_main: bool,
        m_dynamic: bool,
        m_dynamic_offset: bool,
        m_dynamic_darkness: f32,
        m_hue_shift: f32,
        m_offset: f32,
        m_color: Color,
        m_pulse: PulseColor,
    ) -> Self {
        Self {
            main: m_main,
            dynamic: m_dynamic,
            dynamic_offset: m_dynamic_offset,
            dynamic_darkness: m_dynamic_darkness,
            hue_shift: m_hue_shift,
            offset: m_offset,
            color: m_color,
            pulse: m_pulse,
        }
    }

    /// C++ `ColorData(mRoot)` — JSON keys and defaults (mapping §2.2):
    /// `"main"` (false), `"dynamic"` (false), `"dynamic_offset"` (false),
    /// `"dynamic_darkness"` (1.f), `"hue_shift"` (0.f), `"offset"` (0.f),
    /// `"value"` (`sf::Color` = Black), `"pulse"` (via `pulse_from_json`).
    ///
    /// Type mismatches → `Err` (the fork throws; see `json` module).
    pub fn from_json(m_root: &Obj) -> Result<Self, JsonError> {
        Ok(Self {
            main: json::extr_bool(m_root, "main", false)?,
            dynamic: json::extr_bool(m_root, "dynamic", false)?,
            dynamic_offset: json::extr_bool(m_root, "dynamic_offset", false)?,
            dynamic_darkness: json::extr_float(m_root, "dynamic_darkness", 1.0)? as f32,
            hue_shift: json::extr_float(m_root, "hue_shift", 0.0)? as f32,
            offset: json::extr_float(m_root, "offset", 0.0)? as f32,
            color: if json::has(m_root, "value") {
                json::extr_color(json::get_missing_as_null(m_root, "value"))?
            } else {
                Color::BLACK
            },
            pulse: pulse_from_json(m_root)?,
        })
    }
}

/// C++ `hg::pulse_from_json(root)`:
/// - no `"pulse"` key → `{0, 0, 0, 255}`;
/// - else array indices 0..3 as `int` (missing index → null → 0; a
///   non-array value reads as null at every index → `{0, 0, 0, 0}`).
#[must_use = "mirrors the C++ [[nodiscard]]"]
pub fn pulse_from_json(root: &Obj) -> Result<PulseColor, JsonError> {
    if !json::has(root, "pulse") {
        return Ok(PulseColor::new(0, 0, 0, 255));
    }
    let pulse_obj = json::get_missing_as_null(root, "pulse");
    Ok(PulseColor {
        r: json::as_int(json::get_at_or_null(pulse_obj, 0))? as i32,
        g: json::as_int(json::get_at_or_null(pulse_obj, 1))? as i32,
        b: json::as_int(json::get_at_or_null(pulse_obj, 2))? as i32,
        a: json::as_int(json::get_at_or_null(pulse_obj, 3))? as i32,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pulse_missing_defaults_to_255_alpha() {
        let obj: Obj = serde_json::from_str("{}").unwrap();
        assert_eq!(
            pulse_from_json(&obj).unwrap(),
            PulseColor::new(0, 0, 0, 255)
        );
    }

    #[test]
    fn pulse_reads_array_indices() {
        let obj: Obj =
            serde_json::from_str(r#"{"pulse": [1, 2, 3, 4], "dynamic_offset": true}"#).unwrap();
        let cd = ColorData::from_json(&obj).unwrap();
        assert_eq!(cd.pulse, PulseColor::new(1, 2, 3, 4));
        assert!(cd.dynamic_offset);
        assert_eq!(cd.dynamic_darkness, 1.0f32);
    }
}
