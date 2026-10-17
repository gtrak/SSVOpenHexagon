# Per-frame fingerprint — the C++ ↔ port whole-engine oracle definition

Task 030b (the C++ column). This is the **single definition** implemented twice
(the C++ instrumentation and the Rust port's twin) and the reference for the
committed fingerprint vectors (`cpp_fingerprints.json`). It is a durable
interface: anything that consumes or extends the fingerprint must follow this
file, and a change to it is a definition change (bump `v1` → `v2` below,
regenerate the vectors, never silently reorder fields).

**Version: `v1`.**

## What it is

One line per simulated frame, written to a file by a flag, OFF by default on
both sides:

* C++: `./OpenHexagon -headless <replay>.ohr.z -fingerprint <path>`
  (new flag; the hook is at the end of `hg::HexagonGame::update`,
  `src/SSVOpenHexagon/Core/HGUpdate.cpp`).
* Port: `HexagonGame::set_fingerprint_path(Some(path))` before
  `run_replay_until_death_and_get_score`; the hook is at the end of
  `HexagonGame::update` (`crates/oh-game/src/update.rs`), the verbatim twin of
  the C++ function.

Line format (ASCII, `\n`-terminated):

```
<frame> <hash> <draws>
```

* `frame` — decimal u64. **0-based** index of the `update()` call within the
  current game. The counter is reset by `newGame` (C++ `newGame`, port
  `new_game`), so a replay run's first line is `0` and the last line is the
  frame in which the player died (`executeGameUntilDeath` stops after that
  frame's `update`/`postUpdate`).
* `hash` — 16 lowercase hex digits = the FNV-1a 64-bit digest of the field
  byte string (below).
* `draws` — decimal u64, the number of **game-RNG draw calls** that happened
  during this frame (delta since the previous frame; the running total is
  reset with the frame counter by `newGame`). See "The RNG trap" below.

## Hash algorithm (identical on both sides)

FNV-1a, 64-bit, over the field byte string `S`:

```
h = 0xcbf29ce484222325
for each byte b in S: h ^= b; h = h * 0x100000001b3   (mod 2^64)
hash = lowercase hex of h, zero-padded to 16 digits
```

## Field byte string `S` (field set + EXACT order)

All integers little-endian. **Floats are hashed as raw IEEE-754 bit patterns**
(f32 → `f32::to_bits` / `memcpy`'d `u32`; f64 → `f64::to_bits` / `memcpy`'d
`u64`). No epsilon, no normalisation: a 1-ULP divergence (including `-0.0`
vs `0.0`) MUST change the hash.

| # | field | encoding | source (C++ / port) |
|---|-------|----------|---------------------|
| 1 | wall count | u32 | `walls.size()` / `st.walls.len()` |
| 2 | for each wall `w`, in **vector order** (spawn order): | | `walls[i]` / `st.walls[i]` |
| 2a | 4 vertex positions | 8 × u32 (x then y, v0..v3) | `getVertexPositions()` / `get_vertex_positions()` |
| 2b | speed | 5 × u32: `_speed,_accel,_min,_max,_pingPong` | `getSpeed()` / `get_speed()` |
| 2c | curve | 5 × u32: same five | `getCurve()` / `get_curve()` |
| 2d | hueMod | u32 | `_hueMod` via `setHueMod` symmetry (see below) / `hue_mod` |
| 2e | killed | u8 (0/1) | `isDead()` / `is_dead()` |
| 3 | custom-wall count | u32 | `cwManager.count()` / `cw_manager.count()` |
| 4 | for each custom wall, **handle order 0..count-1**: | | |
| 4a | 4 vertex positions | 8 × u32 | `getVertexPos4(h)` / `get_vertex_pos4(h)` |
| 4b | killingSide | u8 | `getKillingSide(h)` / `get_killing_side(h)` |
| 4c | flags | u8 (bit 0 = `NoCollision`, bit 1 = `Deadly`) | `getCanCollide()/getDeadly()` / `get_can_collide()/get_deadly()` |
| 5 | player | 3 × u32: pos.x, pos.y, angle | `player.getPosition()/getPlayerAngle()` / `est.player.get_position()/get_player_angle()` |
| 6 | status scalars | 11 × u32: `pulse, pulseDirection, pulseDelay, beatPulse, beatPulseDelay, pulse3D, pulse3DDirection, flashEffect, radius, fastSpin, cameraShake` | `getStatus()` / `st.status` |
| 7 | status booleans | 5 × u8: `hasDied, mustStateChange (0=None,1=MustRestart,2=MustReplay), scoreInvalid, started, showPlayerTrail` | same field names |
| 8 | customScore | u32 | `getStatus().getCustomScore()` / `get_custom_score()` |
| 9 | game time (seconds) | u64 (f64 bits) | `getStatus().getTimeSeconds()` / `get_time_seconds()` |
| 10 | increment timer (seconds, time since last increment) | u64 (f64 bits) | `getIncrementTimeSeconds()` / `get_increment_time_seconds()` |
| 11 | level status | u64 `currentIncrements` (LE); u32 `sides`; 3 × u32 `speedMult, delayMult, rotationSpeed` | `getLevelStatus()` / `st.level_status` |
| 12 | difficultyMult | u32 | `HexagonGame::difficultyMult` / `st.difficulty_mult` |
| 13 | lastPlayedScore | u64 (f64 bits) | `HexagonGame::lastPlayedScore` / `est.last_played_score` |
| 14 | entity counts | 3 × u32: `particles, trailParticles, swapParticles` sizes | `particles.size()` etc. / `est.particles.len()` etc. |
| 15 | game-RNG state | u64 (LE) | canonical MCG position (below) |

Notes on the field set:

* **Wall "positions/rotations/types"**: `CWall` carries no standalone
  rotation field — its four vertex positions fully encode position AND
  orientation, so the raw vertices are hashed. No derived trig (e.g.
  `atan2` of an edge) is included: transcendental functions are not
  guaranteed bitwise-identical across libm/libc, and the raw vertices are
  what both implementations actually advance. Wall "type" is implicit:
  entries in `walls` are standard walls, custom walls appear only in
  `cwManager` (a custom wall is never in `walls`).
* `hueMod` is not publicly readable in C++ (private `_hueMod`, only a
  setter). The instrumentation patch adds a `const` getter
  (`getHueMod()`), additive-only. It is 0 for every shipped level; the field
  is included so a future non-zero use cannot silently desync.
* **Score**: this codebase's live score is either the Lua-override
  `customScore` (field 8) or falls back to the played time (field 9, the
  same value the headless runner prints as `Final time`). `lastPlayedScore`
  (field 13) is the score recorded on the replay being executed. All three
  are hashed; none is derived.
* **Excluded, by design** (not comparable / not gameplay state):
  * SFML texture/sprite/texture-ids, cameras (window-only), key icons,
    `sf::Text` contents, style data (recomputed each frame as a pure
    function of the hashed level-status/pulse fields), audio state,
    Steam/Discord.
  * **Timeline-runner wall-clock state** (`HRTimePoint` timestamps inside
    `timeline2_runner` / `Ticker`): these are *real* time points, not
    comparable across runs/machines. Their gameplay consequences (which
    events fired: spawned walls, created custom walls, incremented status)
    ARE covered by the hashed fields.
  * The cosmetic RNG (below), the non-deterministic `random_input_rng`
    (process-static `mt19937` in C++, per-engine `pcg32_fast` seeded from
    entropy in the port — non-deterministic BY DESIGN on both sides).

## The RNG trap — why `draws` is a separate column

The C++ has **ONE** global `pcg32_fast` stream (`HexagonGame::rng`) serving
gameplay AND cosmetics, while the port deliberately splits a separate
`cosmetic_rng` (a documented deviation — `.agents/audits/game.md`, the
"cosmetic RNG" seam). Hashing the raw global stream without care would flag
that known deviation every frame and drown the real signal.

Two parts:

1. **State IS comparable, provided the cosmetic draws never touch the game
   stream.** In the C++, every cosmetic draw is gated:
   `ssvu::getRndR/getRndI` call sites are
   * window-gated (particle spawning `HGUpdate.cpp:944-955,1051-1061`,
     camera-shake `HGGraphics.cpp:89` — all under `window != nullptr`, null
     in headless), or
   * `firstPlay`-gated (`MusicData::playRandomSegment`
     `Data/MusicData.cpp:46` — draws only when `!firstPlay`).
   The synthetic corpus is pinned to `first_play = true`, so during a
   headless replay run the C++ game-RNG stream and the port's
   `st.rng` consume identical draw sequences and their states match.
2. **`draws` per frame is the attribution diagnostic.** If two runs diverge
   at frame N: same prefix + a `draws` spike in C++ (draws > 0 where the
   port has 0, or vice versa) = an unexpected draw into the game stream
   (the cosmetic-split class of bug); identical draws but different hash =
   a real gameplay divergence. Either way the first divergent frame plus the
   `draws` column turns a mismatch into a diagnosis.

**"A draw" is one wrapper call** — one `get_int` / `get_real` on the game
RNG object — NOT the number of underlying u32s the distribution consumes
(rejection sampling makes that variable; counting wrapper calls is what is
symmetric, since both implementations route every game-RNG consumption
through exactly these methods: C++ `hg::random_number_generator::
get_int/get_real`; port `SharedState.rng.get_int/get_real`, which is every
call site in `oh-game`/`oh-lua` except the excluded
`cosmetic_rng` and `random_input_rng` seams).

## The canonical RNG-state label (field 15)

`pcg32_fast` == `pcg_engines::mcg_xsh_rs_64_32`: a 64-bit **MCG**
(`no_stream`, `increment() == 0`), multiplier `M = 6364136223846793005`,
states always `≡ 3 (mod 4)`, the wrapped state is `3`. The engine state is
`protected` in the PCG header (and `engine::distance` is too), so both sides
label the state with the PCG reference's **public** `operator-`:

> **`rng_state = (f − pcg32_fast{3})`** where `operator-` is defined as
> `lhs − rhs == rhs.distance(lhs.state_)` — i.e. the number of engine bumps
> `d ∈ [0, 2^62)` such that `3 · M^d ≡ state (mod 2^64)`: the bump count
> from the wrapped state `3` to the current state, computed by the
> bit-by-bit algorithm in `pcg_random.hpp` (`engine::distance`, the
> `is_mcg` path: `the_bit` starts at 4 and doubles; result `>> 2`).

This is a pure-integer, transcendental-free, deterministic bijection of the
state coset onto `[0, 2^62)` — identical arithmetic on both sides, and a
unique label of the exact internal state (any two distinct states give two
distinct labels, and the label is stable under the same bump/advance
sequence). C++ computes `f − pcg32_fast{3}` (exposed additively as
`random_number_generator::state()`); the port computes the same bit-by-bit
loop in `oh_rng::pcg::distance_from_wrapped`
(`Pcg32Fast::state_position()`). Ground truth for the port's copy: the
C++-probe values committed under
`.agents/plans/001-rust-port/mapping/oracle-vectors.md`
("pcg state position" — `tools/oracle/pcg_state_probe.cpp`).

## Frame / hook placement

* Hook = end of `HexagonGame::update` (the C++ function
  `src/SSVOpenHexagon/Core/HGUpdate.cpp`, port
  `crates/oh-game/src/update.rs`), AFTER the trailing
  `mustStateChange` block, BEFORE the closing brace. The only
  early-`return`s in the function are the fast-forward / tick-advance test
  hooks (never set during replay execution) and, on the
  `goToMenu()` state-change branch, a return that cannot fire during a
  headless replay run (it requires `window == nullptr` AND a
  `mustStateChange != None` while the run is still live; the death frame
  sets `hasDied`, which stops the `executeGameUntilDeath` loop, and
  `death()` only sets `mustStateChange` when `window != nullptr`).
* `executeGameUntilDeath` (both sides) is `while (!status.hasDied) {
  update(TIME_STEP, timescale); postUpdate(); }` — so exactly one hook line
  per loop iteration, and the last line is the death frame.
* The hook is **observation-only**: it must not mutate any gameplay field.
  Off by default (`fingerprint_file == null` / `None`): the hook returns
  immediately and the default behaviour and output of both binaries are
  byte-for-byte unchanged.

## The committed vectors

`.agents/plans/001-rust-port/parity/cpp_fingerprints.json` — for each
corpus replay: the replay's exact `.ohr.z` bytes (hex), the C++ headless
`Final time` / `customScore` result, the frame count, and the full
per-frame `<frame> <hash> <draws>` sequence (the C++ column, generated by
the instrumented binary in the container). The port-side check
(`crates/oh-parity/tests/parity_cpp_fingerprints.rs`) regenerates the
replay bytes in-process (byte-exact against the committed hex — the
02-replay pin), runs the same replay through the port's
`run_replay_until_death_and_get_score` with the fingerprint enabled, and
compares frame-by-frame hash + draws + the final result. The check survives
without the C++ present: the C++ column is frozen in the JSON.

## Diagnosis when a frame diverges

The port-side comparison reports the **first divergent frame** and which
of its three columns differs (`frame` / `hash` / `draws`). `draws` is
already the attribution (the RNG trap section). For a `hash` divergence the
bring-up tool was a pair of **env-gated, observation-only dumps at frame 0**
(added to both twins temporarily, then removed to keep the committed
instrumentation minimal — re-add them the same way if needed):

* `OH030B_FP_DEBUG_PATH=<path>` — the per-field VALUES at frame 0
  (identical key set on both sides; compare the values);
* `OH030B_FP_BYTES_PATH=<path>` — the exact FNV input BYTE STRING at
  frame 0 (hex); a diff localises the field, and an independent FNV over
  the dumped bytes says which side's hash is "wrong" (if the bytes agree
  but the hashes don't, the hash builder itself is asymmetric — the one
  bring-up divergence was exactly that: a transcribed FNV prime off by a
  factor of 16).

Both dumps must be added symmetrically (same fields, same order) and are
inert unless the env var is set; the committed vectors are stable with or
without them (verified: the C++ column regenerated byte-identical with the
dumps present).
