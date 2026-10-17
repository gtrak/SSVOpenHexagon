# Task 029k1 (K3) — the port does not run the C++'s LuaJIT, and the default first level ANRs

TOP PRIORITY. Master is green (gate 4/4, 753 tests). **No worktree** (D-035):
main checkout, commit every 3-5 tool calls.

## The two facts this lane must reconcile

**(1) The port's Lua VM is not the C++'s.** `.agents/skills/lib-luajit.md` states
both pins, and the project has been treating their equivalence as an assumption:

| | LuaJIT used |
|---|---|
| C++ baseline | `vittorioromeo/LuaJIT @ 4dc5ffbf60b16e54274c470fcc022165685c96fe` (a **fork**; `CMakeLists.txt:123-136` CPM pin), built with **`LUAJIT_DISABLE_FFI=ON`** |
| Rust port | `mlua 0.10.5` `luajit`+`vendored` → `luajit-src` = **upstream LuaJIT 2.1.0-beta3** |

`crates/oh-lua/Cargo.toml:9` claims vendored is "matching the C++ build". It is
not: it is upstream, at a different commit, and it is *not* built with
`LUAJIT_DISABLE_FFI`. A "literal 1:1 port" whose script VM is a different
LuaJIT revision (and whose FFI availability differs at the VM level) has an
unverified gap in exactly the subsystem that drives gameplay.

**(2) The default first level ANRs on the phone.** The 029a1-completion device
session found the tutorial pack's `babysteps` level unusable, 3/3 runs: 2 ANRs,
`android_main` pinned at 100% CPU for 85+ s inside `lj_asm_trace` /
`__clear_cache` — i.e. spinning in the **JIT trace assembler**. (Read that
report: `.agents/tasks/issue-029a1-replay-store.md`, `## Report`, and the K3 row
in `deviations.md`.) The `__clear_cache` shim in `crates/oh-android/cshim.c` is
**verified correct** (`__builtin___clear_cache` → `dc cvau`/`ic ivau`/`dsb`/`isb`),
so do not chase that.

Device: `adb -s 192.168.10.138:38721` (wake with `KEYCODE_WAKEUP` first).

## Do this, in order. Stop-and-report points are explicit.

### 1. Characterise the hang (cheap, first)
- Reproduce on the device, and read the Lua side: which construct in the level's
  script is executing?  Grep `_RELEASE` for the `babysteps` level + its pack, read
  its `onUpdate`/`onLoad`, and identify any hot loop that would be trace-compiled.
- **The single most informative cheap experiment: turn the JIT off and see if it
  plays.** If `jit.off()` (or running the interpreter) makes `babysteps` playable,
  the bug is in trace compilation, which localises everything below.
- Determine whether the same Lua source hangs on **desktop/host** (x86_64) — if it
  does not, the bug is in the **arm64** trace assembler, which is what would point
  at a version-specific fix rather than at the script.

### 2. The decisive question
Is the hang fixed by the C++'s actual LuaJIT (the `vittorioromeo` fork at
`4dc5ffbf`), or by `jit.off()`?
- Build the **fork** at that exact commit for **arm64** (NDK; the repo's
  `tools/android-env.sh` has the toolchain vars) and run `babysteps` against it.
- Cheapest decisive framing: the fork is a fork of 2.1 — diff its `src/lj_asm*`
  / `lj_mcode.c` / hotloop-related files against the vendored upstream 2.1.0-beta3
  source (`luajit-src`'s extracted source is under `target/` after a build) and see
  whether it carries a trace-assembler/mcode fix that upstream beta3 lacks. **Report
  what you find — a concrete diff beats a theory.**

### 3. The stdlib-surface question (cheap, and a real deviation either way)
The C++ calls `luaL_openlibs` on a LuaJIT built with **FFI disabled at compile
time**, so `ffi` does not exist there and `require("ffi")` FAILS. The port's
LuaJIT has FFI compiled in and relies on `mlua::Lua::new()`'s "safe" set to not
*open* it (`crates/oh-lua/src/context.rs:38-45`). Test what a script can actually
reach in the port — `require("ffi")`, `jit.status`, `jit.off`, `package.loaded`,
`package.preload` — with a real test, and compare against the C++ semantics.
If `require("ffi")` succeeds in the port (compiled-in but merely unopened), that
is a genuine deviation **and** a 1:1-port concern, not just a sandbox nit.

### 4. Fix what is achievable and faithful
- **Build the port's vendored LuaJIT with `LUAJIT_DISABLE_FFI`** so the VM matches
  the C++ at the compile level (this is the C++'s own flag; find how to pass it
  through `luajit-src`/mlua — an env var, a `[patch]`, or a small build script).
  Keep the sandbox behavior identical.
- **If the fork is the fix** (i.e. it carries the fix upstream beta3 lacks), pin the
  port to that fork commit — e.g. build the fork's source and link mlua
  non-vendored, or patch/vendor `luajit-src`. **Time-box this.** If pinning is not
  achievable within a reasonable effort, do NOT thrash: apply the FFI fix, ship the
  precise diagnosis (the diff, the experiment results), file/refresh the register
  row, and report the blocker for a follow-up lane. A precise "this is what it
  needs and here is why" is an acceptable outcome for this lane; a half-built
  LuaJIT replacement is not.

### 5. Register + report
- Update the **K3** row in `.agents/plans/001-rust-port/deviations.md` (issue 28's
  rule) with what you established, and add a row for the **LuaJIT divergence**
  itself (the port's VM is upstream beta3, not the C++'s fork commit; FFI compiled
  in vs disabled) — that row stands on its own even if the hang turns out to be
  upstream.
- Also correct the false claim in `crates/oh-lua/Cargo.toml:7-10` ("matching the
  C++ build") and the equivalent wording in `.agents/skills/lib-luajit.md` if it
  overstates equivalence.
- Append a `## Report` to this task file: the reproduction, the two experiments
  (jit.off, the fork), the stdlib-surface test result, what you changed, and
  residual risk. **Be explicit about what you did NOT establish.**

## Hygiene — learned the hard way
- **TEXT-ONLY device sessions: at most ONE screenshot read for this whole task.**
  A prior lane died reading four images in a row. Prefer `logcat -d | grep`,
  `dumpsys`, `pidof`, `find`, `ls`, `/proc/<pid>/stat` sampling.
- If a device session starts spinning (no progress, repeated greps), STOP and
  report.
- Keep the gate green: `bash tools/gate.sh` (753 tests). The issue-29 audit
  workflow is finished; `.agents/audits/*.md` are committed — don't rewrite them.
- Do not touch non-phone processes. No emulator.
