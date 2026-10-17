# K3 — babysteps LuaJIT trace-compile hang: A/B experiment suite

Task: `.agents/tasks/issue-029k1-luajit-version-and-anr.md` (K3, issue 28).

The tutorial pack's `babysteps` level (the menu's default first level)
ANRs on the phone with `android_main` pegged ~100% user CPU in the
LuaJIT **arm64 trace assembler** (`lj_asm_trace` / `__clear_cache`,
diagnosed in the 029a1 device session). This directory holds the
host-side experiment machinery that isolates the hang:

* `driver.c` — links against a LuaJIT static library, loads the **real**
  `_RELEASE/Packs/tutorial/Scripts/Levels/babysteps.lua` with the game
  API stubbed out, and calls `onUpdate` from C exactly like the game's
  per-frame `call_on_update` (`lua_pcall` per frame, level time
  advancing at 1/360 s/frame, no input — the 029a1 ANR happened before
  any input was needed, in the step0/step1 welcome state). Prints
  iteration/levelTime/compiled-trace count per window; a SIGALRM
  watchdog aborts with exit 3 if the progress counter stops moving.
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
* `tls64.py` — raises the executable's TLS segment alignment metadata
  8 → 64 in a static aarch64 binary. NDK 27's Bionic aborts static
  aarch64 executables whose executable TLS alignment is below 64
  (its own `__libc_tls_generation_copy` is 8-aligned, so every static
  binary gets 8). Pure ELF alignment-metadata change; no code changes.
  Needed only so the experiment binary can start; real-device builds
  (the port's APK) are dynamically linked and unaffected.

## Run

```sh
# Desktop (x86_64) — answers "does the same Lua source hang on the host?"
./build.sh x86_64 vendored
./build/x86_64-vendored/driver ../../_RELEASE/Packs/tutorial/Scripts/Levels/babysteps.lua

# Phone architecture (aarch64) under qemu-user — answers "is the hang in
# the arm64 assembler, and does the C++'s pinned LuaJIT avoid it?"
./build.sh aarch64 vendored qemu
./build.sh aarch64 cpp qemu
QEMU_AARCH64=/path/to/qemu-aarch64 \
  ./build/aarch64-vendored-qemu/driver ../../_RELEASE/.../babysteps.lua
QEMU_AARCH64=/path/to/qemu-aarch64 \
  ./build/aarch64-cpp-qemu/driver ../../_RELEASE/.../babysteps.lua
# The cheap "jit.off()" experiment (any build):
QEMU_AARCH64=/path/to/qemu-aarch64 \
  ./build/aarch64-vendored-qemu/driver ../../_RELEASE/.../babysteps.lua --jit-off
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
not the on-device observation: QEMU emulates the Cortex-A55/A75 (big)
+ A55 (little) cores generically, does not reproduce the phone's
address-space layout (the 029a1 session noted the hang might be
ASLR/trace-layout-dependent), and runs with the cache flush no-op'ed
(qemu-mode builds). A hang that reproduces under QEMU in the same
assembler frames is decisive for "bug is in this LuaJIT revision's
arm64 path"; a non-reproduction does NOT clear the phone-specific
hypothesis.
