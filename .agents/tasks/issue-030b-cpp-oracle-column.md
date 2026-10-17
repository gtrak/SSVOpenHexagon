# Task 030b — the C++ column: replay-driven three-way verification

Read `.agents/WORKER-HYGIENE.md` first. This is the operator's **bonus axis**: prove
the Luau parity against the *actual C++*, not only against LuaJIT.

**Prerequisite:** `030a` landed (the differential harness + the host-selectable
backends + the synthetic-replay generator). If 030a has not landed, do the
bounded C++ build survey in §1 and stop.

---

## 1. The interface — a real find, use it

The C++ entry point accepts a **compressed replay filename as a command-line
argument** (`src/SSVOpenHexagon/Core/main.cpp:146`
`getFirstCompressedReplayFilenameFromArgs`; `:468-500` → `decompress_replay_file`
→ `hg.setLastReplay(replayFile)` → `newGame(..., /* mExecuteLastReplay */ true)`),
and it has a **`-headless`** flag (`main.cpp:119-122`). So:

```
./OpenHexagon -headless <replay>.ohr.z
```

executes that replay headless. Because the `.ohr.z` format is **byte-exact
oracle-pinned** (issue 02), the replay file is a *neutral interchange format*
between the two implementations — the port can **generate** a synthetic replay and
hand the same bytes to the C++. That is the whole basis of this lane.

## 2. Build the C++ — bounded

The build tree is **already configured but not built**: `build/` holds
`CMakeCache.txt`, `CMakeFiles`, `CPM_modules`, `_deps`, and helper scripts
(`build/c.sh`, `build/c_notest.sh`, `build/car.sh`, …), but no binary.

- Use those helpers (read `build/c.sh` before inventing a command).
- CPM may need network to fetch dependencies. **Bound the effort**: one honest
  attempt, and if it turns into toolchain/dependency archaeology, **STOP and
  report** with the exact failing command and error. A clean "the C++ does not
  build here, because X" is a useful result; sinking the lane is not.
- Report the build outcome either way.

## 3. Determine how a headless replay reports its result

Read `mainClient(headless=true, …)` and the replay executor to find what the C++
actually emits for a completed replay run (score? played frametime? a printed
line? an exit code?). The port's headless replay path
(`oh-app/src/client.rs`, the `runReplayUntilDeathAndGetScore` analogue) is the
counterpart. **Do not guess the output format** — quote the C++ source.

## 4. Generate and run

- Use 030a's synthetic-replay generator to emit a corpus covering distinct
  levels × difficulties × seeds (small at first: a handful of cases that exercise
  different level scripts, then widen if it is cheap).
- Run each replay through the C++ headless and record the result.
- **Commit the results as oracle vectors** under the project's existing
  parity-vector convention (`.agents/plans/001-rust-port/parity/` +
  `crates/oh-parity`), so the check survives without the C++ present.

## 5. Instrument the C++ for a per-frame fingerprint (authorized)

The operator has authorized instrumenting the C++. That removes the coarse link
entirely — and it upgrades this lane from "a Lua cross-check" into a **whole-engine
fidelity oracle**, which is the most valuable verification asset in the project
(see §6).

**Where.** One hook at the end of the C++'s per-frame update — `HexagonGame::update`
(`src/SSVOpenHexagon/Core/HGUpdate.cpp`) or the loop that calls it; pick the point
where the port's fingerprint is taken so the two are commensurable. One hook, one
hash function.

**Definition symmetry is the whole game.** The field set, the field ORDER, and the
hash algorithm must be identical on both sides. Document the definition once (in
this task file or a short companion note) and implement it twice. **Hash floats as
raw bit patterns** (`f32::to_bits`) — never an epsilon comparison, or a 1-ULP
divergence is exactly what you will fail to see. Both sides use the port's
`f32`/`f64` conventions.

**Fields** (gameplay-visible, semantically equivalent in both): the wall set with
positions/rotations/types in a defined order, player pos/rot, `score`,
`customScore`, level-status timers, entity counts.

**The RNG subtlety — do not get this wrong.** The C++ has **ONE** global
`pcg32` stream serving gameplay *and* cosmetics, while the port deliberately splits
`cosmetic_rng` from the game RNG (a documented deviation — `.agents/audits/game.md`,
the "cosmetic RNG" seam). So putting the raw global RNG state in the fingerprint
would flag that *known* deviation on every single frame and drown the real signal.
Instead:

- fingerprint the **state** (which is genuinely comparable), and
- emit the per-frame **draw count** as a separate diagnostic column, so a
  divergence can be attributed: same state + different draws = the documented
  cosmetic split (expected; recordable); different state = a real gameplay
  divergence.

**Emission.** Text, one line per frame (`<frame> <hash>`), to a path given by a NEW
FLAG (e.g. `-fingerprint <path>`). **Off by default, additive only: default
behaviour and default output must be unchanged**, because this C++ tree is the
project's citation reference and must stay faithful for everything else. Keep the
diff minimal and clearly marked, and — if practical — capture it as a patch file
under `tools/` so the instrumentation is reproducible and reviewable rather than an
unrecorded local edit.

## 6. The triangle — now frame-level on all three axes

- **C++ ↔ port(LuaJIT)** — **frame-level**, via §5.
- **port(LuaJIT) ↔ port(Luau)** — **frame-level**, via 030a's harness.
- Therefore **C++ ↔ port(Luau) directly**, not merely by transitive inference.

State plainly which fields were compared and which could not be (e.g.
render-only or sampling-only state), and for every divergence report the **first
divergent frame** plus the field that diverged first — that is what turns a
mismatch into a diagnosis.

**The larger prize, worth saying out loud:** with this instrumentation the oracle
is no longer about Lua. It verifies the **port itself**, frame by frame, against
the real implementation — so the remaining audit fixes, the wasm build, and every
future change can be measured against the C++ instead of being argued about. Treat
the fingerprint definition as a durable interface, since it will outlive this lane.

Report: the build outcome, the exact C++ invocation, the instrumentation diff (or
patch path), the fingerprint definition, the per-axis results, the committed
vectors, and the first-divergent-frame analysis for every divergence found.

## Hygiene
- **No worktree** (D-035); main checkout; one commit per logical step; **explicit
  paths only, never `git add -A`**; `cargo fmt --all` before committing; host gate
  green before each commit.
- Bound every build/run: `timeout`. Never leave a background process running (an
  interrupted lane once wedged in a blocking engine run — see the hygiene file).
- Do not commit a large replay corpus or machine-specific artifacts; commit the
  **vectors** (small, text) and generate the replays in the test's temp dir.
- If the C++ cannot be built, check whether the **existing** committed parity
  vectors already cover replay scores; if so, say what they do and do not cover,
  and report that the C++ column is blocked rather than faking it.

---

## STATUS: BLOCKED on system packages (2026-10-17)

A lane attempted this and died in dependency archaeology. The measured blocker:

- **No passwordless sudo** on this machine, so packages cannot be installed
  non-interactively.
- The SFML the C++ fetches via CPM needs **system** X11/GL dev headers, which are
  mostly MISSING: `xrandr`, `xcursor`, `xi`, `gl`, `freetype2`, `openal`,
  `vorbis`, `vorbisfile`, `flac` (only `x11` and `libudev` are present). SFML can
  vendor freetype/openal/vorbis/flac, but **not** the X11/GL interfaces — those are
  system APIs, so the build cannot proceed without root. There is no
  `SFML_USE_SYSTEM_DEPS` setting in the project's `CMakeLists.txt` to flip.

**To unblock**, one command (needs the operator's password):

```
sudo apt-get install -y libx11-dev libxrandr-dev libxcursor-dev libxi-dev \
  libxext-dev libxrender-dev libxinerama-dev libgl1-mesa-dev libglvnd-dev \
  libfreetype6-dev libopenal-dev libvorbis-dev libflac-dev
```

Then re-dispatch this lane as-is: the design (the `-fingerprint` flag, the
fingerprint definition, the replay-as-interchange-format interface, the
`./OpenHexagon -headless <replay>.ohr.z` invocation) is complete and unchanged, and
`030a`'s harness will already have produced the synthetic-replay corpus to feed it.

**This lane is NOT required for the Luau decision** — `030a` answers
LuaJIT↔Luau on its own. This one adds the C++ cross-check (and the whole-engine
oracle). Do not start it before the packages are installed; an empty run just
repeats the archaeology.

---

## Report — complete (2026-10-17; the BLOCKED status above is superseded)

The blocker (no sudo / missing system X11+GL dev packages) was removed by the
operator's pre-built Docker image (`localhost/oh-cpp-oracle:1`, built 2026-10-17
with all six CPM pins seeded into `$BUILD/*-src`); `tools/cpp-oracle/run.sh`
drives it. No sudo, no host packages, no toolchain archaeology — the build is a
reproducible `run.sh build`. (This lane ran in the `lane/030b` worktree per the
orchestrator's parallel-lane hygiene, §8 of `WORKER-HYGIENE.md`; the task's own
"main checkout" line predates the worktree convention.)

### Build outcome

- **Pristine**: `REPO=$PWD BUILD=/home/gary/dev/oh-cpp-build timeout 5400
  tools/cpp-oracle/run.sh build` — configure + full ninja build, GREEN; binary
  at `$BUILD/SSVOpenHexagon`. Smoke run of a 12-input replay: `Player died. /
  Final time: 2.02917`, exit 0.
- **Instrumented**: the 030b fingerprint patch (7 files, 489 lines, OFF BY
  DEFAULT — see below) applied, incremental build GREEN, output of the
  unflagged binary unchanged (`Final time: 1.725` before and after for the same
  replay).
- **Rebuild-stability**: all six committed vectors regenerated byte-identical
  after a clean rebuild of the instrumented binary — the C++ column is a stable
  pin, not a flaky artifact.

### How a headless replay reports its result (task §3 — quoted, not guessed)

`src/SSVOpenHexagon/Core/main.cpp` headless branch: after
`getFirstCompressedReplayFilenameFromArgs` (`:146`) finds the `.ohr.z` argument,
`hg::decompress_replay_file(crf)` (failure → `"Could not decompress replay
file"` on cerr, exit 1), then:

```cpp
std::cout << "Player died.\nFinal time: "
          << hg.runReplayUntilDeathAndGetScore(replayFile, 1 /*maxProcessingSeconds*/, 1.f /*timescale*/).value().playedTimeSeconds << '\n';
```

So the ONLY stdout result is `Player died.` + `Final time: <double>` (the
`std::ostream` default 6-significant-digit float format of
`GameExecutionResult::playedTimeSeconds`), exit 0. The port's counterpart
(`crates/oh-game/src/hexagon_game.rs::run_replay_until_death_and_get_score`,
mirrored output in `oh-app/src/client.rs`) prints the same lines; the committed
vectors pin the exact printed string (reproduced by
`oh_math::hg::concat::format_ostream_f64`).

### Exact C++ invocation

```
REPO=$PWD BUILD=/home/gary/dev/oh-cpp-build \
  tools/cpp-oracle/run.sh run -- \
  -headless /build/corpus/<name>.ohr.z -fingerprint /build/corpus/<name>.fp.txt
```

`run.sh run` stages `/build/_RELEASE` (symlinks into the read-only `/src`
mount + the built binary) because the C++ `main()` chdirs to its own parent
directory to find `Assets/`/`Packs/`; `[-e KEY=VAL ...]` flags (added this
lane) forward env vars for debug hooks. The `.ohr.z` files were generated by
the PORT (`oh_replay::compress_replay_file`, the byte-exact issue-02 format)
via `cargo test -p oh-parity --test gen_corpus` (env `OH030B_CORPUS_OUT`), and
the committed vectors carry their exact bytes (hex), so the whole pipeline is
reproducible from the port alone + one container run.

### The instrumentation (patch path + definition)

- **Patch**: `tools/cpp-oracle/fingerprint.patch` (verified to apply to the
  pristine tree; the worktree carries the same 7-file change). Files:
  `RandomNumberGenerator.hpp` (`draws()` counter + `state()` canonical label),
  `CWall.hpp` (`getHueMod()`), `CCustomWallManager.hpp` (`aliveHandles()`),
  `HexagonGame.hpp` (fingerprint members + `setFingerprintPath()` +
  `maybeEmitFingerprint()`), `HGUpdate.cpp` (FNV field builder + the
  end-of-`update()` hook), `HexagonGame.cpp` (`newGame` counter reset),
  `main.cpp` (`-fingerprint <path>` parsed headless-only).
- **Definition**: `.agents/plans/001-rust-port/parity/cpp-fingerprint-definition.md`
  (**v1**, the durable interface): line `<frame> <hash> <draws>`; FNV-1a 64-bit
  (offset `0xcbf29ce484222325`, prime `0x100000001b3`) over an exactly-ordered
  little-endian field byte string — wall count → per-wall (8 raw vertex u32,
  speed 5×u32, curve 5×u32, hueMod u32, killed u8) → custom-wall count → per
  alive custom wall (8 vertex u32, killingSide u8, flags u8) → player
  pos/angle (3×u32) → status 11×f32 → 5 bools → customScore f32 → played time +
  increment timer (2×f64 bits) → levelStatus (u64 currentIncrements, u32 sides,
  3×u32) → difficultyMult f32 → lastPlayedScore f64 → 3 entity counts → the
  game-RNG canonical state label (u64). **Floats as raw IEEE-754 bit
  patterns** — a 1-ULP divergence (including `-0.0`) must change the hash.
- **The RNG trap, as designed**: C++'s ONE global pcg32 (gameplay + cosmetics)
  vs the port's split `cosmetic_rng`. The state label (field 15) uses the PCG
  reference's public `operator-` (`f − pcg32_fast{3}` = the MCG bump count from
  the wrapped state, port twin `oh_rng::pcg::distance_from_wrapped`,
  probe-pinned against C++ in `tools/oracle/pcg_state_probe.cpp` +
  `mapping/oracle-vectors.md`). In headless replay runs every C++ cosmetic draw
  site is window-gated (null window) or `firstPlay`-gated (corpus pinned to
  `first_play=true`), so the game streams consume identical draw sequences; the
  per-frame `draws` column (game-RNG wrapper calls only) is the attribution
  diagnostic if that ever breaks: same state + different draws = the cosmetic
  split; different state = a real divergence.

### Per-axis results

- **C++ ↔ port(LuaJIT), frame-level: 3627/3627 frames identical, all 6 corpus
  cases.** `crates/oh-parity/tests/parity_cpp_fingerprints.rs` rebuilds each
  replay in-process, byte-compares the port's `.ohr.z` compression against the
  committed hex (the interchange-format pin), replays it with the fingerprint
  on, and compares the full per-frame `(frame, hash, draws)` sequences + the
  final result (exact `Final time` string + customScore) — frame-by-frame, and
  the first divergent frame + field is reported on any mismatch. A
  deliberately-corrupted-hash test proves the comparison discriminates.
- **The C++ column confirms the pre-existing Rust pins**: the four
  alwaysSpinRight corpus cases (the seeded-replay runs of
  `parity/level_runs.json`) come out 439 frames / `1.725`, 1180 / `4.8125`,
  456 / `1.79583`, 358 / `1.3875` — exactly the committed `level_runs`
  expectations. The C++ engine and the port's prior LuaJIT-derived pins agree
  on those runs.
- **port(LuaJIT) ↔ port(Luau)**: 030a's axis, untouched by this lane.
  **C++ ↔ port(Luau)** follows by composition of the two frame-level axes (this
  lane's harness drives the LuaJIT backend; the Luau backend is verified by
  030a against the same LuaJIT column that this lane verifies against the
  C++).

### First-divergent-frame analysis (the only divergence found)

During bring-up the port's frame-0 hash mismatched the C++ (C++
`2269a86dc3169e8d`, port `63d5106dc3169e8d`) while every dumped field value and
the raw byte string were identical — the diagnosis path worked as designed.
Root cause: a transcription bug in the PORT's FNV prime (`0x1000000001b3`, one
extra zero — 2⁴⁰+0x1b3 instead of 2³⁶+0x1b3); the C++ side was correct. Fixed
in `crates/oh-game/src/update.rs` (now commented with the decimal-free
canonical form), re-verified: all 3627 frames match. Had it been a state
difference, the same env-gated per-field/raw-byte dumps (added temporarily,
removed before the final commit) would have isolated the field; they are
documented in the definition note's diagnosis section.

### The committed vectors

`.agents/plans/001-rust-port/parity/cpp_fingerprints.json` (121 KB): for each
of the 6 cases — the replay's exact `.ohr.z` bytes (hex), the input bytes (hex),
the C++ headless `Final time` (exact printed string) + customScore + exit code,
the frame count, and the full per-frame `<frame> <hash> <draws>` sequence.
Cases: `apeirogon_spin439`, `autotest0_spin1180`, `arcadia_spin456`,
`bipolarity_spin358` (the seeded-replay pins), `apeirogon_spin439_dm25`
(difficulty 2.5 — the level-script `> 1.5` onInit branch + DM speed/delay
scaling), `autotest0_mixed` (mixed right/swap/focus/no-input stream, distinct
seed, DM 1.5). Regeneration: `gen_corpus` test → container runs → re-assemble
the JSON (documented in the file's `evidence`).

### What could NOT be established (stated plainly)

- **Render-only / window-only state**: SFML textures/sprites, cameras, key
  icons, `sf::Text` contents, style data (recomputed each frame as a pure
  function of hashed fields), audio, Steam/Discord — excluded by design (the
  definition note lists them); a headless run exercises none of them on either
  side.
- **Timeline wall-clock internals** (`HRTimePoint` state in
  `timeline2_runner`/`Ticker`): real time points, not comparable across
  runs/machines; their gameplay consequences (spawned walls, created custom
  walls, incremented status) ARE covered by the hashed fields.
- **Live (non-replay) execution**: menus, live profile saves, and the
  non-deterministic `random_input_rng` live-input stream (process-static
  `mt19937` in C++, entropy-seeded per-engine in the port — non-deterministic
  BY DESIGN on both sides). The oracle covers replay execution, which is the
  parity axis.
- **Windowed runs**: the cosmetic-RNG gating argument holds for headless
  (`window == nullptr`); a windowed C++ run would show cosmetic draws in the
  `draws` column — attributable, not a divergence, but the committed vectors
  do not cover windowed execution.
- **The Luau backend against the C++ directly** (see per-axis results above —
  composition only, via 030a).

### Commits

- `9ee3ada2` — the fingerprint definition (v1) + the C++-probe-pinned canonical
  RNG-state label + build-recipe walls #5/#6/#7 (uninitialized extlibs
  submodules, the escaped job count, the GCC-13+ `<cstdint>` pin) + the
  oh-rng state-position label/draw counter.
- `fba189c9` — the C++ column: the instrumented reference (7 files) + the
  captured patch + the port's twin + `gen_corpus` +
  `parity_cpp_fingerprints.rs` + the committed vectors.
