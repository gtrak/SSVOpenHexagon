//! A [`MenuAssets`](crate::seams::MenuAssets) backed by a real
//! [`oh_assets::Assets`] (the `_RELEASE` directory). This is the seam the
//! app layer and the "list every pack/level" test drive `MenuGame` with.
//!
//! The profile surface is the SHARED `oh-assets` store (issue 029c1):
//! the C++ has ONE `HGAssets` (main.cpp:381/:434) — the menu and the engine
//! observe the same profile state, so a score the engine's `Profiles` seam
//! sets during play is visible here immediately. The `Arc<Assets>` is the
//! same object the engine composition holds; no local mirror exists.
//! Packs / levels / styles / load-results are fully backed.

use std::path::Path;
use std::sync::Arc;

use oh_assets::Assets;
use oh_math::hg::color::Color;

use crate::seams::{LevelInfo, LoadResults, MenuAssets, PackDataInfo, StyleBackground};

/// `MenuAssets` over a loaded `oh_assets::Assets`.
///
/// `Arc`-shared: the windowed app builds ONE `Assets` and shares it between
/// the engine composition (whose `Profiles` seam is `compose::RealProfiles`
/// over the same `Arc`) and this seam (the C++ shares one `HGAssets`
/// between `HexagonGame` and `MenuGame`, main.cpp:381/:434). The profile
/// state (the current profile, scores, favorites) lives in that one
/// `Assets` store — a score set through the engine seam during play is
/// read back here with no sync step (the C++'s single-object semantics).
#[derive(Debug)]
pub struct RealMenuAssets {
    pub assets: Arc<Assets>,
}

impl RealMenuAssets {
    /// Load `_RELEASE`-style assets (headless, full).
    pub fn new(root: impl AsRef<Path>) -> Result<Self, oh_assets::FatalLoadError> {
        Ok(Self {
            assets: Arc::new(Assets::new(root, true, false)?),
        })
    }

    /// Wrap an already-loaded `Assets` (no second load).
    #[must_use]
    pub fn from_arc(assets: Arc<Assets>) -> Self {
        Self { assets }
    }
}

impl MenuAssets for RealMenuAssets {
    fn selectable_pack_ids(&self) -> Vec<String> {
        self.assets
            .get_selectable_pack_infos()
            .iter()
            .map(|p| p.id.clone())
            .collect()
    }

    fn level_ids_for_pack(&self, pack_id: &str) -> Vec<String> {
        self.assets
            .get_level_ids_by_pack(pack_id)
            .cloned()
            .unwrap_or_default()
    }

    fn level(&self, level_id: &str) -> Option<LevelInfo> {
        let l = self.assets.get_level_data(level_id)?;
        // C++ `assets.getMusicData(levelData.packId, levelData.musicId)`
        // (MenuGame.cpp:5545-5556): missing music → the C++ default-
        // constructed `MusicData` (empty name/author/album).
        let music = self.assets.get_music_data(&l.pack_id, &l.music_id);
        Some(LevelInfo {
            id: l.id.clone(),
            pack_id: l.pack_id.clone(),
            name: l.name.clone(),
            author: l.author.clone(),
            description: l.description.clone(),
            style_id: l.style_id.clone(),
            difficulty_mults: l.difficulty_mults.clone(),
            lua_script_path: l.lua_script_path.clone(),
            unscored: l.unscored,
            validators: l.validators.clone(),
            validators_without_pack_id: l.validators_without_pack_id.clone(),
            music_name: music.map(|m| m.name.clone()).unwrap_or_default(),
            music_author: music.map(|m| m.author.clone()).unwrap_or_default(),
            music_album: music.map(|m| m.album.clone()).unwrap_or_default(),
        })
    }

    fn style_colors(&self, level_id: &str) -> Vec<Color> {
        let Some(l) = self.assets.get_level_data(level_id) else {
            return Vec::new();
        };
        let Some(style) = self.assets.get_style_data(&l.pack_id, &l.style_id) else {
            return Vec::new();
        };
        let mut style = style.clone();
        style.compute_colors();
        style.get_colors().to_vec()
    }

    fn style_text_color(&self, level_id: &str) -> Color {
        let Some(l) = self.assets.get_level_data(level_id) else {
            return Color::default();
        };
        let Some(style) = self.assets.get_style_data(&l.pack_id, &l.style_id) else {
            return Color::default();
        };
        let mut style = style.clone();
        style.compute_colors();
        *style.get_text_color()
    }

    fn style_background(&self, level_id: &str) -> Option<StyleBackground> {
        let style = self.style_of(level_id)?;
        Some(StyleBackground {
            colors: style.get_colors().to_vec(),
            bg_tile_radius: style.bg_tile_radius,
            bg_rot_off: style.bg_rot_off,
            main_color: *style.get_main_color(),
            cap_color: style.get_cap_color_result(),
        })
    }

    fn pack_data_info(&self, pack_id: &str) -> PackDataInfo {
        let p = self.assets.get_pack_data(pack_id);
        PackDataInfo {
            name: p.map(|d| d.name.clone()).unwrap_or_default(),
            author: p.map(|d| d.author.clone()).unwrap_or_default(),
            version: p.map(|d| d.version).unwrap_or(0),
        }
    }

    fn load_results(&self) -> LoadResults {
        let li = self.assets.get_load_results();
        LoadResults {
            packs: li.packs,
            levels: li.levels,
            assets: li.assets,
            error_messages: li.error_messages.clone(),
        }
    }

    fn profile_count(&self) -> usize {
        self.assets.get_local_profiles_size()
    }

    // The profile surface is the shared `oh-assets` store (issue 029c1 —
    // the C++'s single `HGAssets`; the struct docs above).
    fn profile_names(&self) -> Vec<String> {
        // C++ `getLocalProfileNames` (Assets.cpp:1500-1511) — name-keyed
        // store in `std::map` (byte-sorted) order.
        self.assets.get_profile_names()
    }
    fn has_current_profile(&self) -> bool {
        self.assets.any_local_profile_active()
    }
    fn current_profile_name(&self) -> Option<String> {
        self.assets.current_local_profile_name()
    }
    fn is_level_favorite(&self, level_id: &str) -> bool {
        self.assets.is_level_favorite(level_id)
    }
    fn favorite_level_ids(&self) -> Vec<String> {
        self.assets.get_favorite_level_ids()
    }
    fn add_favorite_level(&self, level_id: &str) {
        self.assets.add_favorite_level(level_id)
    }
    fn remove_favorite_level(&self, level_id: &str) {
        self.assets.remove_favorite_level(level_id)
    }
    fn profile_set_current(&self, name: &str) {
        self.assets.profile_set_current(name)
    }
    fn profile_create(&self, name: &str) {
        if let Err(e) = self.assets.profile_create(name) {
            // C++ `pCreate` (Assets.cpp:1555-1560): the failed reload logs
            // the line and `std::terminate()`s; the port's clean-exit
            // deviation (the `Assets::new` `FatalLoadError` class) keeps
            // running with the profile not created.
            eprintln!("[HGAssets::HGAssets] {e}");
        }
    }
    fn profile_remove(&self, name: &str) {
        self.assets.profile_remove(name)
    }
    fn profile_save_current(&self) {
        // C++ `playLocally`'s `assets.pSaveCurrent()` (MenuGame.cpp:2427).
        self.assets.profile_save_current();
    }
    fn profile_remove_file(&self, name: &str) -> bool {
        // C++ `std::remove("Profiles/" + name + ".json")` (MenuGame.cpp:
        // 2421-2431) — the CWD-relative path rooted at the `Assets` root.
        std::fs::remove_file(
            self.assets
                .root()
                .join("Profiles")
                .join(format!("{name}.json")),
        )
        .is_ok()
    }
    fn reload_assets(&self, _entire_pack: bool, _current_level_id: &str) {}
    fn find_pack_id_by_name(&self, name: &str) -> Option<String> {
        // C++ `loadCommandLineLevel` (MenuGame.cpp:1696-1703): iterate
        // `assets.getPackDatas()` (an `unordered_map`) for the first
        // `d.second.name == pack` — the port's `get_pack_datas()` is a
        // `HashMap`, so (as in the C++) a shared pack name resolves in
        // unspecified iteration order.
        self.assets
            .get_pack_datas()
            .iter()
            .find(|(_id, d)| d.name == name)
            .map(|(id, _)| id.clone())
    }
    fn pack_has_levels(&self, pack_id: &str) -> bool {
        self.assets.pack_has_levels(pack_id)
    }
    fn pack_ids_with_missing_dependencies(&self) -> Vec<String> {
        self.assets
            .get_pack_ids_with_missing_dependencies()
            .iter()
            .cloned()
            .collect()
    }
    fn profile_total_survival_time(&self, name: &str) -> Option<i32> {
        // C++ `formatSurvivalTime(data)` (MenuGame.cpp:3861-3883):
        // `int time{0}; for (auto& s : data->getScores()) time += s.second;`
        // — each `float` score truncates to `int` on the compound
        // assignment, then the `int` sums.
        let profile = self.assets.get_profile_data(name)?;
        Some(profile.get_scores().values().map(|v| *v as i32).sum())
    }
    fn local_profile_score(&self, validator: &str) -> f32 {
        // C++ `assets.getCurrentLocalProfile().getScore(validator)`
        // (MenuGame.cpp:5494): 0 when no current profile / unset score.
        self.assets.get_local_score(validator)
    }
}

impl RealMenuAssets {
    /// The level's `StyleData` after `computeColors` (`None` when the
    /// level or its style is unknown — the C++ would be a null-deref;
    /// the port returns `None` and the draw bridge uses the defaults).
    fn style_of(&self, level_id: &str) -> Option<oh_data::StyleData> {
        let l = self.assets.get_level_data(level_id)?;
        let style = self.assets.get_style_data(&l.pack_id, &l.style_id)?;
        let mut style = style.clone();
        style.compute_colors();
        Some(style)
    }
}
