//! Port of `include/SSVOpenHexagon/Data/PackData.hpp` and
//! `include/SSVOpenHexagon/Data/PackInfo.hpp`.
//!
//! JSON keys/defaults for `pack.json` are applied by the asset loader
//! (`HGAssetsImpl::loadPackData`, mapping §4.2 — issue 05); this port
//! provides a `from_json` helper with those exact defaults so the loader
//! and tests can share it.

use crate::json::{self, JsonError, Obj};

/// C++ `hg::PackDependency` (no defaults).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackDependency {
    pub disambiguator: String,
    pub name: String,
    pub author: String,
    pub min_version: i32,
}

impl PackDependency {
    #[must_use = "mirrors the C++ [[nodiscard]]"]
    pub fn new(disambiguator: &str, name: &str, author: &str, min_version: i32) -> Self {
        Self {
            disambiguator: disambiguator.to_string(),
            name: name.to_string(),
            author: author.to_string(),
            min_version,
        }
    }
}

/// C++ `hg::PackData` (field order = C++ declaration order; no defaults —
/// filled by `loadPackData`).
#[derive(Debug, Clone, PartialEq)]
pub struct PackData {
    /// Pack folder path WITH trailing slash (C++ `Path::normalize()`).
    pub folder_path: String,
    /// `Utils::buildPackId(...)` (§7.2).
    pub id: String,
    pub disambiguator: String,
    pub name: String,
    pub author: String,
    pub description: String,
    pub version: i32,
    pub priority: f32,
    pub dependencies: Vec<PackDependency>,
}

/// C++ `loadPackData` pack.json field reads (mapping §4.2):
/// `"disambiguator"` ("no disambiguator"), `"name"` ("unknown name"),
/// `"author"` ("unknown author"), `"description"` ("no description"),
/// `"version"` (0), `"priority"` (100), and — only when the
/// `"dependencies"` key is present — an array of objects with
/// `"disambiguator"`, `"name"`, `"author"` (no defaults → "") and
/// `"min_version"` (no default → 0).
///
/// `m_pack_path` is the pack folder path; the stored `folder_path` is the
/// normalized form WITH the trailing slash (C++ `Path` behavior).
#[must_use = "mirrors the C++ [[nodiscard]]"]
pub fn pack_data_from_json(
    m_root: &Obj,
    m_pack_path: &str,
    m_pack_id: &str,
) -> Result<PackData, JsonError> {
    let disambiguator = json::extr_str(m_root, "disambiguator", "no disambiguator")?;
    let name = json::extr_str(m_root, "name", "unknown name")?;
    let author = json::extr_str(m_root, "author", "unknown author")?;
    let description = json::extr_str(m_root, "description", "no description")?;
    let version = json::extr_int(m_root, "version", 0)? as i32;
    let priority = json::extr_float(m_root, "priority", 100.0)? as f32;

    let mut dependencies = Vec::new();
    if json::has(m_root, "dependencies") {
        let deps = json::get_missing_as_null(m_root, "dependencies");
        for i in 0..json::size(deps) {
            let dep = json::get_at_or_null(deps, i);
            dependencies.push(PackDependency {
                disambiguator: json::extr_str(dep, "disambiguator", "")?,
                name: json::extr_str(dep, "name", "")?,
                author: json::extr_str(dep, "author", "")?,
                min_version: json::extr_int(dep, "min_version", 0)? as i32,
            });
        }
    }

    let folder_path = if m_pack_path.ends_with('/') {
        m_pack_path.to_string()
    } else {
        format!("{m_pack_path}/")
    };

    Ok(PackData {
        folder_path,
        id: m_pack_id.to_string(),
        disambiguator,
        name,
        author,
        description,
        version,
        priority,
        dependencies,
    })
}
