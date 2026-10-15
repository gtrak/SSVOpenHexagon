//! Port of `extlibs/SSVUtils/include/SSVUtils/Core/Common/Frametime.hpp`.

/// `ssvu::FT` — alias for `float` intended for frametime-related contexts
/// (Frametime.hpp: "It can be read both as 'frametime' and 'float time'").
pub type FT = f32;

/// `ssvu::secondsFTRatio` — default constexpr ratio between seconds and
/// frametime (60 fps).
pub const SECONDS_FT_RATIO: f32 = 60.0;

/// `getFTToSeconds` — converts frametime to seconds: `mFT / secondsFTRatio`.
///
/// C++ is a `constexpr` template over `T`; the game uses it with `float`
/// (FT), so the f32 form is ported (see report: unused-type note).
#[must_use]
pub const fn get_ft_to_seconds(m_ft: f32) -> f32 {
    m_ft / SECONDS_FT_RATIO
}

/// `getSecondsToFT` — converts seconds to frametime: `mSeconds * secondsFTRatio`.
#[must_use]
pub const fn get_seconds_to_ft(m_seconds: f32) -> f32 {
    m_seconds * SECONDS_FT_RATIO
}

/// `getFTToFPS` — converts frametime to frames per second:
/// `secondsFTRatio / mFT`.
#[must_use]
pub const fn get_ft_to_fps(m_ft: f32) -> f32 {
    SECONDS_FT_RATIO / m_ft
}
