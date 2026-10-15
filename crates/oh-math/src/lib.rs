//! Pure math helpers for the SSVOpenHexagon Rust port.
//!
//! One Rust module per C++ file, same names (snake_case), same
//! responsibilities (plan 001, ground rule 1; D-000/D-002):
//!
//! - `ssvu` — SSVUtils Core subset (`extlibs/SSVUtils/include/SSVUtils/Core/**`)
//! - `ssvs` — SSVStart subset (`extlibs/SSVStart/include/SSVStart/**`)
//! - `hg` — SSVOpenHexagon Utils subset (`include/SSVOpenHexagon/Utils/**`)
//!
//! Arithmetic fidelity: every C++ `float` is `f32`, every `double` is
//! `f64`, and no floating-point expression is reordered.

pub mod hg;
pub mod ssvs;
pub mod ssvu;

pub use ssvs::vector2::Vec2;
pub use ssvu::frametime::FT;
