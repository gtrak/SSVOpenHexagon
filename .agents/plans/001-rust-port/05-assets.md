# Issue 05 — `oh-assets`: pack/level discovery + asset registry

## Objective

Port `HGAssets` (`Global/Assets`) and `AssetStorage` so pack/level/style/music
discovery, asset loading, error accumulation, and the asset registry behave
identically on the existing `_RELEASE` tree.

## Key decisions

- Mirror `include/SSVOpenHexagon/Global/Assets.hpp` + `src/.../Assets.cpp`,
  `AssetStorage.hpp` + `.cpp`, and `Data/PackInfo.hpp`.
- Assets is a global singleton in C++ (`getAssets()`). Keep a single instance
  threaded explicitly or behind an accessor; document the choice (prefer explicit
  `&Assets` to match how workers will need it later, unless the C++ call sites make
  that impractical).
- Texture/font/music/shader handles: type-erased ids (`ssvu::UniquePtrArray`,
  `std::size_t` shader ids). Represent as generational handles / indices into Vec
  arenas so the render backend (issue 10) can bind them.
- Pack/level ordering and priority rules: copy `Assets.cpp` exactly.
- Validator strings come from issue 01 (`buildPackId`, `getLevelValidator`).

## Files

| File | Change |
|---|---|
| `crates/oh-assets/Cargo.toml` | new — deps `oh-data`, `oh-math` |
| `crates/oh-assets/src/{assets,asset_storage,pack_scan}.rs` | new |
| `crates/oh-assets/tests/release_tree.rs` | new — scan the real `_RELEASE` |
| `.agents/skills/lib-assets.md` | new — pack/level discovery rules from real source |

## Steps

1. Port `AssetStorage` (texture/font/sound-buffer/shader storage, `getOrNull*`,
   load-from-file with error accumulation).
2. Port the pack/level loading entry points: `loadAssetsFromFolder`,
   `loadPack`, `loadLevel`, `loadStyle`, `loadMusic`, `loadAssets`, the
   `LoadInfo` error accumulation, `getPackData`, `getLevelData`, pack dependency
   checks, and the level-selection data structures.
3. Port `assets.json` parsing (fonts/textures/soundBuffers/musicId lists).
4. Port the shader registry (id allocation, `getShaderByShaderId`,
   `getShaderIdByName`) — the actual GLSL compilation is issue 10's; keep the
   registry + error handling here.
5. Port pack priority ordering and the "selectable"/"unscored" flags.

## Verification

- Scan the real `_RELEASE`; assert pack count and, for `cube`, the level count and
  one level's exact `LevelData` (apeirogon).
- Assert discovery order equals the C++ order — if a C++ oracle is needed, add a
  small probe (compile `Assets.cpp` is heavy; instead assert against the sorted
  directory listing the C++ would produce, and document the rule from source).
- Assert error accumulation: a synthetic bad pack produces the same `LoadInfo`
  counts and one error message per problem.
- `tools/gate.sh` green.

## Notes / risks

- Discovery likely uses `std::filesystem` directory iteration; **iteration order is
  unspecified** in C++ — check whether the code sorts. Copy whatever it does.
- Missing asset → the C++ calls `getTextureOrNullTexture` etc.; reproduce the
  null-texture fallback so the game still runs on incomplete packs.
- Case sensitivity: Linux vs Windows file lookup differs; mirror the C++ behavior
  (which is OS-dependent) and note it.
