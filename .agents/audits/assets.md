# Audit: `Global/Assets` (C++ → Rust 1:1 port)

**C++ side (read completely, 1875 lines):** `src/SSVOpenHexagon/Global/Assets.cpp` (+ `include/SSVOpenHexagon/Global/Assets.hpp`, + `src/SSVOpenHexagon/Global/AssetStorage.cpp` / `.hpp` for the storage half).
**Rust side (read completely):** `crates/oh-assets/src/assets.rs`, `crates/oh-assets/src/asset_storage.rs`, plus the crate's supporting modules the pair depends on: `crates/oh-assets/src/pack_scan.rs`, `crates/oh-assets/src/shaders.rs`, `crates/oh-assets/src/lib.rs`. Cross-crate verification: `crates/oh-data/src/{pack_data,level_data,profile_data,load_info,utils}.rs`, `crates/oh-menu/src/{seams,real,menu_game,menu}.rs`, `crates/oh-game/src/seams.rs`, `crates/oh-app/src/{client,lifecycle,compose,windowed,dispatch}.rs`, `crates/oh-steam/src/manager.rs`.

**Method:** behavior inventory of every C++ member (below §1), implemented list of the Rust pair (§2), adversarial diff (§3 MISSING, §4 SEAMS, §5 DIFFERENT). Every suspected absence verified by grep; exact command + result recorded in the MISSING table.

---

## 1. C++ BEHAVIOR INVENTORY (`src/SSVOpenHexagon/Global/Assets.cpp`)

### 1.1 File-static helpers
| # | Item | file:line | Behavior |
|---|------|-----------|----------|
| S1 | `loadAssetsFromJson` | 168–207 | Reads `fonts` / `textures` / `soundBuffers` `vector<string>` keys from `assets.json` (no defaults → missing key = empty list; wrong type → `getExtr` throws), loads each into `AssetStorage`, logs per failure. |
| S2 | `getScanBuffer` / `scanSingleByExt` / `scanSingleByName` / `scanSingleFolderName` | 210–245 | Single-level directory scans (file-by-ext, file-by-name, folder), shared static buffer, SSVUtils `Sort::Alphabetic` default (byte-sorted normalized full paths; folders get trailing `/`). |

### 1.2 `HGAssetsImpl` state (44–101)
`steamManager` (60), `levelsOnly` (62), `assetStorage` (64), `levelDatas` (66), `levelDataIdsByPack` (67–68), `packDatas` (70), `packInfos` (72), `selectablePackInfos` (73), `musicPathMap` (75), `musicDataMap` (76, `std::map` = sorted), `styleDataMap` (77, sorted), `profileDataMap` (78, `std::map`), `currentProfilePtr` (79), `packIdsWithMissingDependencies` (81), `LoadedShader` + `shaders` / `shadersPathToId` / `shadersById` (83–100), `buf` (84), `luaFileCache` (86), `loadInfo` (87), `cachedWorkshopPackIds` (91).

### 1.3 Member functions (pimpl body)
| # | Item | file:line | Behavior / side effects |
|---|------|-----------|-------------------------|
| M1 | ctor `HGAssetsImpl` | 308–391 | (a) `!levelsOnly && !headless`: `Assets/` must exist else **FATAL `std::terminate()`** (317–324); parse `Assets/assets.json` → `loadAssetsFromJson` + `addFormattedError`. (b) `loadAllPackDatas` false → terminate (326–331). (c) `loadAllPackAssets(headless)` false → terminate (333–338). (d) `verifyAllPackDependencies` false → terminate (340–345, always true). (e) `loadAllLocalProfiles` false → log only (347–352). (f) sort `levelDataIdsByPack` by `menuPriority` (354–361); sort `packInfos` + `selectablePackInfos` by `PackData::priority` (363–370). (g) `loadInfo.errorMessages.shrink_to_fit()` (382). (h) log elapsed ms (384–390). |
| M2 | dtor | 393–396 | Log only. |
| M3 | `loadPackData` | 398–479 | `pack.json` presence check → false if absent; read `disambiguator`("no disambiguator")/`name`("unknown name")/`author`("unknown author")/`description`("no description")/`version`(0)/`priority`(100) + `dependencies` array (`disambiguator`/`name`/`author` no default, `min_version` no default); `buildPackId`; `packInfos.emplace_back`; `packDatas.emplace` (**first wins**). |
| M4 | `loadPackAssets` | 481–552 | Gated sub-loaders: `!headless && Shaders/ && !levelsOnly` → shaders; `!headless && !levelsOnly && Sounds/` → custom sounds; `!levelsOnly && Music/` → (`!headless` → ogg map) + json data; `Styles/` → style data; `Levels/` → level data. Any exception → record `"FATAL ERROR, Exception during asset loading: …"` in `loadInfo` + return false. Then `packHasLevels` → `selectablePackInfos.emplace_back`. |
| M5 | `loadPackAssets_loadShaders` | 873–918 | Headless early-return; scan `.vert`/`.geom`/`.frag`; per-file `sf::Shader::loadFromFile` (fail → log + continue); id = `shadersById.size()` after push; key `packId + '_' + fileName`; `shadersPathToId`; `++loadInfo.assets`. |
| M6 | `loadPackAssets_loadCustomSounds` | 920–934 | `Sounds/*.ogg` → `assetStorage->loadSoundBuffer(packId + '_' + fileName)` (fail → log); `++loadInfo.assets` **even on failure**. |
| M7 | `loadPackAssets_loadMusic` | 936–946 | `Music/*.ogg` → `musicPathMap.emplace(packId + '_' + fileNameNoExt, path)`; `++loadInfo.assets`. |
| M8 | `loadPackAssets_loadMusicData` | 948–962 | `Music/*.json` → `loadMusicFromJson`; `musicDataMap.emplace(packId + '_' + id)`; `addFormattedError`; `++loadInfo.assets`. |
| M9 | `loadPackAssets_loadStyleData` | 964–978 | `Styles/*.json` → `StyleData{obj}`; `styleDataMap.emplace(packId + '_' + id)`; `++loadInfo.assets`. |
| M10 | `loadPackAssets_loadLevelData` | 980–999 | `Levels/*.json` → `LevelData{obj, mPath, mPackId}`; `levelDataIdsByPack[mPackId].emplace_back(assetId)`; `levelDatas.emplace` (**first wins**); `++loadInfo.levels`. |
| M11 | `getLoadResults` | 554–557 | Returns `loadInfo` (the load-results screen data). |
| M12 | `getTexture` / `getTextureOrNullTexture` | 559–575 | Assert / static null texture fallback. |
| M13 | `getFont` / `getFontOrNullFont` | 577–592 | Same for fonts. |
| M14 | `isValidLevelId` / `getLevelData` | 594–605 | find / assert+at. |
| M15 | `packHasLevels` / `getLevelIdsByPack` | 607–619 | count / assert+at. |
| M16 | `getPackDatas` / `isValidPackId` / `getPackData` | 621–638 | map access; assert on get. |
| M17 | `getSelectablePackInfos` | 640–643 | vector ref. |
| M18 | `findPackData` | 645–660 | First match on (disambiguator, name, author) over `packDatas` (unordered → impl-defined first). |
| M19 | `loadWorkshopPackDatasFromCache` | 662–710 | Read `workshopCache.json`; validate `cachedPacks` (null / non-array / empty → log + false); fill `cachedWorkshopPackIds`; missing key → `"[ERROR]: Cannot locate cache array…"` + false; `addFormattedError(cacheError)`. |
| M20 | `loadAllPackDatas` | 715–768 | `Packs/` existence check; local packs via folder scan → `tryLoadPackFromPath` (fail → `"Error loading pack data '…'"` into `loadInfo`; success → `++loadInfo.packs`); **Steam branch** (743–760): `steamManager != nullptr` → if initialized: `for_workshop_pack_folders(tryLoadPackFromPath)`; else if `loadWorkshopPackDatasFromCache()`: try-load every `cachedWorkshopPackIds` path. |
| M21 | `loadAllPackAssets` | 770–790 | Iterate `packDatas` (unordered); per-pack `loadPackAssets`; fail → `"Error loading pack info 'packId'"` + return false. |
| M22 | `verifyAllPackDependencies` | 792–835 | Per pack per dependency: any pack with same (disambiguator,name,author) and `version >= minVersion`? else `"Missing pack dependency 'X' for pack 'Y'"` + `packIdsWithMissingDependencies.emplace`; then `eraseRemoveIf(selectablePackInfos, id in missing)`. Always returns true. |
| M23 | `addLocalProfile` | 839–847 | Erase favorite ids not in `levelDatas`; `profileDataMap.emplace` (first wins). |
| M24 | `loadAllLocalProfiles` | 849–871 | `Profiles/` missing → log + false (non-fatal); scan `*.json`; `loadProfileFromJson`; `addLocalProfile`; `addFormattedError`. |
| M25 | `saveCurrentLocalProfile` | 1001–1025 | **File write**: if `currentProfilePtr == nullptr` return; JSON `{version{major,minor,micro}=GAME_VERSION, name, scores, favorites[]}` → `Profiles/<name>.json`. |
| M26 | `saveAllProfiles` | 1027–1058 | **File write**: same shape for every `profileDataMap` entry → `Profiles/<name>.json`. |
| M27 | `getMusicData` | 1060–1075 | `musicDataMap.find(packId_id)`; miss → log + `assert(!empty)` + **return `begin()->second`** (smallest key, sorted map). |
| M28 | `getStyleData` | 1077–1092 | Same fallback for styles. |
| M29 | `getShader` / `getShaderId` / `getShaderIdByPath` / `getShaderByShaderId` / `isValidShaderId` | 1094–1156 | Registry lookups; miss → null / nullopt. |
| M30 | `reloadAllShaders` | 1159–1171 | Re-`loadFromFile` every registered shader in place (fail → log). |
| M31 | `reloadPack` | 1174–1278 | Debug reload of a whole pack: `Levels/` missing → `"invalid level folder path\n"` (abort all); rescan `Levels/*.json` → insert-or-**replace** in `levelDatas` + append to `levelDataIdsByPack`; `Styles/*.json` → `styleDataMap[key]=` replace; `Music/*.json` → `musicDataMap[key]=` replace; `Music/*.ogg` → `musicPathMap.emplace`; `Sounds/*.ogg` → `loadSoundBuffer` (fail → `"Failed to load sound buffer '…'"`); returns a multi-line result string. |
| M32 | `reloadLevel` | 1280–1443 | Debug reload of one level: by-name file scans (`mId+".json"`, `styleId+".json"`, `musicId+".json"`, `musicId+".ogg"` unless `nullMusicId`, `soundId+".ogg"` unless `nullSoundId`); insert-or-replace level/style/music data; `hasSoundBuffer` short-circuit (`"…already loaded"`); `loadSoundBuffer` on new sound; precise result strings per section. |
| M33 | `getLocalScore` / `setLocalScore` | 1446–1454 | Current profile's `getScore` / `setScore` (assert profile active). Callers: `HexagonGame.cpp:871,908,915` (personal best). |
| M34 | `anyLocalProfileActive` / `getCurrentLocalProfile` (r+c) / `getLocalProfileByName` (r+c) | 1459–1488 | `currentProfilePtr != nullptr` / assert+deref / assert+find. Callers: `HGUpdate.cpp:393` (restart gate), `HexagonGame.cpp:615,995,1179`, `MenuGame.cpp:2979,4789,4845,4893,5570`. |
| M35 | `getCurrentLocalProfileFilePath` | 1490–1494 | `"Profiles/" + name + ".json"`. |
| M36 | `getLocalProfilesSize` / `getLocalProfileNames` | 1497–1514 | map size / names in `std::map` (byte-sorted) order. |
| M37 | `pIsValidLocalProfile` / `pGetName` | 1517–1525 | Active? / current profile name (`MenuGame.cpp:2408,5796`). |
| M38 | `pSaveCurrent` / `pSaveAll` | 1527–1535 | → M25 / M26. Callers: `MenuGame.cpp:1785`, `main.cpp:384` (exit scope guard). |
| M39 | `pSetCurrent` | 1537–1543 | Find + assert + set `currentProfilePtr`. |
| M40 | `pCreate` | 1545–1561 | **File write** `Profiles/<name>.json` `{name, scores:{}, favorites:{}}`; `profileDataMap.clear()`; reload all profiles; reload failure → **terminate**. |
| M41 | `pRemove` | 1563–1566 | `profileDataMap.erase(name)` (in-memory only; the file delete lives in `MenuGame::eraseAction`, `MenuGame.cpp:2420–2426`). |
| M42 | `getSoundBuffer` / `getMusicPath` | 1568–1580 | storage lookup / `musicPathMap` lookup. |
| M43 | `getLevelDatas` / `getPackIdsWithMissingDependencies` | 1582–1591 | map / set refs. |
| M44 | `getLuaFileCache` (r + c) | 1593–1603 | Exposes the per-`Assets` Lua source cache. Callers: `Utils.cpp:53` (`runLuaFileCached`), `MenuGame.cpp:1525` (advanced "clear lua file cache" → `.clear()`). |
| M45 | `HGAssets` pimpl facade | 1607–1871 | One-to-one forwarding of all of the above; ctor forwards `(steamManager, headless, levelsOnly)`. |

**Events/subscriptions:** none (Assets is not an event subscriber; no risingEdge polls).
**Side effects:** file reads (`assets.json`, `pack.json`, `Levels/`/`Styles/`/`Music/`/`Sounds/`/`Shaders/` files, `Profiles/*.json`, `workshopCache.json`); file writes (`saveCurrentLocalProfile`, `saveAllProfiles`, `pCreate`); `std::terminate()` on 4 fatal constructor conditions; SFML GPU/audio object creation (textures, fonts, sound buffers, shaders).
**State transitions:** `currentProfilePtr` set by `pSetCurrent` (M39); `profileDataMap` rebuilt by `pCreate` (M40); level/style/music/sound tables grown/updated by `reloadPack`/`reloadLevel` (M31/M32); `selectablePackInfos` built in M4 and pruned in M22; `LoadInfo` accumulated throughout, surfaced by M11.

---

## 2. RUST IMPLEMENTED LIST (`crates/oh-assets`)

### 2.1 `assets.rs`
| Item | file:line | Behavior |
|------|-----------|----------|
| `FatalLoadError` | 39–43 | Replacement for `std::terminate()` (carries `LoadInfo` + reason). |
| `Assets` struct | 48–91 | Mirrors the C++ state **minus** `currentProfilePtr`, `luaFileCache`, `cachedWorkshopPackIds`, `buf` (replaced by `format!`), and the steam manager. |
| `Assets::new(root, headless, levels_only)` | 94–157 | Constructor flow (1)–(6): `Assets/` check → fatal; `assets.json` + `load_assets_from_json`; `load_all_pack_datas` (false → fatal "Folder Packs/ does not exist"; JsonError → fatal "Exception during asset loading: …"); `load_all_pack_assets` (Err → fatal); `verify_all_pack_dependencies`; `load_all_local_profiles` (non-fatal); 3 sorts. Missing vs C++: `shrink_to_fit`, timing log, Steam branch. |
| `fatal` | 159–164 | Builds `FatalLoadError`. |
| `load_all_pack_datas` | 174–190 | `Packs/` check + local folder scan → `try_load_pack_from_path`; **Steam branch absent** (documented, 186–187). |
| `try_load_pack_from_path` | 192–205 | Same `loadInfo.packs` / `"Error loading pack data '…'"` semantics. |
| `load_pack_data` | 208–233 | `pack.json` check; fields via `json::extr_*` (same defaults as C++); `build_pack_id`; `pack_data_from_json` (description/priority/dependencies, `crates/oh-data/src/pack_data.rs:44–101`); `pack_infos.push` + `pack_datas.insert` (note: `insert` ≠ C++ `emplace`). |
| `load_all_pack_assets` | 235–254 | Sorted pack-id iteration (determinism note 236–238); per-pack error message + Err. |
| `load_pack_assets` | 256–280 | Sub-loader run; fatal-message recording `"FATAL ERROR, Exception during asset loading: …"`; `pack_has_levels` → `selectable_pack_infos.push`. |
| `run_pack_subloaders` | 281–308 | Exact C++ gating (Shaders/Sounds/Music/Styles/Levels × headless × levels_only). |
| `load_pack_assets_load_shaders` | 310–335 | `.vert`/`.geom`/`.frag`; "file exists" stand-in for `loadFromFile`; registry `add`; `load_info.assets += 1`. |
| `load_pack_assets_load_custom_sounds` | 337–349 | `.ogg` → `load_sound_buffer`; `load_info.assets += 1` even on failure (parity with C++). |
| `load_pack_assets_load_music` | 351–359 | `.ogg` → `music_path_map.insert` (no-ext key); `assets += 1`. |
| `load_pack_assets_load_music_data` | 361–377 | `.json` → `load_music_from_json`; `music_data_map.insert`; `assets += 1`. |
| `load_pack_assets_load_style_data` | 379–396 | `.json` → `StyleData::from_json`; `style_data_map.insert`; `assets += 1`. |
| `load_pack_assets_load_level_data` | 398–424 | `.json` → `LevelData::from_json` (`oh-data/src/level_data.rs:47`); ids-by-pack push; `level_datas.insert`; `levels += 1`. |
| `verify_all_pack_dependencies` | 426–449 | Same error string + missing-deps set + `retain` prune. |
| `dependency_exists` | 451–457 | Same 4-field predicate. |
| `load_all_local_profiles` | 464–480 | `Profiles/` check (log-equivalent, non-fatal); scan `.json`; `load_profile_from_json` (fail → skip, vs C++ which would throw); `add_local_profile`. |
| `add_local_profile` | 482–494 | Favorite-id filter + insert (private; C++ version is public but has no external callers). |
| `sort_level_ids_by_menu_priority` | 503–517 | Stable sort by snapshot priorities (vs C++ `std::sort`). |
| `sort_pack_infos_by_priority` / `sort_selectable_pack_infos_by_priority` | 519–529 → `sort_infos_by_priority` 768–779 | Priority-ascending sort; missing pack id → 0.0 fallback (vs C++ assert). |
| Getters | 533–722 | `get_load_results` (533), `root` (539, Rust-only), `levels_only` (544, Rust-only), `get_texture_or_null` (550), `get_font_or_null` (555), `get_texture` (560), `get_font` (565), `get_sound_buffer` (570), `get_sound_path` (578, Rust-only for oh-audio), `is_valid_level_id` (584), `get_level_data` (589), `pack_has_levels` (595), `get_level_ids_by_pack` (600), `get_pack_datas` (605), `is_valid_pack_id` (611), `get_pack_data` (616), `get_selectable_pack_infos` (622), `find_pack_data` (629), `get_music_data` (643, BTreeMap `iter().next()` fallback), `get_style_data` (651), `get_music_path` (659), `get_level_datas` (664), `get_pack_ids_with_missing_dependencies` (669), `get_profile_data` (675, Rust-only), `get_local_profiles_size` (680), `get_profile_names` (687), `get_shader` (697), `get_shader_id` (702), `get_shader_id_by_path` (707), `get_shader_by_shader_id` (712), `is_valid_shader_id` (718). |
| `load_assets_from_json` | 726–748 | `if let Ok` per key (silent skip on wrong type; C++ throws). |
| `file_name_no_ext` / `logical_filename` | 752–766 | C++ `getFileNameNoExtensions` / `getFileName`. |

### 2.2 `asset_storage.rs`
`AssetRef` (36–41), `AssetStorage{textures,fonts,sound_buffers,sound_paths}` (47–56), `new` (61), `try_emplace_and_load` (68–78, emplace-once semantics), `load_texture` (80), `load_font` (85), `load_sound_buffer` (90–104, first-load-wins path recording), `get_sound_path` (106, Rust-only), `get_texture` (112), `get_font` (117), `get_sound_buffer` (122), `has_texture` (128), `has_font` (133), `has_sound_buffer` (138), `texture_or_null` (144), `font_or_null` (153). Parity with C++ `AssetStorage.cpp:37–75,88–128` is exact for the registry semantics (failed load keeps an entry; second load no-ops).

### 2.3 Supporting modules
`pack_scan.rs` — ports the 4 scan call sites with the byte-sorted, trailing-slash-on-folders, case-sensitive-ext rule (whole file; tests pin it).
`shaders.rs` — `ShaderRegistry` = `shaders` + `shadersPathToId` + `shadersById` without the GPU payload (`add` 47–59, `get` 62–65, `get_id_by_name` 68–70, `get_id_by_path` 73–75, `get_by_id` 82–88, `is_valid_id` 91–93).

### 2.4 Explicit stubs / no-ops / TODOs in the pair (quoted)
- `assets.rs:10–22` module docs: *"What is intentionally deferred (documented, not silently dropped): … the actual SFML texture/font/sound/shader handles … Steam Workshop pack enumeration (`steamManager == nullptr` here … oh-steam owns it) … the profile mutation/save API (`pCreate`/`pSetCurrent`/`saveAllProfiles` / `getLocalScore` ...) … the `reloadPack` / `reloadLevel` / `reloadAllShaders` dev-tool paths."*
- `assets.rs:87`: constructor *"minus the Steam manager (see module docs) and the timing log"*.
- `assets.rs:186–187`: *"Steam Workshop packs (C++ `if(steamManager != nullptr) ...`): this port has no steam manager, so the branch is skipped (oh-steam)."*
- `asset_storage.rs:9–21`: *"The `loadFromFile` success here is stood in by 'the file exists on disk' (deterministic, no SFML). A real binary parse is issue 10's job"*.
- `shaders.rs:6–14`: *"GLSL compilation is issue 10's job."*
- (Outside the pair, but standing in for the deferred behavior — `oh-menu/src/real.rs:170–172`): `fn profile_create(&self, _name: &str) {}` / `fn profile_remove(&self, _name: &str) {}` / `fn reload_assets(&self, _entire_pack: bool, _current_level_id: &str) {}` — documented no-op seams.

---

## 3. MISSING (C++ behavior with no Rust implementation anywhere in the tree), highest risk first

| Item | C++ evidence (file:line) | Absence proof (command + result) | Risk |
|------|--------------------------|----------------------------------|------|
| **Profile save at exit** — `saveAllProfiles` + `pSaveAll` (scope guard, `main.cpp:382–386` calling `assets.pSaveAll()` at `main.cpp:384`): writes every `Profiles/<name>.json` (version block + scores + favorites) | `Assets.cpp:1027–1058` (impl), `:1532–1535` (`pSaveAll`), `:1815–1818` (facade); caller `src/SSVOpenHexagon/Core/main.cpp:384` | `grep -rn 'save_all_profiles\|save_current_local_profile\|write_to_file.*[Pp]rofile\|profile.*write' crates/oh-assets/src crates/oh-data/src` → only `crates/oh-data/src/profile_data.rs:160` (a comment inside a test). `grep -rn 'pSaveAll' crates/oh-app/src` → comments only: `client.rs:23`, `client.rs:97`, `client.rs:116` ("C++ main.cpp:381-387 pSaveAll scope guard — issue 17: the profile mutation API is unported"), `lifecycle.rs:94–96`. No Rust code writes any profile JSON. | **uservisible-gameplay** (all local scores/favorites lost at every exit) |
| **Save current profile before local play** — `saveCurrentLocalProfile` + `pSaveCurrent` (`MenuGame::playLocally` calls it first) | `Assets.cpp:1001–1025`, `:1527–1530`; caller `MenuGame.cpp:1785` | Same P3 grep above (no writer exists). Rust `play_locally` (`crates/oh-menu/src/menu_game.rs:630–638`) performs no save; its own doc comment admits the gap: `menu_game.rs:628` *"C++ `playLocally()` — `assets.pSaveCurrent(); …`"*. | **uservisible-gameplay** |
| **Local personal-best read/write** — `getLocalScore` / `setLocalScore` (PB detection + "PB: …s" display in-game, and score persistence after a run) | `Assets.cpp:1446–1454`, `:1744–1752`; callers `HexagonGame.cpp:871,908,915` | `grep -rn 'setScore\|set_score' crates/oh-assets/src crates/oh-game/src` → only the seam trait doc `crates/oh-game/src/seams.rs:100`. Sole non-test `Profiles` impl in the tree is `NullProfiles` (`seams.rs:105–118`: `get_local_score → 0.0`, `set_local_score {}`), used by the binary: `oh-app/src/client.rs:187`, `oh-app/src/windowed.rs:2112`, `oh-app/src/dispatch.rs:115`. No real implementation anywhere in `crates/*/src`. | **uservisible-gameplay** (PB never detected correctly / never stored; in-game PB line always 0.0) |
| **Current-profile state machine** — `currentProfilePtr` + `pSetCurrent` / `pIsValidLocalProfile` / `pGetName` / `anyLocalProfileActive` / `getCurrentLocalProfile` (drives: restart gate `HGUpdate.cpp:393`, replay `player_name` `HexagonGame.cpp:615,995–997`, "CURRENT PROFILE:" line + erase-guard `MenuGame.cpp:2408,5796`, favorites `MenuGame.cpp:2979,4789,4845,4893`) | `Assets.cpp:79` (state), `:1459–1488`, `:1517–1543`, `:1800–1828` (facade) | `grep -rn 'current_profile\|currentProfile' crates/oh-assets/src` → **no matches** (exit 1). Engine side: `NullProfiles` hardcodes `any_local_profile_active → false`, `current_local_profile_name → ""` (`oh-game/src/seams.rs:105–118`); the menu-side `current_profile` is a menu-local `RefCell<Option<String>>` that never touches `Assets` or the profile data (`oh-menu/src/real.rs:31,158–163`). | **uservisible-gameplay** |
| **Profile creation** — `pCreate`: writes `Profiles/<name>.json` `{name, scores:{}, favorites:{}}`, clears the whole in-memory profile map, reloads (terminate on reload failure) | `Assets.cpp:1545–1561`, `:1825–1828`; caller `MenuGame.cpp:2235` | `grep -rn 'pCreate\|profile_create' crates/oh-menu/src crates/oh-app/src` → `menu_game.rs:676` calls `self.assets.profile_create(&name)` but `real.rs:170` is `fn profile_create(&self, _name: &str) {}` (no-op); `oh-menu/src/seams.rs:753–757` is the fake (records into `inner.current_profile` only). No file write, no map rebuild, no reload. | **uservisible-gameplay** (new profiles not persisted; they vanish on restart) |
| **Profile removal** — `pRemove`: erase from `profileDataMap` (the file delete is MenuGame's; this is the Assets half) | `Assets.cpp:1563–1566`, `:1830–1833`; caller `MenuGame.cpp:2430` | `grep -rn 'profile_remove' crates/oh-menu/src` → `menu_game.rs:1778` calls it; `real.rs:171` is `fn profile_remove(&self, _name: &str) {}` (no-op). Removed profiles come back on the next load. | **uservisible-gameplay** |
| **Favorites via the current profile** — `getCurrentLocalProfile().isLevelFavorite/getFavoriteLevelIds/addFavoriteLevel/removeFavoriteLevel` (favorite star, favorites drawer, "(un)favorite level" action) | `Assets.cpp:1464,1477` (the accessor the C++ menu uses); call sites `MenuGame.cpp:2979,4789,4845,4893` | `real.rs:164–166`: `fn is_level_favorite(&self, _level_id: &str) -> bool { false }` — favorites are always "off" in the real seam; `change_favorite_levels_to_profile` is an empty stub: `menu_game.rs:643–651` (*"Coarse: favorites list is derived from `assets` when entering the favorites view (see `switchToFromFavoriteLevels`, a 13b detail)."*). | **uservisible-gameplay** |
| **Level/pack debug reload** — `reloadAllShaders` + `reloadPack` + `reloadLevel` (re-read level/style/music/sound tables from disk in place, recompile shaders, return the result log the dialog shows; insert-or-replace semantics) | `Assets.cpp:1159–1443` (impls), `:1727–1742` (facade); caller `MenuGame.cpp:3092–3125` (`reloadAssets`, `Config::getDebug()`-gated) | `grep -rn 'reload_pack\|reload_level\|reload_all_shaders\|reloadPack\|reloadLevel\|reloadAllShaders' crates/*/src` → **only** `crates/oh-assets/src/assets.rs:20` (the deferral comment). Real impl: `real.rs:172` `fn reload_assets(&self, _entire_pack: bool, _current_level_id: &str) {}`. | **uservisible-gameplay** (level-creation workflow: edited pack files never take effect; dialog claims success) |
| **Workshop-cache fallback pack loading** — `loadWorkshopPackDatasFromCache` + `cachedWorkshopPackIds` + the workshop half of `loadAllPackDatas` (when the Steam API is unavailable, installed workshop packs are still loaded from `workshopCache.json`) | `Assets.cpp:662–710` (impl), `:743–760` (branch in `loadAllPackDatas`), `:91` (state) | `grep -rn 'workshopCache' crates/*/src` → **only the write side**: `oh-steam/src/manager.rs:246,249–252` (`std::fs::write("workshopCache.json", …)`). No Rust code ever *reads* `workshopCache.json`; `Assets::new` takes no Steam manager (`assets.rs:94–157`) and `load_all_pack_datas` has no workshop branch (`assets.rs:174–190`). The declared seam (assets.rs:13–15) only covers live enumeration — the cache fallback is reachable C++ behavior (Steam present but uninitialized) with no Rust counterpart anywhere. | **uservisible-gameplay** (workshop packs disappear exactly in the Steam-down case the C++ cache was built for) |
| **"Clear lua file cache" advanced action** — `getLuaFileCache().clear()` (MenuGame.cpp:1525); the cache itself was relocated to `oh-app` and works, but the clear does not | `Assets.cpp:1593–1603` (accessor), `:86` (member); caller `MenuGame.cpp:1525` | `grep -rn 'clear_lua_file_cache' crates/oh-app/src` → `windowed.rs:995–1001`: body is empty with comment *"the composition's host owns the equivalent map (compose.rs) and exposes no clear — documented no-op (the cache is per-process, so a new run clears it)."* (The cache map itself exists at `compose.rs:172–180`.) | **internal** (dev convenience; stale scripts only observable while a pack is edited mid-session) |
| Constructor tail bookkeeping — `loadInfo.errorMessages.shrink_to_fit()` + "Loaded all assets in …ms" timing log | `Assets.cpp:382–390` | `grep -rn 'shrink_to_fit\|Loaded all assets' crates/oh-assets/src` → no matches. Rust doc: `assets.rs:87` *"minus the Steam manager (see module docs) and the timing log"*. | **none** |

**Note on `addLocalProfile` visibility:** C++ exposes it publicly (`Assets.cpp:1857`), Rust keeps it private (`assets.rs:482`). No external C++ caller exists (only `loadAllLocalProfiles`), so this is not a behavior gap.

---

## 4. SEAMS (accepted, documented deviations)

| Seam | Marker (quoted) | C++ side | Rust side |
|------|-----------------|----------|-----------|
| **Steam workshop enumeration** (live) inside `Assets` | `assets.rs:13–15`: *"Steam Workshop pack enumeration (`steamManager == nullptr` here, so the C++ `if(steamManager != nullptr)` branch is skipped; oh-steam owns it)"*; `assets.rs:186–187` | `Assets.cpp:743–760` | `oh-steam/src/manager.rs:488–499` (`for_workshop_pack_folders`) exists but is **not wired into `Assets::new`** — integration pending; the cache-fallback half is NOT covered by this marker (see MISSING #9) |
| **SFML handle payloads** (textures/fonts/sound buffers) + **GLSL compilation** — registry records `loaded`/ids/paths only | `assets.rs:11–13`: *"the actual SFML texture/font/sound/shader *handles* (render/audio backend, issue 10) — the registry records `loaded`/ids only"*; `asset_storage.rs:9–21`: *"The `loadFromFile` success here is stood in by 'the file exists on disk' (deterministic, no SFML)"*; `shaders.rs:6–14`: *"GLSL compilation is issue 10's job"* | `AssetStorage.cpp:37–75`; `Assets.cpp:873–918` | `asset_storage.rs:68–157`; `shaders.rs:44–93`; extra `get_sound_path` for `oh-audio` (`asset_storage.rs:101–110`) |
| **Profile editor/mutation API deferred** to "issue 17" | `assets.rs:16–19`: *"the profile mutation/save API … — profile *loading* is ported … the profile *editor* API is out of this task's build list"*; `oh-app/src/client.rs:116`: *"issue 17: the profile mutation API is unported"*; `oh-menu/src/real.rs:4–8`: *"those are documented no-op seams here"*; `oh-game/src/seams.rs:8–13` | `Assets.cpp:1001–1566` (see MISSING §3) | Test doubles: `NullProfiles` (`oh-game/src/seams.rs:105–118`), `RealMenuAssets.current_profile` RefCell (`oh-menu/src/real.rs:31`), menu fake `seams.rs:739–768`. **Documentation ≠ behavior**: the underlying C++ behavior is absent (MISSING §3), listed here only because the deferral is explicitly marked |
| **`luaFileCache` relocated** out of `Assets` into the Lua host (one host per one `Assets` → "per-assets" scope preserved) | `oh-app/src/compose.rs:172–180`: *"C++ `assets.getLuaFileCache()` — a per-assets `unordered_map<string, string>`. oh-assets defers that member, so the host owns the equivalent map"* | `Assets.cpp:86,1593–1603` | `LuaScriptHost.file_cache` (`compose.rs:179`) + `run_lua_file_cached` (`compose.rs:233+`) |
| **`std::terminate()` → `FatalLoadError` + exit 1** (fatal-load conditions now recoverable) | `assets.rs:34–42`: *"A fatal asset-load condition (the C++ constructor `std::terminate()`s on these). The port instead returns this"*; `oh-app/src/client.rs:104–110` | `Assets.cpp:317–352` | `FatalLoadError` (`assets.rs:39–43`); `client.rs` prints `[FATAL ERROR] …` and returns 1 |
| **Explicit `root` parameter** instead of process-CWD global | `assets.rs:6–8`: *"Everything is threaded through an explicit `root` (the process CWD in C++, the `_RELEASE` directory in tests)"* | all C++ paths are CWD-relative | `Assets::new(root, …)` (`assets.rs:94`); `root()`/`levels_only()` accessors (`assets.rs:539–545`) |

---

## 5. DIFFERENT (present but renamed / behaviorally different)

| # | C++ | Rust | Delta | Risk |
|---|-----|------|-------|------|
| D1 | `getTexture`/`getFont`/`getSoundBuffer` return SFML object refs (assert; empty object after failed load) — `Assets.cpp:559–592,1568–1572` | `Option<bool>` (present? loaded?) + `AssetRef` — `assets.rs:550–576`, `asset_storage.rs:112–126` | Payload is the issue-10 stand-in (SEAM S2); registry semantics (failed load keeps entry; `has*` true) are preserved exactly | internal |
| D2 | `getMusicData`/`getStyleData` on **empty** map: `SSVOH_ASSERT(!map.empty())` then `begin()->second` (release: UB) — `Assets.cpp:1060–1092` | `Option` fallback: smallest-key entry via `BTreeMap::iter().next()`, `None` when empty — `assets.rs:643–657` | Non-empty case identical (smallest key, both sorted maps); empty-map edge changed from assert/UB to `None` | internal |
| D3 | `getShader*` return live `sf::Shader*` (compiled GLSL) — `Assets.cpp:1094–1156` | `LoadedShader{path, shader_type, id}` registry — `shaders.rs:24–93`, `assets.rs:697–720` | GLSL payload deferred (SEAM S2); id allocation/lookup semantics identical (`by_id` index == id) | internal |
| D4 | `loadPackData` JSON **type** mismatch (e.g. `version` as string) → `getExtr` throws, uncaught → process abort | `JsonError` → `FatalLoadError("Exception during asset loading: …")` — `assets.rs:117–124,208–233` | Documented (`assets.rs:119–123`); outcome: clean error instead of abort | internal |
| D5 | `loadAssetsFromJson`: wrong-type `fonts`/`textures`/`soundBuffers` → throw → abort | `if let Ok(…)` → key silently skipped — `assets.rs:726–748` | Malformed `assets.json` no longer fatal | internal |
| D6 | **First-wins** on duplicate asset keys: `packDatas.emplace` (`:471`), `musicPathMap.emplace` (`:942`), `musicDataMap.emplace` (`:959`), `styleDataMap.emplace` (`:974`), `levelDatas.emplace` (`:995`) | **Last-wins** `insert`: `assets.rs:231,357,375,393,414` | With duplicate ids inside a pack (two level files declaring the same `"id"`) or duplicate pack identities across folders, C++ keeps the byte-sorted first, Rust keeps the last (and `levelDataIdsByPack` lists the entry either way, so the menu row points at different data) | internal (malformed/duplicate packs only) |
| D7 | `loadAllPackAssets` iterates `unordered_map` (impl-defined order → which pack's fatal error surfaces is nondeterministic) | Sorted pack-id iteration, explicitly documented — `assets.rs:235–240` | Determinism improvement, no behavior change on success | none |
| D8 | End-of-ctor sorts via `std::sort` (unstable; ties impl-defined); comparator asserts via `getPackData` | Stable `sort_by_key` with snapshot priorities; unknown pack id → 0.0 instead of assert — `assets.rs:503–517,768–779` | Tie order may differ from a given C++ run (C++ tie order was unspecified anyway); assert edge → 0.0 | none |
| D9 | `findPackData` over `unordered_map` — impl-defined first on duplicate (disambiguator,name,author) triples | `find_pack_data` over `HashMap` — unspecified first — `assets.rs:629–637` | Equivalent (both unspecified); matches C++ in the well-formed case | none |
| D10 | `getLocalProfileNames` / `getLocalProfilesSize` | `get_profile_names` / `get_local_profiles_size` — `assets.rs:680–692` | Rename only; both iterate a name-keyed sorted map (C++ `std::map`, Rust `BTreeMap`) | none |
| D11 | `luaFileCache` owned by `Assets` | Owned by `LuaScriptHost` (SEAM S4); cache read/write behavior present (`compose.rs:233+`), clear action is a no-op (MISSING #10) | Relocation; observable difference only via the advanced "clear" item | internal |
| D12 | C++-only: `shrink_to_fit`, ctor timing log, dtor log, `getCurrentLocalProfileFilePath` helper | Absent | Logging/memory bookkeeping; `getCurrentLocalProfileFilePath` is subsumed by the (missing) save behavior | none |
| D13 | Rust-only additions (no C++ counterpart — not defects): `FatalLoadError`, `AssetRef`, `root()`, `levels_only()`, `get_sound_path()`, `get_profile_data()` | — | API additions for the port's composition | none |

---

## 6. CITATION-DRIFT FLAGS (wrong line numbers cited in Rust comments)

Verified against the C++ source at audit time:

| Rust comment | Claims | Actual |
|--------------|--------|--------|
| `crates/oh-menu/src/real.rs:74` | *"C++ `assets.getMusicData(levelData.packId, levelData.musicId)` (MenuGame.cpp:5545-5556)"* | `grep -n 'getMusicData' src/SSVOpenHexagon/Core/MenuGame.cpp` → **5470** (5545–5556 is the leaderboard/album render block) — off by ~75 lines |
| `crates/oh-menu/src/real.rs:204` | *"C++ `assets.getCurrentLocalProfile().getScore(validator)` (MenuGame.cpp:5494)"* | `grep -n 'getScore' src/SSVOpenHexagon/Core/MenuGame.cpp` → **5570** (5494 is album rendering) — off by 76 lines |
| `crates/oh-steam/src/manager.rs:488` | *"C++ `for_workshop_pack_folders(f)` (Steam.cpp:612-624)"* | `grep -n 'for_workshop_pack_folders' src/SSVOpenHexagon/Core/Steam.cpp` → impl **654**, public wrapper **931–934** (612–624 is stat/achievement code) |
| `crates/oh-assets/src/assets.rs:686` and `crates/oh-menu/src/real.rs:180` | *"C++ `getLocalProfileNames` (Assets.cpp:1500-1511)"* | definition body **1502–1514** (declaration at 194) — 2-line drift |
| `crates/oh-app/src/client.rs` (~line 80) | *"assets (`:380`)"* for `HGAssets` construction | `main.cpp:381` (`hg::HGAssets assets{&steamManager, headless};`); 380 is the section comment. pSaveAll guard cited as `main.cpp:381-387` → actual `382–386` (`client.rs:116`) |

---

## VERDICT

**FAIL.** The discovery/loading pipeline (pack/level/style/music/shader/asset-storage/LoadInfo/dependencies/ordering) is a faithful 1:1 port, but the **entire local-profile surface is absent from `oh-assets`** — no current-profile state, no score get/set, no profile save (exit or pre-play), no create/remove — and the level-editor reload paths (`reloadPack`/`reloadLevel`/`reloadAllShaders`) and the workshop-cache fallback are no-ops. Single most important gap: **profile persistence** (`pSaveAll`/`saveAllProfiles`, `pSaveCurrent`/`saveCurrentLocalProfile`, `getLocalScore`/`setLocalScore` — C++ `Assets.cpp:1001–1058,1446–1454`; zero Rust counterpart, marked "issue 17" in `oh-app/src/client.rs:116`), i.e. every local score and favorite in the Rust build is read-only-in-memory and lost on exit.
