# K3 — babysteps LuaJIT trace-compile hang: A/B experiment suite

Task: `.agents/tasks/issue-029k1-luajit-version-and-anr.md` (K3, issue 28).

The tutorial pack's `babysteps` level (the menu's default first level)
ANRs on the phone with `android_main` pegged ~100% user CPU in the
LuaJIT **arm64 trace assembler** (`lj_asm_trace` / `__clear_cache`,
diagnosed in the 029a1 device session). This directory holds the
host-side experiment machinery that isolates the hang.

## Results (this lane)

| arch (host) | LuaJIT | JIT | babysteps full-fidelity run |
|---|---|---|---|
| x86_64 (native) | vendored @ `a4f56a4` (2025) | on | **no hang** — tutorial completes |
| aarch64 (qemu-user) | vendored @ `a4f56a4` (2025) | on | **no hang** — tutorial completes |
| aarch64 (qemu-user) | cpp pin @ `4dc5ffbf` (2023), FFI off | on | **no hang** — tutorial completes |
| aarch64 (qemu-user) | vendored | off (interpreter) | no hang (expected) |

**The phone hang does not reproduce on any host-side platform, with
either LuaJIT variant, running the complete level** (real dependency
scripts, full state machine steps 0→6, `onStep` pattern spawning,
`e_kill` exit). The decisive A/B (C++ pin vs `jit.off()`) therefore
requires the device; see the task Report for what was and was not
established.

## Components

* `driver.c` — links against a LuaJIT static library and runs the
  **full-fidelity babysteps**:
  * loads the real
    `_RELEASE/Packs/tutorial/Scripts/Levels/babysteps.lua` **and** its
    real dependency scripts (`Packs/base/Scripts/{utils,common,
    commonpatterns}.lua`, exactly as `u_execDependencyScript` /
    `u_execScript` resolve them in the game — `--pack-root`, default
    `.`; the repo checkout uses `--pack-root _RELEASE`);
  * stubs the game API (the ~40 `l_*`/`u_*`/`e_*`/`a_*`/`w_*`/`t_*`
    functions the scripts reference; `l_getLevelTime`,
    `l_getNumTimelineEntries`, `l_getNextTimelineStep`, `l_resetTime`,
    `t_wait`, `w_wall` are C functions backed by a minimal timeline
    model so `onStep` cadence matches the game);
  * calls `onUpdate` from C exactly like the game's per-frame
    `call_on_update` (`lua_pcall` per frame, level time advancing
    1/360 s/frame), plus the scripted tutorial `onInput` events that
    drive the state machine through every step (both rotations while
    step1, focus+rotations while step25) to the `e_kill` at
    levelTime > 45 after the mid-level `l_resetTime`;
  * prints iteration/levelTime/compiled-trace count per window
    (`jit.util.traceinfo` scan — `jit.util.event` no longer exists in
    the 2025 tree); a SIGALRM watchdog aborts with exit 3 if progress
    stops. `--dump N` dumps the tutorial step globals at iteration N.
* `build.sh <arch> <variant> [qemu]` — clones the chosen LuaJIT tree at
  the exact pin, builds `libluajit.a` the same way `luajit-src` does
  (Makefile, `BUILDMODE=static`, NDK clang for aarch64), links the
  driver.
  * `vendored` — upstream LuaJIT @ `a4f56a4` (2025-01-13, "v2.1 +
    master"), the exact tree `luajit-src 210.5.12+a4f56a4` bundles —
    i.e. **what the port's `mlua` `vendored` feature compiles today**
    (FFI compiled in).
  * `cpp` — `vittorioromeo/LuaJIT` @ `4dc5ffbf` (2023-04-02, the
    `CMakeLists.txt` CPM pin) — verified to be plain upstream v2.1 with
    CMake support and **no original commits** (see the task Report),
    built with `-DLUAJIT_DISABLE_FFI=1` exactly like the CMake pin.
  * `qemu` — separate source copy with `lj_mcode_sync` no-op'ed (the
    `dc cvau`/`ic ivau` cache-maintenance instructions are not
    emulated by qemu-user; under QEMU the generated JIT code is
    translated on the fly, so the flush is a no-op there by
    construction). **Never run a qemu-mode binary on a real device.**
  * aarch64 driver linking adds `-fno-emulated-tls
    -ftls-model=local-exec`: NDK 27's Bionic aborts static aarch64
    executables whose executable TLS segment is not 64-aligned with
    zero skew (its own 8-aligned `__libc_tls_generation_copy` would
    otherwise make it 8). The 64-aligned, referenced
    `k3_tls_align_probe` TLS variable in `driver.c` forces a
    64-aligned zero-skew PT_TLS (verified with
    `llvm-readelf -l | grep TLS`). `tls64.py` remains as a
    diagnostic/post-patch fallback that rewrites the alignment
    metadata.
* `tls64.py` — raises the executable's TLS segment alignment metadata
  8 → 64 in a static aarch64 binary (fallback; see above for the
  primary fix).

## Run

```sh
# Desktop (x86_64) — answers "does the same Lua source hang on the host?"
./build.sh x86_64 vendored
./build/x86_64-vendored/driver \
  ../../_RELEASE/Packs/tutorial/Scripts/Levels/babysteps.lua \
  --pack-root ../../_RELEASE

# Phone architecture (aarch64) under qemu-user — answers "is the hang in
# the arm64 assembler, and does the C++'s pinned LuaJIT avoid it?"
./build.sh aarch64 vendored qemu
./build.sh aarch64 cpp qemu
QEMU_AARCH64=/path/to/qemu-aarch64 \
  ./build/aarch64-vendored-qemu/driver ../../_RELEASE/.../babysteps.lua \
  --pack-root ../../_RELEASE
QEMU_AARCH64=/path/to/qemu-aarch64 \
  ./build/aarch64-cpp-qemu/driver ../../_RELEASE/.../babysteps.lua \
  --pack-root ../../_RELEASE
# The cheap "jit.off()" experiment (any build):
... babysteps.lua --jit-off
```

Exit codes: 0 = completed (level killed or N reached); 3 = watchdog
fired (**HANG**); 4 = Lua runtime error.

Build outputs land in `./build/` (git-ignored); override with
`K3_BUILD=...`.

## q&a

**Why qemu-user at all?** The phone's ADB-over-TCP listener was down for
the whole lane (see the task Report), so the on-device A/B was
unavailable; qemu-user aarch64 is the only host-side way to execute the
real arm64 trace assembler.

**Is the qemu result a proof about the phone?** It is strong evidence,
not the on-device observation: QEMU emulates the cores generically,
does not reproduce the phone's address-space layout (the 029a1 session
noted the hang might be ASLR/trace-layout-dependent), and runs with the
cache flush no-op'ed (qemu-mode builds). A hang that reproduces under
QEMU in the same assembler frames is decisive for "bug is in this
LuaJIT revision's arm64 path"; a non-reproduction does NOT clear the
phone-specific hypotheses (CPU erratum, ASLR/trace layout, full game
process context with the K2 free-run feeding `onUpdate` at 360–500 fps
and the real `__clear_cache` shim).
