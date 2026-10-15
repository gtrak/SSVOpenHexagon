# Task spec — issue 008: `oh-game` types (walls, player, custom walls, status)

You are a worker. Implement exactly this issue. Do not redesign or widen scope.

## Context

Repo: `/home/gary/dev/SSVOpenHexagon`. Literal 1:1 Rust port. Issues 001
(workspace/`oh-math`/`oh-rng`), 002 (`oh-replay`), 003 (`oh-data`), 005
(`oh-assets`) are LANDED.

Read first:
1. `.agents/plans/001-rust-port/PLAN.md`, `DECISIONS.md`
2. `.agents/plans/001-rust-port/08-game-types.md` (the contract)
3. `.agents/plans/001-rust-port/mapping/gameplay.md` — **the authoritative
   transcription** of every gameplay type (17 sections; §1 SpeedData, §2 CWall,
   §3 CPlayer, §4 CCustomWall*, §5 HGStatus, §6 LevelStatus, §8 Ticker,
   §9 timeline2, §10 CustomTimeline*, §11 FastVertexVector, §12 PointInPolygon/
   Geometry, §13 Utils, plus §16 cross-cutting notes). Re-derive any number.
4. `.agents/plans/001-rust-port/mapping/oracle-vectors.md` — the **gameplay
   geometry pins** (wall vertices, wall update, player position, wall push,
   line-circle codes). These are the acceptance evidence.
5. The real C++ sources named in the contract.
6. `.agents/skills/README.md`

## Working agreement

- **Skills are truth.** Write `.agents/skills/lib-gameplay.md` documenting the
   collision-escape algorithm, vertex ordering, and the RNG-in-update coupling
   from the real source.
- **Write first**, compile early.
- **Arithmetic exactness (critical here)**: this is where replay parity is won or
  lost. `float`→`f32`, `double`→`f64`; no expression reordering; copy every literal
  (`5.02f`, `24.f`, `0.5f`, `+8.f`, `1.f/60.f`, ...). `Vec2` is `oh-math`'s type.
- **Commit at the first green build**.

## What to build

1. `SpeedData` (§1), `CWall` (§2) including the vertex constructor, `moveTowardsCenter`
   bounds logic, `moveCurve`, `getCurveRadians`.
2. `CPlayer` (§3): every field; `push` for both wall kinds incl.
   `checkWallCollisionEscape`; `draw*` writing into the vertex batches;
   `update`/`updateInputMovement`/`updatePosition`/triangle-width transition.
3. `CCustomWall` + `CCustomWallManager` (§4, handle reuse + flags).
4. `Ticker` (§8), `Utils::timeline2` + runner (§9), `CustomTimeline(Manager)` (§10).
5. `HexagonGameStatus` (§5) and `LevelStatus` (§6) with all timers/pause/accumulators.
6. `FastVertexVectorTris` (§11) and `PointInPolygon`/`Geometry` (§12).

## Verification

1. `bash tools/gate.sh` — paste all `GATE_EXIT=` lines.
2. **Oracle tests**: assert the exact values in `mapping/oracle-vectors.md`
   "Gameplay geometry" section — wall_v0..v3, wall_u_v0..v3, wall_overlap,
   wall_dead, player_pos/angle, push_result/push_pos, lci_code2/lci_code0.
   Reproduce them yourself with `bash tools/oracle/run.sh` first.
3. A curve-rotation sign test (hand-computed sin/cos) and a Ticker overflow test.
4. Status-timer pause/resume/accumulate test.

## Report format

Module map, design summary, gate outputs, skill corrections, deviations, known gaps.
