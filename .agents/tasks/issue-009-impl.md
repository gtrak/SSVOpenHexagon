# Task spec — issue 009: `oh-game` update loop (HGUpdate / HexagonGame), headless

You are a worker. Implement exactly this issue. Do not redesign or widen scope.

## Context

Repo: `/home/gary/dev/SSVOpenHexagon`. Literal 1:1 Rust port. Issues 001–008,
010 are LANDED; `bash tools/gate.sh` passes 4/4 with 241 tests.

**This is the issue where replay parity is won or lost.** Any reordered float
operation in the update loop changes replay outcomes.

Read first:
1. `.agents/plans/001-rust-port/PLAN.md`, `DECISIONS.md` (D-000 arithmetic,
   D-003 RNG bit-exactness, D-009 Lua `%.14g`)
2. `.agents/plans/001-rust-port/09-game-loop.md` (the contract)
3. `.agents/plans/001-rust-port/mapping/gameplay.md` — **THE AUTHORITATIVE
   TRANSCRIPTION**: §13 is `Utils.cpp`/helper formulas, §14 is `HGUpdate.cpp`
   (the whole update loop, method by method), §15 is `HexagonGame.cpp`
   (lifecycle, newGame, death, difficulty, sideChange), §16 the cross-cutting
   notes (esp. the per-frame RNG fix-up and the `updatePulse3D`-always-runs rule).
4. `.agents/plans/001-rust-port/mapping/lua-api.md` §5 (callbacks) — the exact Lua
   invocation points
5. `test/ReplayExecution.t.cpp` + `test/ReplayExecutionBenchmark.t.cpp` — the C++
   tests. NOTE (D-018): they assert self-consistency
   (`score == replayPlayedTimeSeconds`), NOT hard-coded scores.
6. The real C++: `src/SSVOpenHexagon/Core/HGUpdate.cpp`, `HexagonGame.cpp`,
   `include/SSVOpenHexagon/Core/HexagonGame.hpp`

## Working agreement

- **Write first**: scaffold `crates/oh-game/src/update.rs` + `hexagon_game.rs`
  immediately, compile early.
- **Rendering is behind a seam.** Define a `Renderer` trait (or vertex-sink
  struct) so this crate does NOT depend on `oh-render`; headless = a no-op
  renderer that still records batches, so tests can assert vertex output without a
  display. `oh-render` implements it later (issue 11).
- **Lua integration**: use the state interfaces that issue 007 defined. Call the
  callbacks at exactly the points the C++ does.
- **Arithmetic exactness (critical)**: `f32`/`f64` exactly, no reordering, every
  literal verbatim. Reference `TIME_STEP` from `oh-config` (240 Hz tick).
- **Commit at the first green build.**

## What to build

1. `HexagonGame` struct + lifecycle: constructor, `newGame`, `start`,
   `setLevelData`, `incrementDifficulty`, `sideChange`, `performPlayerSwap`,
   `performPlayerKill`, the `death*` family (replay-file creation, PB effects,
   score-save seam).
2. The full update loop from `HGUpdate.cpp`, in order: `update`, `updateInput*`
   (incl. joystick/touch resolution and impl→movement), `updateWalls`,
   `updateIncrement`, `updateEvents`, `updateLevel`, `updateCustomTimelines`,
   `updateCustomWalls`, `updatePulse`, `refreshPulse`, `updateBeatPulse`,
   `refreshBeatPulse`, `updateRotation`, `updateCameraShake`, `updateFlash`,
   `updatePulse3D`, `updateText`, `updateParticles`, `updateTrailParticles`,
   `updateSwapParticles`, `postUpdate`.
3. **The per-frame RNG fix-up** (the 5 `rng.advance(u64(|float|))` calls) exactly
   as mapped, and `updatePulse3D` running even when 3D is disabled.
4. Replay recording (`updateInput_RecordCurrentInputToLastReplayData`) and
   playback (`ActiveReplay`, `mustReplayInput`).
5. `executeGameUntilDeath`, `runReplayUntilDeathAndGetScore`, `fastForwardTo`,
   `advanceByTicks` with the same time budgets.

## Verification

1. `bash tools/gate.sh` — paste all `GATE_EXIT=` lines.
2. **Determinism harness**: given a packed level + seed + a fixed input vector,
   run headless and assert the exact final score / frame count / RNG draw
   sequence. Derive the expectation from a C++ run if you can (see below);
   otherwise pin the Rust value and clearly mark it as self-consistent, not
   C++-verified.
3. **Port `test/ReplayExecution.t.cpp`** — but see the CORRECTION below: it
   carries **no expected numbers**. What it asserts is `score ==
   replayPlayedTimeSeconds` (the live run's played-time equals the replay run's),
   i.e. *self-consistency*. Reproduce exactly that property (both the same-game and
   the `differentHG` variants) and label it **self-consistent, not C++-verified**.
   Do NOT invent a numeric pin, and do NOT present a self-consistent number as
   C++-verified.

   **CORRECTION (D-018, verified against source):** earlier drafts of this spec
   claimed the C++ test "encodes expected scores for known level/seed/replay
   combos". That is false. `grep -n TEST_ASSERT test/ReplayExecution.t.cpp` gives
   lines 81/104/109 only: `rf.has_value()`, `score2.has_value()`, and
   `TEST_ASSERT_EQ(score, replayPlayedTimeSeconds)` (a plain `==` on two computed
   doubles, `TestUtils.hpp:174`). No literals. There is also no external input
   vector — inputs come from the engine's own RNG (`executeRandomInputs` /
   `alwaysSpinRight`, lines 40-51). The packs it names ARE real and shipped:
   `_RELEASE/Packs/cube` (level `apeirogon`) and `_RELEASE/Packs/experimental`
   (level `autotest0`) — level ids are `packId + "_" + levelId`
   (`MenuGame.cpp:4840`). Note `autotest0.json` has `"difficultyMults": []` while
   the test passes `diffMult = 1.f`; mirror that, do not fix it.
4. A callback-ordering test (a level script that logs; assert the order matches the
   C++ firing points).
5. `v0` sanity: a 1000-tick headless run of a real level completes without panic
   and produces a plausible score.

## Notes on getting C++ expectations

`tools/oracle/` compiles real C++ sources against minimal SFML shims and prints
golden values; extend it if you need a new vector (add a probe + a `run.sh` line,
and record the output in `mapping/oracle-vectors.md`). A full C++ *game* build
needs SFML+LuaJIT+ImGui and is not available, so prefer: (a) hand-computed
expectations for small update steps, (b) C++ probes for isolated math, and
(c) explicitly-labelled self-consistent pins. Never present a self-consistent pin
as C++-verified.

## Report format

Module map, gate outputs, the determinism evidence and exactly how each expectation
was derived, skill corrections, deviations, known gaps.
