# Issue 33 — the arm64 LuaJIT hang (K3): run the phone with the JIT off, and delete the class with Luau

> **CLOSED BY CONSTRUCTION — D-037 landed (lane 030d, 2026-10-17).** The
> revert condition below was met: the Luau unification landed, and this
> whole class disappeared with the machinery it named — the `jit.off()`
> call (deviation **F29**, removed from `deviations.md`), the
> `__clear_cache` shim (`crates/oh-android/cshim.c`, deleted), the vendored
> LuaJIT (`extlibs/luajit-src` + the root `[patch.crates-io]`, deleted),
> and the per-target Lua split (deleted; the port now ships ONE VM, Luau —
> mlua's `luau` feature, statically built from `luau0-src`). K3 and K4 are
> recorded as closed by construction in `deviations.md` (citing D-037),
> and the C++ parity guarantee is re-proven per gate against the committed
> C++ vectors (030c: 3627/3627 frames under exactly this configuration).
> Everything below is the HISTORICAL record of the confirmed hang, the
> measured mitigation, and the reasoning that led to the unification —
> kept verbatim, not rewritten.

Read `.agents/WORKER-HYGIENE.md` first. This is **landed** (`249d268d`); the file exists
because the code and `deviations.md` F29 reference it, and because the *revert* condition
matters. Evidence and the full device session: `.agents/tasks/issue-029k1-luajit-version-and-anr.md`
(its "Device session 2026-10-17" section) and `deviations.md` K3/F29.

## 1. What was wrong (confirmed by stack trace, not inferred)

The operator's report was *"I can't get through the android tutorial b/c it wants me to pass
a 'shift' key input and **it hangs non-responsive**"*. Two separate faults: the missing
focus input (issue 31, landed) and this hang.

The installed build carried TEN `data_app_anr` dropbox entries plus one crash. Two sampled
entries share:

```
Subject: Input dispatching timed out (info.vittorioromeo.openhexagon/
  android.app.NativeActivity (server) is not responding. Waited 5001ms for MotionEvent)
```

with the **Android UI thread idle** (`__epoll_pwait` → `Looper::pollInner` →
`MessageQueue.nativePollOnce`) and the **`android_main` (game) thread in `state=R`** at
~29.5 s user CPU stacked:

```
#00 __clear_cache   (the arm64 icache shim, crates/oh-android/cshim.c)
#01 lj_asm_trace    <- LuaJIT's JIT assembler
#02 trace_start   #03 lj_vm_cpcall   #04 lj_trace_*   #05 lj_dispatch
#06 lj_vm_ins*    #07 lua_pcall      #08..#29 the port's own Rust frames
```

So the game thread is stuck **assembling a LuaJIT trace**. Not input plumbing; not the
tutorial failing to advance; not the port's own code.

## 2. The mitigation (landed) and its measurement

`LuaContext::new` now calls the named, tested `disable_jit()` under
`cfg(all(target_os = "android", feature = "backend-luajit"))`:
- **LuaJIT backend only** — the Luau backend has no `jit` table at all (the same test
  asserts that divergence), and it is **not** applied on desktop, so host/C++ parity is
  untouched (`stdlib_surface` 7/7 green).
- The interpreter has no trace assembler, so the hang cannot occur in it.

**Measured on the device** (the APK rebuilt from that tree, `adb install -r`): **100+
seconds of live, ANIMATED gameplay with ZERO new ANRs** — the dropbox count stayed at 10
while two screenshots 2.5 s apart differed (a real frame loop, ~78% of one core). The
JIT-on build has ten ANR records from comparable sessions. Not a laboratory A/B (the levels
differ), but unambiguous in direction.

## 3. Why this is a DEVIATION (F29), and when it goes away

The C++ never disables the JIT, and the JIT is the whole reason the project uses LuaJIT. The
justification is K4's version skew: this build runs a different LuaJIT than the C++
(`luajit-src`'s 2025 tree vs the C++'s `vittorioromeo/LuaJIT @ 4dc5ffbf`), and *this* one's
arm64 trace assembler is unusable here. So F29 is a forced, platform-scoped deviation.

**Revert condition:** the operator's direction (issue 30 §12) is to unify on Luau, and if
that lands this whole class disappears — the `jit.off()` call, the `__clear_cache` shim, the
vendored LuaJIT, the root `[patch.crates-io]`, and the per-target Lua split all go together,
and K3 and K4 close **by construction**. Until then this stays, because an interpreter is
playable and an ANR is not.

## 4. What is still open here

- **Multi-touch acceptance of issue 31's overlay** on the device (hold Focus + rotate until
  the tutorial's `step1_trFocus` reads `"yes!"`): needs the tutorial level selected and a
  two-pointer injection. ADB's `input` drives ONE pointer; `sendevent` with the `ABS_MT_*`
  protocol would be required. The overlay itself is verified DRAWN on the device (static
  patches over an animating scene) and its inputs are covered by host tests.
- **The device-evidence tooling**: the K3 A/B install replaced a debuggable build with the
  release APK, which is NOT debuggable, so `run-as` no longer works. Build the dev profile
  (`cargo apk build`) or add a temporary `debuggable = true` (reverted in its own commit,
  WORKER-HYGIENE §6) before doing any further file-level device evidence.
