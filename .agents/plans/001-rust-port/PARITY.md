# PARITY — SSVOpenHexagon Rust-port parity verification (issue 23)

The parity harness proves the Rust port reproduces the C++ ground truth on
the pinned vectors: the RNG streams, the replay serialization, the pack-id /
level-validator construction, and the determinism-harness level pairs.

## Harness

| Piece | What it is |
|---|---|
| `crates/oh-parity` | the primary harness — a workspace test crate; 12 tests across 4 suites (`parity_rng`, `parity_replay`, `parity_pack_ids`, `parity_level_runs`); part of the `cargo test --workspace` gate |
| `tools/parity.sh` | the corpus regeneration + verification script: compiles the REAL C++ sources against `tools/oracle/parity_probe.cpp` + the minimal SFML shims, byte-compares the three C++-derived corpus files against the committed corpus, sanity-parses `level_runs.json` (the timekeeping identity), then runs `cargo test -p oh-parity` |
| `tools/oracle/parity_probe.cpp` | the probe: compiles the real `Core/RandomNumberGenerator.cpp`, `Core/Replay.cpp`, `Utils/BuildPackId.cpp`, `Utils/LevelValidator.cpp` and emits the corpus on the `=== corpus: <file> ===` markers |
| `.agents/plans/001-rust-port/parity/` | the corpus (see `README.md` there for the evidence classes) |

## Results

| Check | Evidence | Result |
|---|---|---|
| RNG streams (7 seeds × int/f32/f64 + the `advance(1)` wrapper shift) | **C++-probe-derived** (real `RandomNumberGenerator.cpp` + libstdc++ distributions); bit-exact f32/f64 + int streams; cross-checked against the pre-existing `mapping/oracle-vectors.md` pins (seed 12345) | `parity_rng::rng_streams_match_cpp_probe` — green |
| Replay serialization (file form / zlib level 9 / `sf::Packet` packet) | **C++-probe-derived** (real `Replay.cpp`): the mixed-keyboard-stream fixture + the four alwaysSpinRight level-run envelopes + the `replay_player` cursor walk, byte-exact | `parity_replay::{replay_serialization_matches_cpp_probe, replay_player_walk_matches_cpp_probe}` — green |
| Pack ids + level validators (all 8 shipped `_RELEASE` packs) | **C++-probe-derived** (real `BuildPackId.cpp` + `LevelValidator.cpp`); third leg: the corpus ids re-derive from the SHIPPED `pack.json` fields through the Rust comment-tolerant loader (the `workshopexample` pack.json is comment-laden) | `parity_pack_ids::{pack_ids_match_cpp_probe, corpus_ids_rederive_from_shipped_pack_jsons}` — green |
| Level-run pairs (the determinism-harness pairs) | **Seeded-replay pins** (see below) + **live-run properties**; the C++-derived envelope bytes of exactly these replay files pinned by the replay-serialization fixtures | `parity_level_runs::*` (4 tests) — green |
| Deliberately-wrong-pin discrimination (the harness is tested) | 1-ulp score drift, corrupted input stream, wrong seed, corrupted corpus byte, corrupted pack-id field — each must be rejected by the exact asserts | `deliberately_wrong_pin_discriminates` in all four suites — green |

### Level-run pins (seed 987654321, alwaysSpinRight 0x02 stream, 1.0×, first play)

| Pair | Pinned death frame F | Pinned score S | Identity |
|---|---|---|---|
| `cube/apeirogon` | 439 | 1.725 | S = ((439 − 25) × 0.25) / 60 |
| `experimental/autotest0` | 1180 | 4.8125 | S = ((1180 − 25) × 0.25) / 60 |
| `orthoplex/arcadia` | 456 | 1.7958333333333334 | S = ((456 − 25) × 0.25) / 60 |
| `orthoplex/bipolarity` | 358 | 1.3875 | S = ((358 − 25) × 0.25) / 60 |

The identity is C++-derived timekeeping: the seeded **replay** run defers its
start by one frame (the replay branch of the C++ update loop, HGUpdate.cpp:187)
then `HexagonGameStatus::resetTime`'s `currentPause = 0.1 × 60` (6.0
frametime units) consumes 24 frames at `TIME_STEP = 0.25` (Config.hpp:26,
240 ticks/s) → `(F − 25) × 0.25` accumulated ÷ 60, exact in f64 (integer ×
0.25, then one correctly-rounded division — the same op sequence the engine
accumulates through). Verified stable across independent processes.

The **live** alwaysSpinRight run's outcome is NOT pinned: its engine RNG is
entropy-seeded (C++ `initializeRng()`) and the levels' onIncrement side
changes draw from it (C++ HGUpdate.cpp:300-303,
`rng.get_int(sidesMin, sidesMax)`; e.g. apeirogon sets 5/7 below the 1.5×
gate) — so the live death frame varies with the seed, by C++ design (the C++
test's own live run carries the same property and asserts only
`score == replayPlayedTimeSeconds`). The pinned live-run **properties** are:
the recorded input bytes are all 0x02 (C++-derived bit order) and the
recorded replay reproduces the live score to the exact bit, running for the
live frame count + 1 (the structural one-frame start deferral). Observed
live runs on this machine: apeirogon 438/445 frames (1.725/1.7542), varying
per seed.

### The determinism-harness pair mapping

The "six pairs" of the determinism harness reduce to the four unique pairs
above: the 2-pair 25-iteration `ReplayExecution` loop (cube/apeirogon,
experimental/autotest0) + the 4-pair 100-iteration `ReplayExecutionBenchmark`
loop (those two + orthoplex/arcadia, orthoplex/bipolarity). Mapped in
`level_runs.json` `determinism_harness_pairs`.

## Contract corrections (verified against source)

1. **"6 pairs" = 6 harness entries = 4 unique pairs** (above).
2. **There is no "menu action" input class in the C++ replay** — the
   replay input is 4 bits (`input_bit::left/right/swap/focus`,
   `InputBits.hpp`); `alwaysSpinRight` sets only `right` (0x02) through the
   CW-only resolve branch (C++ InputHandler.cpp:91-95 — pinned by probe
   fixture 2).
3. **The live alwaysSpinRight run's outcome is seed-dependent** (the
   onIncrement side-change draws) — the corpus therefore pins the SEEDED
   REPLAY run's outcome (fully deterministic in level + seed + stream) and
   the live run's properties only.

## Gaps (documented, not silently dropped)

- **The full C++ game build is not available on this machine** (no
  SFML/LuaJIT/ImGui dev environment; no root for package installation), so
  the full-level-execution outcome (the live-run death frame/score, the
  onIncrement side-change sequences) cannot be C++-probe-derived here. The
  seeded-replay pins are Rust-self-consistent + analytically anchored to the
  C++ timekeeping/RNG source; the envelope BYTES of exactly those replay
  files ARE C++-probe-derived (real `Replay.cpp`), so the serialization
  round trip through the Rust engine is end-to-end C++-anchored. The
  `level_runs.json` `evidence` field states this per corpus.
- The C++ `ReplayExecution.t.cpp`'s windowed phase (the `GameWindow` loop) is
  untestable headless; the manual-update `update` → `post_update` pattern
  (the D-018 pattern) exercises the same engine code path the windowed loop
  drives.

## Reproduce

```sh
bash tools/parity.sh          # probe + corpus byte-compare + cargo test -p oh-parity
cargo test -p oh-parity       # the harness alone
```
