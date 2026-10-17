//! Port of `include/SSVOpenHexagon/Data/LevelData.hpp` +
//! `src/SSVOpenHexagon/Data/LevelData.cpp`.

use crate::json::{self, JsonError, Obj};
use crate::utils::get_level_validator;

/// C++ `hg::LevelData` (field order = C++ declaration order).
///
/// `validators` / `validatorsWithoutPackId` are C++
/// `std::unordered_map<float, std::string>` — keyed by the EXACT bit
/// pattern of the difficulty mult (floats are not `Eq`/`Hash` in Rust; the
/// C++ hash likewise depends only on the bits, and every lookup uses the
/// very float that was inserted, so `to_bits()` keys are faithful).
#[derive(Debug, Clone, PartialEq)]
pub struct LevelData {
    pub pack_path: String,
    pub pack_id: String,

    pub id: String,
    pub name: String,
    pub description: String,
    pub author: String,
    pub menu_priority: i32,
    pub selectable: bool,
    pub music_id: String,
    pub sound_id: String,
    pub style_id: String,
    pub lua_script_path: String,
    pub difficulty_mults: Vec<f32>,
    pub unscored: bool,
    pub validators: std::collections::BTreeMap<u32, String>,
    pub validators_without_pack_id: std::collections::BTreeMap<u32, String>,
}

impl LevelData {
    /// C++ `LevelData(mRoot, mPackPath, mPackId)` (mapping §2.3).
    ///
    /// JSON keys/defaults: `"id"` ("nullId"), `"name"` ("nullName"),
    /// `"description"` (""), `"author"` (""), `"menuPriority"` (0),
    /// `"selectable"` (true), `"musicId"` ("nullMusicId"), `"soundId"`
    /// ("nullSoundId"), `"styleId"` ("nullStyleId"), `"luaFile"`
    /// ("nullLuaPath", **concatenated** with the pack path),
    /// `"difficultyMults"` ({}) — then `1.0` is appended and the vector is
    /// sorted. For every resulting mult `dm`:
    /// - `validators[dm] = getLevelValidator(packId + "_" + id, dm)`
    /// - `validatorsWithoutPackId[dm] = getLevelValidator(id, dm)`
    pub fn from_json(m_root: &Obj, m_pack_path: &str, m_pack_id: &str) -> Result<Self, JsonError> {
        let pack_path = m_pack_path.to_string();
        let pack_id = m_pack_id.to_string();
        let id = json::extr_str(m_root, "id", "nullId")?;
        let name = json::extr_str(m_root, "name", "nullName")?;
        let description = json::extr_str(m_root, "description", "")?;
        let author = json::extr_str(m_root, "author", "")?;
        let menu_priority = json::extr_int(m_root, "menuPriority", 0)? as i32;
        let selectable = json::extr_bool(m_root, "selectable", true)?;
        let music_id = json::extr_str(m_root, "musicId", "nullMusicId")?;
        let sound_id = json::extr_str(m_root, "soundId", "nullSoundId")?;
        let style_id = json::extr_str(m_root, "styleId", "nullStyleId")?;
        let lua_script_path =
            pack_path.clone() + &json::extr_str(m_root, "luaFile", "nullLuaPath")?;
        let mut difficulty_mults =
            json::extr_vec_f32(json::get_missing_as_null(m_root, "difficultyMults"))?;
        let unscored = json::extr_bool(m_root, "unscored", false)?;

        difficulty_mults.push(1.0f32);
        difficulty_mults.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

        let mut validators = std::collections::BTreeMap::new();
        let mut validators_without_pack_id = std::collections::BTreeMap::new();
        for dm in &difficulty_mults {
            // Keyed by the SAME f32 bits that were inserted (C++
            // `unordered_map<float, _>` hash = bit pattern); every lookup
            // uses a mult from `difficultyMults`, so the bits match.
            let key = dm.to_bits();
            validators.insert(key, get_level_validator(&format!("{pack_id}_{id}"), *dm));
            validators_without_pack_id.insert(key, get_level_validator(&id, *dm));
        }

        Ok(Self {
            pack_path,
            pack_id,
            id,
            name,
            description,
            author,
            menu_priority,
            selectable,
            music_id,
            sound_id,
            style_id,
            lua_script_path,
            difficulty_mults,
            unscored,
            validators,
            validators_without_pack_id,
        })
    }

    /// C++ `getNthDiffMult(index)` — wraps modulo (negative and out-of-range
    /// both wrap; C++ `while` loops, identical).
    #[must_use = "mirrors the C++ [[nodiscard]]"]
    pub fn get_nth_diff_mult(&self, mut index: i32) -> f32 {
        let size = self.difficulty_mults.len() as i32;
        while index < 0 {
            index += size;
        }
        while index >= size {
            index -= size;
        }
        self.difficulty_mults[index as usize]
    }

    /// C++ `getValidator(diffMult)` (asserts presence; port: `Option`).
    #[must_use = "mirrors the C++ [[nodiscard]]"]
    pub fn get_validator(&self, diff_mult: f32) -> Option<&str> {
        self.validators
            .get(&diff_mult.to_bits())
            .map(String::as_str)
    }

    /// C++ `getValidatorWithoutPackId(diffMult)`.
    #[must_use = "mirrors the C++ [[nodiscard]]"]
    pub fn get_validator_without_pack_id(&self, diff_mult: f32) -> Option<&str> {
        self.validators_without_pack_id
            .get(&diff_mult.to_bits())
            .map(String::as_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nth_diff_mult_wraps() {
        let obj: Obj =
            serde_json::from_str(r#"{"id": "pi", "difficultyMults": [0.5, 1.8]}"#).unwrap();
        let ld = LevelData::from_json(&obj, "Packs/cube/", "cube").unwrap();
        // mults after append+sort: [0.5, 1.0, 1.8]
        assert_eq!(ld.difficulty_mults, vec![0.5f32, 1.0f32, 1.8f32]);
        assert_eq!(ld.get_nth_diff_mult(0), 0.5);
        assert_eq!(ld.get_nth_diff_mult(-1), 1.8);
        assert_eq!(ld.get_nth_diff_mult(3), 0.5);
    }

    #[test]
    fn validators_are_built_with_pack_and_bare_ids() {
        let obj: Obj =
            serde_json::from_str(r#"{"id": "pi", "difficultyMults": [0.5, 1.8]}"#).unwrap();
        let ld =
            LevelData::from_json(&obj, "Packs/cube/", "ohvrvanilla_vittorio_romeo_cube_1").unwrap();
        assert_eq!(
            ld.get_validator(1.0),
            Some("ohvrvanilla_vittorio_romeo_cube_1_pi_m_1")
        );
        assert_eq!(
            ld.get_validator(0.5),
            Some("ohvrvanilla_vittorio_romeo_cube_1_pi_m_0.5")
        );
        assert_eq!(
            ld.get_validator(1.8),
            Some("ohvrvanilla_vittorio_romeo_cube_1_pi_m_1.8")
        );
        assert_eq!(ld.get_validator_without_pack_id(0.5), Some("pi_m_0.5"));
        assert_eq!(ld.get_validator_without_pack_id(1.8), Some("pi_m_1.8"));
        // a mult not in the level has no entry:
        assert_eq!(ld.get_validator(2.0), None);
    }
}
