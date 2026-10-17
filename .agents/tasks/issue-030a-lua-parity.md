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
- **a corpus smoke (new, the important one)**: load all 42 scripts under
  `_RELEASE/Packs/**/*.lua`, run `onLoad` + N `onUpdate` frames with a
  deterministic input script, and dump an **observable-state fingerprint** (engine
  RNG draws, wall/entity counts and positions, level-status fields, spawn
  timings) per backend, then diff. Reuse the existing headless harness rather than
  writing a new engine driver. This is the gameplay-parity evidence.

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
