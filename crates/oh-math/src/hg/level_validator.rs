//! Port of `include/SSVOpenHexagon/Utils/LevelValidator.hpp` +
//! `src/SSVOpenHexagon/Utils/LevelValidator.cpp` — verbatim:
//! `concat(levelId, "_m_", diffMult)` (f32 through the C++ ostream
//! formatting).

use crate::hg::concat::{concat, ConcatPart};

/// `hg::Utils::getLevelValidator` — `concat(levelId, "_m_", diffMult)`.
#[must_use]
pub fn get_level_validator(level_id: &str, diff_mult: f32) -> String {
    concat(&[
        ConcatPart::Str(level_id),
        ConcatPart::Str("_m_"),
        ConcatPart::F32(diff_mult),
    ])
}
