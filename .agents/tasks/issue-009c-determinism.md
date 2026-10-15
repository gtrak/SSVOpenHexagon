# Task spec — issue 009 (step c): the determinism harness + the test-side composition

You are a worker. Implement exactly this. Do not redesign or widen scope.
Issue 009 steps (a)+(b) are LANDED on `master` (`a406f5a8`); the update loop itself
is ported. What is missing is the **evidence** the issue requires, and the small
amount of composition glue that evidence needs.

## Context

Repo: `/home/gary/dev/SSVOpenHexagon`. Literal 1:1 Rust port of SSVOpenHexagon.
`bash tools/gate.sh` must be 4/4 green when you finish (it is green now; 298 tests).

Read first (all in-repo, authoritative):
1. `.agents/plans/001-rust-port/09-game-loop.md` — **including its `CORRECTION
   (D-018)` section: read it before you plan anything.**
2. `.agents/plans/001-rust-port/DECISIONS.md` — D-000 (arithmetic), D-012 (source
   beats contract), D-013 (one shared engine/Lua state), D-015 (accessor trait),
   D-018 (what the C++ replay test really asserts).
3. `test/ReplayExecution.t.cpp` (136 lines — read all of it) and
   `test/ReplayExecutionBenchmark.t.cpp`.
4. `.agents/plans/001-rust-port/mapping/lua-api.md` §5 (callback firing points).
5. `.agents/skills/lib-luajit.md` + `.agents/skills/lib-mlua.md` (the Lua runtime).
6. The C++ sources you are mirroring: `Core/HexagonGame.cpp`,
   `Core/HGUpdate.cpp`, `Core/HGScripting.cpp`, `Utils/Utils.cpp`
   (`runLuaFile` / `runLuaFileCached`).

## What D-018 means for your plan (do not skip)

`test/ReplayExecution.t.cpp` has exactly **three** assertions (lines 81, 104, 109):
a replay was created, playback terminated, and
`TEST_ASSERT_EQ(score, replayPlayedTimeSeconds)` — the live run's
`playedTimeSeconds` equals the replay run's. **There are no expected numbers in the
file, and no external input vector.** The inputs come from the engine's own RNG
(`executeRandomInputs` / `alwaysSpinRight`, lines 40-51).

So the property to port is **self-consistency**: *a deterministic engine replays its
own recording to an identical played-time.* Label it exactly that way — never call
it C++-verified. That is the whole point of the test, and it is still a genuinely
strong signal (it fails loudly on any replay-affecting desync).

## What to build

### 1. The test-side composition (new file, e.g. `crates/oh-game/tests/` support module)

A real `oh_game::seams::ScriptHost` over `oh_lua`, so a level's `.lua` can actually
run. **This is the hard part — read this whole section before writing code.**

**Start from the existing working template, do not invent a new shape:**
`crates/oh-lua/tests/api.rs` already (a) builds a `State` =
`Arc<ReentrantMutex<GameApiState>>` via `GameApiState::new(...)` from the `oh_game`
gameplay types (see its `game_state(in_menu)` helper, lines 148-176), (b) calls
`init_lua_api` + `init_level_api` (lines 484-485), and (c) shows how `oh_game`'s
gameplay types plug into `oh_lua`'s API state. Your composition is the same shape
with the real `oh_assets::Assets` behind `AssetsApi` and **this crate's
`HexagonGame`** behind `GameApi` (it already implements `GameApi`:
`hexagon_game.rs:2092`). The two-phase build is: construct `HexagonGame` with
`Arc<dyn GameApi>` = a clone of itself (the engine IS the backend), then
`attach_state(state)` (`hexagon_game.rs:559`) — `attach_state` is required before
any use, and the `Arc<dyn GameApi>` must be built before the state exists, hence
the `OnceLock` + two-phase pattern documented at `hexagon_game.rs:336-357` and the
caller contract at `hexagon_game.rs:555-568`.

The obstacle you WILL hit and must solve correctly: `oh_game::seams::ScriptHost`
requires `Send + Sync + 'static`, and the engine stores it as
`Arc<dyn ScriptHost>` inside `Inner<S>`, which must be `Send + Sync` (because
`impl GameApi for HexagonGame<S>` and `GameApi: Send + Sync`, and
`GameApiState` must be `Send` to live in the `ReentrantMutex` — see
`lua_state.rs` docs around `ReentrantMutex`). But `mlua::Lua` is `!Send + !Sync`
(the `send` feature is deliberately OFF), so a struct that *owns* a `LuaContext`
can never be `Send`.

`oh-lua` already provides the intended bridge — use it:
`oh_lua::api::state::SharedLua` (`state.rs:55-104`) is a `#[derive(Clone, Copy)]`
handle holding a raw `*const LuaContext`, with documented `unsafe impl Send/Sync`,
whose contract is: *the `LuaContext` must outlive every closure holding a
`SharedLua`, and is only dereferenced on the single thread that owns it.* Its
`new(&ctx)` extends the borrow to `'static` under that contract.

So: keep the `LuaContext` alive outside the `Send + Sync` host (a `Box::leak`, a
`static`/`thread_local`, or an owner the test holds for the whole run), construct
`SharedLua::new(&ctx)`, and have the host hold the `SharedLua` + the
`Arc<ReentrantMutex<GameApiState>>` (the 007 `State` alias,
`oh_lua::api::State`). Driving the callbacks through
`oh_lua::api::callbacks::*` gives you `call_on_init`, `call_on_load`,
`call_on_update(ft)`, `call_on_step`, `call_on_input(ft, movement, focused, swap)
-> Result<bool>`, `call_on_pre_death`, `call_on_death`, `call_on_increment`,
`call_on_pre_unload`, `call_on_unload`, `call_on_cursor_swap`. `init_lua` must run
`init_lua_api` (`oh_lua::api::init_lua_api`) — the shared registration — and your
host's `run_lua_file(path)` must mirror the C++:

- `Utils/Utils.cpp:49-78` `runLuaFileCached`: a **per-assets file cache**
  (`assets.getLuaFileCache()`), read the file bytes once, then `executeCode` on the
  cached string; returns whether it was already cached (a hit).
- `Utils/Utils.cpp:80-110` `runLuaFile`: open failure ⇒
  `"Fatal Lua error\nCould not open file: <name>\n"` + throw; execute error ⇒
  `"Fatal Lua error\nFilename: <name>\nError: <what>\n"` + rethrow.
- `HexagonGame::runLuaFile` (`HGScripting.cpp:1127-1147`): headless **or**
  `Config::getUseLuaFileCache()` ⇒ the *cached* path; else the uncached one. On
  any error, unless `Config::getDebug()`, `goToMenu(false, true)`.
- `LuaContext::execute_code` uses chunk name `"chunk"` (already ported in
  `oh-lua`'s `SharedLua::execute_code`) — verify, don't assume.

You also need a real `oh_lua::api::state::AssetsApi` adapter over
`oh_assets::Assets` (the trait's 13 methods; lookups are
`Assets::get_music_data/get_style_data/find_pack_data/get_shader_id_by_path/
is_valid_shader_id` + the `set_shader_uniform_*` no-ops). Accumulate the
`LoadInfo`/error semantics the C++ has (a missing style/music is an **error** in
`AssetsApi`, `ok`-wrapping the `Option` into the C++ exception message).
`oh-game` already depends on `oh-assets` (normal dep), and `oh-lua` is already a
dev-dep — so this belongs in the test crate or a `#[cfg(test)]` module.

**Scope guard:** put the composition in the test tree (a support module) unless you
hit a wall that genuinely requires production code; if you do, keep it additive and
say so in your report. Do NOT redesign the D-013 shared-state model.

### 2. `crates/oh-game/tests/determinism.rs` — the ported C++ test

Mirror `test/ReplayExecution.t.cpp` faithfully:

- Use the **real shipped content** the C++ names. Verified: the level ids are
  `packId + "_" + levelId` (`MenuGame.cpp:4840`) and `buildPackId` (space→underscore
  per field joined by `_` + version) gives
  `ohvrvanilla_vittorio_romeo_cube_1` (`_RELEASE/Packs/cube`) and
  `ohvrvanilla_vittorio_romeo_experimental_1` (`_RELEASE/Packs/experimental`).
  The levels are `..._apeirogon` and `..._autotest0`. Load via
  `oh_assets::Assets::new(<repo>/_RELEASE, true /*headless*/, false)` (see
  `crates/oh-assets/tests/release_tree.rs` for the exact root-path idiom).
- **`autotest0.json` has `"difficultyMults": []`** while the C++ test passes
  `diffMult = 1.f`. The C++ `LevelData` ctor *appends* `1.f` and sorts
  (`Data/LevelData.cpp:36-37`) — the Rust port does the same
  (`oh-data/src/level_data.rs:64-65`), so `getNthDiffMult` is safe. Mirror the
  `1.f`, do not "fix" it, and do not add a guard the C++ does not have.
- Create the profile the C++ creates (`ProfileData{GAME_VERSION, "testProfile"}`,
  `assets.addLocalProfile`, `pSetCurrent("testProfile")`) — check `oh-assets`'
  profile API for what is actually public; if `pSetCurrent` is not exposed, supply
  your own `Profiles` seam impl for the engine (that IS a seam) and note it.
- The property: for a chosen `(pack, level)`, run **live** to death with the
  `executeRandomInputs` **or** `alwaysSpinRight` policy (mirror the C++
  `getRndBool()` choice; a fixed choice is fine and less flaky — document which
  and why), capture the replay via the engine's `on_death_replay_created` hook
  (`hexagon_game.rs:1350`), then replay it with
  `run_replay_until_death_and_get_score` and assert
  `live.played_time_seconds == replay.played_time_seconds` **exactly** (`==`, as
  the C++ does — not an epsilon).
- Do BOTH variants the C++ does: the same `HexagonGame` instance and a **fresh**
  one (`differentHG`) — a fresh instance is the stronger check.
- Loop over at least the two `(pack, level)` pairs the C++ uses. The C++ does 25
  iterations and then a windowed set; headless-only here (there is no window), so
  port the headless loop. Keep the runtime sane — `executeGameUntilDeath` takes a
  `max_processing_seconds` budget (the C++ passes 1); if 25 iterations of both
  levels is too slow for a gate, use fewer and **state the number and the reason in
  your report** (do not silently reduce it).
- Mark the test's nature in a doc comment: **self-consistent, not C++-verified**.

### 3. A genuine C++-derived pin (do not skip this either)

The header comment must not claim more than it delivers. Add at least one
**C++-derived** expectation via the existing oracle route: a small deterministic
headless run whose score/frame count you can pin from C++-computed math, OR a new
`tools/oracle/` probe (add the probe + a `run.sh` line, record the output in
`mapping/oracle-vectors.md`). If a C++ number genuinely cannot be obtained, say so
explicitly in the report as a gap — do NOT quietly present a self-consistent value
as C++-derived.

### 4. A Lua-callback ordering test

A level script that appends callback names to a Lua table (or a Rust-side sink),
asserting the firing **order** matches the C++ points in `mapping/lua-api.md` §5 /
`HGUpdate.cpp`. Keep it small; it can live in the same test file or a sibling.

## Working agreement

- **Write first**, compile early. Commit at the first green build.
- Everything in `crates/oh-game/` (and `_RELEASE`-reading test helpers) is yours.
  Do **not** touch `crates/oh-menu/`, `crates/oh-render/`, or other crates; if you
  believe a change is needed there, STOP and report instead.
- Arithmetic exactness: no reordered float ops, no `as f64` round-trips to "fix"
  a mismatch. If the numbers disagree, the port is wrong — find it in the C++.
- Do not modify `tools/gate.sh`.

## Verification (report ALL of these)

1. `bash tools/gate.sh` — paste every `GATE_EXIT=` line (must be 4/4).
2. The new test names + what each asserts, and for each expected value **exactly
   how it was derived** (C++ probe / C++ source / self-consistent).
3. The **discrimination check** (this is required evidence, not optional): mutate
   one replay-affecting constant in a *scratch copy* of the tree (e.g. bump a
   multiplier or reorder one float op) and show the determinism test reddens.
   Revert the mutation; report the exact mutation and the observed failure.
4. A clean-checkout run:
   `rm -rf /tmp/v9c && mkdir -p /tmp/v9c && git archive HEAD | tar -x -C /tmp/v9c && cd /tmp/v9c && CARGO_TARGET_DIR=/tmp/v9c/target bash tools/gate.sh`.
5. Any gap you could not close, stated plainly.

## Report format

Module map · gate outputs (all `GATE_EXIT=` lines) · the composition design (how
you satisfied `ScriptHost: Send + Sync` given `mlua::Lua: !Send`) · each expected
value and its derivation · the discrimination check · the clean-checkout result ·
deviations · known gaps.
