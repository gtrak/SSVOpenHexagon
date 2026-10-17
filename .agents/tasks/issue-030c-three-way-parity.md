# Task 030c — the three-way parity proof (C++ ↔ LuaJIT ↔ Luau): the go/no-go for unifying on Luau

Read `.agents/WORKER-HYGIENE.md` first. Context: `.agents/plans/001-rust-port/30-lua-runtime.md`
(issue 30, especially §12) is the decision surface. Operator direction (2026-10-17):
*"once we have instrumentation and proof of luau luajit parity, we will likely unify on
luau unless there's a hard blocker."*

**Both of the operator's inputs now exist and this lane is the proof that joins them:**
- **030b landed the instrumentation** — the flag-gated `-fingerprint` hook on the C++
  `HexagonGame::update`, the committed per-frame vectors
  (`.agents/plans/001-rust-port/parity/cpp_fingerprints.json`), `gen_corpus`, and
  `parity_cpp_fingerprints.rs` (3627/3627 frames identical port-vs-C++).
- **030a landed the A/B parity** — `oh-parity`'s differential harness + 8 VM probes,
  run by `tools/lua-parity.sh` as **gate stage 5** (43/43 cases byte-identical
  LuaJIT vs Luau).

What is missing is the **third column**: the same corpus under **Luau** compared against
the **committed C++ vectors**, not merely against LuaJIT. This lane produces that, plus
the two unmeasured axes in §12's blocker table. **This lane measures and reports — it
does not flip any default.**

---

## 1. The three-way comparison (the core deliverable)

Reuse, do not rebuild: `cpp_fingerprints.json` is the C++ ground truth and
`parity_cpp_fingerprints.rs` already knows how to compare a port run against it for the
LuaJIT backend. Extend that comparison so the **same cases** can be run under
`backend-luau` (`--no-default-features --features backend-luau`, the 030a mechanism) and
diffed against the same C++ vectors.

Deliver a per-case table — `case | LuaJIT vs C++ | Luau vs C++` — and, for any
mismatch, the **first divergent frame** with the diverging field, exactly as 030b's
comparator reports it. A three-way mismatch is the finding of the lane, so report it
precisely rather than working around it.

Do **not** regenerate the C++ column unless you have to (it needs the container:
`tools/cpp-oracle/run.sh instrument` → `build` → `run` → `deinstrument`; the patch is
the single source of the instrumentation and **the reference tree must stay pristine** —
`git diff master -- src include` empty after any regeneration).

## 2. The third-party dialect risk, assessed rather than assumed

§12's only plausible *hard* blocker is 5.1-isms that Luau removed
(`setfenv`, `getfenv`, `loadstring`, `unpack`, `newproxy`, `module`, `gcinfo`) appearing
in **third-party** packs. The in-repo census found zero, so this lane must decide
whether that risk is mitigable:

1. Re-run the census over `_RELEASE/Packs/**/*.lua` (and any level scripts in the
   shipped packs) and record the count explicitly — the number is the evidence.
2. **Prototype the 5.1 compat prelude**: a Lua-level shim set defined on top of Luau
   (`unpack = table.unpack`, `loadstring = load`, emulated `setfenv`/`getfenv`,
   `module`/`newproxy`/`gcinfo`), with a test asserting a 5.1-style snippet runs under
   Luau with the prelude installed and fails without it.
3. Report which of the seven are faithfully emulatable, which are approximated, and
   which are impossible — that distinction is what turns "hard blocker" into "known,
   bounded cost" or confirms it is genuinely hard.

## 3. The unmeasured axes (§12)

- **Release-mode perf** on both backends (the 030a numbers are debug): same corpus,
  `--release`, report µs/frame per case and the delta. Bounded runs only
  (`timeout`), and do not run a perf measurement concurrently with a heavy build.
- **On-device** perf is NOT required here (it needs the phone; ADB has been down). If
  ADB happens to be up, a single measurement is a bonus — otherwise report PENDING, do
  not infer.

## 4. Output

A `## Report` appended to a task file for this lane with: the three-way table, the
first-divergent-frame detail for anything that differs, the census count, the
compat-prelude result (which 5.1-isms emulate, which do not), the release-mode perf
table, and — explicitly — a **verdict on whether any hard blocker was found**, phrased
so the operator can act on it. "No blocker found, with these residual risks" is a
complete and useful answer.

## 5. Hygiene

`tools/gate.sh` (or targeted checks while other lanes run); `cargo fmt` before
committing; bounded processes (`timeout`); commit with **explicit paths**, one commit
per milestone; at most one image read (probably none needed); **stop-and-report** on a
wall. Do not flip the default backend, do not delete the LuaJIT machinery, and do not
touch the C++ reference tree.

---

## Report

Lane complete. All deliverables measured on this worktree
(`/home/gary/dev/oh-lanes/030c`, branch `lane/030c`); commits `34b19dab` (milestone 1,
three-way comparison), `bff83c74` (milestone 2, census gate), `b52bde80` (milestone 3,
prelude prototype), milestone 4 (release perf — measurement only, no code), this
report (milestone 5). The C++ reference tree was not touched; `cpp_fingerprints.json`
was not regenerated; the default backend was not flipped.

### 1. The three-way comparison (C++ ↔ LuaJIT ↔ Luau)

The committed C++ vectors (`cpp_fingerprints.json`, 6 cases, 3627 frames) were re-diffed
per case under **both** backends (`parity_cpp_fingerprints.rs ::
cpp_fingerprint_per_case_table`, run per gate as stage 4 / `tools/lua-parity.sh` stage
5):

| case | frames | LuaJIT vs C++ | Luau vs C++ |
|---|---|---|---|
| `apeirogon_spin439` | 439 | OK | OK |
| `autotest0_spin1180` | 1180 | OK | OK |
| `arcadia_spin456` | 456 | OK | OK |
| `bipolarity_spin358` | 358 | OK | OK |
| `apeirogon_spin439_dm25` | 408 | OK | OK |
| `autotest0_mixed` | 786 | OK | OK |
| **total** | **3627** | **OK (6/6)** | **OK (6/6)** |

**No mismatch in any case.** There is therefore no first-divergent-frame detail to
report: every frame of every case is identical C++ ↔ LuaJIT and C++ ↔ Luau (the
per-case table prints `OK (N frames)` for all 12 backend×case cells; had any case
diverged, the comparator prints the first divergent frame + field, 030b's
attribution, and the test panics with the whole table). The three-way column is
closed: the LuaJIT↔Luau A/B (030a, 43/43 cases) plus the two C++ columns now form a
closed triangle — C++ is the common vertex.

### 2. The third-party dialect risk

**Census count: 0.** Re-run over `_RELEASE/Packs/**/*.lua` — 42 shipped `.lua`
files (verified: no `.lua` anywhere else in `_RELEASE`), 13 word-boundary patterns
(the seven §12 isms + the plan-§6 reference set: `ffi`, `bit.`, `bit32.`, `jit.`,
`require(call)`, `string.dump`, and `unpack` matched as a *global* use, not the
`table.unpack` substring that appears in `u_getDifficultyMult`): **every pattern =
0**. The gate test `dialect_census.rs` re-proves the zero on every gate run and
self-checks that each pattern discriminates (a planted hit must be caught).

**Compat prelude result** (`lua51_prelude.lua` + `lua51_snippet.lua` +
`lua51_compat_prelude.rs`, all green on both backends):

FINDING OF RECORD — **the plan's premise ("Luau removed the 5.1-isms") does not
hold for the Luau this port builds.** Luau 0.663 (luau0-src 0.12.3 via mlua 0.10.5)
re-added most of the 5.1 surface with REAL semantics (verified in the vendored
source — `lbaselib.cpp`, `ltablib.cpp` — and in the running VM). Classification of
the seven named isms on the two VMs of this build:

| ism | this Luau 0.663 | this LuaJIT 2.1 fork | classification |
|---|---|---|---|
| `unpack` | NATIVE (5.1-compat global reinstall) | native | **faithful** — prelude guard is a no-op |
| `getfenv` | native, real env | native | **faithful** — prelude leaves it |
| `setfenv` | native, REAL env semantics (`setfenv(f,t)` re-binds; `setfenv(0,t)` the thread; errors on C functions — 5.1 behaviour) | native | **faithful** — the plan's "not emulatable" verdict does not apply to this build; non-global re-binds verified working on both |
| `newproxy` | native (zero-size userdata + empty metatable, like LuaJIT) | native (userdata; flag in an internal weak table) | **faithful for the portable idiom** — note: `newproxy` is a 5.2+ construct, and the plan's "5.2 table + setmetatable" expectation is wrong on BOTH observed VMs: both return userdata and both refuse the Lua-level `setmetatable`; the finalizer idiom that runs everywhere is metatable mutation, `getmetatable(newproxy(true)).__gc = fn` (LuaJIT's own profiler's usage) |
| `gcinfo` (the 5.1 `collectgarbage("gcinfo")` option form) | option ABSENT (but a native global `gcinfo()` function exists) | option ABSENT (the fork adds the global `gcinfo()` in `lib_base.c` too) | **faithful** — the prelude's `collectgarbage` wrapper (pcall-probed, installed on both backends) adds the option as integer KB = `floor(count)` and forwards every native option unnormalised |
| `module` | ABSENT | native (5.2-style `package.module`) | **approximated on Luau** — the prelude's shim restores the table + read side (`__index = _G`) AND the defining-chunk write side (native `setfenv`, level 3 = the calling chunk, exactly 5.1's contract); deliberate LENIENT superset: the defining chunk keeps global visibility (5.1 self-indexes; LuaJIT's table has no metatable — both stricter) |
| `loadstring` | ABSENT — and `load` is ABSENT too: **Luau's base library has no chunk compiler at all** | native | **NOT emulatable at the Lua level** — no Lua code can create a chunk compiler; the mitigation is a ONE-LINE RUST-SIDE binding (global `load` from mlua's own `Lua::load`), which the prelude's `loadstring = load` line then activates. Demonstrated test-locally (`loadstring('return 6*7')() == 42`); not shipped in the Lua-level prelude |

Deliverable test, as specified: the 5.1-style snippet (all seven isms in canonical
form) **fails bare** on both backends (Luau at `loadstring` — proving `unpack`, the
construct before it, is native; LuaJIT at the `gcinfo` option) and **runs to
completion with the prelude installed** (on Luau plus the test-local one-line `load`
binding). The per-ism table, the three-way `loadstring` pin (bare / prelude-only /
binding+prelude), the approximation boundaries (observed, not assumed), and
idempotency are all gate tests, both backends.

### 3. Release-mode perf (both backends, `--release`, bounded, single run each)

`perf_on_update_per_frame` (030a's harness, re-run release; representative cases:
the lightest `babysteps` and the heaviest `stress1`, `alwaysSpinRight`, 2500 frames
flat-out headless on the dev host):

| case | LuaJIT pure | Luau pure | delta (Luau−LuaJIT) | LuaJIT + fingerprint | Luau + fingerprint | delta (Luau−LuaJIT) |
|---|---|---|---|---|---|---|
| `babysteps` | 0.86 µs/frame | 0.91 µs/frame | +0.05 µs (+5.8%) | 1.80 µs/frame | 1.75 µs/frame | −0.05 µs (−2.8%) |
| `stress1` | 136.03 µs/frame | 131.50 µs/frame | −4.53 µs (−3.3%) | 574.63 µs/frame | 581.39 µs/frame | +6.76 µs (+1.2%) |

**The inter-backend delta is at noise level** (≤ 6.8 µs/frame on the heaviest case,
≤ 5.8%, and the sign flips across cases/metrics). Both backends sit > 10× inside
the 2.8–5.0 ms frame budget even on `stress1` (575–581 µs with the gate's
per-frame fingerprint). For reference, the 030a debug numbers (light 6.09–6.60,
`stress1` 1029–1037 µs/frame) were debug-profile; release is ~7–8× faster on the
heavy case. **On-device: PENDING** — `adb` is not installed on this dev host
(probed, bounded); per the task, on-device is not required and was not inferred.
Host perf is a measurement, not a pin — the phone remains the real constraint.

### 4. Verdict

**No hard blocker found.** §12's only plausible hard blocker — "removed 5.1-isms in
third-party packs" — is, on the Luau this port actually builds, a **known, bounded
cost**, not a wall: six of the seven isms are native-and-faithful (or
prelude-faithful for the `gcinfo` option form) on both backends, and the seventh
(`loadstring`) needs a one-line Rust-side `load` binding, demonstrated and
documented. The three-way proof is closed (C++ ↔ LuaJIT ↔ Luau, 6/6 cases × 3627
frames identical on both columns), the shipped-pack dialect census is a re-proven
zero, and release-mode perf is backend-neutral. The operator can proceed toward
unifying on Luau; this lane flipped no default, and the LuaJIT machinery remains in
place as the second column of the gate.

**Residual risks (the "with these" half of the answer):**

1. **`loadstring` on Luau needs the Rust-side `load` binding** to work at all — a
   one-line host change (expose `mlua::Lua::load` as a global), proven in-test but
   NOT shipped in this lane. Any future third-party pack that calls `loadstring`
   will hit this on a Luau-only build until that line is added.
2. **The `module` shim is a lenient superset**: the defining chunk keeps global
   visibility through `__index = _G` (5.1 self-indexes its own table; LuaJIT's
   module table has no metatable). Scripts that *rely on* module() strictness
   (e.g., expecting a leaked global to error) behave more leniently than on
   5.1/LuaJIT.
3. **Host contract**: the prelude must be installed before a 5.1-style third-party
   script runs; there is no auto-detection of 5.1 scripts.
4. **Perf evidence is dev-host, single-run, release**: the inter-backend conclusion
   (noise-level delta) is sound at this host but on-device numbers are PENDING;
   the budget headroom (> 10×) makes a host↔device inversion unlikely but not
   excluded.
5. **VM-version specificity**: the classification is for the exact VMs this port
   builds (Luau 0.663 / the vendored LuaJIT fork). A future Luau that DID remove
   `unpack`/`setfenv` etc. would exercise the prelude's dormant fallback shims
   (refuse-loud `setfenv`, table-based `newproxy`) — guarded and unit-pinned, but
   not proven on a second real VM.
6. **The zero census is a point-in-time zero over the shipped packs**; it is
   re-proven on every gate run, and the prelude + this classification exist
   precisely for the day a third-party pack introduces one of the isms.
