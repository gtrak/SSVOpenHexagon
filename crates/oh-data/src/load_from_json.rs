//! Port of `include/SSVOpenHexagon/Utils/LoadFromJson.hpp` +
//! `src/SSVOpenHexagon/Utils/LoadFromJson.cpp`.

use crate::json::{self, JsonError, Obj};
use crate::music_data::MusicData;
use crate::profile_data::ProfileData;
use crate::version::GameVersion;

/// C++ `hg::Utils::loadMusicFromJson` (mapping §3.1):
/// `"id"`, `"file_name"`, `"name"`, `"album"`, `"author"` — all strings with
/// **no defaults** (missing → `""` via the null singleton); then each
/// element of the `"segments"` array: `time` (no default → 0.0f if the key
/// or the array is missing) and `beat_pulse_delay_offset` (default 0.f).
#[must_use = "mirrors the C++ [[nodiscard]]"]
pub fn load_music_from_json(m_root: &Obj) -> Result<MusicData, JsonError> {
    let mut result = MusicData::new(
        &json::as_str(json::get_missing_as_null(m_root, "id"))?,
        &json::as_str(json::get_missing_as_null(m_root, "file_name"))?,
        &json::as_str(json::get_missing_as_null(m_root, "name"))?,
        &json::as_str(json::get_missing_as_null(m_root, "album"))?,
        &json::as_str(json::get_missing_as_null(m_root, "author"))?,
    );

    let segments = json::get_missing_as_null(m_root, "segments");
    for i in 0..json::size(segments) {
        let segment = json::get_at_or_null(segments, i);
        result.add_segment(
            json::as_float(json::get_missing_as_null(segment, "time"))? as f32,
            json::extr_float(segment, "beat_pulse_delay_offset", 0.0)? as f32,
        );
    }

    Ok(result)
}

/// C++ `hg::Utils::loadVersionFromJson`: `"major"`/`"minor"`/`"micro"` ints,
/// no defaults (missing → 0).
#[must_use = "mirrors the C++ [[nodiscard]]"]
pub fn load_version_from_json(m_root: &Obj) -> Result<GameVersion, JsonError> {
    Ok(GameVersion {
        major: json::as_int(json::get_missing_as_null(m_root, "major"))? as i32,
        minor: json::as_int(json::get_missing_as_null(m_root, "minor"))? as i32,
        micro: json::as_int(json::get_missing_as_null(m_root, "micro"))? as i32,
    })
}

/// C++ `hg::Utils::loadProfileFromJson` (mapping §3.3) —
/// **including the source bug, verbatim**:
///
/// ```cpp
/// const GameVersion version =
///     ssvuj::isObj("version")   // <-- checks the STRING literal, always false
///         ? loadVersionFromJson(ssvuj::getObj(mRoot, "version"))
///         : GameVersion{-1, 0, 0};
/// ```
///
/// `isObj("version")` asks whether the JSON *string* `"version"` is an
/// object — always false — so every loaded profile gets
/// `version = {-1, 0, 0}` regardless of the file (`loadVersionFromJson`
/// is dead code at runtime). The port preserves that: the version is
/// always `{-1, 0, 0}`.
///
/// Other keys: `"name"` (no default → ""), `"scores"` (object →
/// map<string, float>, missing/null value → 0.0), `"favorites"`
/// (`vector<string>`, default `{}`).
#[must_use = "mirrors the C++ [[nodiscard]]"]
pub fn load_profile_from_json(m_root: &Obj) -> Result<ProfileData, JsonError> {
    // BUG (verbatim from source): the C++ condition `ssvuj::isObj("version")`
    // tests the string literal, not `mRoot`, and is always false — so the
    // `else` branch always wins and `load_version_from_json` is never called.
    let version = GameVersion::new(-1, 0, 0);

    let name = json::as_str(json::get_missing_as_null(m_root, "name"))?;
    let scores = json::extr_map_str_f32(json::get_missing_as_null(m_root, "scores"))?;
    let favorites = json::extr_vec_str(json::get_missing_as_null(m_root, "favorites"))?;

    Ok(ProfileData::new(version, &name, &scores, &favorites))
}
