//! Port of `include/SSVOpenHexagon/Data/ProfileData.hpp` +
//! `src/SSVOpenHexagon/Data/ProfileData.cpp`.
//!
//! Write format (C++ `saveAllProfiles`, mapping §4.7): `ssvuj::arch` of
//! `version` (object with `major`/`minor`/`micro` = GAME_VERSION), `name`,
//! `scores` (object: level asset id → float) and `favorites` (array of
//! level asset ids), then `StyledStreamWriter` to
//! `"Profiles/" + name + ".json"`. Object keys come out byte-sorted
//! (fork member storage = `std::map`).

use crate::json;
use crate::json::Obj;
use crate::version::GameVersion;

/// C++ `hg::ProfileData`.
///
/// C++ `std::unordered_map`/`unordered_set` are ported as `BTreeMap`/
/// `BTreeSet` — unordered iteration order is unspecified in C++, and the
/// JSON output is byte-sorted regardless (fork writer), so the ordered
/// containers reproduce every observable behavior.
#[derive(Debug, Clone, PartialEq)]
pub struct ProfileData {
    version: GameVersion,
    name: String,
    scores: std::collections::BTreeMap<String, f32>,
    favorite_levels_data_ids: std::collections::BTreeSet<String>,
}

impl ProfileData {
    /// C++ constructor `(mVersion, mName, mScores, mFavorites)` —
    /// favorites come in as a `Vec<String>` and are converted to a set
    /// (`Utils::toUnorderedSet`).
    #[must_use = "mirrors the C++ [[nodiscard]]"]
    pub fn new(
        m_version: GameVersion,
        m_name: &str,
        m_scores: &std::collections::BTreeMap<String, f32>,
        m_favorites: &[String],
    ) -> Self {
        Self {
            version: m_version,
            name: m_name.to_string(),
            scores: m_scores.clone(),
            favorite_levels_data_ids: m_favorites.iter().cloned().collect(),
        }
    }

    #[must_use = "mirrors the C++ [[nodiscard]]"]
    pub fn get_version(&self) -> GameVersion {
        self.version
    }

    #[must_use = "mirrors the C++ [[nodiscard]]"]
    pub fn get_name(&self) -> &str {
        &self.name
    }

    #[must_use = "mirrors the C++ [[nodiscard]]"]
    pub fn get_scores(&self) -> &std::collections::BTreeMap<String, f32> {
        &self.scores
    }

    /// C++ non-const `getFavoriteLevelIds()`.
    pub fn get_favorite_level_ids_mut(&mut self) -> &mut std::collections::BTreeSet<String> {
        &mut self.favorite_levels_data_ids
    }

    #[must_use = "mirrors the C++ [[nodiscard]]"]
    pub fn get_favorite_level_ids(&self) -> &std::collections::BTreeSet<String> {
        &self.favorite_levels_data_ids
    }

    pub fn set_score(&mut self, m_id: &str, m_score: f32) {
        self.scores.insert(m_id.to_string(), m_score);
    }

    /// C++ `getScore` → **0.0 if missing**.
    #[must_use = "mirrors the C++ [[nodiscard]]"]
    pub fn get_score(&self, m_id: &str) -> f32 {
        self.scores.get(m_id).copied().unwrap_or(0.0)
    }

    pub fn add_favorite_level(&mut self, m_level_id: &str) {
        self.favorite_levels_data_ids.insert(m_level_id.to_string());
    }

    pub fn remove_favorite_level(&mut self, m_level_id: &str) {
        self.favorite_levels_data_ids.remove(m_level_id);
    }

    #[must_use = "mirrors the C++ [[nodiscard]]"]
    pub fn is_level_favorite(&self, m_level_id: &str) -> bool {
        self.favorite_levels_data_ids.contains(m_level_id)
    }
}

/// C++ profile save shape (`saveCurrentLocalProfile` / `saveAllProfiles`):
///
/// ```text
/// {
///     "favorites" : [ "<levelAssetId>", ... ],
///     "name" : "<profile name>",
///     "scores" : { "<levelAssetId>" : <float>, ... },
///     "version" : { "major" : 2, "micro" : 7, "minor" : 1 },
/// }
/// ```
///
/// (top-level keys byte-sorted; the `version` object itself is also
/// byte-sorted, hence `major`, `micro`, `minor`).
#[must_use = "mirrors the C++ [[nodiscard]]"]
pub fn profile_to_obj(profile: &ProfileData) -> Obj {
    let mut obj = Obj::Object(serde_json::Map::new());
    json::arch(
        &mut obj,
        "version",
        serde_json::json!({
            "major": profile.get_version().major,
            "minor": profile.get_version().minor,
            "micro": profile.get_version().micro,
        }),
    );
    json::arch(&mut obj, "name", json::get_arch_str(profile.get_name()));
    let mut scores = Obj::Object(serde_json::Map::new());
    for (id, score) in profile.get_scores() {
        json::arch(&mut scores, id, json::get_arch_f32(*score));
    }
    json::arch(&mut obj, "scores", scores);
    json::arch(
        &mut obj,
        "favorites",
        json::get_arch_vec_str(
            &profile
                .get_favorite_level_ids()
                .iter()
                .cloned()
                .collect::<Vec<_>>(),
        ),
    );
    obj
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn score_defaults_to_zero_when_missing() {
        let p = ProfileData::new(
            GameVersion::new(2, 1, 7),
            "p",
            &std::collections::BTreeMap::new(),
            &[],
        );
        assert_eq!(p.get_score("nope"), 0.0);
        assert!(!p.is_level_favorite("nope"));
    }

    #[test]
    fn round_trip_through_the_cpp_write_format() {
        // C++ writes profiles with `profile_to_obj` + `write_to_string` and
        // reads them back with `load_profile_from_json` (which — the
        // `isObj("version")` bug aside, mapping §3.3 — recovers name,
        // scores and favorites exactly).
        let mut scores = std::collections::BTreeMap::new();
        scores.insert(
            "ohvrvanilla_vittorio_romeo_cube_1_pi_m_1".to_string(),
            3.5f32,
        );
        scores.insert(
            "ohvrvanilla_vittorio_romeo_cube_1_pi_m_0.5".to_string(),
            1.25f32,
        );
        let p = ProfileData::new(
            crate::version::GAME_VERSION,
            "player one",
            &scores,
            &["ohvrvanilla_vittorio_romeo_cube_1_pi_m_1".to_string()],
        );
        let text = json::write_to_string(&profile_to_obj(&p));
        let obj = json::parse(&text).unwrap();
        let back = crate::load_profile_from_json(&obj).unwrap();
        assert_eq!(back.get_name(), "player one");
        assert_eq!(
            back.get_score("ohvrvanilla_vittorio_romeo_cube_1_pi_m_1"),
            3.5f32
        );
        assert_eq!(
            back.get_score("ohvrvanilla_vittorio_romeo_cube_1_pi_m_0.5"),
            1.25f32
        );
        assert!(back.is_level_favorite("ohvrvanilla_vittorio_romeo_cube_1_pi_m_1"));
        // the C++ load bug: version is always {-1, 0, 0} after loading.
        assert_eq!(back.get_version(), GameVersion::new(-1, 0, 0));
    }

    #[test]
    fn write_format_matches_cpp_styled_output() {
        let mut scores = std::collections::BTreeMap::new();
        scores.insert("cube_l_m_1".to_string(), 100.5f32);
        let p = ProfileData::new(
            GameVersion::new(2, 1, 7),
            "player",
            &scores,
            &["cube_l_m_1".to_string()],
        );
        // The C++ writer starts with `writeWithIndent("{")` even for the
        // root → leading newline; non-empty objects go on their own line
        // (`" : " is emitted first, hence the space before the newline —
        // visible in the shipped config.json); `100.5f` archives as double
        // → `%#.16g` + trim → `100.50`.
        assert_eq!(
            json::write_to_string(&profile_to_obj(&p)),
            "\n{\n\t\"favorites\" : [ \"cube_l_m_1\" ],\n\t\"name\" : \"player\",\n\
             \t\"scores\" : \n\t{\n\t\t\"cube_l_m_1\" : 100.50\n\t},\n\t\"version\" : \
             \n\t{\n\t\t\"major\" : 2,\n\t\t\"micro\" : 7,\n\t\t\"minor\" : 1\n\t}\n}\n"
        );
    }
}
