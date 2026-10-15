# 001 — Literal 1:1 Rust port of SSVOpenHexagon

Status: IN PROGRESS (planning complete, execution starting)

## Why

`SSVOpenHexagon` (Open Hexagon) is a ~42k-line C++20 game built on a custom SFML
fork, LuaJIT scripting, ImGui, libsodium, zlib, SQLite, Steam and Discord SDKs.
The goal is a **literal 1:1 port to Rust** that runs on **Windows and Linux**,
reproducing behavior exactly — including replay determinism and on-disk/wire
compatibility — so that existing packs, replays, configs, and the online server
keep working. Where a library has no adequate Rust analogue, we write our own to
fill the gap.

This is a *port*, not a redesign: file/struct/function structure mirrors the C++
source, arithmetic types (f32 vs f64) are preserved exactly, and every constant
and formula is copied, not reinterpreted.

## What

### Deliverable

A Cargo workspace at the repo root, crates under `crates/`, that builds:

- `open-hexagon` — the game binary (Windows + Linux)
- `oh-server-control` — the server control tool
- `oh-workshop-uploader` — the workshop uploader tool

running against the existing `_RELEASE/` assets, packs, config and replays.

### Ground rules

1. **Structure mirrors the source.** One Rust module per C++ file, same names
   (snake_case), same responsibilities. A reader should be able to diff the two
   trees mentally.
2. **Arithmetic fidelity.** Every `float`→`f32`, `double`→`f64`. No "clean"
   refactors that change evaluation order of floating-point expressions. Replay
   determinism depends on it.
3. **Bit-exact shared formats.** Replay files, compressed replays, SFML packet
   wire format, SQLite schema, config JSON, and the Lua API are compatibility
   surfaces and are pinned by tests with vectors produced from the C++ build.
4. **The Lua API is a content contract.** All ~140 globals, exact spellings and
   argument types. Existing packs must run unmodified.
5. **Skills are truth.** For every library (vendored C++ lib, Rust crate, system
   lib) an in-repo skill is written from **real source** and is authoritative for
   all workers. See `.agents/skills/README.md`.
6. **Commit at every step.** One commit per issue after PASS + gates green.
   Never leave work uncommitted.
7. **Cross-platform by construction.** No unix-only APIs in library code; every
   platform-specific path behind a small backend module.

### Architecture (decided)

Workspace crates (dependency order):

| Crate | Mirrors | Responsibility |
|---|---|---|
| `oh-math` | SSVUtils math + SSVStart Vector2 | `Vec2`, `FT`, angles, easing, geometry, color/hue, `MoveTowards`, `Match`, `TinyVariant`, `Split`, `String`, `Concat`, `BuildPackId`, `LevelValidator`, `Timestamp` |
| `oh-rng` | `RandomNumberGenerator`, PCG | **bit-exact** `pcg32_fast` + `uniform_int`/`uniform_real` semantics |
| `oh-timeline` | `Utils::Timeline2`, `Ticker`, `CustomTimeline` | timelines + tickers, bit-exact |
| `oh-replay` | `Core/Replay` | byte-exact replay + zlib compression + packet codec |
| `oh-data` | `Data/*` + `SSVUtilsJson` + `LoadFromJson` | data structs + comment-tolerant JSON loaders |
| `oh-config` | `Global/Config` + `BindControl` + `Joystick` | every config key, binds, joystick, save/load |
| `oh-assets` | `Global/Assets`, `AssetStorage` | pack/level discovery, asset registry |
| `oh-lua` | `Utils/LuaWrapper` + `LuaScripting` + `HGScripting` | LuaJIT runtime (mlua) + the whole game API |
| `oh-game` | `Components/*`, `Core/HGStatus`, `HGUpdate`, `HexagonGame` | gameplay simulation, incl. headless |
| `oh-audio` | `Global/Audio`, `Data/MusicData` | music/sound, volumes, pitch, segments |
| `oh-net` | `Online/*`, `Core/Hexagon{Client,Server}`, `LeaderboardCache` | protocol, sodium, DB, client/server |
| `oh-menu-system` | SSVMenuSystem | menu model (own implementation) |
| `oh-steam` | `Core/Steam` | Steam SDK analogue |
| `oh-discord` | `Core/Discord` | Discord SDK analogue |
| `oh-render` | SFML analogue + `HGGraphics` + camera | window/input/vertex-batch/shader/font/text backend |
| `oh-menu` | `Core/MenuGame`, `HexagonDialogBox`, `Imgui` | menus, states, dialog, Lua console |
| `oh-app` | `Core/main` | boot, game loop, arg parsing, modes |

**Vendored-C++-library analogues** (SSVUtils, SSVStart, SSVMenuSystem, vrm_pp):
folded into `oh-math`/`oh-timeline`/`oh-render`/`oh-menu-system` rather than
separate crates — they are thin.

**Third-party substitution decisions** are recorded in
`.agents/plans/001-rust-port/DECISIONS.md` as each is settled by an issue; the
initial candidate set:

| C++ dependency | Rust analogue (candidate) |
|---|---|
| SFML (window/graphics/audio/network) | `rsfml` (pure-Rust SFML reimplementation) — primary; falls back to a hand-written `winit`+`glow`+`rodio` backend if gaps are found. Settled by issue 10. |
| LuaJIT + LuaWrapper | `mlua` (`luajit` feature, vendored) |
| JsonCpp fork (+`//` comments) | `serde_json` + comment stripper, or `json5` |
| zlib `compress2`/`uncompress` | `flate2` (zlib backend) |
| libsodium `crypto_box` etc. | `sodiumoxide` (exact primitive match) |
| SQLite + sqlite_orm | `rusqlite` (bundled) + hand-written SQL |
| ImGui + imgui-sfml | `egui` + `egui-winit` + `egui_glow` |
| Steam SDK | `steamworks` |
| Discord Game SDK | `discord-rich-presence` / `discord-sdk` |
| boost::pfr | `serde` derives |

### Success criteria

- All existing packs (`base`, `cube`, `hypercube`, `orthoplex`, `tutorial`,
  `experimental`, `artwork`, `workshopexample`) load and play headlessly with no
  script errors.
- Replay execution is **bit-identical** to the C++ build on a corpus of
  generated replays (same final score, same frame count, same RNG draws).
- Replay/config/packet serialization round-trips byte-for-byte against vectors
  generated by the C++ build.
- The game builds and runs on Windows (msvc or gnu) **and** Linux.
- The online client can talk to the existing C++ server (protocol compatibility),
  and the Rust server can serve the existing client.
- Rendered output is visually identical (golden-frame comparison on a fixed
  level/seed where feasible).

## Task order (dependency graph)

```
01 workspace+math+rng ──┬── 02 replay ──┐
                        │               │
                        ├── 03 data ────┤
                        ├── 04 config ──┤
                        └── 05 assets ──┘
                                         │
                    06 lua runtime ──────┤
                    07 lua game API ─────┤
                                         │
                    08 gameplay types ───┤
                    09 game update loop ─┤
                                         │
                    10 render backend ───┤
                    11 draw/graphics ────┤
                    12 menu-system ──────┤
                    13 menu game ────────┤
                    14 lua console ──────┤
                    15 app boot/loop ────┤
                    16 net protocol ─────┤
                    17 client+database ──┤
                    18 server ───────────┤
                    19 tools ────────────┤
                    20 steam ────────────┤
                    21 discord ──────────┤
                    22 windows+linux build/CI ── 23 parity verification ── 24 packaging
```

Issues 03/04/05 are independent of 02 and of each other. 10 gates 11–15. 16
gates 17–19. 20/21 are leaf integrations. 22–24 close out.

## Issue index

| # | Title | Depends | Status |
|---|---|---|---|
| 01 | Workspace scaffold, C++ test-port, `oh-math`+`oh-rng` | — | OPEN |
| 02 | `oh-replay` byte-exact + zlib + packet codec | 01 | OPEN |
| 03 | `oh-data`: structs + comment-tolerant JSON loaders | 01 | OPEN |
| 04 | `oh-config`: all keys, binds, joystick | 01 | OPEN |
| 05 | `oh-assets`: pack/level discovery + asset registry | 01,03 | OPEN |
| 06 | `oh-lua` runtime: LuaContext analogue + conversions | 01 | OPEN |
| 07 | `oh-lua` game API: all globals + callbacks | 06,03,04 | OPEN |
| 08 | `oh-game` types: walls, player, custom walls, status | 01,02 | OPEN |
| 09 | `oh-game` update loop (HGUpdate/HexagonGame) headless | 08,07 | OPEN |
| 10 | `oh-render` backend (window/input/vertex/shader/font) | 01 | OPEN |
| 11 | `oh-render` graphics: stages, draw methods, cameras | 10,08 | OPEN |
| 12 | `oh-menu-system`: SSVMenuSystem analogue | 01 | OPEN |
| 13 | `oh-menu` MenuGame: all states, dialog box | 11,12,07 | OPEN |
| 14 | Lua console overlay + `-printLuaDocs` | 13,07 | OPEN |
| 15 | `oh-app`: boot, game loop, CLI, modes | 13,11,09 | OPEN |
| 16 | `oh-net` protocol: packets, sodium, shared codecs | 01,02 | OPEN |
| 17 | `oh-net` client + LeaderboardCache + SQLite DB | 16,15 | OPEN |
| 18 | `oh-net` server (HexagonServer) | 17 | OPEN |
| 19 | `oh-server-control` + `oh-workshop-uploader` tools | 17,15 | OPEN |
| 20 | Steam integration | 15 | OPEN |
| 21 | Discord integration | 15 | OPEN |
| 22 | Windows + Linux builds, CI, cross-compile check | all | OPEN |
| 23 | Parity verification harness + corpus | 15,09 | OPEN |
| 24 | Packaging: `_RELEASE` layout, asset bundling | 22 | OPEN |

(Status is mirrored in `.agents/plans/STATUS.md`, the authoritative table.)

## Mapping references (`.agents/plans/001-rust-port/mapping/`)

Produced by transcription workers; every issue spec cites the relevant file:

| File | Content |
|---|---|
| `gameplay.md` | walls, player, custom walls, status, RNG, tickers, timelines, update loop |
| `data-json.md` | data structs, JSON loaders, config keys, pack discovery, validators |
| `lua-api.md` | the complete Lua function table + callbacks + corpus usage |
| `rendering-menu.md` | render stages, draw methods, MenuGame state machine, dialog |
| `networking.md` | wire protocol, sodium, DB schema, client/server threading |
| `core-runtime.md` | main.cpp boot, replay byte format, config, audio, utils, tests |
| `libs-ssv.md` | SSVUtils/SSVStart/SSVMenuSystem/vrm_pp public API + PCG engine |
| `thirdparty-cpp.md` | every third-party dep + pin + call sites |

## Process

- Plan-process skill (`~/.pi/agent/skills/plan-process/SKILL.md`): PLAN.md +
  numbered issue files here; task specs in `.agents/tasks/`.
- Subagent-orchestration skill (`~/.pi/agent/skills/subagent-orchestration/SKILL.md`):
  orchestrator (session model) plans/reviews/commits; **workers run the local
  model** (`local/local`) for mechanical work; a gate-reviewer verifies each
  issue with executed evidence.
- Per issue: spec → worker implements → gate-reviewer executes gates → orchestrator
  re-runs gates → commit → next issue.
- In-repo skills: every library touched gets a skill in `.agents/skills/` written
  from real source, committed with the issue that first needs it.
