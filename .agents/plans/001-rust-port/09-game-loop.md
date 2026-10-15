# Issue 09 — `oh-game` update loop (HGUpdate / HexagonGame), headless-capable

## Objective

Port the game simulation: `HexagonGame` lifecycle (`newGame`, `start`,
`setLevelData`, `death*`, `incrementDifficulty`, `sideChange`, `performPlayerSwap`,
`performPlayerKill`, replay record/playback) and the full update loop from
`HGUpdate.cpp` (`update`, `updateWalls`, `updateInput*`, `updateEvents`,
`updateLevel`, `updatePulse`, `updateBeatPulse`, `updateRotation`, `updateFlash`,
`updateCameraShake`, `updatePulse3D`, `updateText`, `updateParticles*`,
`updateCustomTimelines`, `updateCustomWalls`, `postUpdate`), wiring the Lua API
from issue 07 and the types from issue 08. Rendering calls are abstracted behind a
"renderer" trait so this loop can run **headless** for determinism testing.

## Key decisions

- Mirror `HexagonGame.hpp` + `HexagonGame.cpp` + `HGUpdate.cpp` 1:1.
- Provide a `Renderer` trait (or a set of vertex sinks) so `oh-game` does not depend
  on the render backend (issue 10); `oh-render` implements it. Headless = a no-op
  renderer that still records vertices/batches for tests.
- Replay recording: every tick records `(left,right,swap,focus)` exactly as the C++
  does (`updateInput_RecordCurrentInputToLastReplayData`), so replays match.
- `executeGameUntilDeath`, `runReplayUntilDeathAndGetScore`, `fastForwardTo`,
  `advanceByTicks` — port with the same time budgets.

## Files

| File | Change |
|---|---|
| `crates/oh-game/src/hexagon_game.rs` | new — lifecycle + state |
| `crates/oh-game/src/update.rs` | new — the update loop |
| `crates/oh-game/src/renderer.rs` | new — the renderer trait (seam for issue 10) |
| `crates/oh-game/tests/determinism.rs` | new — replay execution determinism |

## Steps

1. Port `HexagonGame` struct fields + constructors + `newGame`/`start`.
2. Port the input pipeline (`updateInput*`, joystick/touch resolution, impl→movement).
3. Port every `update*` method in `HGUpdate.cpp`, in order, with exact constants.
4. Port `incrementDifficulty`, `sideChange`, `performPlayerSwap`, `performPlayerKill`,
   `death*` (replay file creation, score saving seam).
5. Port Lua callback firing at the exact points the C++ fires them.
6. Port replay playback: `ActiveReplay`, `mustReplayInput`, feed inputs from
   `replay_player` each tick.

## Verification

- **Determinism harness** (`crates/oh-game/tests/determinism.rs`): given a packed
  level + seed + a policy-driven input vector, run headlessly and assert the exact
  final score / frame count / RNG draws. The policy is the C++ test's own
  (`executeRandomInputs` / `alwaysSpinRight`, `test/ReplayExecution.t.cpp:40-51`),
  so it is reproducible without an external input file.
- Port the C++ tests in `test/ReplayExecution.t.cpp` and
  `test/ReplayExecutionBenchmark.t.cpp` to Rust, asserting the same outcomes —
  i.e. **the self-consistency property they actually assert** (see the CORRECTION
  below; they carry no expected numbers).
- Lua-callback ordering test: a level script that logs callbacks; assert the order
  matches the C++ firing points.
- `tools/gate.sh` green.

### CORRECTION (D-018) — what the C++ replay tests actually assert

This section previously claimed the C++ tests "encode expected scores ... port those
numbers verbatim as the pin". **That is false** (verified against the source this
session). `test/ReplayExecution.t.cpp` contains exactly three assertions
(lines 81, 104, 109): the replay was created, playback terminated, and
`TEST_ASSERT_EQ(score, replayPlayedTimeSeconds)` — the live run's played-time equals
the replay run's played-time. There is **no numeric literal in the file**; it is a
*self-consistency* check, not an oracle. Nor is there an external input vector: the
inputs come from the engine's own RNG. So the ported test must be labelled
**self-consistent, not C++-verified**, and the C++-derived evidence for this issue is
the `oracle_gameplay.rs` geometry vectors plus any new `tools/oracle/` probes.

The C++ tests DO name real shipped content (re-derived via `buildPackId`): pack
`ohvrvanilla_vittorio_romeo_cube_1` (⇒ `_RELEASE/Packs/cube`, version 1) with level
`..._apeirogon`, and pack `ohvrvanilla_vittorio_romeo_experimental_1` (⇒
`_RELEASE/Packs/experimental`) with level `..._autotest0`. The harness can use these
exact ids. Note `autotest0.json` has `difficultyMults: []` while the test passes
`diffMult = 1.f` — mirror that, do not "fix" it.

## Notes / risks

- This is where "1:1" is won or lost: any reordered float op in the update loop
  changes replay outcomes. The gate must compare against **C++-derived expected
  values**, not self-consistency.
- The C++ `test/ReplayExecution.t.cpp` encodes expected scores for known
  level/seed/replay combos — port those numbers verbatim as the pin.
