# STATUS — SSVOpenHexagon Rust port

Authoritative status table for plan 001. One row per issue. Updated in the same
commit as the work it describes (plan-process skill: "Landing a lane includes
updating the tracker").

Plan: `.agents/plans/001-rust-port/PLAN.md`
Mapping references: `.agents/plans/001-rust-port/mapping/`
Task specs: `.agents/tasks/`
Decisions log: `.agents/plans/001-rust-port/DECISIONS.md`

## Issues

| # | Title | Depends | State | Commit | Notes |
|---|---|---|---|---|---|
| 01 | Workspace scaffold, C++ test-port, `oh-math`+`oh-rng` | — | LANDED | 43659e33 | gate green (4/4); 36 tests; skills lib-pcg.md+lib-ssvutils.md landed |
| 02 | `oh-replay` byte-exact + zlib + packet codec | 01 | SPECD | — | `issue-002-impl.md` written; oracle pins ready |
| 03 | `oh-data`: structs + comment-tolerant JSON loaders | 01 | LANDED | ec166f35 | gate 4/4; 17 tests incl. real _RELEASE parse; discrimination-verified; skills pending |
| 04 | `oh-config`: all keys, binds, joystick | 01 | LANDED | b354e40d | gate 4/4; 20 tests incl. real config.json (101 keys) + all overrides; discrimination-verified |
| 05 | `oh-assets`: pack/level discovery + asset registry | 01,03 | LANDED | cfa5c896 | gate 4/4; 17 tests; counts independently re-derived (8 packs, 9 cube levels, 57 music+style, 37 levels) |
| 06 | `oh-lua` runtime: LuaContext analogue + conversions | 01 | LANDED | 48dba310 | gate 4/4; 24 tests; %.14g + _setGlobal throw verified against real LuaJIT source |
| 07 | `oh-lua` game API: all globals + callbacks | 06,03,04 | LANDED | 3f5d06eb | gate 4/4; 241 tests; 285-name exact-set test, name count reconciled independently, mutation reddens it |
| 08 | `oh-game` types: walls, player, custom walls, status | 01,02 | LANDED | 65f0a9cc | gate 4/4 on a clean checkout; 24 tests; oracle vectors + discrimination-verified |
| 09 | `oh-game` update loop (HGUpdate/HexagonGame) headless | 08,07 | LANDED | 32a5df93 | Update loop + determinism evidence complete: `tests/` has the 2-pair `ReplayExecution` AND the 4-pair/100-iteration `ReplayExecutionBenchmark` harness (both same-engine/fresh-engine variants), the D-019 regression guard, the callback-order test, and the `ScriptHost`/`AssetsApi` composition; gate 4/4, **306 tests**. `skills/lib-gameplay.md` carries the D-019 lesson. **D-019: found + fixed a REAL replay-determinism bug** (timeline runners not reset in `setLevelData`) that only the benchmark's 4-pair set exposed. REMAINING (the `Renderer`-contract integration, NOT the loop): `oh-render` does not yet implement `oh_game::renderer::Renderer` / drive `oh_render::graphics::draw_frame` from `HexagonGame::draw()` — issue 11 left explicit `TODO(issue-009-wiring)` markers naming those call sites, and it needs an `oh-render`→`oh-game` adapter (composition). Tracked for issue 15 (which depends on 09+11 and owns the app wiring) |
| 10 | `oh-render` backend (window/input/vertex/shader/font) | 01 | LANDED | e31d4d86 | gate 4/4 on clean checkout; 49 tests; GL smoke skips-with-reason (D-011) |
| 11 | `oh-render` graphics: stages, draw methods, cameras | 10,08 | LANDED | cd0a4748, d30f1363 | gate 4/4 (285 tests, +33: 63 lib + 12 draw + golden guard); RenderStage order + per-stage shader binding asserted; golden-frame guard skips-with-reason headless (D-011); free-function port, no oh-render→oh-game dep; 009 wiring TODO seams marked |
| 12 | `oh-menu-system`: SSVMenuSystem analogue | 01 | LANDED | 232a868c | gate 4/4; 252 tests; contract's Controller claim corrected from source |
| 13 | `oh-menu` MenuGame: all states, dialog box | 11,12,07 | LANDED | a406f5a8 | gate 4/4; MenuGame ported (4.3k lines) + tests fixed against the C++ source (dialog centre uses `calculateFMaxAndWAndH`; `diffMultIdx` = index of the 1.0× mult; raw `++` on right). Filled a real gap: the `getSaveLastLoginUsername() ? getLastLoginUsername() : ""` branch (`MenuGame.cpp:6152-6154`) |
| 14 | Lua console overlay + `-printLuaDocs` | 13,07 | SPECD | — | contract corrected from source (D-021..D-024); split: 14a headless core (docs + `produce-lua-metadata` feature + console state machine, spec `issue-014a-lua-console-core.md`), 14b egui overlay with issue 15 |
| 15 | `oh-app`: boot, game loop, CLI, modes | 13,11,09 | OPEN | — | |
| 16 | `oh-net` protocol: packets, sodium, shared codecs | 01,02 | OPEN | — | packet encoding rules in oracle-vectors.md |
| 17 | `oh-net` client + LeaderboardCache + SQLite DB | 16,15 | OPEN | — | |
| 18 | `oh-net` server (HexagonServer) | 17 | OPEN | — | |
| 19 | `oh-server-control` + `oh-workshop-uploader` tools | 17,15 | OPEN | — | |
| 20 | Steam integration | 15 | OPEN | — | |
| 21 | Discord integration | 15 | OPEN | — | |
| 22 | Windows + Linux builds, CI, cross-compile check | all | OPEN | — | |
| 23 | Parity verification harness + corpus | 15,09 | OPEN | — | |
| 24 | Packaging: `_RELEASE` layout, asset bundling | 22 | OPEN | — | |

States: OPEN → SPECD → in-flight → LANDED (commit) → (blocked/parked with reason).

## Mapping corpus progress

| Doc | Lines | State |
|---|---|---|
| `mapping/oracle-vectors.md` | 274 | LANDED (RNG incl. wrapper shift + f32/f64 bits; replay HEX/CZHEX/PHEX; SFML Packet BE encoding; key/button enums; string formatting) |
| `mapping/gameplay.md` | 4455 | LANDED (walls, player, custom walls, status, RNG, tickers, timelines, HGUpdate, HexagonGame) |
| `mapping/data-json.md` | 1580 | LANDED (SSVUtilsJson, Data/*, Assets, Config, LevelValidator, Joystick, BindControl, _RELEASE layout) |
| `mapping/rendering-menu.md` | — | in-flight |
| `mapping/lua-api.md` | — | OPEN |
| `mapping/networking.md` | — | OPEN |
| `mapping/core-runtime.md` | — | OPEN |
| `mapping/libs-ssv.md` | — | OPEN |
| `mapping/thirdparty-cpp.md` | — | OPEN |

## Oracle probes (reproducible)

`tools/oracle/run.sh` compiles the real C++ sources against minimal SFML shims and
prints every golden value pinned in `mapping/oracle-vectors.md`. Currently covers:
PCG engine, the `random_number_generator` wrapper (incl. `advance(1)` shift),
replay file/compressed/packet bytes, and `concat`/`buildPackId`/`getLevelValidator`.
Adding a probe for a new surface is part of the issue that needs it.
