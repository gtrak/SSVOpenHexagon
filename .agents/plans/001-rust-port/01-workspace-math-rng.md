# Issue 01 — Workspace scaffold, C++ test-port, `oh-math` + `oh-rng`

## Objective

Create the Cargo workspace, port the small pure-C++ helpers that everything else
depends on (`oh-math`), and port the RNG (`oh-rng`) **bit-exactly** against the
oracle vectors. Establish the cross-platform gate harness (build + clippy + test
on Linux) that every later issue reuses.

## Key decisions

- Workspace layout per `PLAN.md` / `DECISIONS.md` D-001.
- `float`→`f32`, `double`→`f64`, no expression reordering (D-000).
- RNG must be bit-exact (D-003): reproduce `pcg32_fast` (MCG variant) and the exact
  `std::uniform_int_distribution` / `uniform_real_distribution` sampling.
- Skills-from-real-source applies: `oh-rng` must ship `lib-pcg.md`, `lib-ssvutils.md`
  (math subset) written from `extlibs/SSVUtils` real source.
- Vendor nothing exotic; use `flate2`/`serde_json` only from later issues.

## Files

| File | Change |
|---|---|
| `Cargo.toml` | new — workspace root, members, workspace deps, `[profile]` |
| `rust-toolchain.toml` | new — pin stable |
| `crates/oh-math/Cargo.toml`, `src/lib.rs` + submodules | new |
| `crates/oh-rng/Cargo.toml`, `src/lib.rs` (+ `pcg.rs`) | new |
| `tools/gate.sh` | new — build+clippy+test gate |
| `.agents/skills/lib-pcg.md` | new — from real source |
| `.agents/skills/lib-ssvutils.md` | new — math/FT/log subset from real source |

## Steps

1. `cargo new --lib` each crate under `crates/`; add workspace `Cargo.toml`.
2. Port `ssvu::FT` (= `f32`), `getFTToSeconds`, `getSecondsToFT`, `getFTToFPS`,
   and the math helpers used by the game, from `extlibs/SSVUtils/include/SSVUtils/Core/**`:
   `getRndR`/`getRndI`, `getMod`/`getByModIdx`, `getClamped`/`clamp`/`clampMax`,
   `getSign`, `contains`, `tau`/`pi`, `toRad`, `eraseRemoveIf`, `toLower`,
   `isPunctuation`/`isAlphanumeric`, `rotate`.
3. Port SSVStart `Utils/Vector2.hpp`, `Utils/SFML.hpp`, `Utils/Input.hpp` helpers
   used by gameplay: `getOrbitRad`, `getVecFromRad`, `getNormalized`,
   `getMagSquared`, `getRad`, `getDistEuclidean`, `zeroVec`, and the color/hue math
   from `src/SSVOpenHexagon/Utils/Color.hpp`, `Geometry.hpp` (line–circle
   intersection), `Easing.hpp`, `MoveTowards.hpp`, `Split.hpp`, `String.hpp`,
   `Concat.hpp`, `BuildPackId.hpp`, `LevelValidator.hpp`, `Timestamp.hpp`.
4. Port `random_number_generator` + `pcg32_fast` into `oh-rng`; implement
   `uniform_int`/`uniform_real` matching libstdc++ semantics.
5. Write `tools/gate.sh`: `cargo fmt --check` + `cargo build --workspace` +
   `cargo clippy --workspace --all-targets -- -D warnings` + `cargo test --workspace`,
   redirecting to a log and printing `GATE_EXIT=$?` (never pipe to `tail`).

## Verification

- `cargo test -p oh-rng` reproduces every value in
  `.agents/plans/001-rust-port/mapping/oracle-vectors.md` — raw stream, int
  distribution, real distribution, `advance(100)`, edge seeds.
- Unit tests for each math helper against hand-computed values **and** against a
  C++ probe compiled in-test-time from the vendored header where practical.
- `tools/gate.sh` exits 0.
- `cargo build --workspace` also succeeds for target `x86_64-pc-windows-gnu`
  if the target is installed (non-blocking if not; note it in the report).

## Notes / risks

- libstdc++'s `uniform_int_distribution` uses a rejection algorithm; matching it
  exactly matters only where the game passes non-power-of-two ranges. Document
  the chosen algorithm in `lib-pcg.md` and pin with vectors for non-power-of-two
  ranges (e.g. `{0,5}`, `{0,2}`).
- If a helper is unused by the game, do **not** port it.
