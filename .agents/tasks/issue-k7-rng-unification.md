# Task K7 — one PCG stream, not two: third-party levels' `onRenderStage` must consume the SAME RNG the C++ does

Read `.agents/WORKER-HYGIENE.md` first (§11: the pinned C++ lives in `~/.cache/ohoracle-deps/`, the repo's
extlib submodules are empty).

## The deviation (register row K7 — read it in `.agents/plans/001-rust-port/deviations.md`)

**the C++ has ONE global `pcg32` for gameplay AND cosmetics; the port SPLITS `cosmetic_rng`.** A level
whose `onRenderStage` draws random numbers therefore consumes the port's COSMETIC stream, while in the C++
those draws advance the SAME stream the gameplay draws from — so such a level diverges from the original
(and a replay of it diverges too). The register notes this affects *third-party content only*: the
shipped corpus does not appear to draw random numbers from the render hook, which is exactly why the
3627-frame C++ fingerprint corpus still passes.

## Why this is worth fixing, and why it is bounded

Replay determinism is the port's first-class promise, and this is a place where content that the original
handles correctly would not replay identically. The fix is narrow: make the cosmetic draws consume the
SAME stream as the gameplay draws, as the C++'s single global `pcg32` does. The port's shape today is a
seam — `cosmetic_rng: Arc<dyn CosmeticRng>` in `crates/oh-game/src/hexagon_game.rs:433`, used at
`hexagon_game.rs:1946/2476` and throughout `crates/oh-game/src/update.rs` (~:1329-1349 and neighbours) —
so decide deliberately whether you remove the seam or re-point it at the game's one RNG, and say which and
why. Do NOT introduce a third stream; the point is that there is exactly one.

## Acceptance — the oracle is already in the repo, so this is verifiable, not hopeful

1. **The C++ fingerprint corpus must still pass unchanged**: `.agents/plans/001-rust-port/parity/
   cpp_fingerprints.json` is the C++-generated column (030b) and the parity test compares the port against
   it (see `crates/oh-parity/tests/parity_cpp_fingerprints.rs`). If your change perturbs the corpus, STOP
   and report — that would mean the corpus DOES consume cosmetics, and the correct conclusion changes.
2. **Prove the property with a test**, not by inspection: a level (or a direct test) whose `onRenderStage`
   draws a random number must advance the SAME stream the gameplay reads — e.g. draw in the render hook,
   then assert the next gameplay draw equals what the C++'s single-stream order produces. A test that
   simply re-asserts the old split is worthless here.
3. Update the K7 register row (CLOSED with your landing hash, or still open with the new evidence) — and if
   the corpus DOES turn out to consume cosmetics, the row needs the opposite verdict and I need to know
   first.

## Boundaries

`crates/oh-game/` (the RNG plumbing and its call sites) plus the register row and the tracker. Do NOT
change the RNG algorithm or its seeding (that is 030b's pinned territory), do not touch the shipped
corpus, and do not "fix" this by special-casing the render hook. Host gate before each commit (it includes
the parity stage), `cargo check --target aarch64-linux-android` if you touch shared code, explicit-path
commits, `timeout` everywhere, and append a `## Report` with the verdict, the property test, and the
corpus result.

## Report (K7 lane, 2026-10-18)

**Verdict: the deviation as stated does not exist — the port's render-hook
randoms already advance the SAME stream the C++'s do, and the property is now
test-pinned. The seam stays; it is the faithful port of a C++ stream the
register under-described.**

### What I verified (pinned C++, read directly)

The register's "ONE global pcg32 for gameplay AND cosmetics" is loose. The
C++ has **two** game streams:

1. **`HexagonGame::rng`** — the replay-seeded `random_number_generator`
   (`include/SSVOpenHexagon/Core/RandomNumberGenerator.hpp`:20-62, a wrapped
   `pcg32_fast`; `HexagonGame.cpp`:57-65 `initializeRng`, `:595`
   `rng = initializeRng()`, the replay branch `rng =
   random_number_generator{replayFile._seed}`). The level-code random API is
   bound to IT: `LuaScripting.cpp`'s `initRandom(lua, rng)` (:81-107) binds
   `u_rndReal`/`u_rndIntUpper`/`u_rndInt` to `rng.get_real/get_int`;
   `redefineRandom` (:158-176) rebinds `math.random` to `u_rndSwitch`;
   `HexagonGame::initLua` (`HGScripting.cpp`:1105-1107) passes the game's OWN
   `rng`. (`math.randomseed` is destroyed on both sides — `:201` — so
   `math.random`/`u_rnd*` are the only level randoms.)
2. **The SSVUtils thread-local `pcg32_fast` global** —
   `SSVUtils/Core/Utils/Rnd.hpp`:27-37 (`getRndEngine()`, entropy-seeded, the
   gitlink-pinned `6200143` copy in the main checkout) — driving the
   draw-path camera-shake jitter (`HGGraphics.cpp`:89-92), the PB/swap
   particle spawns (`HGUpdate.cpp`:944-955, :1051-1061), the music segment
   pick (`MusicData.cpp`:46) and the menu's tip/level picks.

The port mirrors this **1:1**: `SharedState.rng` ↔ `mRng` (the update loop
and the Lua `u_rnd*` closures lock the SAME `GameApiState` —
`compose.rs`:653-681, `tests/common/mod.rs`'s two-phase build); the
`cosmetic_rng` seam ↔ the SSVUtils global, its call sites matching the C++'s
exactly (`hexagon_game.rs`:1946 segment pick, `:2476`/`bridge.rs`:446 draw
jitter, `update.rs`:1329-1349/:1425-1433 particles). A level's
`onRenderStage` `math.random` has hit the port's GAME stream since issue 007
(`3f5d06eb`) / D-013 (`58dc5a87`) — `oh-lua/src/api/random.rs` binds
`u_rnd*` to `lock(&st).rng`. The feared mechanism ("the hook consumes the
port's COSMETIC stream") was never in the code.

### The decision (remove the seam vs re-point it)

**Neither.** Re-pointing the seam at the game RNG would make the particle
spawns / shake jitter / segment pick consume the replay-seeded stream —
shifting subsequent gameplay draw values relative to the C++'s `mRng` on any
windowed level, and (via `beatPulseDelay` → `fixup` → `mRng.advance`,
`update.rs`:400-411) changing state the C++'s entropy stream leaves alone.
That would INTRODUCE the divergence the register feared, in the opposite
direction. Removing the seam is a behavior-preserving refactor that drops
the 29d2 `pb_particle_rng_order.rs` double's injection surface with no
parity benefit. The seam stays — exactly two streams, mirroring the C++'s two
objects (no third introduced).

### The property test (acceptance 2) — `render_hook_rng_stream.rs`, `38591224`

`crates/oh-game/tests/render_hook_rng_stream.rs` (1 test, mutation-checked):
a synthetic pack whose `seq` level's `onRenderStage` calls `math.random()`
per hook (recorded in `hookDraws`), `l_setIncTime(1.0)` + `l_setSidesMin(3)`
+ `l_setSidesMax(5)` so the increment's side-selection draw
(`rng.get_int(sidesMin, sidesMax)`, `HGUpdate.cpp`:299-303, port
`update.rs`:338-344) fires at a known frame (266: start frame 2 + the C++'s
0.1 s start pause + 240 `TIME_STEP` frames). Seeded deterministically
(`set_last_replay` seed 42 + `new_game(executeLastReplay)` = the C++'s
`rng = random_number_generator{_seed}`), driven 280 frames in the C++'s
frame order (`update` → `postUpdate` → the hook in the draw), with a
single-stream oracle (`RandomNumberGenerator::new(42)`) advanced by the
exact wrapper-call sequence (hook `get_real`s, the side `get_int`, and the
per-frame `fixup` advances read from the engine's post-frame status values).
Asserted: the hook draw counts in the game stream's `draws()`; it advances
`state_position()` by exactly one bump (the f32 [0,1] canonical, `__m=1`,
no rejection); it is bit-exactly the oracle's value; the next gameplay draw
(the side selection) equals the single-stream order's value; all 281 hook
draws and the final state equal the oracle's; and the no-hook control level
(divergent run, same seed) has draw count exactly 281 lower and a different
final state — under a split stream the two runs would be identical in every
game-stream metric, which the test refutes. Mutation check: replacing the
hook draw with a no-draw body reds the attribution assertion.

### The corpus result (acceptance 1)

**UNCHANGED, passing**: `cargo test -p oh-parity --test
parity_cpp_fingerprints` → 3/3 (incl. `cpp_fingerprint_vectors_match_the_port`,
the 3627-frame C++ column). Expected and consistent: the headless corpus run
has no draw path (`executeGameUntilDeath` = `update` + `postUpdate` only, on
both sides), so the seam's call sites are window-gated / `firstPlay`-gated
out of it (the RNG-trap section of `cpp-fingerprint-definition.md`) — and
this lane made no behavior change at all.

### Boundaries / gates

- Touched: `crates/oh-game/tests/render_hook_rng_stream.rs` (new; test
  target only — no shared (non-test) code, so no Android cross-check owed per
  the spec's "if you touch shared code" clause) + the register row + the
  tracker row + this report. RNG algorithm/seeding (030b's territory) and
  the shipped corpus untouched; no render-hook special-casing.
- Host gate 5/5 on the test commit (fmt / build / clippy `-D warnings` /
  test --workspace / lua-parity).
- Not established: nothing material — the C++ windowed draw-order (shake
  jitter before the hooks, `HGGraphics.cpp`:84-97 then :119+) was verified
  by reading, not by executing the C++ (the full C++ build is unavailable on
  this box, 030b's standing constraint); the property test pins the PORT
  side of that order (hook after update, the port's `draw_frame` step 2
  before its step-3 hook), and the corpus pins the headless column.
