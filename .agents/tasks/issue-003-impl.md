# Task spec — issue 003: `oh-data` (data structs + comment-tolerant JSON)

You are a worker. Implement exactly this issue. Do not redesign, do not widen scope.

## Context

Repo: `/home/gary/dev/SSVOpenHexagon`. Literal 1:1 Rust port of the C++ game.
Issues 001 (`oh-math`, `oh-rng`) and 002 (`oh-replay`) are LANDED; the workspace
and `tools/gate.sh` exist and are green.

Read first, in order:
1. `.agents/plans/001-rust-port/PLAN.md`, `DECISIONS.md`
2. `.agents/plans/001-rust-port/03-data-json.md` (the contract)
3. `.agents/plans/001-rust-port/mapping/data-json.md` — **the authoritative
   transcription** (10 sections: SSVUtilsJson, Data/*, LoadFromJson, Assets,
   AssetStorage, Config, LevelValidator/BuildPackId, Joystick, BindControl,
   on-disk layout, subtleties). Re-derive any count from source before relying on it.
4. The real C++ sources under `include/SSVOpenHexagon/Data/`, `SSVUtilsJson/`,
   `Global/UtilsJson.*`, `Utils/LoadFromJson.*`
5. `.agents/skills/README.md`

## Working agreement

- **Skills are truth.** Write `.agents/skills/lib-jsoncpp.md` (the pinned JsonCpp
  fork's reader semantics, from `include/SSVOpenHexagon/SSVUtilsJson/JsonCpp/**` +
  the fork source if present) and `.agents/skills/lib-serde-json.md` (from the
  crate's real source) and update `.agents/skills/README.md`.
- **Write first**: create the crate + module skeletons in the first few tool
  calls, compile early, iterate on named errors.
- **Exactness**: field names/types/defaults and JSON key strings copied verbatim.
  Do not reorder struct fields.
- **Comments**: the mapping doc §1.6 documents the fork's exact `//` and `/* */`
  tolerance and its trailing-comma behavior. Reproduce the tolerance; add a test
  whose fixture contains comments (name the property it pins).
- **Commit at the first green build**, then keep going.

## What to build

Port, per the contract:

1. `ColorData`, `PulseColor`, `CapColor` + `parseCapColor` (exact mode precedence).
2. `LevelData` (+ `getValidator`, `getValidatorWithoutPackId`, `getNthDiffMult`),
   `LevelStatus` (every field + default + ctor), `LoadInfo`, `MusicData`,
   `PackData`, `PackDependency`, `PackInfo`, `ProfileData`, `StyleData`
   (all fields + defaults + ctor; drawing methods are out of scope).
3. The JSON reader subset actually used by the game (`get`/`getOr`/typed `as*`/
   array+object iteration), built on the chosen parser, preserving the fork's
   comment tolerance.
4. `LoadFromJson` helpers and the `StyleData`/`LevelData` from-JSON constructors.

## Verification

Run and paste outputs:
1. `bash tools/gate.sh` — all `GATE_EXIT=` lines.
2. A test that parses **every** `_RELEASE/Packs/*/pack.json`, one level, one style,
   one music, `_RELEASE/Assets/assets.json`, asserting exact field values (e.g.
   cube/apeirogon: `menuPriority=40`, `difficultyMults=[1.6,0.35]`).
3. A comment-tolerance test (fixture with `//` comments parses; a fixture with a
   trailing comma behaves as the fork does — document which).
4. `ProfileData` write-format round-trip if the C++ writes it.

## Report format

Module map, design summary, gate outputs, skill corrections, deviations, known gaps.
