//! Port of `extlibs/SSVUtils/include/SSVUtils/Core/Utils/Math.hpp` — only the
//! helpers the game actually uses (issue 001 task spec §3). Unused SSVUtils
//! math (getLerp, getMap, getWrapRad, toDeg, distance variants, index
//! arithmetic, …) is NOT ported; see the issue report "known gaps".
//!
//! Arithmetic fidelity (D-000): constants are copied verbatim and no
//! floating-point expression is reordered.

use std::ops::{Add, AddAssign, Div, Mul, Rem, Sub};

/// `ssvu::pi` — `constexpr float{3.14159265359f}`.
///
/// Parity (D-000): byte-identical to Math.hpp:18. `std::f32::consts::PI`
/// is a different value and must not be substituted.
#[allow(clippy::approx_constant, clippy::excessive_precision)]
pub const PI: f32 = 3.14159265359f32;

/// `ssvu::tau` — `constexpr float{6.28318530718f}`.
///
/// Parity (D-000): byte-identical to Math.hpp:24. `std::f32::consts::TAU`
/// is a different value and must not be substituted.
#[allow(clippy::approx_constant, clippy::excessive_precision)]
pub const TAU: f32 = 6.28318530718f32;

/// `ssvu::radDegRatio` — `constexpr float{0.01745329251f}`.
///
/// Parity (D-000): byte-identical to Math.hpp:27; the full-precision
/// decimal literal must stay as written in the C++ source.
#[allow(clippy::excessive_precision)]
pub const RAD_DEG_RATIO: f32 = 0.01745329251f32;

/// `toRad` — `mX * radDegRatio`.
///
/// C++ is a template; the game uses it with `float` only.
#[must_use]
pub const fn to_rad(m_x: f32) -> f32 {
    m_x * RAD_DEG_RATIO
}

/// `getSign` — C++ dispatches on signedness:
/// - signed: `(0 < mX) - (mX < 0)`
/// - unsigned: `0 < mX`
///
/// This single generic form is the signed expression, with the C++ `0`
/// literals materialized via `T::from(0)`; for an unsigned Rust type the
/// `m_x < zero` comparison is always false, so it degenerates exactly to
/// the unsigned overload. Returns `1` if > 0, `-1` if < 0, `0` if == 0.
#[must_use]
pub fn get_sign<T>(m_x: T) -> i32
where
    T: PartialOrd + Copy + From<u8>,
{
    let zero = T::from(0u8);
    (zero < m_x) as i32 - (m_x < zero) as i32
}

/// `clamp` — in-place clamp (`assert(mMin <= mMax)` in C++).
pub fn clamp<T: PartialOrd + Copy>(m_x: &mut T, m_min: T, m_max: T) {
    debug_assert!(m_min <= m_max);
    // C++: `if(mX < mMin) mX = mMin; else if(mX > mMax) mX = mMax;`
    if *m_x < m_min {
        *m_x = m_min;
    } else if *m_x > m_max {
        *m_x = m_max;
    }
}

/// `getClamped` — returns the clamped value without modifying it
/// (`assert(mMin <= mMax)` in C++).
///
/// C++ ternary: `CT(mX) < CT(mMin) ? CT(mMin) : (CT(mX) > CT(mMax) ? CT(mMax) : CT(mX))`.
#[must_use]
pub fn get_clamped<T: PartialOrd + Copy>(m_x: T, m_min: T, m_max: T) -> T {
    debug_assert!(m_min <= m_max);
    if m_x < m_min {
        m_min
    } else if m_x > m_max {
        m_max
    } else {
        m_x
    }
}

/// `getMod(mVal, mUB)` — mathematically correct modulo; assumes `mUB > 0`.
///
/// C++: `auto result(((mVal % mUB) + mUB) % mUB);` (copied verbatim).
#[must_use]
pub fn get_mod<T>(m_val: T, m_ub: T) -> T
where
    T: Copy + PartialOrd + From<u8> + Add<Output = T> + Rem<Output = T>,
{
    let zero = T::from(0u8);
    debug_assert!(m_ub > zero);
    let result = m_val.rem(m_ub) + m_ub;
    result.rem(m_ub)
}

/// `getMod(mVal, mLB, mUB)` — wraps a value into `[mLB, mUB)`; assumes
/// `mLB < mUB`.
///
/// C++ verbatim:
/// ```cpp
/// const auto& rangeSize(mUB - mLB);
/// if(mVal < mLB) mVal += rangeSize * ((mLB - mVal) / rangeSize + 1);
/// return mLB + ((mVal - mLB) % rangeSize);
/// ```
#[must_use]
pub fn get_mod_range<T>(m_val: T, m_lb: T, m_ub: T) -> T
where
    T: Copy
        + PartialOrd
        + From<u8>
        + Add<Output = T>
        + AddAssign
        + Sub<Output = T>
        + Mul<Output = T>
        + Div<Output = T>
        + Rem<Output = T>,
{
    let zero = T::from(0u8);
    let one = T::from(1u8);
    debug_assert!(m_lb < m_ub);
    let range_size = m_ub - m_lb;
    debug_assert!(range_size > zero);
    let mut m_val = m_val;
    if m_val < m_lb {
        m_val += range_size * ((m_lb - m_val) / range_size + one);
    }
    m_lb + ((m_val - m_lb) % range_size)
}

/// `getDistSquaredEuclidean` — dependency of `ssvs::getDistEuclidean`
/// (also used directly by the game through SSVStart's Vec2 overload).
#[must_use]
pub fn get_dist_squared_euclidean<T>(m_x1: T, m_y1: T, m_x2: T, m_y2: T) -> T
where
    T: Copy + Add<Output = T> + Sub<Output = T> + Mul<Output = T>,
{
    (m_x1 - m_x2) * (m_x1 - m_x2) + (m_y1 - m_y2) * (m_y1 - m_y2)
}

/// `getDistEuclidean` — `std::sqrt(getDistSquaredEuclidean(...))`.
///
/// C++ is a template; the game uses it with `float` only (via SSVStart's
/// Vec2 overload).
#[must_use]
pub fn get_dist_euclidean(m_x1: f32, m_y1: f32, m_x2: f32, m_y2: f32) -> f32 {
    get_dist_squared_euclidean(m_x1, m_y1, m_x2, m_y2).sqrt()
}
