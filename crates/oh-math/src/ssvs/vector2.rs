//! Port of `extlibs/SSVStart/include/SSVStart/Utils/Vector2.hpp` — only the
//! helpers the game actually uses (issue 001 task spec §3):
//! `getOrbitRad`, `getVecFromRad`, `getNormalized`, `getMagSquared`,
//! `getRad`, `getDistEuclidean`, `zeroVec2f` (+ their dependencies
//! `getMag` and `ssvu::getDistSquaredEuclidean`).
//!
//! `Vec2` mirrors `sf::Vector2f` usage in the game: a plain pair of `f32`
//! components named `x` / `y`.
//!
//! Arithmetic fidelity (D-000): expressions are copied verbatim.

use std::ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Sub, SubAssign};

/// `sf::Vector2f` analogue: two `f32` components.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

impl Vec2 {
    /// `sf::Vector2f(x, y)` ctor.
    #[must_use]
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

/// `ssvs::zeroVec2f` — `static Vec2f{0.f, 0.f}`.
pub const ZERO_VEC: Vec2 = Vec2 { x: 0.0, y: 0.0 };

/// `getMagSquared` — `mVec.x * mVec.x + mVec.y * mVec.y`.
#[must_use]
pub fn get_mag_squared(m_vec: &Vec2) -> f32 {
    m_vec.x * m_vec.x + m_vec.y * m_vec.y
}

/// `getMag` — `std::sqrt(getMagSquared(mVec))`.
#[must_use]
pub fn get_mag(m_vec: &Vec2) -> f32 {
    get_mag_squared(m_vec).sqrt()
}

/// `normalize` (no known-magnitude) — in place; internally checks that the
/// magnitude is not 0.
pub fn normalize(m_vec: &mut Vec2) {
    let mag = get_mag(m_vec);
    if mag != 0.0 {
        m_vec.x /= mag;
        m_vec.y /= mag;
    }
}

/// `getNormalized` — a normalized copy of the original vector (does not
/// modify it); internally checks that the magnitude is not 0.
#[must_use]
pub fn get_normalized(m_vec: Vec2) -> Vec2 {
    let mut m_vec = m_vec;
    normalize(&mut m_vec);
    m_vec
}

/// `getRad` — `std::atan2(mVec.y, mVec.x)`.
#[must_use]
pub fn get_rad(m_vec: &Vec2) -> f32 {
    f32::atan2(m_vec.y, m_vec.x)
}

/// `getVecFromRad(mRad, mMag = 1.f)` —
/// `Vec2(mMag * cos(mRad), mMag * sin(mRad))`.
#[must_use]
pub fn get_vec_from_rad(m_rad: f32, m_mag: f32) -> Vec2 {
    Vec2::new(m_mag * m_rad.cos(), m_mag * m_rad.sin())
}

/// `getOrbitRad` — `Vec2(mVec) + getVecFromRad(mRad, mRadius)`.
#[must_use]
pub fn get_orbit_rad(m_vec: &Vec2, m_rad: f32, m_radius: f32) -> Vec2 {
    *m_vec + get_vec_from_rad(m_rad, m_radius)
}

/// `ssvs::getDistSquaredEuclidean` — forwards to `ssvu::getDistSquaredEuclidean`.
#[must_use]
pub fn get_dist_squared_euclidean(m_a: &Vec2, m_b: &Vec2) -> f32 {
    crate::ssvu::math::get_dist_squared_euclidean(m_a.x, m_a.y, m_b.x, m_b.y)
}

/// `ssvs::getDistEuclidean` — forwards to `ssvu::getDistEuclidean`.
#[must_use]
pub fn get_dist_euclidean(m_a: &Vec2, m_b: &Vec2) -> f32 {
    crate::ssvu::math::get_dist_euclidean(m_a.x, m_a.y, m_b.x, m_b.y)
}

// `sf::Vector2f` arithmetic used by the helpers above (and by the game).

impl Add for Vec2 {
    type Output = Vec2;
    fn add(self, other: Vec2) -> Vec2 {
        Vec2::new(self.x + other.x, self.y + other.y)
    }
}

impl Sub for Vec2 {
    type Output = Vec2;
    fn sub(self, other: Vec2) -> Vec2 {
        Vec2::new(self.x - other.x, self.y - other.y)
    }
}

impl Mul<f32> for Vec2 {
    type Output = Vec2;
    fn mul(self, scalar: f32) -> Vec2 {
        Vec2::new(self.x * scalar, self.y * scalar)
    }
}

impl Div<f32> for Vec2 {
    type Output = Vec2;
    fn div(self, scalar: f32) -> Vec2 {
        Vec2::new(self.x / scalar, self.y / scalar)
    }
}

impl AddAssign for Vec2 {
    fn add_assign(&mut self, other: Vec2) {
        self.x += other.x;
        self.y += other.y;
    }
}

impl SubAssign for Vec2 {
    fn sub_assign(&mut self, other: Vec2) {
        self.x -= other.x;
        self.y -= other.y;
    }
}

impl MulAssign<f32> for Vec2 {
    fn mul_assign(&mut self, scalar: f32) {
        self.x *= scalar;
        self.y *= scalar;
    }
}

impl DivAssign<f32> for Vec2 {
    fn div_assign(&mut self, scalar: f32) {
        self.x /= scalar;
        self.y /= scalar;
    }
}
