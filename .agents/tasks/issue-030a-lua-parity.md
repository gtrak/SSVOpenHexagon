# Task 030a — the LuaJIT↔Luau parity spike (the go/no-go for unifying on Luau)

Read `.agents/WORKER-HYGIENE.md` first. Context: `.agents/plans/001-rust-port/30-lua-runtime.md`
(issue 30) is the decision surface; the wasm lane (D-036, `origin/wasm`) settled
the *portability* half by building it — LuaJIT cannot target emscripten, Luau can
— and chose **per-target** (LuaJIT desktop, Luau emscripten). This lane measures
the **parity half**, so the operator can decide whether to unify on Luau
everywhere (which would delete a lot of machinery, see §5).

**This lane measures and recommends. Do NOT migrate anything.** No deleting
LuaJIT, no switching defaults.

---

## 1. Make the backend selectable on the host

Today the backend is selected by *target* (the wasm branch's approach). For a
parity measurement both backends must be buildable on the **host**, one per build
(mlua permits only one backend per build).

Add mutually-exclusive `oh-lua` features — e.g. `backend-luajit` (default) and
`backend-luau` — forwarding to mlua's `luajit` / `luau` features, and make them
compose with the wasm branch's per-target rule (features first, target-based
default after). The project already runs the suite in two feature modes, so this
fits existing practice. **Document the exact commands** to run the whole suite
under each backend (that is the deliverable's most reusable part).

If plumbing this turns out to require rewriting large parts of `oh-lua`, STOP and
report — that itself is a finding about the cost of unification.

## 2. Measure the parity surface

Run the **same tests** under both backends and diff the outcomes:

- **the full `oh-lua` suite** — the 285-fn game API, the runtime tests, and the
  stdlib-surface pins (the latter were written for LuaJIT; expect them to need
  per-backend arms, and **record every difference rather than papering over it**);
- **the whole workspace gate** under each backend (oh-game's determinism tests
  run Lua per frame — they are a behavioral diff for free);
- **a differential-simulation harness (new — THE important one, and the reason
  this lane is worth running)**: the same harness built twice (LuaJIT / Luau),
  driving the engine headless as fast as it will go (no window ⇒ no vsync, no
  `WaitUntil` cap — headless is already flat-out; a 60 s playthrough at 200 fps is
  ~12k frames, sub-second). For each case — `(level, seed, input program, dt
  schedule)` over all 42 scripts under `_RELEASE/Packs/**/*.lua` — emit a
  **per-frame fingerprint** of gameplay-visible state (game-RNG state and draw
  count, the wall set with positions/rotations, player pos/rot, score and
  `customScore`, level-status timers, entity counts) and report the **first
  divergent frame index**, not merely a final hash: first-divergence points at the
  construct that differs, which is the difference between a verdict and a mystery.
  Reuse the existing headless harness; generate the input programs as **synthetic
  replays** so the harness rides the project's most oracle-pinned path (issue 02)
  rather than inventing a new driver.
- **a multi-process repeat (do not skip this)**: run each backend N times in
  **separate processes** and compare fingerprints. If a VM randomizes
  string/table hashing per process (LuaJIT has build-time hash options; Luau's
  behaviour must be *measured*, not assumed), then `pairs` order varies run-to-run
  *within* that VM — so any fingerprint that changes across runs means
  **nondeterministic gameplay**, a red flag independent of the Luau question.
  Nearly free, and it settles the biggest risk empirically. Report the exact
  fingerprint definition alongside the results, so the claim is auditable.
- **(optional, if cheap) a third column**: the C++ binary running the same replay
  headless would give a transitive proof — port(LuaJIT) ↔ C++ and port(Luau) ↔
  port(LuaJIT), therefore Luau ≈ C++ for gameplay. If building the C++ is more than
  a bounded effort, skip it and say so; the A/B is what this lane must deliver.

**Probes to write explicitly** (each one a documented difference or a pass):

| probe | why |
|---|---|
| `tostring(n)` over a value set incl. `1e20`, `0.5`, `1234567.89`, `0.0001`, `9999999999999.999` | the skill documents **two** number spellings; LuaJIT vs Luau may add a third |
| `string.format('%.14g', n)` | the C API's `lua_number2str` path |
| the port's `read_string` / `lua_number2str` | what the game actually reads |
| `error('x')` message shape | LuaJIT appends `"\nstack traceback:…"` to the error object; the C++ surfaces script errors, so this can be user-visible |
| `doesVariableExist` / the dotted-path semantics | `_getGlobal`-era behaviours the port mirrors |
| sandbox post-state: `require('ffi')`, `io.*`, `os.*`, `debug.*`, `package.loaded`/`preload`/`loaders`, `jit.*`, `bit.*`/`bit32.*` | Luau has no `io`/`os`/`debug`/`ffi` **at all**, so the C++'s `destroyMaliciousFunctions` steps must produce the *same observable* post-state, not merely "also safe" |
| `pairs`/`next` iteration order over the same table | **the one gameplay-affecting risk**: a level that creates walls while iterating a table would differ. The corpus smoke is what actually detects it; this probe isolates the mechanism |
| integer semantics | the wasm branch already had to fix `mlua::Integer` width for Luau — characterise it (which values round-trip, `math.floor`/`%` results, `tostring` of an integral float) |

## 3. Performance

A per-frame `onUpdate` benchmark for one or two representative levels, under each
backend, reported in µs/frame with the frame budget context (the game runs
200–360 fps, so ~2.8–5 ms/frame total). LuaJIT runs its **JIT**; Luau is an
**interpreter** unless `luau-jit` (x86_64) is wired. Report both if cheap to get,
and note that the phone (arm64, weaker CPU) is the real constraint — a host
number alone is not a verdict.

## 4. Luau must also build for Android

The whole point is one VM on all three targets. Verify Luau builds for
`aarch64-linux-android` (NDK toolchain via `tools/android-env.sh`; Luau is C++ so
it needs the C++ toolchain). A web-only Luau does not justify unification. If you
can, also confirm the Android build still compiles with that backend end-to-end
(`cargo check -p oh-app --target aarch64-linux-android`).

## 5. The recommendation, with its consequences stated

Produce a table: **probe → LuaJIT result → Luau result → verdict**
(`identical` / `shimmable` / `gameplay-affecting` / `user-visible`), plus the perf
numbers and the Android-build result. Then recommend.

State explicitly what unification would let us **delete** if the verdict is "go":
`extlibs/luajit-src` + the `[patch.crates-io]` (029k1), `ffi_stub.c`, the vendored
FFI-off build, `crates/oh-android/cshim.c` (the `__clear_cache` shim), the
per-target backend split in `oh-lua`, and — most importantly — **K3** (the arm64
`lj_asm_trace` ANR) and **K4** (the LuaJIT version skew) become moot by
construction. Also state what it would cost.

## 6. Report

Write the report to `.agents/plans/001-rust-port/30a-lua-parity.md` (in-repo,
versioned) and append a short summary to this task file: the commands to run each
backend, the diff table, the perf numbers, the Android result, the recommendation,
and **explicitly what you could not establish** (e.g. no device confirmation).

## 7. It must be a TEST MODULE, not a one-shot experiment

The operator's requirement: this is a **permanent gate**, not an experiment that
decays. So:

- **Live in the repo as a test target.** `oh-parity` is the natural home — it
already owns cross-implementation verification and the committed oracle vectors.
  e.g. `crates/oh-parity/tests/lua_engine_parity.rs`.
- **Two feature modes, artifact-based.** Each backend run emits a per-case
  fingerprint artifact under `target/` (large + machine-specific ⇒ **never
  committed**) together with the **fingerprint definition** that produced it, plus
a comparator run that reads both artifacts, fails on divergence, and prints the
case id + the **first divergent frame**. Reproducible: the case generator is
seeded.
- **Wired into the gate.** `tools/lua-parity.sh` runs both modes + the comparator,
  and the gate gains a stage for it (the project already runs the suite in two
  feature modes, so this is existing practice, not a new mechanism).
- **The C++ column (task `030b`) commits its results as oracle vectors** under the
  project's existing parity-vector convention, so the most valuable check keeps
  running in CI **without** the C++ present. That is what makes it durable rather
than "we ran it once on Gary's machine".

## Hygiene
- **No worktree** (D-035); work in the main checkout; one commit per logical step;
  **stage explicit paths, never `git add -A`**; `cargo fmt --all` before
  committing; gate before each commit (expect the current baseline + yours).
- Docker **does** work here — but this lane does not need emscripten. (The web
  integration will use the `emscripten/emsdk` container, later.)
- Do not attempt global-Luau migration in this lane, and do not rewrite
  `.agents/audits/*.md`.
- If a probe cannot be made to work for one backend, report that as a **finding**
  (an unmeasurable surface is itself a cost of unification) rather than dropping
  it silently.

---

## Summary (lane 030a — done; full report: `.agents/plans/001-rust-port/30a-lua-parity.md`)

**Measured, nothing migrated.** `oh-lua` now has mutually-exclusive
`backend-luajit` (default, unchanged behavior) / `backend-luau` features,
forwarded through oh-app/oh-game/oh-parity (features first, wasm
target-default after).

**Commands** (per the lane's env; debug profile):

- LuaJIT: `cargo test -p oh-lua` / `-p oh-game` / `-p oh-parity`;
  `cargo build -p oh-app`; `source tools/android-env.sh && cargo check
  -p oh-app --target aarch64-linux-android`
- Luau: same commands with `--no-default-features --features
  backend-luau` appended (one flag set per selected crate)
- Both (gate stage 5, `tools/lua-parity.sh`, ~7 min): N separate-VM-process
  runs per backend (default 2, `LUA_PARITY_REPEATS_COUNT=K`) → A/B compare
  (fails with case + first divergent frame) + per-backend repeats + probe
  diff table. Artifacts: `target/lua-parity/<backend>/run-<k>/` (never
  committed).

**Result (43-case shipped corpus, fixed dt, seeded input):**

- **A/B: 43/43 cases byte-identical across backends (run-0); and
  identical across 3 separate processes per backend** — no per-process
  gameplay nondeterminism observed.
- Probes: 6/8 differ, all at the dialect surface — number spellings of
  wide ints (`tostring(1e20)`: `1e+20` vs `100000000000000000000`;
  `tostring(9999999999999.999)` differs in the *parsed value*, not just
  the spelling), `error()`/nil-index message shapes (chunk-location
  prefix on Luau), the post-sandbox surface (Luau lacks `io`/`jit`/
  `load*`/`bit`/`module`/`dofile`; adds `bit32`/`buffer`/`utf8`/
  `vector`), `mlua::Integer` width (8B i64 exact vs 4B i32 saturating),
  and `pairs` order — **Luau stable across processes (4 obs.), LuaJIT
  varies (3 obs.)**, while the gameplay fingerprints stayed
  byte-identical throughout. `string.format('%.14g')` and the port's
  number→string path are identical on both.
- Perf (debug, 2500 frames, alwaysSpinRight): light level 6.09
  (LuaJIT) vs 6.60 (Luau) µs/frame — +8%, ~1.2% of the 200–360 fps
  frame budget; heavy `stress1` 1037.5 vs 1029.3 µs/frame (Lua-bound,
  backend-neutral). Release-mode not measured.
- **Android (REQUIRED): Luau PASSES `cargo check -p oh-app --target
  aarch64-linux-android`** (and LuaJIT still passes — NDK 27.2.12479018).
- Suites: oh-lua 36/36 on both backends (all divergences pinned with
  documented arms), oh-game + oh-parity green on both, workspace 117
  binaries 0 failed, fmt + clippy clean.

**Recommendation:** keep `backend-luajit` as the default (it is the
C++-parity, third-party-pack-compatible backend; nothing measured forces
a move); keep `backend-luau` first-class and gate-covered — unification,
if ever chosen, becomes a one-line default change with the §3-§6 evidence
re-running in the gate. What unification would delete (luajit-src +
patch, ffi_stub.c, cshim.c, the per-target split, K3 + K4 moot) is
recorded in the report §6; what it would cost, in §3/§6.

**Could not establish (explicit):** release-mode perf on both backends;
on-device (phone) performance (compile-only for Android); pairs-order
stability beyond 3–4 processes per backend; any third-party/user-pack
content (no such corpus in the repo); the full integer-operator matrix
beyond the probed set; the C++ third column (030b is blocked on system
X11/GL dev packages); an emscripten build under these features (verified
by construction, not by a wasm toolchain run).
