//! Port of `include/SSVOpenHexagon/Global/Version.hpp`.

/// C++ `hg::GameVersion` — `{int major; int minor; int micro;}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd)]
pub struct GameVersion {
    pub major: i32,
    pub minor: i32,
    pub micro: i32,
}

impl GameVersion {
    #[must_use = "mirrors the C++ [[nodiscard]]"]
    pub const fn new(major: i32, minor: i32, micro: i32) -> Self {
        Self {
            major,
            minor,
            micro,
        }
    }
}

/// C++ `GAME_VERSION` = `{2, 1, 7}`.
pub const GAME_VERSION: GameVersion = GameVersion::new(2, 1, 7);

/// C++ `GAME_VERSION_STR` = `"2.1.7"`.
pub const GAME_VERSION_STR: &str = "2.1.7";
