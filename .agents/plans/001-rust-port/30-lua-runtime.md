# Issue 30 — The Lua runtime choice (absorbs issue 26's blocker #1)

Status: **research / decision surface** (no code landed). Supersedes the
"Luau-instead-of-LuaJIT **on web only**" decision request in
`26-wasm-web.md`. The question generalized: the *same* VM decision now affects
desktop and Android, because the port is already running a LuaJIT that is not
the C++'s.

> Numbering note: the operator asked for the next issue to be 27, but
> `27-coverage-audit.md` already exists (the 260-row register audit). This doc
> is therefore **30**; 26's decision request #1 now points here.

## 1. What forced this

Three facts, discovered independently:

1. **LuaJIT cannot target wasm** (26 blocker #1 — argued in §2 below, now with
   the mechanism).
2. **The port does not run the C++'s LuaJIT.** `oh-lua/Cargo.toml` claims
   vendored is "matching the C++ build". The K3 lane established the real
   relationship: `vittorioromeo/LuaJIT @ 4dc5ffbf` is **plain upstream v2.1 with
   CMake added, no original commits** (2023-04-02), while `luajit-src
   210.5.12+a4f56a4` bundles upstream `a4f56a4` (2025-01-13). Same project,
   **21 months apart** — not a fork.
3. **The default first level ANRs on the phone** (K3) inside LuaJIT's arm64
   trace assembler — with the corrected framing of (2), that is a candidate
   *regression inside upstream LuaJIT*, which makes the pin `4dc5ffbf` a real
   fix candidate rather than a fork hunt.

(1) makes a web build impossible with the current VM. (2)+(3) mean the
"byte-exact Lua parity with the C++" property was already resting on an
unverified version equivalence. So the VM choice is no longer "web only".

## 2. Can LuaJIT run in wasm? No — and not for a fixable reason

1. **The JIT is impossible by construction.** LuaJIT writes machine code into a
   buffer and jumps into it (`PROT_EXEC` + computed jump). Wasm has no W^X and
   cannot execute written data as code — that is the sandbox, not a toolchain
   limitation.
2. **There is no portable fallback.** `jit.off()`/`-DLUAJIT_DISABLE_JIT` does not
   give you a C interpreter: LuaJIT's `lj_vm` interpreter is **hand-written
   per-arch assembly** (`vm_x86.dasc` etc. via DynASM) with no C implementation.
   DynASM has no wasm backend, and emscripten cannot lower x86/ARM inline asm.
   So LuaJIT does not *build* for wasm — the interpreter blocks it before the
   JIT does.
3. **The build system rejects unknown architectures.**

Even in the hypothetical where someone wrote a wasm DynASM backend, you would get
an interpreter-only LuaJIT that is *slower than reference Lua 5.1 compiled to
wasm*. Dead end, not merely hard.

**The useful corollary: the wasm blocker is about LuaJIT, not about Lua.** C
Lua ("reference" 5.1–5.4) and Luau are both plain C/C++ and compile to wasm
without difficulty.

## 3. The axes the decision actually turns on

1. **Language fidelity to the C++**, which is LuaJIT = Lua 5.1 language + a few
   extensions (`bit`, `jit`, `table.new`, `table.clear`, `ffi`).
2. **Portability** — wasm needs a C/C++ VM, statically linked (no `dlopen`, no
   JIT); Android today needs a JIT code-cache flush shim for LuaJIT.
3. **Performance** — the game runs 200–360 fps with per-frame script callbacks
   (`onUpdate`, `onRenderStage`), so interpreter-vs-JIT is a real measurement,
   not a footnote.

## 4. Option matrix

mlua 0.10.5's feature surface (verified in the crate source): `lua51`, `lua52`,
`lua53`, `lua54`, `luajit`, `luau`, `luau-jit`, plus `vendored`
(= `lua-src` + `luajit-src`). The VM is therefore essentially a **build flag**.

| option | 5.1 fidelity | JIT | wasm | Android | static | notes |
|---|---|---|---|---|---|---|
| LuaJIT (today) | reference (it *is* 5.1+ext) | yes | **no** | yes + `__clear_cache` shim | **no** (dlopen) | K3 hang; 21-month skew vs the C++ |
| LuaJIT pinned `4dc5ffbf` | same | yes | no | same | no | the cheap native-only fix; may fix K3 |
| **reference Lua 5.1** (`lua51`+`vendored`, `lua-src`) | **highest** | no | **yes** | yes | **yes** | keeps `setfenv`/`getfenv`/`loadstring`/`unpack`/`newproxy`/`module`/`gcinfo`; lacks `bit`/`jit`/`table.new` |
| **Luau** (`luau`, `luau0-src`) | dialect (removes 5.1-isms) | no (`luau-jit` = native-only) | **yes** | yes | **yes** | actively maintained; sandbox-first; fastest interpreter |
| Lua 5.4 (`lua54`) | **no** — integer subtype breaks 5.1 doubles | no | yes | yes | yes | disqualified |
| pure-Rust Lua | immature | – | yes | yes | yes | not a serious candidate |
| AOT (Lua→wasm/C) | only shipped content | – | partial | partial | yes | see §5(a) |

Two verified details that matter:

- **Luau via mlua is statically compiled** (`mlua-sys`: `luau = ["luau0-src"]`;
  the `libloading` dependency in mlua's `luau` feature is Luau's
  `require`-native-package path, not the VM — and the shipped corpus uses
  `require(` zero times).
- **`luau-jit` is a genuine JIT** (`ffi/luau-codegen`) — so it is native-only by
  the same wasm argument as LuaJIT, but it *is* available if we adopt Luau on
  desktop/Android and want the perf back.

## 5. "Do we need a dynamic runtime at all?"

Two readings; both matter.

**(a) A dynamic *evaluator* at all — could we AOT the Lua away?**
No, for real content. Packs are community-authored and loaded at runtime
(disk on native, fetch/VFS on web), and level logic *is* Lua. AOT can only cover
the scripts known at build time; any user pack needs an evaluator at runtime — so
you ship a VM regardless. A hybrid (AOT the shipped packs + interpret user packs)
buys speed on a minority of content at the cost of two behavior paths, and
bytecode cannot be shared across VMs anyway. **Rejected**, in line with 26's own
rejection of "web ships without user Lua levels".

**(b) A dynamically *loaded* VM — must the VM be dlopen'd?**
No, and it should not be. This is a real architectural win independent of the
dialect question:

- today's LuaJIT path is **dynamically linked** — that is precisely why Android
  needed `crates/oh-android/cshim.c` after `dlopen failed: cannot locate symbol
  "__clear_cache"`;
- reference Lua 5.1 (`lua-src`) and Luau (`luau0-src`) are both **compiled into
  the binary**;
- **wasm has no `dlopen` at all** — anything dynamically loaded is simply not
  portable to the web target.

So both non-LuaJIT candidates remove the dlopen failure class rather than
porting it.

## 6. What any non-LuaJIT VM costs (be explicit, not optimistic)

The shipped corpus is *clean* — a census over all 42 `_RELEASE/Packs/**/*.lua`
found **zero** occurrences of `ffi`, `bit.`, `bit32.`, `jit.`, `require(`,
`unpack(`, `setfenv`, `getfenv`, `loadstring`, `string.dump`, `newproxy`,
`module(`, `gcinfo`. (An earlier unscoped census showed large numbers; it was
polluted by `target/` and submodules. Do not re-run it unscoped.) Note also that
`ffi` cannot be a real concern at all: the C++ builds with `LUAJIT_DISABLE_FFI`,
so any pack using it is already broken on the official build.

Nevertheless:

- **`error()` text.** LuaJIT appends `"\nstack traceback:\n\t[C]: in ?"` to the
  error object; Luau and reference Lua do not. The C++ surfaces script errors, so
  this can be user-visible, and tests may match on it.
- **`tostring` / `%.14g`.** Both use `%.14g`, but through different formatters;
  the skill already documents two spellings of number formatting.
- **`pairs` iteration order** — the only *gameplay-affecting* risk: a level that
  creates walls (or drives anything ordered) while iterating a table would behave
  differently. (LuaJIT and Lua 5.1 already disagree here, so well-behaved levels
  cannot rely on it — but it must be measured, not assumed.)
- **GC timing / `collectgarbage` counts**; **`math.*` libm** calls.
- **What does *not* change**: the 285-function API is Rust-side; the replay
  *format* is Rust-side (`oh-replay`), so on-disk and wire compatibility survive
  intact. Only re-simulation differs — and on web, cross-platform determinism is
  already best-effort (26 concedes this for `miniz_oxide` vs system zlib and for
  libm), which is the same order of concession, not a new one.

## 7. The experiment — this issue's go/no-go gate (bounded)

Spike matrix: `{lua51+vendored, luau}` × `{host x86_64, wasm32, aarch64-android}`,
against the LuaJIT baseline. In this order:

1. **Portability**: does it even build for wasm (emscripten `longjmp` support —
   Lua 5.1 uses `setjmp`/`longjmp`; recent emscripten may need an explicit flag,
   *verify, do not assume*), and for the NDK (Luau is C++, so it needs a C++
   toolchain per target).
2. **Behavioral parity**: the 285-fn API suite + `oh-parity`'s Lua vectors,
   diffed against LuaJIT.
3. **Corpus smoke**: load all 42 shipped scripts, run `onLoad` + N `onUpdate`
   frames, diff observable state vs LuaJIT.
4. **Error/format diffs**: `error('x')`, `tostring(n)`, `string.format('%.14g',n)`.
5. **Per-frame script cost on the phone at 200 fps** — interpreter vs LuaJIT's
   JIT baseline, and `luau-jit` on native if Luau is the candidate.

**Decision rule.** If the only deltas are the known-shimmable ones (`error` text,
`bit`→`bit32`, `jit`→no-op, `table.new`→`table.create`), **unify on one VM**.
If a gameplay-affecting delta appears (`pairs`-order-driven wall creation,
pattern semantics), keep **LuaJIT on native, pinned to `4dc5ffbf`** (which also
removes the 21-month skew and may fix K3) **+ Luau on web**, feature-gated inside
the one `oh-lua` crate — precedent exists: the suite already runs in two feature
modes.

## 8. Recommendation

Single-VM is the goal; the two credible candidates are **reference Lua 5.1** and
**Luau**.

- Reference Lua 5.1 wins on **fidelity**: it *is* the language LuaJIT implements
  (5.1), it keeps every 5.1-ism Luau removed (so no shims for them at all), and
  its number formatting shares `luaconf.h` lineage with LuaJIT. It loses on
  speed (slowest interpreter) and on maintenance (frozen since 2012).
- Luau wins on **maintenance, speed, and sandbox-by-design** (the C++
  `destroyMaliciousFunctions` exists precisely to harden LuaJIT — Luau is
  hardened structurally), and the wasm session is already integrating it. It
  loses on dialect risk.

**Decide with §7's spike, not by preference.** Whichever way: record a
`DECISIONS.md` entry (the number is **D-037** — D-036 was claimed by the wasm/web
target's decision while this issue was open), a `deviations.md` row, correct the
now-falsified claims (`oh-lua/Cargo.toml:7-10` "matching the C++ build"; the
skill's "fork" framing), and keep any determinism claim scoped to the VM(s) that
actually run.

## 9. Non-goals

- Does not rewrite `oh-lua`: mlua abstracts the VM. The VM-specific spots are
  narrow — `number2str`/`%.14g`, error mapping and traceback text, the sandbox
  teardown steps (Luau has no `io`/`os`/`debug` to clear), and the FFI flag.
- Does not change the replay format or the wire protocol.
- Does not decide 26's other questions (VFS, `miniz_oxide`, WebAudio, net-off).

## 10. Sub-lane proposal (issue 30)

| lane | scope |
|---|---|
| 30a | the §7 spike matrix + report — **the go/no-go gate** |
| 30b | the `oh-lua` VM-abstraction seam (`number2str`, error/traceback, sandbox steps, FFI) behind features |
| 30c | wire the chosen VM; re-pin the Lua parity corpus; gate green on the chosen feature mode(s) |
| 30d | (only if unified on a non-LuaJIT VM) the desktop/Android perf decision — interpreter vs `luau-jit` |

## 11. The measurement — 030a LANDED (2026-10-17), and what it settles

The §7 spike is no longer a one-off experiment: it is a permanent, gate-wired test
module. `oh-parity` carries the differential-simulation harness (43 corpus cases over
the shipped packs; per-frame fingerprints of RNG/walls/player/score/customScore/status
timers/entities; a first-divergent-frame comparator; a multi-process repeat; perf; and a
self-check) plus eight explicit VM probes, run by `tools/lua-parity.sh` as **gate stage
5**. `backend-luajit` (default) and `backend-luau` are mutually-exclusive host features,
so both VMs are buildable and comparable on the host — the property the spike needed.

**Measured:**

- **43/43 corpus cases byte-identical across backends**, and **3 separate processes per
  backend identical** — no gameplay nondeterminism from the VM swap. This is the result
  the spike existed to get, and it is now re-proved on every gate rather than believed.
- **`pairs` order IS VM-dependent** (Luau stable across 4 observations, LuaJIT varying
  across 3) **and gameplay is unaffected** — which settles the question §3 raised:
  iteration order is not observable in this engine, because the corpus is byte-identical
  regardless.
- **6 of 8 VM probes differ, and every difference is dialect surface**: number spellings
  (including a literal-parse difference), the `error()` chunk prefix, the sandbox surface
  (e.g. `require('ffi')`), and Integer width (8-byte exact vs 4-byte saturating). Two are
  identical (`%.14g`, number→string).
- **Perf (debug, 2500 frames)**: light level 6.09 vs 6.60 µs/frame (LuaJIT/Luau, ~+8%,
  ≈1.2% of the 200–360 fps frame budget); the Lua-bound heavy case (`stress1`) is
  backend-neutral (1037 vs 1029 µs/frame).
- **Android**: `cargo check -p oh-app --target aarch64-linux-android` passes with Luau
  (LuaJIT regression-checked too), NDK 27.2.12479018.

**For §8's decision: keep `backend-luajit` as the default.** The parity evidence says
Luau would be safe *on the shipped corpus*, but two things argue against unifying now:
(a) the C++ build runs LuaJIT, so LuaJIT is the parity default and a divergence class is
eliminated rather than managed; (b) the corpus is in-repo-only — third-party/user packs
are exactly where an untested dialect difference would show up, and there is no such
corpus to test against here. Keeping `backend-luau` gate-covered (stage 5 proves parity
on every run) leaves the switch as a one-line default change if a reason appears
(wasm/Android runtime constraints, say), rather than a migration.

**Not established, stated plainly:** release-mode perf on either backend; on-device
perf (the Android evidence is compile-only); `pairs`-order stability beyond 3–4
processes; third-party pack content; the emscripten build under these features
(verified by construction so far — issue 26 is the lane that actually runs it in a
browser).

**One stale item in 030a's own report, corrected here:** it lists "the C++ third column
(030b blocked on system X11/GL packages)" as not established. 030b has since landed the
frame-level C++ oracle (3627/3627 frames identical, with committed vectors), so that
column now exists — meaning the Luau arm can be compared **against the C++ directly**,
not only against LuaJIT. That three-way check (C++ ↔ LuaJIT ↔ Luau over the same corpus)
is the natural next evidence if the default is ever revisited; it is a follow-up, not a
gap in 030a.

## 12. Operator direction (2026-10-17) — unifying on Luau is the LIKELY endpoint

Operator, verbatim: *"once we have instrumentation and proof of luau luajit parity, we
will likely unify on luau unless there's a hard blocker."* §8/§11's "keep the LuaJIT
default" is therefore a **staging position, not the goal** — it is what the default
should be *until* the proof §12 defines exists. Read the rest of this issue with that
intent.

**The operator's two conditions, and where they stand:**

1. **Instrumentation — DONE.** 030b landed the frame-level C++ oracle (the flag-gated
   `-fingerprint` hook, committed per-frame vectors, `gen_corpus`,
   `parity_cpp_fingerprints`): 3627/3627 frames identical against the port, with the
   four seeded cases matching the pre-existing `level_runs.json` pins exactly. The C++
   column is now a *reusable* comparator, not a one-off.
2. **Proof of Lua parity — half done.** 030a landed the A/B proof (43/43 corpus cases
   byte-identical LuaJIT vs Luau; 3 processes per backend identical). What is still
   missing is the **third column**: C++ ↔ LuaJIT ↔ Luau over the same corpus. That is
   now *possible* precisely because both landings exist, and it is the remaining proof.

**Hard-blocker taxonomy — what could still say no, and each one's mitigation.** The
point of writing these down is that most of them are *manageable* rather than hard:

| candidate blocker | status / mitigation |
|---|---|
| **Removed 5.1-isms in third-party packs** (`setfenv`, `getfenv`, `loadstring`, `unpack`, `newproxy`, `module`, `gcinfo`) | The in-repo census found **zero** occurrences across all 42 shipped pack scripts — but third-party user packs are the genuinely untested surface. **Mitigable**: implement them as a Lua-level 5.1 compat prelude over Luau (`unpack = table.unpack`, `loadstring = load`, emulated `setfenv`/`getfenv`, `module`/`newproxy`/`gcinfo` shims). The residual risk then shrinks to number spellings and error text, which the probes measured and which do not touch gameplay. |
| **Perf** (interpreter vs JIT) | Measured: ~+8% on a light level ≈ **1.2% of the 200–360 fps budget**; heavy/Lua-bound cases are backend-neutral. **Unmeasured: release mode and on-device.** A blocker only if release/on-device shows a real frame-budget loss — measurable, and `luau-jit` (native-only) exists as a fallback. |
| **Error text / traceback fidelity** | `%.14g` and number→string are **identical** (probed). Error chunk prefixes and number spellings differ — this affects console/log text, not replays (error text is not fingerprinted). A fidelity note, not a gameplay blocker. |
| **Replay determinism** | Already answered: 43/43 byte-identical across backends, 3 processes per backend identical. |
| **wasm / Android embedding** | Luau is statically linked (no `dlopen`), and emscripten cannot take LuaJIT at all — so unification **removes** the per-target split rather than creating one. Android `cargo check` with Luau passes. |

**The upside, which is the argument for doing it:** unification deletes
`extlibs/luajit-src` (the vendored LuaJIT), the root `[patch.crates-io]`, the
`ffi_stub.c` link shim, the Android `cshim.c` `__clear_cache` shim, and the per-target
mlua selection — and, more importantly, it makes **K3** (the arm64 `lj_asm_trace` ANR)
and **K4** (LuaJIT version skew) **moot by construction**. K3 is a live on-device ANR
risk, and the operator's own "hangs non-responsive" report is a candidate K3 sighting,
so unification may retire a real bug class rather than merely simplifying the build.

**Sequence once the proof is green:** add the 5.1 compat prelude (if we want the
third-party safety net) → flip `default = ["backend-luau"]` → delete the LuaJIT
machinery → close K3/K4. `tools/lua-parity.sh` re-proves parity on every gate either
way, so the switch is reversible and evidence-backed rather than a leap.
