//! Port of `include/SSVOpenHexagon/Utils/Easing.hpp` — every curve, verbatim.

/// `hg::Utils::getSaturated` — clamps to `[0, 1]`.
///
/// C++ (Easing.hpp) checks `mValue < 0.f` then `mValue > 1.f` and returns
/// the input unchanged otherwise; on the pinned toolchain `f32::clamp`
/// matches that exactly, including `NaN -> NaN`.
#[must_use]
pub fn get_saturated(m_value: f32) -> f32 {
    m_value.clamp(0.0f32, 1.0f32)
}

/// `hg::Utils::getSmoothStep` — formulas verbatim.
#[must_use]
pub fn get_smooth_step(edge0: f32, edge1: f32, x: f32) -> f32 {
    let mut x = get_saturated((x - edge0) / (edge1 - edge0));
    x = x * x * (3.0f32 - 2.0f32 * x);
    x
}

/// `hg::Utils::getSmootherStep` — formulas verbatim.
#[must_use]
pub fn get_smoother_step(edge0: f32, edge1: f32, x: f32) -> f32 {
    let mut x = get_saturated((x - edge0) / (edge1 - edge0));
    x = x * x * x * (x * (x * 6.0f32 - 15.0f32) + 10.0f32);
    x
}
