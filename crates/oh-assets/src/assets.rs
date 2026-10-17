//! Port of `include/SSVOpenHexagon/Global/Assets.hpp` +
//! `src/SSVOpenHexagon/Global/Assets.cpp` (the `HGAssetsImpl` pimpl body).
//!
//! Scope (issue 05): the pack/level/style/music **discovery and loading**
//! pipeline, the `LoadInfo` error accumulation, `assets.json` parsing, the
//! shader registry (issue 10 compiles the GLSL), and the level-selection data
//! structures. Everything is threaded through an explicit `root` (the process
//! CWD in C++, the `_RELEASE` directory in tests) instead of a hidden global
//! singleton, matching how `oh-config` threads its base dir.
//!
//! What is intentionally deferred (documented, not silently dropped):
//! - the actual SFML texture/font/sound/shader *handles* (render/audio
//!   backend, issue 10) — the registry records `loaded`/ids only;
//! - Steam Workshop pack enumeration (`steamManager == nullptr` here, so the
//!   C++ `if(steamManager != nullptr)` branch is skipped; oh-steam owns it);
//! - the profile mutation/save API (`pCreate`/`pSetCurrent`/`saveAllProfiles`
//!   / `getLocalScore` ...) — profile *loading* is ported (needed by the
//!   constructor flow), the profile *editor* API is out of this task's build
//!   list;
//! - the `reloadPack` / `reloadLevel` / `reloadAllShaders` dev-tool paths.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use oh_data::json::{self, read_file_with_errors, Obj};
use oh_data::{
    build_pack_id, load_music_from_json, load_profile_from_json, pack_data_from_json, LevelData,
    LoadInfo, MusicData, PackData, PackInfo, ProfileData, StyleData,
};

use crate::asset_storage::{AssetRef, AssetStorage};
use crate::pack_scan;
use crate::shaders::{ShaderRegistry, ShaderType};

/// A fatal asset-load condition (the C++ constructor `std::terminate()`s on
/// these). The port instead returns this, carrying the `LoadInfo` accumulated
/// up to (and including) the fatal step so callers/tests can inspect it.
#[derive(Debug, Clone, PartialEq)]
pub struct FatalLoadError {
    /// `LoadInfo` accumulated up to the fatal step.
    pub load_info: LoadInfo,
    /// A short description of the fatal condition.
    pub reason: String,
}

/// C++ `hg::HGAssets` (the `HGAssetsImpl` body).
#[derive(Debug)]
pub struct Assets {
    /// Process-CWD equivalent (the `_RELEASE` dir in tests).
    root: PathBuf,
    levels_only: bool,

    storage: AssetStorage,

    // C++ `levelDatas` (`unordered_map<string, LevelData>`).
    level_datas: HashMap<String, LevelData>,
    // C++ `levelDataIdsByPack` (`unordered_map<string, vector<string>>`).
    level_data_ids_by_pack: HashMap<String, Vec<String>>,

    // C++ `packDatas` (`unordered_map<string, PackData>`).
    pack_datas: HashMap<String, PackData>,
    // C++ `packInfos` / `selectablePackInfos` (`vector<PackInfo>`).
    pack_infos: Vec<PackInfo>,
    selectable_pack_infos: Vec<PackInfo>,

    // C++ `musicPathMap` (`unordered_map<string, string>`).
    music_path_map: HashMap<String, String>,
    // C++ `musicDataMap` / `styleDataMap` are `std::map` (SORTED) — the
    // missing-asset fallback returns the smallest-key entry, so the port keeps
    // them ordered.
    music_data_map: BTreeMap<String, MusicData>,
    style_data_map: BTreeMap<String, StyleData>,

    // C++ `profileDataMap` (`std::map<string, ProfileData>`).
    profile_data_map: BTreeMap<String, ProfileData>,

    // C++ `packIdsWithMissingDependencies` (`unordered_set<string>`).
    pack_ids_with_missing_deps: BTreeSet<String>,

    // C++ `shaders` / `shadersPathToId` / `shadersById`.
    shaders: ShaderRegistry,

    // C++ `loadInfo`.
    load_info: LoadInfo,
}

impl Assets {
    /// C++ `HGAssetsImpl(mSteamManager, mHeadless, mLevelsOnly)` constructor
    /// flow, minus the Steam manager (see module docs) and the timing log.
    ///
    /// - `root` — the process CWD (the `_RELEASE` directory in tests);
    /// - `headless` — C++ `mHeadless`;
    /// - `levels_only` — C++ `mLevelsOnly`.
    pub fn new(
        root: impl AsRef<Path>,
        headless: bool,
        levels_only: bool,
    ) -> Result<Self, FatalLoadError> {
        let root = root.as_ref().to_path_buf();
        let mut a = Self {
            root,
            levels_only,
            storage: AssetStorage::new(),
            level_datas: HashMap::new(),
            level_data_ids_by_pack: HashMap::new(),
            pack_datas: HashMap::new(),
            pack_infos: Vec::new(),
            selectable_pack_infos: Vec::new(),
            music_path_map: HashMap::new(),
            music_data_map: BTreeMap::new(),
            style_data_map: BTreeMap::new(),
            profile_data_map: BTreeMap::new(),
            pack_ids_with_missing_deps: BTreeSet::new(),
            shaders: ShaderRegistry::new(),
            load_info: LoadInfo::default(),
        };

        // (1) assets.json — C++: only when `!levelsOnly && !mHeadless`.
        if !levels_only && !headless {
            if !a.root.join("Assets").is_dir() {
                return Err(a.fatal("Folder Assets/ does not exist"));
            }
            let (object, mut error) =
                read_file_with_errors(&a.root.join("Assets").join("assets.json"));
            load_assets_from_json(&mut a.storage, &a.root, "Assets/", &object);
            a.load_info.add_formatted_error(&mut error);
        }

        // (2) load all pack datas (C++: `std::terminate` if false / on throw).
        match a.load_all_pack_datas() {
            Ok(false) => return Err(a.fatal("Folder Packs/ does not exist")),
            Ok(true) => {}
            Err(e) => {
                // C++ has no global catch here: a pack.json type mismatch would
                // abort the process. The port reports it as a fatal error.
                return Err(a.fatal(&format!("Exception during asset loading: {}", e.message())));
            }
        }

        // (3) load all pack assets (C++: `std::terminate` if false).
        if let Err(reason) = a.load_all_pack_assets(headless) {
            return Err(a.fatal(&reason));
        }

        // (4) verify pack dependencies (C++: always returns true; never fatal).
        a.verify_all_pack_dependencies();

        // (5) load local profiles (C++: non-fatal; missing Profiles/ -> log).
        let _ = a.load_all_local_profiles();

        // (6) ordering (C++: after all the loads above).
        a.sort_level_ids_by_menu_priority();
        a.sort_pack_infos_by_priority();
        a.sort_selectable_pack_infos_by_priority();

        Ok(a)
    }

    fn fatal(self, reason: &str) -> FatalLoadError {
        FatalLoadError {
            load_info: self.load_info,
            reason: reason.to_string(),
        }
    }

    fn is_dir(&self, logical: &str) -> bool {
        self.root.join(logical).is_dir()
    }

    // ------------------------------------------------------------------
    // PACK DATA DISCOVERY (C++ `loadAllPackDatas` / `loadPackData`)
    // ------------------------------------------------------------------

    fn load_all_pack_datas(&mut self) -> Result<bool, oh_data::json::JsonError> {
        if !self.root.join("Packs").is_dir() {
            return Ok(false);
        }

        // Local packs first: immediate sub-folders of `Packs/`, byte-sorted
        // (the C++ `scanSingleFolderName` default `Sort::Alphabetic`).
        for pack_path in pack_scan::scan_single_folder_name(&self.root, "Packs/") {
            self.try_load_pack_from_path(&pack_path)?;
        }

        // Steam Workshop packs (C++ `if(steamManager != nullptr) ...`): this
        // port has no steam manager, so the branch is skipped (oh-steam).

        Ok(true)
    }

    /// C++ `tryLoadPackFromPath` lambda.
    fn try_load_pack_from_path(&mut self, pack_path: &str) -> Result<(), oh_data::json::JsonError> {
        match self.load_pack_data(pack_path)? {
            true => self.load_info.packs += 1,
            false => {
                self.load_info
                    .error_messages
                    .push(format!("Error loading pack data '{pack_path}\n"));
            }
        }
        Ok(())
    }

    /// C++ `loadPackData(packPath)`: `packPath` is the logical folder path
    /// WITH a trailing slash (from the scan). Returns `Ok(true)` on success,
    /// `Ok(false)` when `pack.json` is not a file, `Err` when a field has the
    /// wrong JSON type (C++ would throw here with no surrounding catch).
    fn load_pack_data(&mut self, pack_path: &str) -> Result<bool, oh_data::json::JsonError> {
        let pack_json = format!("{pack_path}pack.json");
        if !self.root.join(&pack_json).is_file() {
            return Ok(false);
        }

        let (pack_root, mut error) = read_file_with_errors(&self.root.join(&pack_json));
        self.load_info.add_formatted_error(&mut error);

        // Read the fields `buildPackId` needs first (mirrors the C++ locals).
        let disambiguator = json::extr_str(&pack_root, "disambiguator", "no disambiguator")?;
        let name = json::extr_str(&pack_root, "name", "unknown name")?;
        let author = json::extr_str(&pack_root, "author", "unknown author")?;
        let version = json::extr_int(&pack_root, "version", 0)? as i32;
        let pack_id = build_pack_id(&disambiguator, &author, &name, version);

        let pack_data = pack_data_from_json(&pack_root, pack_path, &pack_id)?;

        self.pack_infos.push(PackInfo::new(&pack_id, pack_path));
        self.pack_datas.insert(pack_id, pack_data);
        Ok(true)
    }

    // ------------------------------------------------------------------
    // PACK ASSET LOADING (C++ `loadPackAssets` + the `load*` sub-routines)
    // ------------------------------------------------------------------

    fn load_all_pack_assets(&mut self, headless: bool) -> Result<(), String> {
        // C++ iterates `packDatas` (`unordered_map`); iterate a sorted copy so
        // which pack's fatal error (if any) is reported is deterministic.
        let mut pack_ids: Vec<String> = self.pack_datas.keys().cloned().collect();
        pack_ids.sort();
        for pack_id in pack_ids {
            let pack_data = self.pack_datas.get(&pack_id).unwrap().clone();
            if let Err(reason) = self.load_pack_assets(&pack_data, headless) {
                self.load_info
                    .error_messages
                    .push(format!("Error loading pack info '{pack_id}\n"));
                return Err(reason);
            }
        }
        Ok(())
    }

    /// C++ `loadPackAssets(packData, headless)`. The `Result`'s `Err(String)`
    /// is the exact `"FATAL ERROR, Exception during asset loading: ..."`
    /// message the C++ `catch` records (also already appended to
    /// `load_info.error_messages`).
    fn load_pack_assets(&mut self, pack_data: &PackData, headless: bool) -> Result<(), String> {
        let pack_path = pack_data.folder_path.clone();
        let pack_id = pack_data.id.clone();

        let result = self.run_pack_subloaders(&pack_path, &pack_id, headless);
        match result {
            Ok(()) => {
                if self.pack_has_levels(&pack_id) {
                    self.selectable_pack_infos
                        .push(PackInfo::new(&pack_id, &pack_path));
                }
                Ok(())
            }
            Err(e) => {
                let what = e.message().to_string();
                let msg = format!("FATAL ERROR, Exception during asset loading: {what}\n");
                self.load_info.error_messages.push(msg.clone());
                Err(msg)
            }
        }
    }

    /// The ordered sub-loader sequence, with the exact `if` gating from the
    /// C++ (mapping §4.4). A type-mismatch `JsonError` propagates (the C++
    /// `catch` turns it into the fatal message).
    fn run_pack_subloaders(
        &mut self,
        pack_path: &str,
        pack_id: &str,
        headless: bool,
    ) -> Result<(), oh_data::json::JsonError> {
        if !headless && !self.levels_only && self.is_dir(&format!("{pack_path}Shaders/")) {
            self.load_pack_assets_load_shaders(pack_id, pack_path, headless);
        }
        if !headless && !self.levels_only && self.is_dir(&format!("{pack_path}Sounds/")) {
            self.load_pack_assets_load_custom_sounds(pack_id, pack_path);
        }
        if !self.levels_only && self.is_dir(&format!("{pack_path}Music/")) {
            if !headless {
                self.load_pack_assets_load_music(pack_id, pack_path);
            }
            self.load_pack_assets_load_music_data(pack_id, pack_path)?;
        }
        if self.is_dir(&format!("{pack_path}Styles/")) {
            self.load_pack_assets_load_style_data(pack_id, pack_path)?;
        }
        if self.is_dir(&format!("{pack_path}Levels/")) {
            self.load_pack_assets_load_level_data(pack_id, pack_path)?;
        }
        Ok(())
    }

    /// C++ `loadPackAssets_loadShaders`. GLSL compilation is issue 10's; the
    /// registry records a successfully-"loaded" (file present) shader.
    fn load_pack_assets_load_shaders(&mut self, pack_id: &str, pack_path: &str, headless: bool) {
        if headless {
            return;
        }
        let mut load_of_type = |ext: &str, stype: ShaderType| {
            for p in pack_scan::scan_single_by_ext(&self.root, &format!("{pack_path}Shaders/"), ext)
            {
                // `sf::Shader::loadFromFile` stand-in: the file must exist.
                if !self.root.join(&p).is_file() {
                    // C++: log "Failed to load shader '<p>'" and continue.
                    continue;
                }
                // C++ `p.getFileName()` = the bare file name (with ext).
                let file_name = logical_filename(&p);
                let name_key = format!("{pack_id}_{file_name}");
                self.shaders.add(&name_key, &p, stype);
                self.load_info.assets += 1;
            }
        };

        // `.vert`, `.geom`, `.frag` in that order (C++).
        load_of_type(".vert", ShaderType::Vertex);
        load_of_type(".geom", ShaderType::Geometry);
        load_of_type(".frag", ShaderType::Fragment);
    }

    /// C++ `loadPackAssets_loadCustomSounds`.
    fn load_pack_assets_load_custom_sounds(&mut self, pack_id: &str, pack_path: &str) {
        for p in pack_scan::scan_single_by_ext(&self.root, &format!("{pack_path}Sounds/"), ".ogg") {
            // C++ `p.getFileName()` = the bare file name (with `.ogg`).
            let file_name = logical_filename(&p);
            let asset_id = format!("{pack_id}_{file_name}");
            let real = self.root.join(&p);
            if !self.storage.load_sound_buffer(&asset_id, &real) {
                // C++: log "Failed to load sound buffer '<p>'"
            }
            self.load_info.assets += 1;
        }
    }

    /// C++ `loadPackAssets_loadMusic` (the `.ogg` files → `musicPathMap`).
    fn load_pack_assets_load_music(&mut self, pack_id: &str, pack_path: &str) {
        for p in pack_scan::scan_single_by_ext(&self.root, &format!("{pack_path}Music/"), ".ogg") {
            let base = file_name_no_ext(&p);
            let asset_id = format!("{pack_id}_{base}");
            self.music_path_map.insert(asset_id, p.clone());
            self.load_info.assets += 1;
        }
    }

    /// C++ `loadPackAssets_loadMusicData` (the `.json` files).
    fn load_pack_assets_load_music_data(
        &mut self,
        pack_id: &str,
        pack_path: &str,
    ) -> Result<(), oh_data::json::JsonError> {
        for p in pack_scan::scan_single_by_ext(&self.root, &format!("{pack_path}Music/"), ".json") {
            let (object, mut error) = read_file_with_errors(&self.root.join(&p));
            self.load_info.add_formatted_error(&mut error);

            let music_data = load_music_from_json(&object)?;
            let asset_id = format!("{pack_id}_{}", music_data.id);
            self.music_data_map.insert(asset_id, music_data);
            self.load_info.assets += 1;
        }
        Ok(())
    }

    /// C++ `loadPackAssets_loadStyleData`.
    fn load_pack_assets_load_style_data(
        &mut self,
        pack_id: &str,
        pack_path: &str,
    ) -> Result<(), oh_data::json::JsonError> {
        for p in pack_scan::scan_single_by_ext(&self.root, &format!("{pack_path}Styles/"), ".json")
        {
            let (object, mut error) = read_file_with_errors(&self.root.join(&p));
            self.load_info.add_formatted_error(&mut error);

            let style_data = StyleData::from_json(&object)?;
            let asset_id = format!("{pack_id}_{}", style_data.id);
            self.style_data_map.insert(asset_id, style_data);
            self.load_info.assets += 1;
        }
        Ok(())
    }

    /// C++ `loadPackAssets_loadLevelData`.
    fn load_pack_assets_load_level_data(
        &mut self,
        pack_id: &str,
        pack_path: &str,
    ) -> Result<(), oh_data::json::JsonError> {
        for p in pack_scan::scan_single_by_ext(&self.root, &format!("{pack_path}Levels/"), ".json")
        {
            let (object, mut error) = read_file_with_errors(&self.root.join(&p));
            self.load_info.add_formatted_error(&mut error);

            // C++ `LevelData{object, mPath, mPackId}` (mPath = packPath string).
            let level_data = LevelData::from_json(&object, pack_path, pack_id)?;
            let asset_id = format!("{pack_id}_{}", level_data.id);

            self.level_data_ids_by_pack
                .entry(pack_id.to_string())
                .or_default()
                .push(asset_id.clone());
            self.level_datas.insert(asset_id, level_data);
            self.load_info.levels += 1;
        }
        Ok(())
    }

    // ------------------------------------------------------------------
    // DEPENDENCIES (C++ `verifyAllPackDependencies`)
    // ------------------------------------------------------------------

    fn verify_all_pack_dependencies(&mut self) {
        let entries: Vec<(String, Vec<oh_data::PackDependency>)> = self
            .pack_datas
            .iter()
            .map(|(id, pd)| (id.clone(), pd.dependencies.clone()))
            .collect();
        for (pack_id, deps) in &entries {
            let pack_data = &self.pack_datas[pack_id];
            for pd in deps {
                if self.dependency_exists(pd) {
                    continue;
                }
                self.load_info.error_messages.push(format!(
                    "Missing pack dependency '{}' for pack '{}'\n",
                    pd.name, pack_data.name
                ));
                self.pack_ids_with_missing_deps.insert(pack_id.clone());
            }
        }

        // C++ `eraseRemoveIf(selectablePackInfos, id in missingDeps)`.
        self.selectable_pack_infos
            .retain(|pi| !self.pack_ids_with_missing_deps.contains(&pi.id));
    }

    fn dependency_exists(&self, pd: &oh_data::PackDependency) -> bool {
        self.pack_datas.values().any(|cand| {
            cand.disambiguator == pd.disambiguator
                && cand.name == pd.name
                && cand.author == pd.author
                && cand.version >= pd.min_version
        })
    }

    // ------------------------------------------------------------------
    // PROFILES (C++ `loadAllLocalProfiles` / `addLocalProfile`)
    // ------------------------------------------------------------------

    fn load_all_local_profiles(&mut self) -> bool {
        if !self.root.join("Profiles").is_dir() {
            // C++: log "Folder Profiles/ does not exist" and return false
            // (NOT fatal).
            return false;
        }
        for p in pack_scan::scan_single_by_ext(&self.root, "Profiles/", ".json") {
            let (object, mut error) = read_file_with_errors(&self.root.join(&p));
            self.load_info.add_formatted_error(&mut error);
            if let Ok(profile) = load_profile_from_json(&object) {
                self.add_local_profile(profile);
            }
        }
        true
    }

    /// C++ `addLocalProfile`: drop favorite ids that are not valid level ids,
    /// then emplace by name.
    fn add_local_profile(&mut self, mut profile: ProfileData) {
        let to_remove: Vec<String> = profile
            .get_favorite_level_ids()
            .iter()
            .filter(|id| !self.level_datas.contains_key(*id))
            .cloned()
            .collect();
        for id in to_remove {
            profile.remove_favorite_level(&id);
        }
        self.profile_data_map
            .insert(profile.get_name().to_string(), profile);
    }

    // ------------------------------------------------------------------
    // ORDERING (C++ end of the constructor)
    // ------------------------------------------------------------------

    /// C++: sort each `levelDataIdsByPack` vector by `menuPriority` ascending.
    /// (C++ uses `std::sort` — unstable; Rust's `sort_by_key` is a valid,
    /// deterministic resolution of the unspecified tie order.)
    fn sort_level_ids_by_menu_priority(&mut self) {
        // Snapshot priorities (owned) so the sort closure does not borrow
        // `self` while `level_data_ids_by_pack` is mutably borrowed.
        let prios: HashMap<String, i32> = self
            .level_datas
            .iter()
            .map(|(id, ld)| (id.clone(), ld.menu_priority))
            .collect();
        for ids in self.level_data_ids_by_pack.values_mut() {
            ids.sort_by_key(|id| prios.get(id).copied().unwrap_or(0));
        }
    }

    /// C++: sort `packInfos` / `selectablePackInfos` by `PackData::priority`
    /// ascending (smaller = earlier; bundled packs use very negative
    /// priorities so they come before user packs with the default 100).
    fn sort_pack_infos_by_priority(&mut self) {
        sort_infos_by_priority(&mut self.pack_infos, &self.pack_datas);
    }

    fn sort_selectable_pack_infos_by_priority(&mut self) {
        sort_infos_by_priority(&mut self.selectable_pack_infos, &self.pack_datas);
    }

    // ------------------------------------------------------------------
    // GETTERS (C++ `GET` / lookup section; fail-soft semantics, mapping §4.9)
    // ------------------------------------------------------------------

    /// C++ `getLoadResults()`.
    #[must_use = "mirrors the C++ [[nodiscard]]"]
    pub fn get_load_results(&self) -> &LoadInfo {
        &self.load_info
    }

    /// The process-CWD root this `Assets` instance was built from.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    #[must_use]
    pub fn levels_only(&self) -> bool {
        self.levels_only
    }

    /// C++ `getTextureOrNullTexture(id)`: the slot if present, else the
    /// `static` null texture so the game keeps running on a missing asset.
    pub fn get_texture_or_null<'a>(&'a self, id: &'a str) -> AssetRef<'a> {
        self.storage.texture_or_null(id)
    }

    /// C++ `getFontOrNullFont(id)`.
    pub fn get_font_or_null<'a>(&'a self, id: &'a str) -> AssetRef<'a> {
        self.storage.font_or_null(id)
    }

    /// C++ `getTexture(id)` (`assert` non-null in C++; `Option` here).
    pub fn get_texture(&self, id: &str) -> Option<bool> {
        self.storage.get_texture(id)
    }

    /// C++ `getFont(id)`.
    pub fn get_font(&self, id: &str) -> Option<bool> {
        self.storage.get_font(id)
    }

    /// C++ `getSoundBuffer(assetId)`.
    pub fn get_sound_buffer(&self, asset_id: &str) -> Option<bool> {
        self.storage.get_sound_buffer(asset_id)
    }

    /// The on-disk path of a registered sound buffer (the C++ `getSoundBuffer`
    /// returns the in-memory `sf::SoundBuffer*`; the port exposes the source
    /// path so `oh-audio` can decode it — issue 025e).
    #[must_use]
    pub fn get_sound_path(&self, asset_id: &str) -> Option<&std::path::Path> {
        self.storage.get_sound_path(asset_id)
    }

    /// C++ `isValidLevelId`.
    #[must_use]
    pub fn is_valid_level_id(&self, level_id: &str) -> bool {
        self.level_datas.contains_key(level_id)
    }

    /// C++ `getLevelData(assetId)` (`assert` validity; `Option` here).
    pub fn get_level_data(&self, asset_id: &str) -> Option<&LevelData> {
        self.level_datas.get(asset_id)
    }

    /// C++ `packHasLevels(packId)`.
    #[must_use]
    pub fn pack_has_levels(&self, pack_id: &str) -> bool {
        self.level_data_ids_by_pack.contains_key(pack_id)
    }

    /// C++ `getLevelIdsByPack(packId)` (`assert` presence; `Option` here).
    pub fn get_level_ids_by_pack(&self, pack_id: &str) -> Option<&Vec<String>> {
        self.level_data_ids_by_pack.get(pack_id)
    }

    /// C++ `getPackDatas()`.
    pub fn get_pack_datas(&self) -> &HashMap<String, PackData> {
        &self.pack_datas
    }

    /// C++ `isValidPackId`.
    #[must_use]
    pub fn is_valid_pack_id(&self, pack_id: &str) -> bool {
        self.pack_datas.contains_key(pack_id)
    }

    /// C++ `getPackData(packId)` (`assert` validity; `Option` here).
    pub fn get_pack_data(&self, pack_id: &str) -> Option<&PackData> {
        self.pack_datas.get(pack_id)
    }

    /// C++ `getSelectablePackInfos()`.
    #[must_use]
    pub fn get_selectable_pack_infos(&self) -> &[PackInfo] {
        &self.selectable_pack_infos
    }

    /// C++ `findPackData(disambiguator, name, author)`: first match over
    /// `packDatas` (unordered in C++, so the "first" is implementation-defined
    /// when several packs share the triple).
    pub fn find_pack_data(
        &self,
        disambiguator: &str,
        name: &str,
        author: &str,
    ) -> Option<&PackData> {
        self.pack_datas
            .values()
            .find(|pd| pd.disambiguator == disambiguator && pd.name == name && pd.author == author)
    }

    /// C++ `getMusicData(packId, id)`: the entry, or the smallest-key entry
    /// as a fail-soft fallback (C++ `assert`s the map is non-empty; `None`
    /// here when it is).
    pub fn get_music_data(&self, pack_id: &str, id: &str) -> Option<&MusicData> {
        let asset_id = format!("{pack_id}_{id}");
        self.music_data_map
            .get(&asset_id)
            .or_else(|| self.music_data_map.iter().next().map(|(_, v)| v))
    }

    /// C++ `getStyleData(packId, id)` (same fail-soft fallback).
    pub fn get_style_data(&self, pack_id: &str, id: &str) -> Option<&StyleData> {
        let asset_id = format!("{pack_id}_{id}");
        self.style_data_map
            .get(&asset_id)
            .or_else(|| self.style_data_map.iter().next().map(|(_, v)| v))
    }

    /// C++ `getMusicPath(assetId)`.
    pub fn get_music_path(&self, asset_id: &str) -> Option<&String> {
        self.music_path_map.get(asset_id)
    }

    /// C++ `getLevelDatas()`.
    pub fn get_level_datas(&self) -> &HashMap<String, LevelData> {
        &self.level_datas
    }

    /// C++ `getPackIdsWithMissingDependencies()`.
    pub fn get_pack_ids_with_missing_dependencies(&self) -> &BTreeSet<String> {
        &self.pack_ids_with_missing_deps
    }

    /// C++ `getProfileData` (the loaded profile store; the profile *editor*
    /// API is deferred — see module docs).
    pub fn get_profile_data(&self, name: &str) -> Option<&ProfileData> {
        self.profile_data_map.get(name)
    }

    #[must_use]
    pub fn get_local_profiles_size(&self) -> usize {
        self.profile_data_map.len()
    }

    /// C++ `getLocalProfileNames` (Assets.cpp:1500-1511) — the profile names
    /// in `std::map` (byte-sorted) order, mirroring the C++ `std::map`.
    #[must_use]
    pub fn get_profile_names(&self) -> Vec<String> {
        self.profile_data_map
            .values()
            .map(|p| p.get_name().to_string())
            .collect()
    }

    // ---- shader registry (C++ `getShader*` / `isValidShaderId`) ---------

    /// C++ `getShader(packId, id)` → the loaded shader, or `None` (+ log).
    pub fn get_shader(&self, pack_id: &str, id: &str) -> Option<&crate::shaders::LoadedShader> {
        self.shaders.get(&format!("{pack_id}_{id}"))
    }

    /// C++ `getShaderId(packId, id)` → id or `None` (nullopt).
    pub fn get_shader_id(&self, pack_id: &str, id: &str) -> Option<usize> {
        self.shaders.get_id_by_name(&format!("{pack_id}_{id}"))
    }

    /// C++ `getShaderIdByPath(shaderPath)` → id or `None`.
    pub fn get_shader_id_by_path(&self, path: &str) -> Option<usize> {
        self.shaders.get_id_by_path(path)
    }

    /// C++ `getShaderByShaderId(shaderId)` → the shader, or `None`.
    pub fn get_shader_by_shader_id(&self, id: usize) -> Option<&crate::shaders::LoadedShader> {
        self.shaders.get_by_id(id)
    }

    /// C++ `isValidShaderId(shaderId)`.
    #[must_use]
    pub fn is_valid_shader_id(&self, id: usize) -> bool {
        self.shaders.is_valid_id(id)
    }
}

/// C++ `loadAssetsFromJson(assetStorage, mRootPath, mObj)`: reads the three
/// `vector<string>` keys (no defaults → missing key = empty list) and loads
/// each into the storage. Failures only log (not recorded in `LoadInfo`).
fn load_assets_from_json(storage: &mut AssetStorage, root: &Path, root_path: &str, obj: &Obj) {
    if let Ok(fonts) = json::extr_vec_str(json::get_missing_as_null(obj, "fonts")) {
        for f in fonts {
            if !storage.load_font(&f, &root.join(format!("{root_path}{f}"))) {
                // C++: log "Failed to load font '<f>'"
            }
        }
    }
    if let Ok(textures) = json::extr_vec_str(json::get_missing_as_null(obj, "textures")) {
        for f in textures {
            if !storage.load_texture(&f, &root.join(format!("{root_path}{f}"))) {
                // C++: log "Failed to load texture '<f>'"
            }
        }
    }
    if let Ok(sounds) = json::extr_vec_str(json::get_missing_as_null(obj, "soundBuffers")) {
        for f in sounds {
            if !storage.load_sound_buffer(&f, &root.join(format!("{root_path}{f}"))) {
                // C++: log "Failed to load sound buffer '<f>'"
            }
        }
    }
}

/// C++ `Path::getFileNameNoExtensions()` (the last path component, minus its
/// final extension).
fn file_name_no_ext(logical: &str) -> String {
    let name = logical_filename(logical);
    match name.rfind('.') {
        Some(i) if i > 0 => name[..i].to_string(),
        _ => name.to_string(),
    }
}

/// C++ `Path::getFileName()` — the last path component (e.g. `"gradient.frag"`).
fn logical_filename(logical: &str) -> &str {
    logical.rsplit('/').next().unwrap_or(logical)
}

/// Sort a `Vec<PackInfo>` by `PackData::priority` ascending (C++ end-of-ctor
/// sort). Priorities are snapshotted into an owned map first so the `sort_by`
/// closure does not borrow `pack_datas` while the vector is mutably borrowed.
fn sort_infos_by_priority(infos: &mut [PackInfo], pack_datas: &HashMap<String, PackData>) {
    let prios: BTreeMap<String, f32> = infos
        .iter()
        .map(|pi| {
            let p = pack_datas.get(&pi.id).map_or(0.0f32, |pd| pd.priority);
            (pi.id.clone(), p)
        })
        .collect();
    infos.sort_by(|a, b| {
        let x = prios.get(&a.id).copied().unwrap_or(0.0);
        let y = prios.get(&b.id).copied().unwrap_or(0.0);
        x.partial_cmp(&y).unwrap_or(std::cmp::Ordering::Equal)
    });
}
