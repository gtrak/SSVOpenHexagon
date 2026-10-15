# Issue 08 — `oh-game` types: walls, player, custom walls, status

## Objective

Port the gameplay data types and their math: `CWall`, `CPlayer`, `CCustomWall`,
`CCustomWallManager`, `SpeedData`, `HexagonGameStatus`, `Ticker`,
`Utils::timeline2` (+ runners), `CustomTimeline(Manager)`, `FastVertexVectorTris`,
and the polygon/geometry helpers — with exact formulas, constants, and float types.

## Key decisions

- Mirror the C++ headers/impls 1:1 (paths in `PLAN.md`).
- `sf::Vector2f` → `oh-math::Vec2` (f32); `sf::Color` → a `Color` type matching
  SFML's 8-bit RGBA semantics and constructor clamping.
- `std::byte`/vertex batching: `FastVertexVectorTris` → a `VertexBatch` with the
  same `reserve_more`, `batch_unsafe_emplace_back{,_quad}` semantics (append 3/6
  vertices in tri/quad order) so draw order is identical.
- `HexagonGameStatus` timers use `Clock.hpp` (`HRTimePoint`) → `std::time::Instant`
  or a monotonic equivalent; copy pause/accumulate semantics exactly.
- `timeline2` uses `std::chrono::high_resolution_clock` time points; port to
  `Instant` with the same wait/wait-until/wait-for logic.

## Files

| File | Change |
|---|---|
| `crates/oh-game/Cargo.toml` | new — deps `oh-math`, `oh-rng`, `oh-timeline` |
| `crates/oh-game/src/{wall,player,custom_wall,custom_wall_manager,speed_data,status,ticker,timeline,custom_timeline,vertex_batch}.rs` | new |
| `crates/oh-game/tests/` | new — collision, vertex-ordering, curve-rotation, ticker, status-timer tests |

## Steps

1. Port `SpeedData` (update formula + pingPong).
2. Port `CWall` (constructor vertex math, `moveTowardsCenter` bounds logic,
   `moveCurve`, `getCurveRadians`, hue mod).
3. Port `CPlayer` (all fields; `push` for both wall types incl.
   `checkWallCollisionEscape` line–circle intersection; `draw*` produce vertices
   into the batches; `update`/`updateInputMovement`/`triangleWidthTransition`).
4. Port `CCustomWall` + `CCustomWallManager` (handle allocation/reuse, flags).
5. Port `Ticker` (loop/running/target semantics) and `timeline2`+runner.
6. Port `CustomTimeline`/`CustomTimelineManager`.
7. Port `HexagonGameStatus` (all timers, pause, accumulators, custom score).
8. Port `PointInPolygon`, `Geometry` (exact return codes/formulas).

## Verification

- Collision tests: construct a wall, move the player into it, assert `push` returns
  the same bool as a C++ probe. **Generate C++ oracle vectors** (a small probe
  compiled from `CPlayer.cpp` + `CWall.cpp` — do this if practical; otherwise pin
  the arithmetic against hand-computed values and document the gap).
- Vertex-ordering test: assert the exact 4 vertex positions for a known wall
  constructor call, quoted from the formula.
- Curve-rotation sign test: assert the rotated vertices match hand-computed sin/cos.
- Status-timer test: pause/resume/accumulate produce the documented seconds.
- `tools/gate.sh` green.

## Notes / risks

- `always_inline` / `[[gnu::always_inline]]` in the C++ is an optimization hint; the
  Rust port keeps the functions but need not force-inline.
- The player's `checkWallCollisionEscape` depends on iteration order
  (`vxIncrement` = 1 for custom walls, 2 for normal) — copy exactly.
- Any `float` literal like `24.f`, `0.5f`, `5.02f` must remain `f32`.
