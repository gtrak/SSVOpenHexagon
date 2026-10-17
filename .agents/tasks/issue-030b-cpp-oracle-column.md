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

## 5. The triangle, and its asymmetry stated honestly

- **C++ ↔ port(LuaJIT)** — at whatever granularity the C++ actually reports
  (realistically **score/frametime level**; a per-frame trace would require
  instrumenting the C++, which is out of scope — say so if that is the
  limitation).
- **port(LuaJIT) ↔ port(Luau)** — **frame level**, from 030a's fingerprint.
- Together: if LuaJIT matches the C++ at score level and Luau matches LuaJIT
  frame-by-frame, Luau matches the C++ for gameplay. **Be precise that this is
  a transitive inference with a coarse first link**, not a direct proof.

Report: the build outcome, the exact C++ invocation, the committed vectors, the
triangle table, and what could not be established.

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
