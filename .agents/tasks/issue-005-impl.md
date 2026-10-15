# Task spec — issue 005: `oh-assets` (pack/level discovery + asset registry)

You are a worker. Implement exactly this issue. Do not redesign or widen scope.

## Context

Repo: `/home/gary/dev/SSVOpenHexagon`. Literal 1:1 Rust port. Issues 001
(workspace/`oh-math`/`oh-rng`), 002 (`oh-replay`), 003 (`oh-data`) are LANDED.

Read first:
1. `.agents/plans/001-rust-port/PLAN.md`, `DECISIONS.md`
2. `.agents/plans/001-rust-port/05-assets.md` (the contract)
3. `.agents/plans/001-rust-port/mapping/data-json.md` §4 (`Global/Assets`),
   §5 (`AssetStorage`), §9 (on-disk layout), §10 (subtleties)
4. `include/SSVOpenHexagon/Global/Assets.hpp`, `AssetStorage.hpp` +
   `src/.../Assets.cpp`, `AssetStorage.cpp`
5. `.agents/skills/README.md`

## Working agreement

- **Skills are truth.** Write `.agents/skills/lib-assets.md` from the real source:
   the pack/level discovery rules, priority ordering, error accumulation, and the
   null-asset fallbacks.
- **Write first**, compile early.
- **Exactness**: copy the discovery order and priority rules exactly; note whether
   directory iteration is sorted (C++ `std::filesystem` order is unspecified — copy
   whatever the code does).
- **Commit at the first green build**.

## What to build

1. `AssetStorage` (texture/font/sound/shader storage; `getOrNull*`; load-from-file
   with error accumulation; the null entry after a failed load; the
   non-replacing `emplace` behavior).
2. The pack/level loader: `loadAssetsFromFolder`, `loadPack`, `loadLevel`,
   `loadStyle`, `loadMusic`, `getPackData`, `getLevelData`, pack dependency checks,
   `LoadInfo` error accumulation, and the level-selection data structures.
3. `assets.json` parsing (fonts/textures/soundBuffers/musicId lists).
4. The shader registry (id allocation, `getShaderByShaderId`, `getShaderIdByName`)
   — GLSL compilation itself is issue 10's job; keep the registry + errors here.
5. Pack priority ordering and the `selectable`/`unscored` flags.

## Verification

1. `bash tools/gate.sh` — paste all `GATE_EXIT=` lines.
2. Scan the real `_RELEASE`: assert pack count and, for `cube`, the level count and
   one level's exact `LevelData` (apeirogon).
3. Assert discovery order matches the rule documented from source.
4. A synthetic bad pack produces the same `LoadInfo` counts/messages.
5. Missing asset → null fallback so the game still runs.

## Report format

Module map, design summary, gate outputs, skill corrections, deviations, known gaps.
