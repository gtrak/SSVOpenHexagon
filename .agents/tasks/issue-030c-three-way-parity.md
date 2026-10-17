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
