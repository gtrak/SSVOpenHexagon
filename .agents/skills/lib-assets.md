# `lib-assets.md` — `Global/Assets` + `Global/AssetStorage` (pack/level discovery + asset registry)

Source of truth (read these, not memory):
- `include/SSVOpenHexagon/Global/Assets.hpp`, `src/SSVOpenHexagon/Global/Assets.cpp`
- `include/SSVOpenHexagon/Global/AssetStorage.hpp`, `src/SSVOpenHexagon/Global/AssetStorage.cpp`
- `include/SSVOpenHexagon/Data/{LevelData,PackData,PackInfo,MusicData,StyleData,LoadInfo}.hpp`
- `extlibs/SSVUtils/include/SSVUtils/Core/FileSystem/{Path,Scan,Internal/ScanHelper}.hpp`
- `extlibs/SSVUtils/include/SSVUtils/Core/FileSystem/Enums.hpp`
- mapping doc `.agents/plans/001-rust-port/mapping/data-json.md` §4 (Assets), §5 (AssetStorage), §7 (id formats), §9 (layout), §10 (subtleties)

Rust port: `crates/oh-assets` (issue 05). `pack_scan.rs`, `asset_storage.rs`,
`shaders.rs`, `assets.rs`.

## 1. Discovery order — the single most important rule

`Assets.cpp` discovers packs/levels/styles/music via `ssvufs::scan`
(`ScanHelper.hpp`). **The raw `readdir` order is unspecified, but it is always
normalized to byte-wise lexicographic order of the full normalized path**
because every call uses the default `Sort::Alphabetic`, which does
`std::sort(begin, end)` on `std::vector<Path>` using `Path::operator<`
= `getStr() < other.getStr()` (byte-wise `strcmp`).

Concretely (`ScanHelper::scan`):
- single-level (`Mode::Single`): `opendir` + `readdir` over the immediate
  children only (non-recursive);
- skips entries where `Path::isRootOrParent()` (`.` / `..`);
- `Type::Folder` keeps dirs, `Type::File` keeps files;
- `Pick::ByExt` keeps files whose **name** `endsWith(extension)` —
  **case-sensitive** (`ssvu::endsWith`, NOT the case-insensitive
  `Path::hasExtension`); `Pick::ByName` keeps files whose name == the name
  exactly (case-sensitive);
- default `Sort::Alphabetic` → `std::sort` by the normalized full-path string.

`Path::normalize()` facts (`Path.hpp`): a folder path gets a trailing `/`
appended (`"Packs/cube"` → `"Packs/cube/"`); a file path does not
(`"Packs/cube/Levels/pi.json"` stays as-is). So **folder scan results carry a
trailing slash; file scan results do not**, and both are byte-sorted on the
full path. Since siblings share a directory prefix, sorting the full path is
equivalent to sorting the file/folder name.

The three call sites in `Assets.cpp` (all default `Sort::Alphabetic`):
- `scanSingleFolderName("Packs/")` → `scan<Single, Folder>` — the pack folders.
- `scanSingleByExt(path, ext)` → `scan<Single, File, ByExt>` — levels/styles/
  music `.json`, sounds/shaders `.ogg`/`.vert`/`.geom`/`.frag`.
- `scanSingleByName(path, name)` → `scan<Single, File, ByName>` (reload only).

Rust: `oh_assets::pack_scan::{scan_single_folder_name, scan_single_by_ext,
scan_single_by_name}` reproduce this exactly (collect → filter →
`Vec::sort()` byte-wise; folders get a trailing `/`).

## 2. Pack discovery (`loadAllPackDatas` / `loadPackData`)

1. `Packs/` must be a folder, else the whole load fails (constructor
   `std::terminate()` → Rust `FatalLoadError`).
2. **Local packs first**: for each folder in `scanSingleFolderName("Packs/")`
   (byte-sorted), call `loadPackData`.
3. **Steam Workshop packs second** — only `if(steamManager != nullptr)`:
   `is_initialized()` → `for_workshop_pack_folders(...)`, else
   `loadWorkshopPackDatasFromCache()` (reads `workshopCache.json`
   `{"cachedPacks":[...]}`). *The Rust port has no steam manager (oh-steam),
   so this whole branch is skipped — the `_RELEASE` tree has no workshop packs
   in `Packs/` anyway.*
4. `loadPackData(packPath)` (packPath = the folder scan result, trailing
   slash):
   - `packPath + "/pack.json"` must be a file, else return **false** and the
     caller logs `"Error loading pack data '<packPath>'\n"` into
     `loadInfo.errorMessages` (and does NOT increment `packs`).
   - else `getFromFileWithErrors(packPath + "/pack.json")`; the parse `error`
     is appended via `loadInfo.addFormattedError(error)` (see §5). Even a
     parse-failed `pack.json` still yields a pack entry from the default
     values (see §3 defaults).
   - reads (defaults): `"disambiguator"` ("no disambiguator"), `"name"`
     ("unknown name"), `"author"` ("unknown author"), `"description"`
     ("no description"), `"version"` (0), `"priority"` (100);
     `"dependencies"` only `if hasObj` → array of
     `{"disambiguator","name","author","min_version"}` (no defaults).
   - `packId = buildPackId(disambiguator, author, name, version)` (§7.2; every
     `' '`/`'\n'`/`'\t'` → `'_'`).
   - `packInfos.emplace_back(PackInfo{packId, packPath})`;
     `packDatas.emplace(packId, PackData{folderPath = packPath.getStr(), ...})`.
   - success → `++loadInfo.packs`.
   - A `pack.json` **type mismatch** throws with no surrounding catch →
     C++ aborts; Rust returns `FatalLoadError`.

Dedup: `packDatas.emplace` does **not** overwrite a duplicate id, but
`packInfos` still appends (in practice ids are unique).

## 3. Per-pack asset loading (`loadPackAssets`, exact gating)

`packPath` = `packData.folderPath` (trailing slash); `packId` = `packData.id`.
The whole body is in a `try`/`catch`; any thrown JSON exception becomes the
fatal message (§5). Order + gates (verbatim):

| step | gate (all must hold) | id / counter |
|---|---|---|
| shaders | `!headless && Shaders/ isFolder && !levelsOnly` | `.vert`→Vertex, `.geom`→Geometry, `.frag`→Fragment, each byte-sorted; per loaded shader: `shadersById.push`, `id=size-1`, `shaders[packId+"_"+fileName.ext]`, `shadersPathToId[fullPath]=id`, `++loadInfo.assets` |
| custom sounds | `!headless && !levelsOnly && Sounds/ isFolder` | per `.ogg`: `assetStorage.loadSoundBuffer(packId+"_"+fileName, path)`; `++loadInfo.assets` (always) |
| music files | `Music/ isFolder && !levelsOnly && !headless` | per `.ogg`: `musicPathMap[packId+"_"+fileNameNoExt] = path`; `++loadInfo.assets` |
| music data | `Music/ isFolder && !levelsOnly` | per `.json`: `loadMusicFromJson`; `musicDataMap[packId+"_"+musicId]`; `++loadInfo.assets` (parse error → `addFormattedError`) |
| styles | `Styles/ isFolder` | per `.json`: `StyleData`; `styleDataMap[packId+"_"+styleId]`; `++loadInfo.assets` |
| levels | `Levels/ isFolder` | per `.json`: `LevelData(obj, packPath, packId)`; `assetId = packId+"_"+id`; `levelDataIdsByPack[packId].push(assetId)`; `levelDatas[assetId]`; `++loadInfo.levels` |

Note: **styles and levels have no `headless`/`levelsOnly` gate** — they load
even in headless. Shaders/sounds/music-ogg are the SFML-backed ones skipped by
`headless`.

After the sub-loaders: `if packHasLevels(packId)` →
`selectablePackInfos.emplace_back(PackInfo{packId, packPath})`.

`loadAllPackAssets` iterates `packDatas`; if a pack's `loadPackAssets` returns
false it logs `"Error loading pack info '<packId>'\n"` and returns false
(constructor terminates).

## 4. Ordering + `selectable` / `unscored` (end of constructor)

- `levelDataIdsByPack[pack]` vectors sorted by `levelDatas[id].menuPriority`
  **ascending** (`std::sort`; ties unspecified — Rust uses a stable
  `sort_by_key`).
- `packInfos` and `selectablePackInfos` sorted by `PackData::priority`
  **ascending** (smaller = earlier). Shipped packs use very negative priorities
  (base `-99999.04`, tutorial `-99999.03`, cube/artwork `-99999.02`, hypercube
  `-99999.01`, orthoplex `-99999.00`), so they precede user packs (default
  `100`). Ties are unspecified (Rust: stable).
- `verifyAllPackDependencies`: a dep is satisfied iff some loaded pack has
  `disambiguator== && name== && author== && version >= minVersion`. Else log
  `"Missing pack dependency '<dep.name>' for pack '<pack.name>'\n"` and add
  the pack id to `packIdsWithMissingDependencies`; then those packs are
  removed from `selectablePackInfos`. Always returns true (never fatal).
- `selectable` (per level, `LevelData.selectable`, default `true`) is a level
  JSON flag; `unscored` (default `false`) marks a level as unscored. `base`
  has no `Levels/` so it is not selectable (not in `selectablePackInfos`).

## 5. `LoadInfo` error accumulation

`LoadInfo { packs, levels, assets, errorMessages }`. Error strings recorded:

- **`addFormattedError(err)`** (jsoncpp parse errors from
  `getFromFileWithErrors`): strips leading `* `, folds the first `\n` to ` - `,
  deletes the rest; only if non-empty. Used for `assets.json`, each
  `pack.json`, each music/style/level/profile `.json`, `workshopCache.json`.
- **Type mismatch** (a `getExtr<T>` where the JSON value has the wrong type):
  only the **per-pack asset** sub-loaders are wrapped in `try/catch` — the
  message is exactly
  `"FATAL ERROR, Exception during asset loading: <what>\n"` where `<what>` is
  the `std::runtime_error` text (e.g. `"Value is not convertible to int."`),
  and the whole pack load aborts (→ constructor terminate). Nothing else in
  the JSON path catches exceptions (a bad `config.json`/`assets.json` type
  mismatch crashes the process in C++).
- `loadPackData` failure (no `pack.json`): `"Error loading pack data '<path>'\n"`.
- `loadAllPackAssets` failure: `"Error loading pack info '<packId>'\n"`.
- missing dependency: `"Missing pack dependency '<dep>' for pack '<pack>'\n"`.

Important: a **parse-failed** level/style/music `.json` is *not* fatal — it
adds a formatted error AND still loads the item with all defaults (level
`id` = `"nullId"`, style `id` = `"nullId"`, music `id` = `""`), and still
increments the counter. Only a **type mismatch** is fatal (for levels/styles).

## 6. Null-asset fallbacks (fail-soft so the game keeps running)

- `AssetStorage::loadTexture/loadFont/loadSoundBuffer` =
  `tryEmplaceAndThenLoadFromFile`: `map.emplace(id, {})` **and only if the
  insert happened**, `value.loadFromFile(path)`. Consequences:
  - a **failed** load leaves the empty entry: `hasX(id)` = true, `getX(id)` =
    the empty SFML object (non-null pointer);
  - a **second** load of the same id is a no-op (returns false; first load
    wins, even if it failed).
- `getTexture`/`getFont` assert non-null (the empty object counts); but
  `getTextureOrNullTexture` / `getFontOrNullFont` return a **`static` null
  object** when the id has no slot at all — this is the missing-asset
  fallback the render code relies on.
- `getMusicData(packId, id)` / `getStyleData(packId, id)`: missing → log
  `Asset '<packId>_<id>' not found` and **return the first (map-ordered, i.e.
  smallest-key) entry** (`std::map` is sorted; asserts the map is non-empty).
- `getShader`/`getShaderId`: missing → log + `nullptr`/`nullopt` (no fallback).
- `musicPathMap.emplace` on reload never replaces an existing mapping.

Rust: `AssetRef::{Present(id), Null}` mirrors the null-fallback pair;
`music_data_map`/`style_data_map` are `BTreeMap` (ordered) so the
smallest-key fallback matches `std::map::begin()`.

## 7. id formats (from `oh-data`)

- pack id: `buildPackId(disamb, author, name, version)` — spaces/newlines/tabs
  → `_`, joined by `_`, then the int version. e.g.
  `ohvrvanilla_vittorio_romeo_cube_1`.
- level asset id: `packId + "_" + levelId`.
- validator id: `getLevelValidator(id, dm)` = `<id>_m_<dm>` with iostream
  6-sig-digit float (`1.0`→`"1"`, `0.35`→`"0.35"`, `1.6`→`"1.6"`).
- music/style asset id: `packId + "_" + musicId/styleId`.
- shader name key: `packId + "_" + fileNameWithExt` (e.g.
  `cube_gradient.frag`); path key = the full shader path.

## 8. `assets.json` (loaded only when `!levelsOnly && !headless`)

`Assets/assets.json` → `loadAssetsFromJson(storage, "Assets/", obj)` reads three
`vector<string>` keys (**no defaults** → missing key = empty list): `"fonts"`,
`"textures"`, `"soundBuffers"`. For each name it loads `<name>` from
`"Assets/" + name`. **The ids are the EXACT strings in `assets.json`, which
retain their extensions** — e.g. `"icon.png"`, `"beep.ogg"`,
`"OpenSquare-Regular.ttf"` (the mapping doc §4.6 shorthand "icon"/"beep" is
loose; the real keys include the extension). Failures only log
(`"Failed to load texture '<f>'"`); they do NOT touch `LoadInfo` and do NOT
increment `loadInfo.assets`.

## 9. Rust port notes / deferrals

- `Assets::new(root, headless, levels_only)` threads the CWD (`_RELEASE`)
  explicitly instead of a global singleton; all fs ops are `root.join(logical)`.
- The actual SFML texture/font/sound handles and GLSL shader compilation are
  **issue 10**'s — this crate records registry state only (`loaded` flag, ids,
  paths). `loadFromFile` success is stood in by *file exists*.
- Steam Workshop enumeration is deferred to **oh-steam** (`steamManager ==
  nullptr` → branch skipped).
- The profile *mutation/save* API (`pCreate`, `pSetCurrent`,
  `saveAllProfiles`, `getLocalScore`, …) is deferred; profile *loading*
  (`loadAllLocalProfiles`, non-fatal) is ported.
- `reloadPack`/`reloadLevel`/`reloadAllShaders` (dev tools) are not ported.
- `build_pack_id` keeps `oh-data`'s `debug_assert!(version > 0)` — a
  `pack.json` missing `"version"` (→0) asserts in debug/Rust-test builds,
  matching C++ debug; shipped packs all have `version: 1`.

## Empirical counts on the real `_RELEASE` (asserted by `tests/release_tree.rs`)

- `loadInfo.packs` = 8; total levels = 37; `cube` has 9 levels.
- headless (`Assets::new(root, true, false)`): `loadInfo.assets` = 57
  (24 music `.json` + 33 style `.json`), `levels` = 37, no errors.
- full (`headless=false`): `loadInfo.assets` = 61 (+4 shaders: 2 in artwork,
  2 in experimental, all `.frag`); storage has 2 fonts + 16 textures + 16
  sound buffers (ids with extensions).
- pack discovery order (byte-sorted): artwork, base, cube, experimental,
  hypercube, orthoplex, tutorial, workshopexample.
