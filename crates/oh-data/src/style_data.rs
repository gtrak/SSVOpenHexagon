//! Port of `include/SSVOpenHexagon/Data/StyleData.hpp` +
//! `src/SSVOpenHexagon/Data/StyleData.cpp`.
//!
//! In scope: every field (C++ order + defaults), the JSON constructor,
//! `update`, `computeColors`, `calculateColor`, `setCapColor` and the
//! getters. The background draw methods (`drawBackgroundImpl` /
//! `drawBackgroundMenuHexagonImpl` and their public wrappers) need the
//! `FastVertexVectorTris` render-vertex buffer — issue 11's concern.

use oh_math::hg::color::{component_clamp, get_color_darkened, get_color_from_hue, Color};
use oh_math::ssvu::math::get_mod;

use crate::cap_color::{parse_cap_color, CapColor};
use crate::color_data::ColorData;
use crate::json::{self, JsonError, Obj};

/// C++ `hg::StyleData`. Public field order = C++ public declaration order;
/// the C++ private fields keep their (snake_cased) names below.
#[derive(Debug, Clone, PartialEq)]
pub struct StyleData {
    // ----- public (C++ order) -------------------------------------------
    pub id: String,
    pub hue_min: f32,
    pub hue_max: f32,
    pub hue_increment: f32,
    pub hue_ping_pong: bool,

    pub pulse_min: f32,
    pub pulse_max: f32,
    pub pulse_increment: f32,
    pub max_swap_time: f32,

    pub _3d_depth: f32,
    pub _3d_skew: f32,
    pub _3d_spacing: f32,
    pub _3d_darken_mult: f32,
    pub _3d_alpha_mult: f32,
    pub _3d_alpha_falloff: f32,
    pub _3d_pulse_max: f32,
    pub _3d_pulse_min: f32,
    pub _3d_pulse_speed: f32,
    pub _3d_perspective_mult: f32,

    pub bg_tile_radius: f32,
    pub bg_color_offset: u32,
    pub bg_rot_off: f32, // In degrees

    // ----- private (C++ order) ------------------------------------------
    current_hue: f32,
    current_swap_time: f32,
    pulse_factor: f32,
    current_main_color: Color,
    current_player_color: Color,
    current_text_color: Color,
    current_wall_color: Color,
    current_3d_override_color: Color,
    current_colors: Vec<Color>,

    _3d_override_color: Color,
    main_color_data: ColorData,
    player_color: ColorData,
    text_color: ColorData,
    wall_color: ColorData,

    cap_color: CapColor,

    color_datas: Vec<ColorData>,
}

impl Default for StyleData {
    /// C++ `StyleData() = default` — in-class defaults; `capColor` defaults
    /// to the tinyvariant's FIRST alternative (`Main`).
    fn default() -> Self {
        Self {
            id: String::new(),
            hue_min: 0.0,
            hue_max: 0.0,
            hue_increment: 0.0,
            hue_ping_pong: false,
            pulse_min: 0.0,
            pulse_max: 0.0,
            pulse_increment: 0.0,
            max_swap_time: 0.0,
            _3d_depth: 0.0,
            _3d_skew: 0.0,
            _3d_spacing: 0.0,
            _3d_darken_mult: 0.0,
            _3d_alpha_mult: 0.0,
            _3d_alpha_falloff: 0.0,
            _3d_pulse_max: 0.0,
            _3d_pulse_min: 0.0,
            _3d_pulse_speed: 0.0,
            _3d_perspective_mult: 0.0,
            bg_tile_radius: 10000.0,
            bg_color_offset: 0,
            bg_rot_off: 0.0,
            current_hue: 0.0,
            current_swap_time: 0.0,
            pulse_factor: 0.0,
            current_main_color: Color::BLACK,
            current_player_color: Color::BLACK,
            current_text_color: Color::BLACK,
            current_wall_color: Color::WHITE,
            current_3d_override_color: Color::BLACK,
            current_colors: Vec::new(),
            _3d_override_color: Color::default(), // sf::Color{} = (0,0,0,255)
            main_color_data: ColorData::default(),
            player_color: ColorData::default(),
            text_color: ColorData::default(),
            wall_color: ColorData::default(),
            cap_color: CapColor::Main,
            color_datas: Vec::new(),
        }
    }
}

impl StyleData {
    /// C++ `StyleData(mRoot)` — exact keys and defaults (mapping §2.9).
    /// Note: `"main"` has **no hasObj check** in the C++ (a missing key
    /// yields the all-defaults ColorData via the null singleton);
    /// `"player_color"`/`"text_color"`/`"wall_color"` fall back to the main
    /// color data.
    pub fn from_json(m_root: &Obj) -> Result<Self, JsonError> {
        let main_color_data = ColorData::from_json(json::get_missing_as_null(m_root, "main"))?;
        let player_color =
            color_data_from_obj_or_default(m_root, "player_color", &main_color_data)?;
        let text_color = color_data_from_obj_or_default(m_root, "text_color", &main_color_data)?;
        let wall_color = color_data_from_obj_or_default(m_root, "wall_color", &main_color_data)?;
        let cap_color = parse_cap_color(json::get_missing_as_null(m_root, "cap_color"))?;

        let obj_colors = json::get_missing_as_null(m_root, "colors");
        let color_count = json::size(obj_colors);
        let mut color_datas = Vec::with_capacity(color_count);
        for i in 0..color_count {
            color_datas.push(ColorData::from_json(json::get_at_or_null(obj_colors, i))?);
        }

        let hue_min = json::extr_float(m_root, "hue_min", 0.0)? as f32;

        Ok(Self {
            id: json::extr_str(m_root, "id", "nullId")?,
            hue_min,
            hue_max: json::extr_float(m_root, "hue_max", 360.0)? as f32,
            hue_increment: json::extr_float(m_root, "hue_increment", 0.0)? as f32,
            hue_ping_pong: json::extr_bool(m_root, "hue_ping_pong", false)?,
            pulse_min: json::extr_float(m_root, "pulse_min", 0.0)? as f32,
            pulse_max: json::extr_float(m_root, "pulse_max", 0.0)? as f32,
            pulse_increment: json::extr_float(m_root, "pulse_increment", 0.0)? as f32,
            max_swap_time: json::extr_float(m_root, "max_swap_time", 100.0)? as f32,
            _3d_depth: json::extr_float(m_root, "3D_depth", 15.0)? as f32,
            _3d_skew: json::extr_float(m_root, "3D_skew", 0.18)? as f32,
            _3d_spacing: json::extr_float(m_root, "3D_spacing", 1.0)? as f32,
            _3d_darken_mult: json::extr_float(m_root, "3D_darken_multiplier", 1.5)? as f32,
            _3d_alpha_mult: json::extr_float(m_root, "3D_alpha_multiplier", 0.5)? as f32,
            _3d_alpha_falloff: json::extr_float(m_root, "3D_alpha_falloff", 3.0)? as f32,
            _3d_pulse_max: json::extr_float(m_root, "3D_pulse_max", 3.2)? as f32,
            _3d_pulse_min: json::extr_float(m_root, "3D_pulse_min", 0.0)? as f32,
            _3d_pulse_speed: json::extr_float(m_root, "3D_pulse_speed", 0.01)? as f32,
            _3d_perspective_mult: json::extr_float(m_root, "3D_perspective_multiplier", 1.0)?
                as f32,
            _3d_override_color: if json::has(m_root, "3D_override_color") {
                json::extr_color(json::get_missing_as_null(m_root, "3D_override_color"))?
            } else {
                Color::TRANSPARENT
            },
            bg_tile_radius: 10000.0,
            bg_color_offset: 0,
            bg_rot_off: 0.0,
            current_hue: hue_min,
            current_swap_time: 0.0,
            pulse_factor: 0.0,
            current_main_color: Color::BLACK,
            current_player_color: Color::BLACK,
            current_text_color: Color::BLACK,
            current_wall_color: Color::WHITE,
            current_3d_override_color: Color::BLACK,
            current_colors: Vec::new(),
            main_color_data,
            player_color,
            text_color,
            wall_color,
            cap_color,
            color_datas,
        })
    }

    /// C++ `calculateColor(currentHue, pulseFactor, colorData)` (static):
    /// `color = cd.color`; if `cd.dynamic`: `h = fmod(hue + cd.hueShift,
    /// 360)/360`, `dyn = getColorFromHue(h)`; if `!cd.main`: if
    /// `cd.dynamicOffset`: if `cd.offset != 0` each channel `+= dyn.c /
    /// offset`, `color.a += dyn.a`; else `color =
    /// getColorDarkened(dyn, cd.dynamicDarkness)`; else (main): `color =
    /// dyn`. Final: each channel =
    /// `componentClamp(color.c + cd.pulse.c * pulseFactor)`.
    fn calculate_color(m_current_hue: f32, m_pulse_factor: f32, m_color_data: &ColorData) -> Color {
        let mut color = m_color_data.color;

        if m_color_data.dynamic {
            // C++ `std::fmod(hue + shift, 360.f)` — fmod keeps the sign of
            // the dividend (truncate division), not rem_euclid.
            let total = m_current_hue + m_color_data.hue_shift;
            let hue = (total - total / 360.0 * 360.0) / 360.0;

            let dynamic_color = get_color_from_hue(hue);

            if !m_color_data.main {
                if m_color_data.dynamic_offset {
                    if m_color_data.offset != 0.0 {
                        // C++: `color.r += dynamicColor.r / offset;` — u8
                        // lvalue, float rvalue: truncate + mod-256 wrap.
                        color.r =
                            wrap_u8(color.r as f32 + dynamic_color.r as f32 / m_color_data.offset);
                        color.g =
                            wrap_u8(color.g as f32 + dynamic_color.g as f32 / m_color_data.offset);
                        color.b =
                            wrap_u8(color.b as f32 + dynamic_color.b as f32 / m_color_data.offset);
                    }

                    color.a = wrap_u8(color.a as f32 + dynamic_color.a as f32);
                } else {
                    color = get_color_darkened(dynamic_color, m_color_data.dynamic_darkness);
                }
            } else {
                color = dynamic_color;
            }
        }

        Color {
            r: component_clamp(color.r as f32 + m_color_data.pulse.r as f32 * m_pulse_factor),
            g: component_clamp(color.g as f32 + m_color_data.pulse.g as f32 * m_pulse_factor),
            b: component_clamp(color.b as f32 + m_color_data.pulse.b as f32 * m_pulse_factor),
            a: component_clamp(color.a as f32 + m_color_data.pulse.a as f32 * m_pulse_factor),
        }
    }

    /// C++ `update(FT mFT, float mMult = 1.f)` — verbatim control flow
    /// (note: `pulseFactor += pulseIncrement * mFT` — **no** `mult`).
    pub fn update(&mut self, m_ft: f32, m_mult: f32) {
        self.current_swap_time += m_ft * m_mult;
        if self.current_swap_time > self.max_swap_time {
            self.current_swap_time = 0.0;
        }

        self.current_hue += self.hue_increment * m_ft * m_mult;

        if self.current_hue < self.hue_min {
            if self.hue_ping_pong {
                self.current_hue = self.hue_min;
                self.hue_increment *= -1.0;
            } else {
                self.current_hue = self.hue_max;
            }
        } else if self.current_hue > self.hue_max {
            if self.hue_ping_pong {
                self.current_hue = self.hue_max;
                self.hue_increment *= -1.0;
            } else {
                self.current_hue = self.hue_min;
            }
        }

        self.pulse_factor += self.pulse_increment * m_ft;

        if self.pulse_factor < self.pulse_min {
            self.pulse_increment *= -1.0;
            self.pulse_factor = self.pulse_min;
        } else if self.pulse_factor > self.pulse_max {
            self.pulse_increment *= -1.0;
            self.pulse_factor = self.pulse_max;
        }
    }

    /// C++ `computeColors()` — recomputes the four current colors, the 3D
    /// override, and `currentColors` (rotated by the swap time when there is
    /// more than one color).
    pub fn compute_colors(&mut self) {
        self.current_main_color =
            Self::calculate_color(self.current_hue, self.pulse_factor, &self.main_color_data);
        self.current_player_color =
            Self::calculate_color(self.current_hue, self.pulse_factor, &self.player_color);
        self.current_text_color =
            Self::calculate_color(self.current_hue, self.pulse_factor, &self.text_color);
        self.current_wall_color =
            Self::calculate_color(self.current_hue, self.pulse_factor, &self.wall_color);

        self.current_3d_override_color = if self._3d_override_color.a != 0 {
            self._3d_override_color
        } else {
            self.current_main_color
        };

        self.current_colors.clear();

        for cd in &self.color_datas {
            self.current_colors.push(Self::calculate_color(
                self.current_hue,
                self.pulse_factor,
                cd,
            ));
        }

        if self.current_colors.len() > 1 {
            // C++: `const unsigned int rotation = currentSwapTime /
            // (maxSwapTime / 2.f);` — float-to-unsigned truncation, then
            // `ssvu::getMod` over unsigned ints.
            let rotation = (self.current_swap_time / (self.max_swap_time / 2.0)) as u32;

            // C++ promotes to `size_t` for `getMod(…, currentColors.size())`.
            let by = get_mod(
                (rotation + self.bg_color_offset) as usize,
                self.current_colors.len(),
            );
            self.current_colors.rotate_left(by);
        }
    }

    /// C++ `setCapColor`.
    pub fn set_cap_color(&mut self, m_cap_color: &CapColor) {
        self.cap_color = m_cap_color.clone();
    }

    #[must_use = "mirrors the C++ [[nodiscard]]"]
    pub fn get_main_color(&self) -> &Color {
        &self.current_main_color
    }

    #[must_use = "mirrors the C++ [[nodiscard]]"]
    pub fn get_player_color(&self) -> &Color {
        &self.current_player_color
    }

    #[must_use = "mirrors the C++ [[nodiscard]]"]
    pub fn get_text_color(&self) -> &Color {
        &self.current_text_color
    }

    #[must_use = "mirrors the C++ [[nodiscard]]"]
    pub fn get_wall_color(&self) -> &Color {
        &self.current_wall_color
    }

    #[must_use = "mirrors the C++ [[nodiscard]]"]
    pub fn get_colors(&self) -> &[Color] {
        &self.current_colors
    }

    /// C++ `getColor(idx)` — mod-indexed into `currentColors` (asserts
    /// non-empty in C++; port: `Option`).
    #[must_use = "mirrors the C++ [[nodiscard]]"]
    pub fn get_color(&self, m_idx: u32) -> Option<&Color> {
        if self.current_colors.is_empty() {
            return None;
        }
        Some(&self.current_colors[m_idx as usize % self.current_colors.len()])
    }

    #[must_use = "mirrors the C++ [[nodiscard]]"]
    pub fn get_current_hue(&self) -> f32 {
        self.current_hue
    }

    #[must_use = "mirrors the C++ [[nodiscard]]"]
    pub fn get_current_swap_time(&self) -> f32 {
        self.current_swap_time
    }

    #[must_use = "mirrors the C++ [[nodiscard]]"]
    pub fn get_3d_override_color(&self) -> &Color {
        &self.current_3d_override_color
    }

    /// C++ `getCapColorResult()` (noexcept; the `ByIndex` branch asserts a
    /// non-empty `currentColors` in C++ → `None` in the port).
    #[must_use = "mirrors the C++ [[nodiscard]]"]
    pub fn get_cap_color_result(&self) -> Option<Color> {
        match &self.cap_color {
            CapColor::Main => Some(*self.get_main_color()),
            CapColor::MainDarkened => Some(get_color_darkened(*self.get_main_color(), 1.4)),
            CapColor::ByIndex { index } => Some(*self.get_color(*index as u32)?),
            CapColor::ColorData(data) => Some(Self::calculate_color(
                self.current_hue,
                self.pulse_factor,
                data,
            )),
        }
    }

    #[must_use = "mirrors the C++ [[nodiscard]]"]
    pub fn get_cap_color(&self) -> &CapColor {
        &self.cap_color
    }

    #[must_use = "mirrors the C++ [[nodiscard]]"]
    pub fn get_color_datas(&self) -> &[ColorData] {
        &self.color_datas
    }

    #[must_use = "mirrors the C++ [[nodiscard]]"]
    pub fn get_main_color_data(&self) -> &ColorData {
        &self.main_color_data
    }

    #[must_use = "mirrors the C++ [[nodiscard]]"]
    pub fn get_player_color_data(&self) -> &ColorData {
        &self.player_color
    }

    #[must_use = "mirrors the C++ [[nodiscard]]"]
    pub fn get_text_color_data(&self) -> &ColorData {
        &self.text_color
    }

    #[must_use = "mirrors the C++ [[nodiscard]]"]
    pub fn get_wall_color_data(&self) -> &ColorData {
        &self.wall_color
    }
}

/// C++ `StyleData::colorDataFromObjOrDefault(root, key, def)`:
/// `hasObj(root, key) ? ColorData{getObj(root, key)} : def`.
fn color_data_from_obj_or_default(
    m_root: &Obj,
    m_key: &str,
    m_default: &ColorData,
) -> Result<ColorData, JsonError> {
    if json::has(m_root, m_key) {
        Ok(ColorData::from_json(json::get_missing_as_null(
            m_root, m_key,
        ))?)
    } else {
        Ok(*m_default)
    }
}

/// C++ `static_cast<uint8_t>(float)` (x86-64): truncate toward zero, then
/// mod 256 (mirrors `u8_from_f32` in `oh-math`).
const fn wrap_u8(x: f32) -> u8 {
    (x as i32).rem_euclid(256) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_defaults_are_the_cpp_defaults() {
        let obj: Obj = serde_json::from_str(r#"{}"#).unwrap();
        let s = StyleData::from_json(&obj).unwrap();
        assert_eq!(s.id, "nullId");
        assert_eq!(s.hue_max, 360.0f32);
        assert_eq!(s.max_swap_time, 100.0f32);
        assert_eq!(s._3d_depth, 15.0f32);
        assert_eq!(s._3d_skew, 0.18f32);
        assert_eq!(s._3d_darken_mult, 1.5f32);
        assert_eq!(s._3d_pulse_max, 3.2f32);
        assert_eq!(s._3d_pulse_speed, 0.01f32);
        assert_eq!(s._3d_override_color, Color::TRANSPARENT);
        assert_eq!(s.bg_tile_radius, 10000.0f32);
        // missing "main" → all-defaults ColorData (Black value, pulse 255).
        assert_eq!(s.main_color_data.color, Color::BLACK);
        assert_eq!(s.main_color_data.pulse.a, 255);
        // player/text/wall fall back to the main color data.
        assert_eq!(s.player_color, s.main_color_data);
        assert_eq!(s.text_color, s.main_color_data);
        assert_eq!(s.wall_color, s.main_color_data);
        assert_eq!(s.cap_color, CapColor::ByIndex { index: 0 });
        assert_eq!(s.color_datas.len(), 0);
    }

    #[test]
    fn update_wraps_swap_time_and_bounces_hue() {
        let obj: Obj = serde_json::from_str(
            r#"{"hue_min": 0, "hue_max": 10, "hue_increment": 5, "hue_ping_pong": true}"#,
        )
        .unwrap();
        let mut s = StyleData::from_json(&obj).unwrap();
        s.update(1.0, 1.0); // hue 0 → 5
        assert_eq!(s.get_current_hue(), 5.0);
        s.update(3.0, 1.0); // hue 5 → 20 > 10 → ping-pong: 10, inc -5
        assert_eq!(s.get_current_hue(), 10.0);
        assert_eq!(s.hue_increment, -5.0);
        s.update(1.0, 0.0); // mult 0: no movement
        assert_eq!(s.get_current_hue(), 10.0);
    }

    #[test]
    fn compute_colors_rotates_by_swap_time() {
        let obj: Obj = serde_json::from_str(
            r#"{"colors": [
                {"value": [255, 0, 0, 255]},
                {"value": [0, 255, 0, 255]},
                {"value": [0, 0, 255, 255]}
            ]}"#,
        )
        .unwrap();
        let mut s = StyleData::from_json(&obj).unwrap();
        s.compute_colors();
        // max_swap_time default 100 → currentSwapTime 0 → rotation 0.
        let colors = s.get_colors().to_vec();
        assert_eq!(colors[0], Color::new(255, 0, 0, 255));
        assert_eq!(colors[1], Color::new(0, 255, 0, 255));
        assert_eq!(colors[2], Color::new(0, 0, 255, 255));
    }

    #[test]
    fn cap_color_result_variants() {
        let obj: Obj = serde_json::from_str(
            r#"{"main": {"value": [10, 20, 30, 255]}, "cap_color": "main_darkened"}"#,
        )
        .unwrap();
        let mut s = StyleData::from_json(&obj).unwrap();
        s.compute_colors();
        assert_eq!(s.get_main_color(), &Color::new(10, 20, 30, 255));
        let cap = s.get_cap_color_result().unwrap();
        assert_eq!(cap, get_color_darkened(Color::new(10, 20, 30, 255), 1.4));
    }
}
