# Task 029k1 (K3) — the port does not run the C++'s LuaJIT, and the default first level ANRs

> **K3 is CLOSED BY CONSTRUCTION — D-037 (2026-10-17, lane 030d, the Luau
> unification).** The premise this task reconciled — "the port's Lua VM is
> not the C++'s" — is resolved by the port no longer running a LuaJIT at
> all: D-037 deleted the vendored LuaJIT (`extlibs/luajit-src` + the root
> `[patch.crates-io]`), the `ffi_stub.c` link shim, the Android `cshim.c`
> `__clear_cache` shim (K3's hang class), and the per-target backend
> selection, and unified on Luau (mlua's `luau` feature, statically built
> from `luau0-src`). The K3 `jit.off()` mitigation (deviation **F29**) went
> with the VM it mitigated (the F29 row is removed from `deviations.md`);
> K3/K4 are recorded there as closed by construction, citing D-037. The
> C++ parity guarantee survives (the oracle fingerprints game STATE, not
> Lua internals; 030c: 3627/3627 frames under the shipping configuration).
> Everything below is the HISTORICAL record of the diagnosis (kept
> verbatim, not rewritten).

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

## Report (029k1, 2026-10-17)

### What was established

**1. The "fork" is not a fork (fact 1 resolved).** `vittorioromeo/LuaJIT @
4dc5ffbf` (the C++ CPM pin, 2023-04-02) contains **zero original commits**:
every commit in its history — including every ARM64-fix commit and Wohlstand's
CMake support — exists in upstream `LuaJIT/LuaJIT`. It is plain upstream v2.1
+ CMake. The port's vendored `luajit-src 210.5.12+a4f56a4` bundles upstream
LuaJIT @ `a4f56a4` (2025-01-13, "v2.1 + master") — **~21 months NEWER** than
the C++ pin. "Pin to the fork" therefore means pin to OLDER upstream. Between
the two pins upstream refactored the arm64 trace assembler heavily (2023-09:
IR_HREF/UREF/OBAR codegen rewrites; `43eff4aa` mcode-limit check fix;
`0705ef6c` TBZ/TBNZ range checks; `91592899` HREF FP-key fix; ADRP/ADR
constant handling) — those are the concrete regression candidates for an
arm64-only trace-compile pathology. (Diffs: scratch clones at
`/home/gary/dev/scratch` — `luajit-fork` @ 4dc5ffbf, `upstream-luajit`,
`lj_asm_arm64.diff`, `fork_arm64_vs_beta3.diff`.)

**2. The hang does NOT reproduce host-side, with EITHER LuaJIT (step 1 + 2
resolved as far as the host can go).** `tools/k3-luajit-hang/` (committed):
`driver.c` runs the **full-fidelity** `babysteps` — the real level script plus
its real dependency scripts (`Packs/base/Scripts/{utils,common,
commonpatterns}.lua` via C `u_execDependencyScript`/`u_execScript`), a minimal
timeline model (`t_wait`/`w_wall`/`l_getNumTimelineEntries`/… in C), the
scripted tutorial `onInput` events driving the complete state machine (steps
0→6, incl. the mid-level `l_resetTime` and `onStep` pattern spawning), and
`onUpdate` called from C per frame exactly like `call_on_update`. `build.sh`
builds either LuaJIT tree at its exact pin (Makefile, `BUILDMODE=static`, the
C++ pin additionally `-DLUAJIT_DISABLE_FFI=1`; `qemu` mode no-ops
`lj_mcode_sync`) and links the driver (aarch64 needs `-fno-emulated-tls` + a
referenced 64-aligned TLS probe — NDK 27 bionic's executable-TLS alignment
check; `tls64.py` remains as fallback). Results (full-fidelity run, tutorial
to `e_kill`):

| arch | LuaJIT | JIT | result |
|---|---|---|---|
| x86_64 native | vendored `a4f56a4` (2025) | on | no hang |
| aarch64 qemu-user | vendored `a4f56a4` (2025) | on | no hang |
| aarch64 qemu-user | C++ pin `4dc5ffbf` (2023, FFI off) | on | no hang |
| aarch64 qemu-user | vendored | off | no hang (expected) |

So the decisive question (C++-pin vs `jit.off()`) **cannot be answered
host-side**: the phone hang has no host-side signature in either VM. Residual
candidates are phone-specific: CPU microarchitecture/erratum (QEMU models the
ISA, not microarchitectural behavior), ASLR/trace layout (the 2025
`asm_guardtnb` exit-stub distance checks are layout-dependent; the 029a1
session already noted the hang may be ASLR-dependent), or the full game process
context (K2 free-run feeding `onUpdate` at 360–500×/s vs the C++'s paced
loop, the real `__clear_cache` shim — no-op'd in qemu-mode builds, and other
Lua entry points / threads).

**3. The stdlib surface a script can reach is C++-equivalent (step 3 done,
pinned by `crates/oh-lua/tests/stdlib_surface.rs`, 6 tests, passing).**
- `require('ffi')` FAILS in the port exactly like the C++ (pre-sandbox:
  "module 'ffi' not found"; post-sandbox: `'package.loaders' must be a table` —
  the verbatim sandbox's `package.loaders` clear makes any `require` raise that
  error on both sides). Pre-029k1 the port's VM merely left FFI *compiled in
  but unopened* (mlua safe-set); now it is compiled out (see 4).
- `package.preload` empty in both; `jit.status`/`jit.off`/`jit.on` reachable
  and working in both; `package.loaded` shape documented in the test (port:
  `_G` present, `debug` absent; C++ pre-sandbox: the inverse — the sandbox's
  `destroy_malicious_functions` makes the post-sandbox shape identical).
- `jit.util.event` no longer exists in the 2025 tree (removed upstream) — the
  driver counts traces by scanning `jit.util.traceinfo(n)`.

**4. Fix applied: the port's LuaJIT is now built with the C++'s own FFI
switch.** The C++'s `LUAJIT_DISABLE_FFI=ON` (CMakeLists.txt:127, "TODO (P2):
revisit this decision about FFI") compiles in `-DLUAJIT_DISABLE_FFI` →
`lj_arch.h` `LJ_HASFFI 0` → the FFI C sources compile to empty objects and
`luaopen_ffi` is not exported. `luajit-src` exposed no hook, so the crate is
vendored locally (`extlibs/luajit-src`, verbatim + the flag in `build_unix`
`XCFLAGS` and in `msvcbuild.bat` `LJCOMPILE`) and wired through
`[patch.crates-io]` (same crate name/version). Verified: the port's
`libluajit-5.1.a` no longer exports `luaopen_ffi`; its `luaopen_*` set now
matches the C++ FFI-off build. One coupling discovered and handled: mlua
0.10.5's `luajit` feature references `luaopen_ffi` at LINK time even in safe
mode (runtime-guarded, but the relocation is emitted) — so `oh-lua` now
compiles `ffi_stub.c`, a documented, never-called link-compat stub (safe-mode
states reject `StdLib::FFI` with a `SafetyError` before any load; the stub
raises the C++-like error if ever reached). Observable behavior is unchanged
(the 6 surface tests pass against the FFI-off VM).

**5. Register + docs updated.** K3 row refreshed (version-skew facts + host
A/B matrix + residual candidates); new K4 row for the LuaJIT divergence itself
(FFI half fixed by 029k1; version half open pending the device A/B);
`crates/oh-lua/Cargo.toml`'s "matching the C++ build" claim corrected;
`.agents/skills/lib-luajit.md`'s "Two pins, one runtime" rewritten as "Two
pins, NOT the same build".

### What was NOT established (explicit)

- **The on-device A/B was never run.** The phone's ADB-over-TCP listener
  (`192.168.10.138:38721` and `:5555`) stayed closed for the entire lane
  (the 029a1 residual: re-arming `adb tcpip` needs the device screen), so
  neither "C++-pinned LuaJIT plays `babysteps` on the phone" nor "`jit.off()`
  plays `babysteps` on the phone" was observed. The hang's root cause remains
  OPEN.
- **QEMU results are evidence, not device truth.** qemu-user emulates the ARM64
  ISA generically (no SD439 microarchitecture/erratum modeling), does not
  reproduce the phone's address-space layout, and runs with the cache flush
  no-op'd (qemu-mode builds only — never run those on a real device).
  A non-reproduction under QEMU does not clear the phone-specific hypotheses.
- **No version pin decision was made.** Pinning the port to `4dc5ffbf` (the
  C++'s exact 2023 snapshot) was time-boxed per this task and NOT done: it is
  only warranted if the device A/B shows the C++-pinned VM avoids the hang;
  if the hang is not the version, pinning would be a regression (older
  assembler). The decision needs the device.
- **The full gate was run at each commit step** (see the commit trail) but the
  final full `tools/gate.sh` run for this Report is recorded in the commit
  trail of the register/report commit.

### Residual risk

- **D-037 (2026-10-17): the K3 hang is CLOSED BY CONSTRUCTION** — the
  Luau unification (lane 030d) deleted the vendored LuaJIT, the
  `__clear_cache` shim, and the per-target split; the shipping VM (Luau)
  has no trace assembler and no `jit` table, so the ANR class cannot
  exist. See `DECISIONS.md` D-037 + `deviations.md` K3/K4 (closed by
  construction) + `33-k3-jit-off.md` (the historical record).
- The K3 hang (pre-D-037, historical): unchanged (device-side, OPEN). If it persists on the phone,
  the 029a1-completion lane should (a) run the two-arm device A/B (rebuild the
  port against `4dc5ffbf` FFI-off vs `jit.off()` on `a4f56a4`), (b) capture the
  pathological trace on-device (`jit.dump` / `traceinfo` from a hookable
  build), and (c) weigh mitigations (JIT off for level scripts, default-level
  choice, the K2 fps cap).
- The FFI stub (`ffi_stub.c`) is dead code by construction; the risk is a
  future unsafe-mode state (`StdLib::FFI`) in oh-lua — impossible through the
  current `LuaContext::new` (safe-only) and guarded by the `SafetyError`.
- Vendored `extlibs/luajit-src` must stay in sync if the port ever bumps
  `luajit-src` — the `[patch]` overrides the registry version, so a version
  bump must update the local copy (README in `extlibs/luajit-src` documents
  the patch).


## Device session 2026-10-17 (orchestrator) — K3 CONFIRMED, mitigation MEASURED, and how to drive the phone

**Reaching the device.** Wireless debugging was re-enabled and the port was RANDOM again
(not 38721): `adb mdns services` found nothing (mDNS does not traverse here), so the port
was found by scanning the phone (`nc -z` over 30000-50000) -> **192.168.10.138:41611**.
The stored pairing key still worked, so no `adb pair` was needed. Device:
moto g power (2021), Android 11, arm64-v8a; display 720x1600 with the app window
**1600x720 / app=1530x678**; `input` coordinates are in the 1600x720 space. The installed
build is **DEBUGGABLE**, so `run-as info.vittorioromeo.openhexagon` works (that is the key
for every file/evidence pull). A stale `emulator-5554 offline` entry exists; ignore it.

**K3 was CONFIRMED from device traces** (recorded in `deviations.md` `K3`): the installed
build has TEN `data_app_anr` entries plus one crash, and two sampled entries share
`Subject: Input dispatching timed out (... Waited 5001ms for MotionEvent)` with the Android
UI thread IDLE in `__epoll_pwait` and the `android_main` game thread in `state=R` at ~29.5s
user CPU stacked `__clear_cache -> lj_asm_trace -> trace_start -> lj_vm_cpcall ->
lj_trace_* -> lj_dispatch -> lj_vm_ins* -> lua_pcall` -> the port's Rust frames. So the app
is stuck ASSEMBLING A LUAJIT TRACE; it is not input plumbing and not the tutorial failing
to advance.

**The mitigation (F29) was then MEASURED on the device.** `LuaContext::new` now applies
`jit.off()` under `cfg(all(target_os = "android", feature = "backend-luajit"))`; the APK was
rebuilt from that tree and installed with `adb install -r` (app data preserved):
**100+ seconds of live, ANIMATED gameplay with ZERO new ANRs** (the dropbox count stayed at
10, and two screenshots 2.5 s apart differ — a real game frame loop at ~78% of one core).
The JIT-on build has ten ANR records from comparable sessions. Not a laboratory A/B (levels
differ), but the direction is unambiguous.

**031's overlay was ALSO verified drawn on the device**, without a second image read: the
overlay's buttons are STATIC content composited over an ANIMATING scene, so a patch that
does not change between two frames while its neighbourhood does is overlay pixels. At the
geometry from `touch_overlay::button_centers` (BUTTON_HALF=32, PAD=12, MIDDLE_GAP=80) the
bottom-centre Focus, bottom-centre-right Swap and bottom-right RotateCW patches came back
static, while the top-left Exit / bottom-left RotateCCW sampled black because the app's
1530x678 viewport is INSET within the 1600x720 display. (Those two are drawn; the sample
just straddled the black bar.)

**How to actually drive this app (hard-won, reuse it):**
- The boot screen's text field takes focus on a tap and **opens the IME**, which then eats
  every later tap. The screen then reads mean-brightness ~45 (that is GBOARD, not the menu).
  **Send `input keyevent 66` (ENTER) to accept the name and get out of text entry.**
- The touch grid is `MenuGame.cpp:2801-2834`, verbatim: only when
  `getFingerDownCount() == 1`; the MIDDLE BAND (hThird < y < hLT) maps left third ->
  `leftAction()`, right third -> `rightAction()`, **middle third -> `okAction()`**; the TOP
  band -> `upAction()`, the BOTTOM band -> `downAction()`.
- A short `input tap` is unreliable (the game samples finger-down per frame); use
  `input swipe X Y X Y 400` as a HELD press.
- The port's own log lines do NOT reach logcat, so navigate with DIFFERENTIAL SCREENSHOTS:
  capture, press, re-capture, and compare a 16x16 mean-brightness fingerprint
  (`screencap -p /sdcard/x.png` + `pull`; the `exec-out screencap -p` path failed here).
- `run-as <pkg> find files -type f` + `cat files/Profiles/*.json` is the text-evidence path.
  Current state: `files/Replays/.../0_20261017_022919_no_profile_..._1.85417s.ohr.z` is 98
  BYTES at the C++-shaped path (029a1 VERIFIED), and `files/Profiles/` is EMPTY with
  `users.json`/`scores.json` at 5 bytes each — consistent with the operator having played as
  `no_profile`, so **029c1's profile persistence is still UNVERIFIED** (it needs a profile
  actually created on the device).

**Still open on the device:** the tutorial's own acceptance (hold Focus + rotate until
`step1_trFocus` reads "yes!"), which needs the tutorial pack/level selected and MULTI-TOUCH
(ADB's `input` drives one pointer; `sendevent` with the ABS_MT_* protocol would be needed),
and the two PENDING visual items (029e's textures).

**OPERATIONAL NOTE (a self-inflicted issue, recorded so nobody re-derives it):** the K3
A/B install replaced a DEBUGGABLE build with `tools/package-android.sh`'s RELEASE APK
(`cargo apk build --release`), and that APK is NOT debuggable — `dumpsys package` now shows
`flags=[ALLOW_CLEAR_USER_DATA ALLOW_BACKUP]` (no DEBUGGABLE) and `run-as` fails with
"package not debuggable". All `run-as`-based evidence collection (the profile/replay pulls,
029c1's persistence check) therefore STOPPED WORKING after that install. To get it back:
build with the DEV profile (`cargo apk build`, no `--release`) or add a temporary
`debuggable = true` to the android metadata — per WORKER-HYGIENE §6, the latter must be
reverted in its own commit and said so. What survives: 029a1 is already verified (the
98-byte replay was pulled BEFORE the reinstall), and the app's data-dir wiring is still
evidenced without run-as (config.json 8225 B, users.json/scores.json written at first run),
so 029c1's ProfileStore remains plausible-but-unverified on device rather than suspicious.
