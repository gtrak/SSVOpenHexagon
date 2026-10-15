//! Port of `include/SSVOpenHexagon/Utils/Geometry.hpp` +
//! `src/SSVOpenHexagon/Utils/Geometry.cpp` — the line/circle intersection
//! helpers the game uses (`CPlayer`, `MenuGame`), formulas verbatim.

use crate::ssvs::vector2::get_mag_squared;
use crate::ssvs::Vec2;

/// `hg::Utils::epsilon` — `inline constexpr float{1.0e-4}`.
pub const EPSILON: f32 = 1.0e-4;

/// `hg::Utils::getLineCircleIntersection` — finds the intersection(s) of
/// line `p1 → p2` with the circle of squared radius `mRadiusSquared`
/// (centered at the origin).
///
/// Returns the number of intersections (0, 1 or 2); writes them to `i1`
/// (always when ≥ 1) and `i2` (only when 2) — mirroring the C++ out-params.
#[must_use]
pub fn get_line_circle_intersection(
    i1: &mut Vec2,
    i2: &mut Vec2,
    p1: &Vec2,
    p2: &Vec2,
    m_radius_squared: f32,
) -> u32 {
    let dx = p2.x - p1.x;
    let dy = p2.y - p1.y;
    let a = dx * dx + dy * dy;
    let b = 2.0f32 * (dx * p1.x + dy * p1.y);
    let c = p1.x * p1.x + p1.y * p1.y - m_radius_squared;
    let delta = b * b - 4.0f32 * a * c;

    // No intersections.
    if delta < 0.0f32 {
        return 0;
    }

    let mut t;
    let two_a = 2.0f32 * a;

    // One intersection.
    if delta < EPSILON {
        t = -b / two_a;
        *i1 = Vec2::new(p1.x + t * dx, p1.y + t * dy);
        return 1;
    }

    // Two intersections.
    let sqrt_delta = delta.sqrt();
    t = (-b + sqrt_delta) / two_a;
    *i1 = Vec2::new(p1.x + t * dx, p1.y + t * dy);
    t = (-b - sqrt_delta) / two_a;
    *i2 = Vec2::new(p1.x + t * dx, p1.y + t * dy);
    2
}

/// `hg::Utils::getLineCircleClosestIntersection` — like
/// `get_line_circle_intersection`, but picks the intersection closest to
/// `mPos`; returns `false` when there are no intersections.
#[must_use]
pub fn get_line_circle_closest_intersection(
    m_intersection: &mut Vec2,
    m_pos: &Vec2,
    p1: &Vec2,
    p2: &Vec2,
    m_radius_squared: f32,
) -> bool {
    let mut v1 = Vec2::default();
    let mut v2 = Vec2::default();

    match get_line_circle_intersection(&mut v1, &mut v2, p1, p2, m_radius_squared) {
        1 => {
            *m_intersection = v1;
            true
        }
        2 => {
            // C++: `if(ssvs::getMagSquared(v1 - mPos) > ssvs::getMagSquared(v2 - mPos))`
            *m_intersection = if get_mag_squared(&(v1 - *m_pos)) > get_mag_squared(&(v2 - *m_pos)) {
                v2
            } else {
                v1
            };
            true
        }
        _ => false,
    }
}
