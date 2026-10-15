# Issue 03 — `oh-data`: data structs + comment-tolerant JSON loaders

## Objective

Port every `hg::` data struct and the JSON loading stack (`SSVUtilsJson` +
`LoadFromJson` + `UtilsJson`), so packs/styles/music/levels/config parse the
existing on-disk files exactly. The JSON parser must tolerate `//` line comments
(present in real pack files).

## Key decisions

- Mirror `include/SSVOpenHexagon/Data/*.hpp` + `src/.../Data/*.cpp`, plus
  `SSVUtilsJson/**` and `Global/UtilsJson.*`, `Utils/LoadFromJson.*`.
- JSON backend (D-004 JSON entry): prefer `serde_json` with a small comment-and-
  trailing-comma stripping pass that matches the C++ fork's tolerance; if that
  cannot reproduce the C++ behavior, use `json5`. **Settle by reading the C++ fork's
  parser** (`include/SSVOpenHexagon/SSVUtilsJson/JsonCpp/`) and documenting its
  exact tolerance rules in `lib-jsoncpp.md`.
- `boost::pfr` reflection → explicit `serde` derives or hand-written loaders
  (whichever matches the C++ field enumeration order and default behavior).
- `CapColor` = `tinyvariant<Main, MainDarkened, ByIndex, ColorData>` → a Rust enum
  with the same 4 variants and the same parse precedence.

## Files

| File | Change |
|---|---|
| `crates/oh-data/Cargo.toml` | new — deps `serde`, `serde_json` (or `json5`) |
| `crates/oh-data/src/{cap_color,color_data,level_data,level_status,load_info,music_data,pack_data,pack_info,profile_data,style_data}.rs` | new |
| `crates/oh-data/src/json/{mod,value,linked_value,load}.rs` | new — the SSVUtilsJson analogue |
| `crates/oh-data/tests/load_release.rs` | new — parse the real `_RELEASE` files |
| `.agents/skills/lib-jsoncpp.md` | new — the fork's parser + reader API from real source |
| `.agents/skills/lib-serde-json.md` | new — from the crate's real source (version pinned) |

## Steps

1. Port `ColorData`, `PulseColor`, `CapColor` + `parseCapColor` (exact mode
   precedence from `CapColor.cpp`).
2. Port `LevelData` (+ `getValidator`, `getValidatorWithoutPackId`, `getNthDiffMult`),
   `LevelStatus` (every field + default + ctor), `LoadInfo`, `MusicData`,
   `PackData`, `PackDependency`, `PackInfo`, `ProfileData` (read/write with the
   exact `users.json` format), `StyleData` (every field + ctor + `update`,
   `computeColors`, background draw helpers are issue 11's concern).
3. Port the JSON reader subset actually used: `get`, `getOr`, `getOrDefault`,
   `isArray`, typed `as*`, iteration over arrays/objects (`forArray`/`forObj` where
   used), and `LinkedValue` if used by the game.
4. Port `LoadFromJson` helpers and `utils::getLevelValidator` /
   `BuildPackId` string formats (issue 01 may already have the latter — depend on it).
5. Tests: parse each real file under `_RELEASE/` (all `pack.json`, one level, one
   style, one music, `Assets/assets.json`, `_RELEASE/config.json`) and assert exact
   field values (e.g. apeirogon `menuPriority=40`, `difficultyMults=[1.6,0.35]`).

## Verification

- Parse the **entire** `_RELEASE/Packs/**` tree without error; assert counts
  (pack count, level count) match `find`-derived numbers.
- At least one test that would fail if `//` comments were not stripped (feed a
  fixture with comments; assert it parses) — name the property.
- A round-trip test for `ProfileData`/config write format if the C++ writes it
  (issue 04 owns config; keep the data-layer write format tests here for
  `ProfileData`).
- `tools/gate.sh` green.

## Notes / risks

- Do not reorder struct fields relative to the C++ (serialization order matters for
  config/users files).
- `StyleData` has many float members with defaults; copy each default verbatim.
- The `TinyVariant` type (`Utils/TinyVariant.hpp`) — if its semantics (visitation,
  `is<T>`, `get<T>`) are relied upon, port the subset in `oh-math` and note it.
