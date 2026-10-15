//! `oh-data` — port of SSVOpenHexagon's `Data/*` structs and the
//! `SSVUtilsJson` / `LoadFromJson` JSON loading stack.
//!
//! One Rust module per C++ file (plan 001, ground rule 1; D-000/D-002):
//!
//! - `json` — the `ssvuj` layer (pinned-JsonCpp-fork semantics on a
//!   `serde_json` backend; comment tolerance per mapping §1.6);
//! - `color_data` / `cap_color` — `Data/ColorData` / `Data/CapColor`;
//! - `level_data`, `level_status`, `load_info`, `music_data`,
//!   `pack_data`, `pack_info`, `profile_data`, `style_data` —
//!   `Data/*`;
//! - `load_from_json` — `Utils/LoadFromJson`;
//! - `utils` — `Utils/LevelValidator` + `Utils/BuildPackId` string formats;
//! - `linked_value` — `SSVUtilsJson/LinkedValue` (consumed by `Config`,
//!   issue 04);
//! - `version` — `Global/Version` (`GameVersion`, `GAME_VERSION`).
//!
//! Arithmetic fidelity: every C++ `float` is `f32`, every `double` is
//! `f64`, and no floating-point expression is reordered (D-000).

pub mod cap_color;
pub mod color_data;
pub mod json;
pub mod level_data;
pub mod level_status;
pub mod linked_value;
pub mod load_from_json;
pub mod load_info;
pub mod music_data;
pub mod pack_data;
pub mod pack_info;
pub mod profile_data;
pub mod style_data;
pub mod utils;
pub mod version;

pub use cap_color::{parse_cap_color, CapColor};
pub use color_data::{ColorData, PulseColor};
pub use level_data::LevelData;
pub use level_status::LevelStatus;
pub use linked_value::{JsonConvertible, LinkedValue};
pub use load_from_json::{load_music_from_json, load_profile_from_json, load_version_from_json};
pub use load_info::LoadInfo;
pub use music_data::{MusicData, Segment};
pub use pack_data::{pack_data_from_json, PackData, PackDependency};
pub use pack_info::PackInfo;
pub use profile_data::{profile_to_obj, ProfileData};
pub use style_data::StyleData;
pub use utils::{build_pack_id, get_level_validator};
pub use version::{GameVersion, GAME_VERSION, GAME_VERSION_STR};
