# STATUS — SSVOpenHexagon Rust port

Authoritative status table for plan 001. One row per issue. Updated in the same
commit as the work it describes (see plan-process skill: "Landing a lane includes
updating the tracker").

Plan: `.agents/plans/001-rust-port/PLAN.md`
Mapping references: `.agents/plans/001-rust-port/mapping/`
Task specs: `.agents/tasks/`
Decisions log: `.agents/plans/001-rust-port/DECISIONS.md`

| # | Title | Depends | State | Commit | Notes |
|---|---|---|---|---|---|
| 01 | Workspace scaffold, C++ test-port, `oh-math`+`oh-rng` | — | SPECD | — | `issue-001-impl.md` written |
| 02 | `oh-replay` byte-exact + zlib + packet codec | 01 | OPEN | — | |
| 03 | `oh-data`: structs + comment-tolerant JSON loaders | 01 | OPEN | — | |
| 04 | `oh-config`: all keys, binds, joystick | 01 | OPEN | — | |
| 05 | `oh-assets`: pack/level discovery + asset registry | 01,03 | OPEN | — | |
| 06 | `oh-lua` runtime: LuaContext analogue + conversions | 01 | OPEN | — | |
| 07 | `oh-lua` game API: all globals + callbacks | 06,03,04 | OPEN | — | |
| 08 | `oh-game` types: walls, player, custom walls, status | 01,02 | OPEN | — | |
| 09 | `oh-game` update loop (HGUpdate/HexagonGame) headless | 08,07 | OPEN | — | |
| 10 | `oh-render` backend (window/input/vertex/shader/font) | 01 | OPEN | — | SFML analogue decision |
| 11 | `oh-render` graphics: stages, draw methods, cameras | 10,08 | OPEN | — | |
| 12 | `oh-menu-system`: SSVMenuSystem analogue | 01 | OPEN | — | |
| 13 | `oh-menu` MenuGame: all states, dialog box | 11,12,07 | OPEN | — | |
| 14 | Lua console overlay + `-printLuaDocs` | 13,07 | OPEN | — | |
| 15 | `oh-app`: boot, game loop, CLI, modes | 13,11,09 | OPEN | — | |
| 16 | `oh-net` protocol: packets, sodium, shared codecs | 01,02 | OPEN | — | |
| 17 | `oh-net` client + LeaderboardCache + SQLite DB | 16,15 | OPEN | — | |
| 18 | `oh-net` server (HexagonServer) | 17 | OPEN | — | |
| 19 | `oh-server-control` + `oh-workshop-uploader` tools | 17,15 | OPEN | — | |
| 20 | Steam integration | 15 | OPEN | — | |
| 21 | Discord integration | 15 | OPEN | — | |
| 22 | Windows + Linux builds, CI, cross-compile check | all | OPEN | — | |
| 23 | Parity verification harness + corpus | 15,09 | OPEN | — | |
| 24 | Packaging: `_RELEASE` layout, asset bundling | 22 | OPEN | — | |

States: OPEN → SPECD → in-flight → LANDED (commit) → (blocked/parked with reason).

## Foundation (pre-issue) — landed

| Item | Path | Commit | Notes |
|---|---|---|---|
| plan-process skill copied global | `~/.pi/agent/skills/plan-process/SKILL.md` | n/a (outside repo) | |
| submodule sources fetched | `extlibs/*` | see below | needed for library skills from real source |
| PLAN.md | `.agents/plans/001-rust-port/PLAN.md` | | |
| mapping corpus | `.agents/plans/001-rust-port/mapping/*.md` | | 8 transcription workers |
