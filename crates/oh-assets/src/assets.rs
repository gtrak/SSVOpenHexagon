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
//! The local-profile WRITE/state surface (issue 029c1, the C++
//! `Assets.cpp:1001-1058` `saveCurrentLocalProfile`/`saveAllProfiles`,
//! `:1446-1454` `getLocalScore`/`setLocalScore`, `:1517-1566`
//! `pIsValidLocalProfile`/`pSaveCurrent`/`pSaveAll`/`pSetCurrent`/
//! `pCreate`/`pRemove`) lives on this type too: the C++ has ONE `HGAssets`
//! shared by the engine and the menu (main.cpp:381/:434), so the port shares
//! ONE `Arc<Assets>` between the engine's `Profiles` seam and the menu's
//! `RealMenuAssets`, and the profile state (the C++ `profileDataMap` +
//! `currentProfilePtr`) is the `Mutex<ProfileStore>` field below — interior
//! mutability behind the `Arc` (the `Send + Sync` requirement of the
//! engine's `Arc<dyn Profiles>`; the C++'s single-thread unsynchronized
//! access generalizes to a short lock).
//!
//! What is intentionally deferred (documented, not silently dropped):
//! - the actual SFML texture/font/sound/shader *handles* (render/audio
//!   backend, issue 10) — the registry records `loaded`/ids only;
//! - the Steam SDK itself (oh-steam owns it) — the C++'s
//!   `if(steamManager != nullptr)` workshop half (Assets.cpp:750-764) is
//!   the `WorkshopPacks` seam (issue 29p1): the compositions pass the
//!   `oh_steam::SteamManager` through `new_with_workshop` (the live
//!   enumeration when initialized, the `workshopCache.json` fallback
//!   otherwise); `Assets::new` is the C++'s `nullptr` state (the
//!   local-`Packs/`-only loads).
//!
//! Interior mutability (issue 29i1): the C++'s `HGAssets` is ONE object the
//! menu and the engine share (main.cpp:381/:434), and the menu's F3/F4 debug
//! reload (`reloadPack` / `reloadLevel`, the `MenuGame::reloadAssets`
//! handler) re-reads level/style/music/sound data FROM DISK IN PLACE —
//! observable to the engine through the SAME object. The port shares ONE
//! `Arc<Assets>`, so the reload-mutable state (the `levelDatas` /
//! `levelDataIdsByPack` / `styleDataMap` / `musicDataMap` / `musicPathMap`
//! tables + the `AssetStorage`) lives behind a `Mutex` (`AssetState` below —
//! the same short-lock pattern as the `profiles` store); the getters for
//! those tables return owned clones (the C++'s references generalize).
//! The shader registry stays a plain field: `reloadAllShaders` re-reads the
//! sources but never inserts/removes entries (issue 10 attaches the GLSL
//! payload later).

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use oh_data::json::{self, read_file_with_errors, Obj};
use oh_data::{
    build_pack_id, load_music_from_json, load_profile_from_json, pack_data_from_json, LevelData,
    LoadInfo, MusicData, PackData, PackInfo, ProfileData, StyleData, GAME_VERSION,
};

use crate::asset_storage::{AssetRef, AssetStorage};
use crate::pack_scan;
use crate::shaders::{ShaderRegistry, ShaderType, UniformValue};

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

/// C++ `steam_manager` (Steam.cpp:860-880) — the Assets ctor's
/// `mSteamManager` pointer (the C++'s `HGAssets{&steamManager, headless}`,
/// main.cpp:380; `nullptr` = the no-Steam state, the port's `None`). Only
/// the two methods the C++'s `loadAllPackDatas` uses (Assets.cpp:750-764)
/// are on the trait — the rest of the Steam surface stays in `oh-steam`
/// (the app layer implements this for `SteamManager`, keeping the
/// oh-assets → oh-steam layering one-way).
pub trait WorkshopPacks: Send + Sync {
    /// C++ `steam_manager::is_initialized()` (Steam.cpp:973-975).
    fn is_initialized(&self) -> bool;
    /// C++ `steam_manager::for_workshop_pack_folders(f)`
    /// (Steam.cpp:931-934) — the installed workshop pack folders
    /// (absolute paths, the C++'s `GetItemInstallInfo` `folderBuf` shape;
    /// empty when uninitialized — the C++'s `_initialized` gate, the port's
    /// `oh-steam` mirror). The C++ iterates an `unordered_set` (unspecified
    /// order); the port's `oh-steam` returns its load order (deterministic).
    fn workshop_pack_folders(&self) -> Vec<String>;
}

/// The `Assets` derive's `Debug` half for the trait-object field (the C++
///'s `steam_manager*` has no meaningful debug form either).
impl std::fmt::Debug for dyn WorkshopPacks {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("WorkshopPacks").finish()
    }
}

/// C++ `profileDataMap` + `currentProfilePtr` (the local-profile store).
///
/// `current: Option<String>` mirrors the C++ `ProfileData* currentProfilePtr`
/// (a pointer INTO the map): `None` until a `profile_set_current` picks one
/// (the C++'s `pSetCurrent` — the ONLY writer; loading profiles never sets
/// it, so a fresh `Assets` has no active profile, exactly the C++'s
/// `currentProfilePtr{nullptr}` initial state), the profile's name when set.
#[derive(Debug, Default)]
struct ProfileStore {
    profiles: BTreeMap<String, ProfileData>,
    current: Option<String>,
}

/// The reload-mutable asset tables (the C++ `HGAssetsImpl` members the
/// F3/F4 debug reload replaces in place — module docs): behind the `Mutex`
/// so the menu's `Arc<Assets>`-shared `reload_pack` / `reload_level` write
/// what the engine's getters read (the C++'s single-object semantics).
#[derive(Debug, Default)]
struct AssetState {
    // C++ `levelDatas` (`unordered_map<string, LevelData>`).
    level_datas: HashMap<String, LevelData>,
    // C++ `levelDataIdsByPack` (`unordered_map<string, vector<string>>`).
    level_data_ids_by_pack: HashMap<String, Vec<String>>,
    // C++ `musicPathMap` (`unordered_map<string, string>`).
    music_path_map: HashMap<String, String>,
    // C++ `musicDataMap` / `styleDataMap` are `std::map` (SORTED) — the
    // missing-asset fallback returns the smallest-key entry, so the port
    // keeps them ordered.
    music_data_map: BTreeMap<String, MusicData>,
    style_data_map: BTreeMap<String, StyleData>,
    // C++ `assetStorage` (the sound-buffer registry half the reload
    // `loadSoundBuffer` / `hasSoundBuffer` calls hit; the textures / fonts
    // halves too — one `AssetStorage` object in the C++).
    storage: AssetStorage,
}

/// C++ `hg::HGAssets` (the `HGAssetsImpl` body).
#[derive(Debug)]
pub struct Assets {
    /// Process-CWD equivalent (the `_RELEASE` dir in tests).
    root: PathBuf,
    levels_only: bool,

    // C++ `steamManager` pointer (the `WorkshopPacks` seam — the C++'s
    // `loadAllPackDatas` workshop half; `None` = the C++'s `nullptr`
    // state, the no-Steam compositions).
    workshop: Option<Arc<dyn WorkshopPacks>>,

    // The reload-mutable tables + the storage (module docs).
    state: Mutex<AssetState>,

    // C++ `packDatas` (`unordered_map<string, PackData>`).
    pack_datas: HashMap<String, PackData>,
    // C++ `packInfos` / `selectablePackInfos` (`vector<PackInfo>`).
    pack_infos: Vec<PackInfo>,
    selectable_pack_infos: Vec<PackInfo>,

    // C++ `profileDataMap` (`std::map<string, ProfileData>`) +
    // `currentProfilePtr` (Assets.hpp:67-68) — the shared local-profile
    // store (the struct docs: one object behind the `Arc`, the C++'s
    // single `HGAssets` semantics).
    profiles: Mutex<ProfileStore>,

    // C++ `packIdsWithMissingDependencies` (`unordered_set<string>`).
    pack_ids_with_missing_deps: BTreeSet<String>,

    // C++ `shaders` / `shadersPathToId` / `shadersById`.
    shaders: ShaderRegistry,

    // K6: the uniform store — the C++'s per-`sf::Shader` uniform cache
    // (`Shader.cpp`'s `setUniform*` storage; the C++ keys it by shader
    // OBJECT, the port by the same shader id the Lua `shdr_*` API passes
    // around). The GL upload is the draw-time bind half.
    // `Mutex`: the C++'s `HGAssets` is one shared object whose shader
    // uniforms the Lua `shdr_setUniform*` calls mutate; the port's
    // `Assets` is `Arc`-shared behind the `Send + Sync` `AssetsApi`
    // trait, so the mutation is interior (the C++ is single-threaded at
    // this call site — the `Mutex` never contends).
    shader_uniforms: Mutex<BTreeMap<usize, BTreeMap<String, UniformValue>>>,

    // C++ `loadInfo`.
    load_info: LoadInfo,
}

impl Assets {
    /// C++ `HGAssetsImpl(mSteamManager, mHeadless, mLevelsOnly)` constructor
    /// flow, with `mSteamManager` = `nullptr` (the C++'s no-Steam state —
    /// the local-`Packs/`-only loads) and the timing log elided. The
    /// Steam-manager half is `new_with_workshop` (the C++'s
    /// `HGAssets{&steamManager, headless}`, main.cpp:380).
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
        Self::new_with_workshop(root, headless, levels_only, None)
    }

    /// The `new` constructor with the C++'s `mSteamManager` argument
    /// (main.cpp:380's `HGAssets{&steamManager, headless}` — the C++'s
    /// workshop-pack discovery: the live enumeration when Steam is up, the
    /// `workshopCache.json` fallback when it is not —
    /// `load_all_pack_datas`'s workshop half). `None` = the C++'s
    /// `steamManager == nullptr` state (the local-`Packs/`-only loads, the
    /// no-Steam compositions' `Assets::new`).
    #[must_use = "mirrors the C++ [[nodiscard]] (the load errors)"]
    pub fn new_with_workshop(
        root: impl AsRef<Path>,
        headless: bool,
        levels_only: bool,
        workshop: Option<Arc<dyn WorkshopPacks>>,
    ) -> Result<Self, FatalLoadError> {
        let root = root.as_ref().to_path_buf();
        let mut a = Self {
            root,
            levels_only,
            workshop,
            state: Mutex::new(AssetState::default()),
            pack_datas: HashMap::new(),
            pack_infos: Vec::new(),
            selectable_pack_infos: Vec::new(),
            profiles: Mutex::new(ProfileStore::default()),
            pack_ids_with_missing_deps: BTreeSet::new(),
            shaders: ShaderRegistry::new(),
            shader_uniforms: Mutex::new(BTreeMap::new()),
            load_info: LoadInfo::default(),
        };

        // (1) assets.json — C++: only when `!levelsOnly && !mHeadless`.
        if !levels_only && !headless {
            if !a.root.join("Assets").is_dir() {
                return Err(a.fatal("Folder Assets/ does not exist"));
            }
            let (object, mut error) =
                read_file_with_errors(&a.root.join("Assets").join("assets.json"));
            let mut st = a.state.lock().expect("asset state poisoned");
            load_assets_from_json(&mut st.storage, &a.root, "Assets/", &object);
            drop(st);
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
        if !a.load_all_local_profiles() {
            // C++ `HGAssets` ctor (Assets.cpp:356-360): log + continue —
            // "No need to terminate here, some tests do not require
            // profiles."
            println!("[HGAssets::HGAssets] Error loading local profiles");
        }

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

        // Steam Workshop packs (C++ `if(steamManager != nullptr)` half,
        // Assets.cpp:750-764):
        // - Steam initialized → the LIVE enumeration (the C++'s
        //   `steamManager->for_workshop_pack_folders(tryLoadPackFromPath)`) —
        //   the cache is NOT consulted (the live list fully replaces it);
        // - Steam down (the manager exists but is uninitialized) → the
        //   `workshopCache.json` FALLBACK (the C++'s
        //   `loadWorkshopPackDatasFromCache` + the `cachedWorkshopPackIds`
        //   loop) — the case the cache exists for: installed workshop packs
        //   stay loadable exactly while Steam is unreachable.
        if let Some(steam) = &self.workshop {
            if steam.is_initialized() {
                for folder in steam.workshop_pack_folders() {
                    self.try_load_workshop_pack_from_path(&folder)?;
                }
            } else if let Some(cached_paths) = self.load_workshop_pack_datas_from_cache()? {
                for cached_path in cached_paths {
                    self.try_load_workshop_pack_from_path(&cached_path)?;
                }
            }
        }

        Ok(true)
    }

    /// C++ `tryLoadPackFromPath` lambda's workshop half (the C++'s
    /// `loadPackData` concatenates `packPath + "\/pack.json"` — the
    /// workshop paths (the Steam `GetItemInstallInfo` `folderBuf` / the
    /// cache's array entries) carry NO trailing slash, while the port's
    /// `load_pack_data` expects the trailing-slash form the local `Packs/`
    /// scan provides — the normalization is the `"\/"` the C++ adds).
    fn try_load_workshop_pack_from_path(
        &mut self,
        pack_path: &str,
    ) -> Result<(), oh_data::json::JsonError> {
        let mut path = pack_path.to_string();
        if !path.ends_with('/') {
            path.push('/');
        }
        self.try_load_pack_from_path(&path)
    }

    /// C++ `loadWorkshopPackDatasFromCache` (Assets.cpp:662-710): reads
    /// `workshopCache.json`'s `cachedPacks` array (the oh-steam manager's
    /// `load_workshop_data` cache write, the C++'s
    /// `ssvuj::writeToFile(cacheObj, "workshopCache.json")`, Steam.cpp:
    /// 240-243). Returns the cached pack folder paths when the cache is
    /// usable (the C++'s `cachedWorkshopPackIds` — the C++'s
    /// `unordered_set` iteration order is unspecified; the port keeps the
    /// cache array's order, which is deterministic), `Ok(None)` on the
    /// C++'s four `return false` arms (each logged, verbatim). `Err` = the
    /// C++'s uncaught `getExtr` type error (a non-string array element) —
    /// the port's fatal-error class (the caller's `Assets` ctor mapping).
    ///
    /// The cache lives at `root/workshopCache.json` — the port's root
    /// scope's equivalent of the C++'s CWD-relative `"workshopCache.json"`
    /// (the oh-steam writer's `std::fs::write("workshopCache.json")`
    /// targets the process CWD; in production the CWD IS the game root
    /// the compositions resolve — the two coincide there).
    fn load_workshop_pack_datas_from_cache(
        &mut self,
    ) -> Result<Option<Vec<String>>, oh_data::json::JsonError> {
        let cache_path = self.root.join("workshopCache.json");
        if !cache_path.is_file() {
            println!(
                "[::loadAssets] Workshop cache file does not exist. No workshop packs to load"
            );
            return Ok(None);
        }

        let (cache_object, mut cache_error) = read_file_with_errors(&cache_path);
        println!("[::loadAssets] Loading workshop packs from cache");

        // C++ `ssvuj::hasObj(cacheObject, "cachedPacks")` — a failed read
        // leaves the Null object, so the missing-key arm below covers it
        // (the C++'s `hasObj(null, ...) == false` half).
        let Some(pack_value) = json::get_or_null(&cache_object, "cachedPacks") else {
            println!("[::loadAssets] [ERROR]: Cannot locate cache array in workshop cache file");
            return Ok(None);
        };

        // C++'s null / non-array check (`Json::ValueType::nullValue ||
        // != arrayValue`) — the two arms share one log line.
        let Obj::Array(items) = pack_value else {
            println!("[::loadAssets] Cache array is null. No workshop packs to load");
            return Ok(None);
        };

        if items.is_empty() {
            println!("[::loadAssets] Cache array is empty. No workshop packs to load");
            return Ok(None);
        }

        // C++ `getExtr<std::vector<std::string>>(cacheObject, "cachedPacks")`
        // — a non-string element is the C++'s uncaught type error (the
        // `Err` arm, the caller's fatal mapping).
        let mut pack_array = Vec::with_capacity(items.len());
        for item in items {
            pack_array.push(json::as_str(item)?);
        }

        // C++'s success-only `loadInfo.addFormattedError(cacheError)` —
        // the read error is discarded on the four `return false` arms.
        self.load_info.add_formatted_error(&mut cache_error);
        Ok(Some(pack_array))
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
        // C++ `packDatas.emplace(packId, ...)` — FIRST WINS: a duplicate
        // packId (a cached workshop pack whose `pack.json` matches an
        // already-loaded one — the local `Packs/` scan always runs first)
        // keeps the first entry. (The C++'s `packInfos.emplace_back` is
        // unconditional — the port's `push` above matches.)
        self.pack_datas.entry(pack_id).or_insert(pack_data);
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
                // `sf::Shader::loadFromFile` stand-in: the file must
                // exist (the C++'s compile failure → log + skip).
                let real = self.root.join(&p);
                if !real.is_file() {
                    // C++: log "Failed to load shader '<p>'" and continue.
                    continue;
                }
                // K6: read the source (the C++ compiles it into the
                // `sf::Shader` here; the port keeps the text for the
                // draw-time GL compile). A read failure is the C++'s
                // load-failure path (log + skip).
                let Ok(source) = std::fs::read_to_string(&real) else {
                    continue;
                };
                // C++ `p.getFileName()` = the bare file name (with ext).
                let file_name = logical_filename(&p);
                let name_key = format!("{pack_id}_{file_name}");
                self.shaders.add(&name_key, &p, &source, stype);
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
            let loaded = self
                .state
                .lock()
                .expect("asset state poisoned")
                .storage
                .load_sound_buffer(&asset_id, &real);
            if !loaded {
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
            self.state
                .lock()
                .expect("asset state poisoned")
                .music_path_map
                .insert(asset_id, p.clone());
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
            self.state
                .lock()
                .expect("asset state poisoned")
                .music_data_map
                .insert(asset_id, music_data);
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
            self.state
                .lock()
                .expect("asset state poisoned")
                .style_data_map
                .insert(asset_id, style_data);
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

            let mut st = self.state.lock().expect("asset state poisoned");
            st.level_data_ids_by_pack
                .entry(pack_id.to_string())
                .or_default()
                .push(asset_id.clone());
            st.level_datas.insert(asset_id, level_data);
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
    // PROFILES (C++ `loadAllLocalProfiles` / `addLocalProfile` / the `p*`
    // write + state surface, Assets.cpp:1001-1058, :1446-1566)
    // ------------------------------------------------------------------

    fn load_all_local_profiles(&mut self) -> bool {
        let mut store = self.profiles.lock().expect("profile store poisoned");
        let (ok, mut errors) = self.reload_local_profiles(&mut store);
        for e in &mut errors {
            self.load_info.add_formatted_error(e);
        }
        ok
    }

    /// C++ `loadAllLocalProfiles` (Assets.cpp:849-870) over the locked
    /// store: the `Profiles/` folder check (log + false), the `.json`
    /// scan, the `addLocalProfile` invalid-favorite filter. The C++
    /// `LoadInfo` error accumulation is RETURNED to the caller: the
    /// constructor path appends it to `load_info` before boot (the C++'
    /// `MenuGame` ctor copies `getLoadResults()` there —
    /// MenuGame.cpp:232 — so the `profile_create` reload path's errors
    /// are unobservable in the C++ and are not appended on it; only the
    /// `read_file_with_errors` console line, `ssvuj::logReadError`, fires
    /// on both, as in the C++).
    fn reload_local_profiles(&self, store: &mut ProfileStore) -> (bool, Vec<String>) {
        if !self.root.join("Profiles").is_dir() {
            // C++ `ssvu::lo("::loadAssets") << "Folder Profiles/ does not
            // exist" << std::endl` — log + return false (NOT fatal).
            println!("[::loadAssets] Folder Profiles/ does not exist");
            return (false, Vec::new());
        }
        // C++ `ssvu::lo("::loadAssets") << "loading local profiles\n"`.
        println!("[::loadAssets] loading local profiles");
        store.profiles.clear();
        let mut errors = Vec::new();
        for p in pack_scan::scan_single_by_ext(&self.root, "Profiles/", ".json") {
            let (object, error) = read_file_with_errors(&self.root.join(&p));
            if !error.is_empty() {
                errors.push(error);
            }
            if let Ok(profile) = load_profile_from_json(&object) {
                self.add_local_profile(store, profile);
            }
        }
        (true, errors)
    }

    /// C++ `addLocalProfile`: drop favorite ids that are not valid level ids,
    /// then emplace by name.
    fn add_local_profile(&self, store: &mut ProfileStore, mut profile: ProfileData) {
        let to_remove: Vec<String> = profile
            .get_favorite_level_ids()
            .iter()
            .filter(|id| {
                !self
                    .state
                    .lock()
                    .expect("asset state poisoned")
                    .level_datas
                    .contains_key(*id)
            })
            .cloned()
            .collect();
        for id in to_remove {
            profile.remove_favorite_level(&id);
        }
        store
            .profiles
            .insert(profile.get_name().to_string(), profile);
    }

    /// C++ `getCurrentLocalProfileFilePath()` (Assets.cpp:1491-1494):
    /// `"Profiles/" + name + ".json"` in the CWD (the port's `root`).
    fn profile_file_path(&self, name: &str) -> PathBuf {
        self.root.join("Profiles").join(format!("{name}.json"))
    }

    /// The C++ `saveCurrentLocalProfile` / `saveAllProfiles` file content
    /// (Assets.cpp:1010-1024, :1037-1052): the four keys, `version` = the
    /// CURRENT `GAME_VERSION` (the C++ archives the compile-time constant,
    /// NOT the profile's loaded version — which the C++'s
    /// `isObj("version")` load bug, `LoadFromJson.cpp:40-44`, pins to
    /// -1.0.0 for every file it reads; `profile_to_obj` is the stored-version
    /// builder this lane deliberately does NOT reuse), `name`, `scores`
    /// (the `BTreeMap` keys byte-sorted — the C++'s `std::map` storage +
    /// writer), `favorites` (the set as an array), via the fork-faithful
    /// `write_to_string`.
    fn profile_file_contents(profile: &ProfileData) -> String {
        let version = GAME_VERSION;
        let mut version_obj = json::Obj::default();
        json::arch(
            &mut version_obj,
            "major",
            json::get_arch_int(i64::from(version.major)),
        );
        json::arch(
            &mut version_obj,
            "minor",
            json::get_arch_int(i64::from(version.minor)),
        );
        json::arch(
            &mut version_obj,
            "micro",
            json::get_arch_int(i64::from(version.micro)),
        );
        let mut scores = json::empty_obj();
        for (id, score) in profile.get_scores() {
            json::arch(&mut scores, id, json::get_arch_f32(*score));
        }
        let favorites: Vec<String> = profile.get_favorite_level_ids().iter().cloned().collect();
        let mut root = json::Obj::default();
        json::arch(&mut root, "version", version_obj);
        json::arch(&mut root, "name", json::get_arch_str(profile.get_name()));
        json::arch(&mut root, "scores", scores);
        json::arch(&mut root, "favorites", json::get_arch_vec_str(&favorites));
        json::write_to_string(&root)
    }

    // ------------------------------------------------------------------
    // PROFILE STATE (C++ `currentProfilePtr` accessors, Assets.cpp:
    // 1446-1524)
    // ------------------------------------------------------------------

    /// C++ `anyLocalProfileActive()` (Assets.cpp:1459-1462) —
    /// `currentProfilePtr != nullptr`.
    #[must_use]
    pub fn any_local_profile_active(&self) -> bool {
        self.profiles
            .lock()
            .expect("profile store poisoned")
            .current
            .is_some()
    }

    /// C++ `pGetName()` / `getCurrentLocalProfile().getName()` (Assets.cpp:
    /// 1522-1524) — the active profile's name. The C++ asserts a profile is
    /// active (`getCurrentLocalProfile`, Assets.cpp:1464-1468); `None` is
    /// the port arm when it is not (the `NullProfiles`-legacy empty-name
    /// behavior).
    #[must_use]
    pub fn current_local_profile_name(&self) -> Option<String> {
        self.profiles
            .lock()
            .expect("profile store poisoned")
            .current
            .clone()
    }

    /// C++ `getLocalScore(mId)` (Assets.cpp:1446-1449) — the active
    /// profile's `ProfileData::getScore` (missing id → 0.0). No active
    /// profile: the C++ asserts; the port returns 0.0 (the `NullProfiles`
    /// legacy arm).
    #[must_use]
    pub fn get_local_score(&self, level_id: &str) -> f32 {
        let store = self.profiles.lock().expect("profile store poisoned");
        store
            .current
            .as_deref()
            .and_then(|n| store.profiles.get(n))
            .map(|p| p.get_score(level_id))
            .unwrap_or(0.0)
    }

    /// C++ `setLocalScore(mId, mScore)` (Assets.cpp:1451-1454) — the active
    /// profile's `ProfileData::setScore` (the engine's PB/death path,
    /// HexagonGame.cpp:871,908,915, writes through this). No active profile:
    /// the C++ asserts; the port no-ops (the `NullProfiles` legacy arm —
    /// `shouldSaveScore`'s `anyLocalProfileActive` reject, HexagonGame.cpp:
    /// 1178-1183, already vetoes the call in the C++).
    pub fn set_local_score(&self, level_id: &str, score: f32) {
        let mut store = self.profiles.lock().expect("profile store poisoned");
        if let Some(name) = store.current.clone() {
            if let Some(p) = store.profiles.get_mut(&name) {
                p.set_score(level_id, score);
            }
        }
    }

    /// C++ `getCurrentLocalProfile().isLevelFavorite(levelID)` (the menu's
    /// F1 favorite read, MenuGame.cpp:2978-2979). No active profile: the C++
    /// asserts; the port reports `false`.
    #[must_use]
    pub fn is_level_favorite(&self, level_id: &str) -> bool {
        let store = self.profiles.lock().expect("profile store poisoned");
        store
            .current
            .as_deref()
            .and_then(|n| store.profiles.get(n))
            .is_some_and(|p| p.is_level_favorite(level_id))
    }

    /// C++ `getCurrentLocalProfile().getFavoriteLevelIds()` (the menu's
    /// `changeFavoriteLevelsToProfile` rebuild source, MenuGame.cpp:
    /// 4788-4791). No active profile: the C++ asserts; the port returns the
    /// empty list (the `NullProfiles`-legacy arm, as `is_level_favorite`).
    #[must_use]
    pub fn get_favorite_level_ids(&self) -> Vec<String> {
        let store = self.profiles.lock().expect("profile store poisoned");
        store
            .current
            .as_deref()
            .and_then(|n| store.profiles.get(n))
            .map(|p| p.get_favorite_level_ids().iter().cloned().collect())
            .unwrap_or_default()
    }

    /// C++ `getCurrentLocalProfile().addFavoriteLevel(mLevelID)` (F1's add
    /// arm, MenuGame.cpp:4894). No active profile: the C++ asserts; the port
    /// no-ops (the `NullProfiles`-legacy arm).
    pub fn add_favorite_level(&self, level_id: &str) {
        let mut store = self.profiles.lock().expect("profile store poisoned");
        if let Some(name) = store.current.clone() {
            if let Some(p) = store.profiles.get_mut(&name) {
                p.add_favorite_level(level_id);
            }
        }
    }

    /// C++ `getCurrentLocalProfile().removeFavoriteLevel(mLevelID)` (F1's
    /// remove arm, MenuGame.cpp:4845). No active profile: the C++ asserts;
    /// the port no-ops (the `NullProfiles`-legacy arm).
    pub fn remove_favorite_level(&self, level_id: &str) {
        let mut store = self.profiles.lock().expect("profile store poisoned");
        if let Some(name) = store.current.clone() {
            if let Some(p) = store.profiles.get_mut(&name) {
                p.remove_favorite_level(level_id);
            }
        }
    }

    // ------------------------------------------------------------------
    // PROFILE WRITE (C++ `saveCurrentLocalProfile` / `saveAllProfiles` /
    // `pCreate` / `pRemove` / `pSetCurrent`, Assets.cpp:1001-1058,
    // :1532-1566)
    // ------------------------------------------------------------------

    /// C++ `pSaveCurrent()` = `saveCurrentLocalProfile()` (Assets.cpp:
    /// 1526-1529, :1001-1025): early-return when no current profile; write
    /// the four keys to `Profiles/<name>.json`. The C++ `writeToFile` is an
    /// unchecked `std::ofstream` — a failed write is unobservable through
    /// the C++ surface, so the port's `fs::write` error is ignored too.
    pub fn profile_save_current(&self) {
        let store = self.profiles.lock().expect("profile store poisoned");
        let Some(name) = store.current.clone() else {
            return; // C++ `if(currentProfilePtr == nullptr) return;`
        };
        let Some(profile) = store.profiles.get(&name) else {
            return;
        };
        let _ = std::fs::write(
            self.profile_file_path(&name),
            Self::profile_file_contents(profile),
        );
    }

    /// C++ `pSaveAll()` = `saveAllProfiles()` (Assets.cpp:1532-1535,
    /// :1027-1058): rewrites EVERY profile's file (the `std::map`
    /// iteration = name-sorted; the port's `BTreeMap` matches). The
    /// `mainClient` exit scope guard (main.cpp:382-388) calls this.
    pub fn profile_save_all(&self) {
        let store = self.profiles.lock().expect("profile store poisoned");
        for (name, profile) in &store.profiles {
            let _ = std::fs::write(
                self.profile_file_path(name),
                Self::profile_file_contents(profile),
            );
        }
    }

    /// C++ `pCreate(mName)` (Assets.cpp:1545-1561): write
    /// `Profiles/<name>.json` with EXACTLY the C++'s three keys — `name`,
    /// `scores` = an EMPTY object, `favorites` = an EMPTY object, and NO
    /// `version` key. The C++ archives `ssvuj::Obj{}` for the two empties —
    /// the fork's `ObjImpl` (a sorted map) default-constructed empty, which
    /// the writer emits as `{}`; the files are on-disk compatibility
    /// artifacts the C++ re-reads through `loadProfileFromJson` (where
    /// `scores`/`favorites` extract as empty), so the shape is matched
    /// byte-for-byte.
    ///
    /// Then `profileDataMap.clear()` + `loadAllLocalProfiles()`. The C++
    /// `std::terminate()`s on a failed reload (Assets.cpp:1555-1560); the
    /// port returns `Err` (the `Assets::new` `FatalLoadError` clean-exit
    /// deviation class — the caller logs the C++'s line and keeps running,
    /// where the C++ process dies).
    pub fn profile_create(&self, name: &str) -> Result<(), String> {
        let mut root = json::Obj::default();
        json::arch(&mut root, "name", json::get_arch_str(name));
        // C++ `ssvuj::arch(root, "scores", ssvuj::Obj{})` — the empty object.
        json::arch(&mut root, "scores", json::empty_obj());
        json::arch(&mut root, "favorites", json::empty_obj());
        let _ = std::fs::write(self.profile_file_path(name), json::write_to_string(&root));

        let mut store = self.profiles.lock().expect("profile store poisoned");
        store.profiles.clear();
        let (ok, _errors) = self.reload_local_profiles(&mut store);
        if !ok {
            // C++: `ssvu::lo("HGAssets::HGAssets") << "Error loading local
            // profiles\n"` + `std::terminate()`.
            println!("[HGAssets::HGAssets] Error loading local profiles");
            return Err("Error loading local profiles".to_string());
        }
        Ok(())
    }

    /// C++ `pRemove(mName)` (Assets.cpp:1563-1566) — the store erase ONLY.
    /// The FILE deletion is the menu's job (MenuGame.cpp:2419-2431:
    /// `std::remove("Profiles/" + name + ".json")`, and only on success
    /// `assets.pRemove(name)`); the menu seam exposes the two halves
    /// separately for the same reason.
    pub fn profile_remove(&self, name: &str) {
        let mut store = self.profiles.lock().expect("profile store poisoned");
        store.profiles.remove(name);
    }

    /// C++ `pSetCurrent(mName)` (Assets.cpp:1537-1543): locate (the C++
    /// `SSVOH_ASSERT`s the name exists — a release build would dereference
    /// `end()`, UB) and set the current profile. The port sets it iff the
    /// name is present (documented deviation: a missing name is a no-op,
    /// not a crash — every C++ call site passes a name the menu just
    /// listed/created, so the assert never fires in the C++ either).
    pub fn profile_set_current(&self, name: &str) {
        let mut store = self.profiles.lock().expect("profile store poisoned");
        if store.profiles.contains_key(name) {
            store.current = Some(name.to_string());
        }
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
            .state
            .lock()
            .expect("asset state poisoned")
            .level_datas
            .iter()
            .map(|(id, ld)| (id.clone(), ld.menu_priority))
            .collect();
        let mut st = self.state.lock().expect("asset state poisoned");
        for ids in st.level_data_ids_by_pack.values_mut() {
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
    /// Owned (module docs: the reload-mutable `AssetStorage` lives behind
    /// the `Mutex`; the C++'s reference generalizes to a clone).
    #[must_use]
    pub fn get_texture_or_null(&self, id: &str) -> AssetRef {
        self.state
            .lock()
            .expect("asset state poisoned")
            .storage
            .texture_or_null(id)
    }

    /// C++ `getFontOrNullFont(id)`.
    #[must_use]
    pub fn get_font_or_null(&self, id: &str) -> AssetRef {
        self.state
            .lock()
            .expect("asset state poisoned")
            .storage
            .font_or_null(id)
    }

    /// C++ `getTexture(id)` (`assert` non-null in C++; `Option` here).
    #[must_use]
    pub fn get_texture(&self, id: &str) -> Option<bool> {
        self.state
            .lock()
            .expect("asset state poisoned")
            .storage
            .get_texture(id)
    }

    /// C++ `getFont(id)`.
    #[must_use]
    pub fn get_font(&self, id: &str) -> Option<bool> {
        self.state
            .lock()
            .expect("asset state poisoned")
            .storage
            .get_font(id)
    }

    /// C++ `getSoundBuffer(assetId)`.
    #[must_use]
    pub fn get_sound_buffer(&self, asset_id: &str) -> Option<bool> {
        self.state
            .lock()
            .expect("asset state poisoned")
            .storage
            .get_sound_buffer(asset_id)
    }

    /// The on-disk path of a registered sound buffer (the C++ `getSoundBuffer`
    /// returns the in-memory `sf::SoundBuffer*`; the port exposes the source
    /// path so `oh-audio` can decode it — issue 025e). Owned (module docs).
    #[must_use]
    pub fn get_sound_path(&self, asset_id: &str) -> Option<PathBuf> {
        self.state
            .lock()
            .expect("asset state poisoned")
            .storage
            .get_sound_path(asset_id)
    }

    /// C++ `isValidLevelId`.
    #[must_use]
    pub fn is_valid_level_id(&self, level_id: &str) -> bool {
        self.state
            .lock()
            .expect("asset state poisoned")
            .level_datas
            .contains_key(level_id)
    }

    /// C++ `getLevelData(assetId)` (`assert` validity; `Option` here).
    /// Owned (module docs: the reload replaces these entries in place).
    #[must_use]
    pub fn get_level_data(&self, asset_id: &str) -> Option<LevelData> {
        self.state
            .lock()
            .expect("asset state poisoned")
            .level_datas
            .get(asset_id)
            .cloned()
    }

    /// C++ `packHasLevels(packId)`.
    #[must_use]
    pub fn pack_has_levels(&self, pack_id: &str) -> bool {
        self.state
            .lock()
            .expect("asset state poisoned")
            .level_data_ids_by_pack
            .contains_key(pack_id)
    }

    /// C++ `getLevelIdsByPack(packId)` (`assert` presence; `Option` here).
    /// Owned (module docs).
    #[must_use]
    pub fn get_level_ids_by_pack(&self, pack_id: &str) -> Option<Vec<String>> {
        self.state
            .lock()
            .expect("asset state poisoned")
            .level_data_ids_by_pack
            .get(pack_id)
            .cloned()
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
    /// here when it is). Owned (module docs).
    #[must_use]
    pub fn get_music_data(&self, pack_id: &str, id: &str) -> Option<MusicData> {
        let asset_id = format!("{pack_id}_{id}");
        let st = self.state.lock().expect("asset state poisoned");
        st.music_data_map
            .get(&asset_id)
            .or_else(|| st.music_data_map.iter().next().map(|(_, v)| v))
            .cloned()
    }

    /// C++ `getStyleData(packId, id)` (same fail-soft fallback). Owned
    /// (module docs).
    #[must_use]
    pub fn get_style_data(&self, pack_id: &str, id: &str) -> Option<StyleData> {
        let asset_id = format!("{pack_id}_{id}");
        let st = self.state.lock().expect("asset state poisoned");
        st.style_data_map
            .get(&asset_id)
            .or_else(|| st.style_data_map.iter().next().map(|(_, v)| v))
            .cloned()
    }

    /// C++ `getMusicPath(assetId)`. Owned (module docs).
    #[must_use]
    pub fn get_music_path(&self, asset_id: &str) -> Option<String> {
        self.state
            .lock()
            .expect("asset state poisoned")
            .music_path_map
            .get(asset_id)
            .cloned()
    }

    /// C++ `getLevelDatas()`. Owned (module docs).
    #[must_use]
    pub fn get_level_datas(&self) -> HashMap<String, LevelData> {
        self.state
            .lock()
            .expect("asset state poisoned")
            .level_datas
            .clone()
    }

    /// C++ `getPackIdsWithMissingDependencies()`.
    pub fn get_pack_ids_with_missing_dependencies(&self) -> &BTreeSet<String> {
        &self.pack_ids_with_missing_deps
    }

    /// C++ `getProfileData` (the loaded profile store; the C++ returns a
    /// reference — the port clones, the `Mutex` guard is scope-local).
    #[must_use]
    pub fn get_profile_data(&self, name: &str) -> Option<ProfileData> {
        self.profiles
            .lock()
            .expect("profile store poisoned")
            .profiles
            .get(name)
            .cloned()
    }

    #[must_use]
    pub fn get_local_profiles_size(&self) -> usize {
        self.profiles
            .lock()
            .expect("profile store poisoned")
            .profiles
            .len()
    }

    /// C++ `getLocalProfileNames` (Assets.cpp:1500-1511) — the profile names
    /// in `std::map` (byte-sorted) order, mirroring the C++ `std::map`.
    #[must_use]
    pub fn get_profile_names(&self) -> Vec<String> {
        self.profiles
            .lock()
            .expect("profile store poisoned")
            .profiles
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

    /// The registered shader count (the C++'s `shadersById.size()` —
    /// the diagnostics' full-registry size).
    #[must_use]
    pub fn shader_count(&self) -> usize {
        self.shaders.len()
    }

    /// K6: the shader's source + the current uniform store (the draw-time
    /// snapshot the GL layer compiles + uploads from — the C++'s
    /// `getShader(id)` hands `renderTexture` the live `sf::Shader`,
    /// program + uniform cache, `HGGraphics.cpp:92-97`).
    #[must_use]
    pub fn shader_snapshot(&self, id: usize) -> Option<crate::shaders::ShaderSnapshot> {
        let shader = self.shaders.get_by_id(id)?;
        Some(crate::shaders::ShaderSnapshot {
            path: shader.path.clone(),
            shader_type: shader.shader_type,
            source: shader.source.clone(),
            uniforms: self
                .shader_uniforms
                .lock()
                .expect("shader uniforms poisoned")
                .get(&id)
                .cloned()
                .unwrap_or_default(),
        })
    }

    /// K6: the C++ `sf::Shader::setUniform` family's storage half (all
    /// eight kinds the Lua `shdr_setUniform*` API exposes,
    /// `LuaScripting.cpp:1337-1407`). The value lands in the per-shader
    /// store NOW (the C++ stores it at `setUniform` time,
    /// `Shader.cpp:102-196`; the GL upload is the port's lazy bind-time
    /// half, `Shader.cpp:294-303`). An unknown `id` is a no-op (the C++
    /// `getShader` would have thrown first; the Lua layer's
    /// `withValidShaderId` guard makes the call unreachable).
    pub fn set_shader_uniform_f(&self, id: usize, name: &str, value: f32) {
        self.set_shader_uniform(id, name, UniformValue::F(value));
    }

    pub fn set_shader_uniform_f_vec2(&self, id: usize, name: &str, x: f32, y: f32) {
        self.set_shader_uniform(id, name, UniformValue::FVec2(x, y));
    }

    pub fn set_shader_uniform_f_vec3(&self, id: usize, name: &str, x: f32, y: f32, z: f32) {
        self.set_shader_uniform(id, name, UniformValue::FVec3(x, y, z));
    }

    pub fn set_shader_uniform_f_vec4(&self, id: usize, name: &str, x: f32, y: f32, z: f32, w: f32) {
        self.set_shader_uniform(id, name, UniformValue::FVec4(x, y, z, w));
    }

    pub fn set_shader_uniform_i(&self, id: usize, name: &str, value: i32) {
        self.set_shader_uniform(id, name, UniformValue::I(value));
    }

    pub fn set_shader_uniform_i_vec2(&self, id: usize, name: &str, x: i32, y: i32) {
        self.set_shader_uniform(id, name, UniformValue::IVec2(x, y));
    }

    pub fn set_shader_uniform_i_vec3(&self, id: usize, name: &str, x: i32, y: i32, z: i32) {
        self.set_shader_uniform(id, name, UniformValue::IVec3(x, y, z));
    }

    pub fn set_shader_uniform_i_vec4(&self, id: usize, name: &str, x: i32, y: i32, z: i32, w: i32) {
        self.set_shader_uniform(id, name, UniformValue::IVec4(x, y, z, w));
    }

    /// The store write (all eight `set_shader_uniform_*` funnels here).
    fn set_shader_uniform(&self, id: usize, name: &str, value: UniformValue) {
        if !self.shaders.is_valid_id(id) {
            return;
        }
        self.shader_uniforms
            .lock()
            .expect("shader uniforms poisoned")
            .entry(id)
            .or_default()
            .insert(name.to_string(), value);
    }

    // ------------------------------------------------------------------
    // RELOAD (C++ `reloadAllShaders` / `reloadPack` / `reloadLevel`,
    // Assets.cpp:1159-1443) — the F3/F4 debug asset reload: re-read the
    // pack's (or one level's) data from disk IN PLACE. The state the
    // reload replaces (module docs): `levelDatas` + `levelDataIdsByPack`
    // (insert-or-replace), `styleDataMap` / `musicDataMap` (replace-or-
    // insert), `musicPathMap` (emplace — first wins), the sound-buffer
    // registry (first load wins). A file DELETED from disk is NOT removed
    // (the C++ never erases on reload — the stale entry stays), and the
    // load-results / error state (`loadInfo`) is untouched (the C++'s
    // reloads return the result string instead of recording).
    // ------------------------------------------------------------------

    /// C++ `reloadAllShaders()` (Assets.cpp:1159-1171): re-read every
    /// registered shader's source from disk (the C++ re-compiles the GLSL
    /// in place — issue 10's payload; the port's load stand-in is "the
    /// file exists", so a re-read is a re-check). A failure logs and
    /// leaves the registry untouched (the C++'s `continue`).
    pub fn reload_all_shaders(&self) {
        for id in 0..self.shaders.len() {
            let Some(shader) = self.shaders.get_by_id(id) else {
                continue;
            };
            if !self.root.join(&shader.path).is_file() {
                // C++: `ssvu::lo("hg::HGAssetsImplImpl::reloadAllShaders")
                // << "Failed to load shader '<path>'\n" (the channel keeps
                // the C++'s `ImplImpl` typo).
                println!(
                    "[hg::HGAssetsImplImpl::reloadAllShaders] Failed to load shader '{}'\n",
                    shader.path
                );
            }
        }
    }

    /// C++ `reloadPack(mPackId, mPath)` (Assets.cpp:1174-1278): the debug
    /// reload of a WHOLE pack. `mPath` is the pack's logical folder path
    /// WITH a trailing slash (the C++'s `PackData::folderPath`). Returns
    /// the multi-line result string the C++'s `MenuGame::reloadAssets`
    /// dialog shows.
    pub fn reload_pack(&self, pack_id: &str, pack_path: &str) -> String {
        let mut output = String::new();

        // Levels — the C++ aborts EVERYTHING when the folder is missing.
        let levels = format!("{pack_path}Levels/");
        if !self.root.join(&levels).is_dir() {
            return "invalid level folder path\n".to_string();
        }
        for p in pack_scan::scan_single_by_ext(&self.root, &levels, ".json") {
            // C++ `LevelData{ssvuj::getFromFile(p), mPath, mPackId}`:
            // a parse failure logs (inside `read_file_with_errors` — the
            // C++'s `ssvuj::logReadError` line) and yields a Null object →
            // the default-valued `LevelData` (id = "nullId") — the port
            // mirrors that (the C++'s `getFromFile` discards the error
            // string; the formatted `LogInfo` recording is the
            // `getFromFileWithErrors` half, NOT this path).
            let (object, _error) = read_file_with_errors(&self.root.join(&p));
            let level_data = match LevelData::from_json(&object, pack_path, pack_id) {
                Ok(ld) => ld,
                Err(e) => {
                    // C++: a type-mismatched field `getExtr` would throw
                    // with no surrounding catch (process abort); the
                    // port's clean-error deviation class (the `Assets::
                    // new` `FatalLoadError` analog) — log + skip the file.
                    println!("[ssvuj::logReadError] {}", e.formatted_messages());
                    continue;
                }
            };
            let asset_id = format!("{pack_id}_{}", level_data.id);
            let mut st = self.state.lock().expect("asset state poisoned");
            // C++'s find/emplace/replace: a NEW key appends to
            // `levelDataIdsByPack` + inserts; an EXISTING key replaces in
            // place (no id-list append).
            if !st.level_datas.contains_key(&asset_id) {
                st.level_data_ids_by_pack
                    .entry(pack_id.to_string())
                    .or_default()
                    .push(asset_id.clone());
            }
            st.level_datas.insert(asset_id, level_data);
        }
        output.push_str("Levels successfully reloaded\n");

        // Styles — `styleDataMap[key] =` (replace-or-insert).
        let styles = format!("{pack_path}Styles/");
        if !self.root.join(&styles).is_dir() {
            output.push_str("invalid style folder path\n");
        } else {
            for p in pack_scan::scan_single_by_ext(&self.root, &styles, ".json") {
                let (object, _error) = read_file_with_errors(&self.root.join(&p));
                match StyleData::from_json(&object) {
                    Ok(style_data) => {
                        let asset_id = format!("{pack_id}_{}", style_data.id);
                        self.state
                            .lock()
                            .expect("asset state poisoned")
                            .style_data_map
                            .insert(asset_id, style_data);
                    }
                    Err(e) => {
                        println!("[ssvuj::logReadError] {}", e.formatted_messages());
                    }
                }
            }
            output.push_str("Styles successfully reloaded\n");
        }

        // Music data — `musicDataMap[key] =` (replace-or-insert).
        let music = format!("{pack_path}Music/");
        if !self.root.join(&music).is_dir() {
            output.push_str("invalid music data folder path\n");
        } else {
            for p in pack_scan::scan_single_by_ext(&self.root, &music, ".json") {
                let (object, _error) = read_file_with_errors(&self.root.join(&p));
                match load_music_from_json(&object) {
                    Ok(music_data) => {
                        let asset_id = format!("{pack_id}_{}", music_data.id);
                        self.state
                            .lock()
                            .expect("asset state poisoned")
                            .music_data_map
                            .insert(asset_id, music_data);
                    }
                    Err(e) => {
                        println!("[ssvuj::logReadError] {}", e.formatted_messages());
                    }
                }
            }
            output.push_str("Music data successfully reloaded\n");
        }

        // Music files — `musicPathMap.emplace` (FIRST WINS — an existing
        // entry is NOT replaced).
        if !self.root.join(&music).is_dir() {
            output.push_str("invalid music folder path\n");
        } else {
            for p in pack_scan::scan_single_by_ext(&self.root, &music, ".ogg") {
                let base = file_name_no_ext(&p);
                let asset_id = format!("{pack_id}_{base}");
                self.state
                    .lock()
                    .expect("asset state poisoned")
                    .music_path_map
                    .entry(asset_id)
                    .or_insert(p);
            }
            output.push_str("Music files successfully reloaded\n");
        }

        // Custom sounds — the `AssetStorage` first-load-wins (`p`
        // = the bare file name WITH the `.ogg`, the C++'s
        // `p.getFileName()`).
        let sounds = format!("{pack_path}Sounds/");
        if !self.root.join(&sounds).is_dir() {
            output.push_str("invalid custom sound folder path\n");
        } else {
            for p in pack_scan::scan_single_by_ext(&self.root, &sounds, ".ogg") {
                let file_name = logical_filename(&p);
                let asset_id = format!("{pack_id}_{file_name}");
                let real = self.root.join(&p);
                let loaded = self
                    .state
                    .lock()
                    .expect("asset state poisoned")
                    .storage
                    .load_sound_buffer(&asset_id, &real);
                if !loaded {
                    // C++: the failed line is appended AND the tail line
                    // still follows (no early return).
                    output.push_str("Failed to load sound buffer '");
                    output.push_str(&p);
                    output.push_str("'\n");
                }
            }
            output.push_str("Custom sound files successfully reloaded\n");
        }

        output
    }

    /// C++ `reloadLevel(mPackId, mPath, mId)` (Assets.cpp:1280-1443): the
    /// debug reload of ONE level — by-NAME file scans (`mId + ".json"`,
    /// `styleId + ".json"`, `musicId + ".json"`, `musicId + ".ogg"` unless
    /// `nullMusicId`, `soundId + ".ogg"` unless `nullSoundId` — all from
    /// the NEWLY loaded level data). Returns the result string (the C++
    /// `return`s early on every terminal arm — the port preserves each).
    pub fn reload_level(&self, pack_id: &str, pack_path: &str, level_id: &str) -> String {
        // Level — the C++ aborts on a missing folder or a missing file.
        let levels = format!("{pack_path}Levels/");
        if !self.root.join(&levels).is_dir() {
            return "invalid level folder path\n".to_string();
        }
        let level_files =
            pack_scan::scan_single_by_name(&self.root, &levels, &format!("{level_id}.json"));
        if level_files.is_empty() {
            return "no matching level data file found\n".to_string();
        }
        // "There is only one file, so we can just subscript index 0."
        let level_file = &level_files[0];
        let (object, _error) = read_file_with_errors(&self.root.join(level_file));
        let level_data = match LevelData::from_json(&object, pack_path, pack_id) {
            Ok(ld) => ld,
            Err(e) => {
                // C++: a type-mismatched field `getExtr` would throw with
                // no surrounding catch (process abort); the port's
                // clean-error deviation class — log + return the output so
                // far (empty at this point, as in the C++'s crash).
                println!("[ssvuj::logReadError] {}", e.formatted_messages());
                return String::new();
            }
        };
        let asset_id = format!("{pack_id}_{level_id}");
        {
            let mut st = self.state.lock().expect("asset state poisoned");
            // The C++'s find/emplace/replace (the id-list append only on a
            // NEW key — as `reload_pack`'s level section).
            if !st.level_datas.contains_key(&asset_id) {
                st.level_data_ids_by_pack
                    .entry(pack_id.to_string())
                    .or_default()
                    .push(asset_id.clone());
            }
            st.level_datas.insert(asset_id, level_data.clone());
        }
        let mut output = format!("level data {level_id}.json successfully loaded\n");

        // Style (the NEWLY loaded level's `styleId`).
        let styles = format!("{pack_path}Styles/");
        if !self.root.join(&styles).is_dir() {
            output.push_str("invalid style folder path\n");
        } else {
            let style_files = pack_scan::scan_single_by_name(
                &self.root,
                &styles,
                &format!("{}.json", level_data.style_id),
            );
            if style_files.is_empty() {
                output.push_str("no matching style file found\n");
            } else {
                let (object, _error) = read_file_with_errors(&self.root.join(&style_files[0]));
                match StyleData::from_json(&object) {
                    Ok(style_data) => {
                        let asset_id = format!("{pack_id}_{}", level_data.style_id);
                        self.state
                            .lock()
                            .expect("asset state poisoned")
                            .style_data_map
                            .insert(asset_id, style_data);
                        output.push_str(&format!(
                            "style data {}.json successfully loaded\n",
                            level_data.style_id
                        ));
                    }
                    Err(e) => {
                        println!("[ssvuj::logReadError] {}", e.formatted_messages());
                        return output;
                    }
                }
            }
        }

        // Music data (the NEWLY loaded level's `musicId`).
        let music = format!("{pack_path}Music/");
        if !self.root.join(&music).is_dir() {
            output.push_str("invalid music data folder path\n");
        } else {
            let music_data_files = pack_scan::scan_single_by_name(
                &self.root,
                &music,
                &format!("{}.json", level_data.music_id),
            );
            if music_data_files.is_empty() {
                output.push_str("no matching music data file found\n");
            } else {
                let (object, _error) = read_file_with_errors(&self.root.join(&music_data_files[0]));
                match load_music_from_json(&object) {
                    Ok(music_data) => {
                        let asset_id = format!("{pack_id}_{}", level_data.music_id);
                        self.state
                            .lock()
                            .expect("asset state poisoned")
                            .music_data_map
                            .insert(asset_id, music_data);
                        output.push_str(&format!(
                            "music data {}.json successfully loaded\n",
                            level_data.music_id
                        ));
                    }
                    Err(e) => {
                        println!("[ssvuj::logReadError] {}", e.formatted_messages());
                        return output;
                    }
                }
            }
        }

        // Music files (`nullMusicId` → the section is skipped; the
        // missing-folder line still fires — the C++'s `else if` gates the
        // INNER half only).
        if !self.root.join(&music).is_dir() {
            output.push_str("invalid music folder path\n");
        } else if level_data.music_id != "nullMusicId" {
            let asset_id = format!("{pack_id}_{}", level_data.music_id);
            let music_files = pack_scan::scan_single_by_name(
                &self.root,
                &music,
                &format!("{}.ogg", level_data.music_id),
            );
            if music_files.is_empty() {
                output.push_str("no matching music file found\n");
            } else {
                // C++ `musicPathMap.emplace` — first wins.
                self.state
                    .lock()
                    .expect("asset state poisoned")
                    .music_path_map
                    .entry(asset_id)
                    .or_insert(music_files[0].clone());
            }
        }

        // Sound files (`nullSoundId` → the C++'s early `return output`).
        if level_data.sound_id == "nullSoundId" {
            return output;
        }
        let sounds = format!("{pack_path}Sounds/");
        if !self.root.join(&sounds).is_dir() {
            output.push_str("invalid custom sound folder path\n");
            return output;
        }
        // The C++'s `assetId` here is WITHOUT the extension (`soundId`
        // only — unlike `reload_pack`'s scan, which keys on the bare file
        // name WITH `.ogg`; the port mirrors the C++ verbatim).
        let sound_asset_id = format!("{pack_id}_{}", level_data.sound_id);
        // The C++'s `hasSoundBuffer` short-circuit.
        if self
            .state
            .lock()
            .expect("asset state poisoned")
            .storage
            .has_sound_buffer(&sound_asset_id)
        {
            output.push_str(&format!(
                "custom sound file {}.ogg is already loaded\n",
                level_data.sound_id
            ));
            return output;
        }
        let sound_files = pack_scan::scan_single_by_name(
            &self.root,
            &sounds,
            &format!("{}.ogg", level_data.sound_id),
        );
        if sound_files.is_empty() {
            output.push_str("no matching custom sound file found\n");
            return output;
        }
        let real = self.root.join(&sound_files[0]);
        let loaded = self
            .state
            .lock()
            .expect("asset state poisoned")
            .storage
            .load_sound_buffer(&sound_asset_id, &real);
        if !loaded {
            output.push_str("Failed to load sound buffer '");
            output.push_str(&sound_files[0]);
            output.push_str("'\n");
            return output;
        }
        output.push_str(&format!(
            "new custom sound file {}.ogg successfully loaded\n",
            level_data.sound_id
        ));
        output
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
