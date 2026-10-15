//! Port of `include/SSVOpenHexagon/Utils/Color.hpp` — `sf::Color` and the
//! hue math the game uses: `transformHue`, `getColorFromHue`,
//! `getColorDarkened`, `componentClamp`.
//!
//! Arithmetic fidelity (D-000): every formula is copied verbatim, constants
//! spelled exactly as in the C++ source.
//!
//! NOTE (residual risk): the pinned SFML fork
//! (`vittorioromeo/SFML@dacceddb`) is not vendored in this tree, so its
//! `sf::Color(float, float, float, float)` ctor cannot be read directly.
//! The game always passes *pre-scaled* channels (`r * 255.f`, …) into it,
//! so the ctor must truncate to `u8` without re-scaling (a `* 255.f`
//! ctor, as in upstream SFML, would wrap the values and break rendering).
//! This assumption should be re-verified against the fork source when issue
//! 10 (render backend) lands.

/// `sf::Color` analogue (pinned fork): four `u8` channels; the default
/// ctor leaves `r/g/b` at 0 and `a` at 255.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Default for Color {
    /// C++ `Color()`: `r{0}, g{0}, b{0}, a{255}`.
    fn default() -> Self {
        Self {
            r: 0,
            g: 0,
            b: 0,
            a: 255,
        }
    }
}

impl Color {
    /// `sf::Color(r, g, b, a)` (u8 ctor).
    #[must_use]
    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    /// `sf::Color(r, g, b)` — u8 ctor with the default `a = 255`.
    #[must_use]
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    /// `sf::Color(float, float, float, float)` — each channel truncated to
    /// `u8` (see module note on the pinned-fork assumption). The game's
    /// pre-scaled call sites pass e.g. `r * 255.f` with the alpha default
    /// (255), i.e. `a` as an already-scaled 0..255 float.
    #[must_use]
    pub fn from_floats(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self {
            r: u8_from_f32(r),
            g: u8_from_f32(g),
            b: u8_from_f32(b),
            a: u8_from_f32(a),
        }
    }

    pub const BLACK: Self = Self::new(0, 0, 0, 255);
    pub const WHITE: Self = Self::new(255, 255, 255, 255);
    pub const RED: Self = Self::new(255, 0, 0, 255);
    pub const GREEN: Self = Self::new(0, 255, 0, 255);
    pub const BLUE: Self = Self::new(0, 0, 255, 255);
    pub const TRANSPARENT: Self = Self::new(0, 0, 0, 0);
}

/// C++ `static_cast<std::uint8_t>(x)` on an `f32`, matching the x86-64 GCC
/// behavior the C++ build relies on: truncate toward zero; out-of-range
/// values wrap modulo 256 (two's complement); NaN / overflowing values
/// saturate in `cvttss2si` to `INT_MIN`, whose low byte is 0.
///
/// Rust's `f32 as i32` (truncate, saturate on NaN/overflow) plus
/// `rem_euclid(256)` reproduces exactly that.
const fn u8_from_f32(x: f32) -> u8 {
    (x as i32).rem_euclid(256) as u8
}

/// `hg::Utils::transformHue` — rotates the color's hue by `H` degrees via
/// the C++ source's fixed matrices (formulas verbatim).
///
/// The C++ literal is the deliberate approximation `3.14f` (Color.hpp:33,
/// `std::cos(H * 3.14f / 180.f)`); `std::f32::consts::PI` would change the
/// hue-matrix values and break 1:1 parity (D-000).
#[allow(clippy::approx_constant)]
#[must_use]
pub fn transform_hue(m_in: &Color, m_h: f32) -> Color {
    // C++: `std::cos(H * 3.14f / 180.f)` / `std::sin(H * 3.14f / 180.f)` —
    // the same expression is evaluated for both; hoisting it into a local
    // changes no evaluation order.
    let angle = m_h * 3.14f32 / 180.0f32;
    let u = angle.cos();
    let w = angle.sin();

    Color {
        r: u8_from_f32(
            (0.701f32 * u + 0.168f32 * w) * m_in.r as f32
                + (-0.587f32 * u + 0.330f32 * w) * m_in.g as f32
                + (-0.114f32 * u - 0.497f32 * w) * m_in.b as f32,
        ),
        g: u8_from_f32(
            (-0.299f32 * u - 0.328f32 * w) * m_in.r as f32
                + (0.413f32 * u + 0.035f32 * w) * m_in.g as f32
                + (-0.114f32 * u + 0.292f32 * w) * m_in.b as f32,
        ),
        b: u8_from_f32(
            (-0.3f32 * u + 1.25f32 * w) * m_in.r as f32
                + (-0.588f32 * u - 1.05f32 * w) * m_in.g as f32
                + (0.886f32 * u - 0.203f32 * w) * m_in.b as f32,
        ),
        // C++: `static_cast<std::uint8_t>(255)` — a is unchanged.
        a: 255,
    }
}

/// `hg::Utils::getColorFromHue` — `hue` in degrees; the C++ `switch`
/// (cases 0..4, everything else → `ret(1.f, 0.f, q)`), formulas verbatim.
#[must_use]
pub fn get_color_from_hue(hue: f32) -> Color {
    // C++: `const int i = std::floor(hue * 6.f);`
    let i = (hue * 6.0f32).floor() as i32;
    // C++: `const float f = (hue * 6.f) - i;`
    let f = (hue * 6.0f32) - i as f32;
    let q = 1.0f32 - f;
    let t = f;

    // C++ `ret` lambda: `sf::Color(r * 255.f, g * 255.f, b * 255.f)`
    // — channels pre-scaled then truncated to `u8` (see module note), and
    // `a` left at the ctor default (255).
    let ret = |r: f32, g: f32, b: f32| {
        Color::from_floats(r * 255.0f32, g * 255.0f32, b * 255.0f32, 255.0f32)
    };

    match i {
        0 => ret(1.0f32, t, 0.0f32),
        1 => ret(q, 1.0f32, 0.0f32),
        2 => ret(0.0f32, 1.0f32, t),
        3 => ret(0.0f32, q, 1.0f32),
        4 => ret(t, 0.0f32, 1.0f32),
        // C++: the return after the `switch` (i == 5 or out of range).
        _ => ret(1.0f32, 0.0f32, q),
    }
}

/// `hg::Utils::getColorDarkened` — divides `r`, `b`, `g` (each independently,
/// in this order) by `mMultiplier` unless it is 0.
#[must_use]
pub fn get_color_darkened(mut m_color: Color, m_multiplier: f32) -> Color {
    if m_multiplier != 0.0f32 {
        m_color.r = u8_from_f32(m_color.r as f32 / m_multiplier);
        m_color.b = u8_from_f32(m_color.b as f32 / m_multiplier);
        m_color.g = u8_from_f32(m_color.g as f32 / m_multiplier);
    }

    m_color
}

/// `hg::Utils::componentClamp` — `> 255.f` → 255, `< 0.f` → 0, otherwise
/// truncated to `u8`.
#[must_use]
pub const fn component_clamp(value: f32) -> u8 {
    if value > 255.0f32 {
        255
    } else if value < 0.0f32 {
        0
    } else {
        value as u8
    }
}
