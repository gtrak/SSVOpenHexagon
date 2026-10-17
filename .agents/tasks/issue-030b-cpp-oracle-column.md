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
