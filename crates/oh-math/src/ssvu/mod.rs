//! SSVUtils Core subset (`extlibs/SSVUtils/include/SSVUtils/Core/**`),
//! folded into `oh-math` per DECISIONS.md D-002.

pub mod containers;
pub mod frametime;
pub mod math;

pub use frametime::get_ft_to_seconds;
pub use math::{get_clamped, get_sign, PI};
