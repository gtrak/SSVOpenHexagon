//! Port of `include/SSVOpenHexagon/Data/PackInfo.hpp`.

/// C++ `hg::PackInfo` (no defaults).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackInfo {
    pub id: String,
    pub path: String,
}

impl PackInfo {
    #[must_use = "mirrors the C++ [[nodiscard]]"]
    pub fn new(id: &str, path: &str) -> Self {
        Self {
            id: id.to_string(),
            path: path.to_string(),
        }
    }
}
