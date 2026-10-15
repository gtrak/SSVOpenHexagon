//! A [`MenuAssets`](crate::seams::MenuAssets) backed by a real
//! [`oh_assets::Assets`] (the `_RELEASE` directory). This is the seam the
//! app layer and the "list every pack/level" test drive `MenuGame` with.
//!
//! The profile-editor operations (`pCreate`/`pSetCurrent`/`pRemove`) and
//! `getLocalProfileNames` are not yet exposed by `oh-assets` (it defers the
//! profile *editor* API, issue 005); those are documented no-op seams here.
//! Packs / levels / styles / load-results are fully backed.

use std::path::Path;
use std::sync::Arc;

use oh_assets::Assets;
use oh_math::hg::color::Color;

use crate::seams::{LevelInfo, LoadResults, MenuAssets};

/// `MenuAssets` over a loaded `oh_assets::Assets`.
///
/// `Arc`-shared: the windowed app builds ONE `Assets` and shares it between
/// the engine composition and this seam (the C++ shares one `HGAssets`
/// between `HexagonGame` and `MenuGame`, main.cpp:381/:434).
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
        Some(LevelInfo {
            id: l.id.clone(),
            pack_id: l.pack_id.clone(),
            name: l.name.clone(),
            author: l.author.clone(),
            style_id: l.style_id.clone(),
            difficulty_mults: l.difficulty_mults.clone(),
            lua_script_path: l.lua_script_path.clone(),
            music_name: String::new(),
            music_author: String::new(),
            music_album: String::new(),
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

    // The profile-name list / editor API are not yet exposed by `oh-assets`;
    // these are documented no-op seams (matching the C++ `p*` deferral).
    fn profile_names(&self) -> Vec<String> {
        Vec::new()
    }
    fn has_current_profile(&self) -> bool {
        false
    }
    fn current_profile_name(&self) -> Option<String> {
        None
    }
    fn is_level_favorite(&self, _level_id: &str) -> bool {
        false
    }
    fn profile_set_current(&self, _name: &str) {}
    fn profile_create(&self, _name: &str) {}
    fn profile_remove(&self, _name: &str) {}
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
}
