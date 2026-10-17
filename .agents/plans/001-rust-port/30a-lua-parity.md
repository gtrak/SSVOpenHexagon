# 30a — the LuaJIT↔Luau parity spike (the "unify on Luau?" decision tool)

Lane 030a. Task: `.agents/tasks/issue-030a-lua-parity.md`. Context:
`30-lua-runtime.md` (issue 30 — the decision surface) and the wasm lane's
settled portability half (D-036: LuaJIT cannot target emscripten, Luau can —
the wasm branch chose **per-target**: LuaJIT desktop, Luau emscripten).

**This lane measures and recommends. It migrated nothing:** no LuaJIT
deleted, no defaults switched. It made the backend a first-class,
gate-covered choice and produced the measurement the operator needs.

**Headline result:** over the full shipped `_RELEASE/Packs` corpus (43
cases: 37 level cases + the 6 `base/Scripts` helper-script cases), at a
fixed `TIME_STEP` dt with seeded deterministic input, the two backends
produce **byte-identical per-frame gameplay fingerprints** — on run-0
(A/B) and across 3 separate processes per backend (repeats). The
differences are all at the **dialect surface** (documented below), none
of them gameplay-visible in shipped content.

---

## 1. The feature plumbing (what now exists)

`oh-lua` gained two mutually-exclusive features (enforced at build-script
time with an actionable error; mlua-sys also rejects a double backend):

- `backend-luajit` (default) = `mlua/luajit` + `mlua/vendored` — the
  C++-matching build (vendored LuaJIT, FFI off, `ffi_stub.c` link shim),
  behavior unchanged.
- `backend-luau` = `mlua/luau` — the Luau interpreter (no `io`, no `jit`,
  no FFI; `lua_Integer` is 32-bit).

Forwarded through `oh-app` (normal dep, `default-features=false` on the
oh-lua edge), `oh-game` (dev-dep) and `oh-parity` (dev-dep + `mlua`
dev-dep for the width probe), each with `default = ["backend-luajit"]`.
Features compose with the wasm branch's per-target rule exactly as
specified: **features first, target-based default after** (an explicit
`--features backend-luau` on emscripten wins; without one, the target
default applies as before — the wasm lane's rules are untouched).

Source changes required for Luau to build and behave: `convert.rs` maps
`mlua::Integer` to `i64` (LuaJIT) / `i32` (Luau) with documented
saturation on out-of-width values; `context.rs` gained a tolerant
`variable_exists()`; `sandbox.rs` skips absent targets (`io.*`,
`package.loaders`, …) instead of erroring — the LuaJIT observable
post-state is unchanged; `build.rs` compiles `ffi_stub.c` only for the
LuaJIT backend.

One unification trap found and fixed: `oh-lua`'s normal dependency on
`oh-game` had `default-features` on, which re-enabled `oh-game`'s default
(`backend-luajit`) via feature unification inside `oh-game`'s own Luau
test builds → double backend → `mlua-sys` compile error. Now
`default-features=false` on that edge (observably neutral: the forwarding
is a no-op for oh-game-as-a-dependency).

### Exact commands (the reusable deliverable)

All per the lane's build convention (`CARGO_TARGET_DIR=$PWD/target
CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
CARGO_PROFILE_DEV_INCREMENTAL=false`).

| what | `backend-luajit` (default) | `backend-luau` |
|---|---|---|
| oh-lua suite | `cargo test -p oh-lua` | `cargo test -p oh-lua --no-default-features --features backend-luau` |
| oh-game suite (runs Lua per frame) | `cargo test -p oh-game` | `cargo test -p oh-game --no-default-features --features backend-luau` |
| oh-parity suite (incl. the harness) | `cargo test -p oh-parity` | `cargo test -p oh-parity --no-default-features --features backend-luau` |
| app build | `cargo build -p oh-app` | `cargo build -p oh-app --no-default-features --features backend-luau` |
| Android check (task §4, REQUIRED) | `source tools/android-env.sh && cargo check -p oh-app --target aarch64-linux-android` | `source tools/android-env.sh && cargo check -p oh-app --target aarch64-linux-android --no-default-features --features backend-luau` |
| the gate stage (both backends) | `tools/lua-parity.sh` (emits/compares **both** backends regardless of the host default; `LUA_PARITY_REPEATS_COUNT=K` raises the run count) | same |

Harness env (set on the `cargo test` invocation): `LUA_PARITY_RUN=<k>`
(which run index to emit), `LUA_PARITY_COMPARE=1` (A/B run-0 vs run-0 —
the comparator fails with the case id + **first divergent frame**),
`LUA_PARITY_REPEATS=<k list|all>` (per-backend cross-run check),
`LUA_PARITY_PROBES=1` (print the per-probe diff table).

Measured suite state (host x86_64 Linux, debug profile, `debug=0`):

- `oh-lua`: **36 tests green on both backends** (the stdlib-surface and
  runtime pins carry per-backend arms; every divergence is pinned with a
  `DIVERGENCE (documented)` comment in `crates/oh-lua/tests/`).
- `oh-game`: green on both backends (63 engine tests + the
  composition/determinism suites that run Lua per frame).
- `oh-parity`: green on both backends (the differential harness: 5 tests,
  ~85 s per backend run in debug).
- `cargo test --workspace` (defaults): 117 test binaries, 0 failed
  (ALSA console noise only — no sound card in this environment,
  pre-existing).
- `cargo fmt --check` and `cargo clippy --workspace --all-targets -- -D
  warnings`: clean.

---

## 2. The differential simulation (the important one)

`crates/oh-parity/tests/lua_engine_parity.rs` — a permanent test module,
artifact-based, wired into the gate (`tools/lua-parity.sh`, stage 5 of
`tools/gate.sh`).

**Fingerprint definition** (`lua-engine-parity/v1`, committed in the test
source; the artifacts under `target/lua-parity/` are never committed —
large + machine-specific). Per case, per frame: an FNV-1a 64 hash of the
canonical little-endian encoding of the gameplay-visible state:

1. game-RNG state (`RandomNumberGenerator` engine state) + draw count
2. wall count + per wall: position + rotation
3. custom-wall count + per custom wall: position + rotation
4. custom-timeline count
5. player position + rotation
6. score
7. `customScore`
8. level-status timers
9. entity counts

Per-frame = after each `update(DT)`/`post_update()` pair; frame 0 =
post-`new_game`. `DT = 0.25` s (the engine's `TIME_STEP` — the fixed
schedule the spec requires; no variable-dt in the comparison).

**Case generator (seeded, reproducible):** every level json under
`_RELEASE/Packs/*/Levels/*.json` whose `luaFile` exists → a level case
(id `lvl-<pack_id>_<level_id>`, 37 cases); plus every
`_RELEASE/Packs/*/base/Scripts/*.lua` (6 cases) ridden on a neutral
level. Input programs: a per-case seed (FNV-1a of the case id) drives a
deterministic left/right input program (level cases) or the C++
`alwaysSpinRight` 0x02 stream (script cases — long-lived). Frame cap
8000 (~33 s of game time) — trimmed from the spec's ~12k so the
permanent gate stage stays bounded (the `stress1` level's deliberately
heavy `onUpdate` makes 12k frames the single most expensive case at ~118
s in debug); the cap is identical for both backends, so cap-reached
cases are fair prefix comparisons.

**Results (this lane's measurement):**

- **A/B (run-0):** `43 cases byte-identical across luajit vs luau` —
  zero divergent frames in any case, including the heavy-Lua levels
  (`stress1`, `cw`, `customtimeline`, `text_shader`, `evotutorial`,
  `babysteps`) and all 6 helper-script smokes.
- **Multi-process repeat (run-0/1/2, separate `cargo test` PROCESSES
  per run, per backend):** every run identical to run-0 on both
  backends → **no per-process gameplay nondeterminism observed** on
  either backend (3 processes each; raise via
  `LUA_PARITY_REPEATS_COUNT`).

**The `pairs`-order probe (the one gameplay-affecting risk — settled,
not assumed):** a fixed 16-string + 8-integer key set, iterated with
`pairs`, observed in separate processes:

- **Luau: STABLE across processes** — 4 separate processes produced the
  identical order (`foxtrot papa november mike oscar lima delta hotel
  echo alpha india kilo juliet golf bravo charlie`).
- **LuaJIT: VARIES across processes** — 3 separate processes produced 3
  distinct orders (e.g. `juliet` first in one, last in another).
- The gameplay fingerprints were byte-identical across all of these
  processes regardless → **shipped content does not observe string-table
  iteration order in any gameplay-visible state**; the risk is real
  mechanistically (a level that created walls while iterating a string
  table could diverge between process restarts on LuaJIT), and it is
  *absent* on Luau in this build.

---

## 3. The probe diff table (task §5)

From `crates/oh-parity/tests/lua_vm_probes.rs` (8 probes; artifacts at
`target/lua-parity/<backend>/run-0/probes.json`; diff via
`LUA_PARITY_PROBES=1`). Verdicts: `identical` / `shimmable` (tests/logs
only) / `user-visible` (error text players could see) /
`gameplay-affecting`.

| probe | LuaJIT (measured) | Luau (measured) | verdict |
|---|---|---|---|
| `tostring` spellings (spec set + wide ints) | `1e20→"1e+20"`, `9999999999999.999→"10000000000000"`, `2^53+1→"9.007199254741e+15"`, `10^16→"1e+16"`; `0.5`, `1234567.89`, `0.0001`, `1`, `0.1`, `1e30`, `1e10`, `-0.0→"-0"` same on both | `1e20→"100000000000000000000"` (full integer spelling), `9999999999999.999→"9999999999999.998"`, `2^53+1→"9007199254740992"`, `10^16→"10000000000000000"` | **user-visible** if levels ever print wide numbers (none in the shipped corpus do). Note `9999999999999.999` differs in the *parsed value itself*, not just the spelling (LuaJIT 2.1's decimal parser rounds that literal to `1e13`; Luau's to `…9999999999999.998`) |
| `string.format('%.14g')` | `0.1\|1e+16\|15000000000\|9.007199254741e+15\|…` | identical, byte for byte (both are C-printf-backed) | identical |
| the port's number→string (`read_variable::<String>`, the C++ `readString`/`lua_number2str` path) | `1\|1e+20\|0\|42.5` (`-0.0`→`"0"`) | identical | identical (backend-independent Rust formatting) |
| `error('x')` message shape | `x\nstack traceback:…` | `[string "chunk"]:1: x\nstack traceback:…` (+ one extra frame line) | **user-visible** (script-error text surfaced by the console). The C++ parity shape is the LuaJIT one |
| dotted-path / nil-index semantics | `attempt to index global 'dpn' (a nil value)` (chunk-prefixed); port `does_variable_exist`: `true/false/Err(VariableDoesntExist)` | `attempt to index nil with 'a'` (chunk-prefixed); port behavior identical | **user-visible** (error text). Port semantics identical |
| sandbox post-state (full `_G` surface + the spec's checklist) | `os`/`debug`/`math.randomseed`/`package.load*`/`loaders`/`loaded.os`/`loaded.debug` all gone; `io.open` = read-only wrapper, `io.popen/flush/write/setvbuf` gone; `require('ffi')` → `'package.loaders' must be a table`; `bit`, `load*`, `dofile`, `module`, `jit` present | all the cleared targets gone the same way; `io.*` absent at all (`io` is nil — the C++'s `luaopen_io == NULL` short-circuit); `require('ffi')` → `module 'ffi' not found`; no `bit`/`load*`/`dofile`/`module`/`jit`; Luau-only `bit32`/`buffer`/`utf8`/`vector`/`typeof`/`rawlen` present | **shimmable** for the shipped corpus (its scripts use none of the differing surface — proven by the byte-identical fingerprints); would be a **user-visible**/script-compat change for third-party packs relying on `io`/`load`/`jit`/`bit` |
| `pairs` order over the same table | **varies per process** (3 distinct orders in 3 processes) | **stable per process** (identical in 4 processes), but a different order than LuaJIT's | the shipped-content check: **identical** (fingerprints); the mechanism: real on LuaJIT, absent on Luau — see §2 |
| integer semantics / `mlua::Integer` width | 8 bytes (`i64`): `i32::MAX+1` and `2^62` round-trip exactly; `math.floor(4.9)=4`, `4.5%2=0.5`, `-4.5%2=1.5`, `tostring(42.0)="42"`, `type(42.0+0)="number"` — all same on both | 4 bytes (`i32`): `i32::MAX+1` and `2^62` **saturate to `2147483647`** on Rust→Lua writes and Lua→Rust reads; `math.floor`/`%`/integral-`tostring` identical to LuaJIT | **gameplay-affecting only if** Rust writes/reads an integer wider than 31 bits through the Lua boundary (the engine's score/seed paths use f32/f64 and i64 *in Rust* — the shipped corpus proves no divergence; a future pack API pushing >2.1e9 integers would saturate on Luau) |

---

## 4. Performance (task §3)

`perf_on_update_per_frame` (in the harness; printed, not pinned — host
perf is a measurement, not a pin): `alwaysSpinRight` input, 2500 frames,
debug profile (`dev` debug=0; **not release**, see "could not
establish"), host x86_64:

| case | LuaJIT pure | Luau pure | LuaJIT +fingerprint | Luau +fingerprint |
|---|---|---|---|---|
| `babysteps` (light, tutorial) | **6.09 µs/frame** | **6.60 µs/frame** (+8%) | 10.87 µs/frame | 11.61 µs/frame |
| `stress1` (deliberately heavy `onUpdate`) | 1037.50 µs/frame | 1029.30 µs/frame (−1%) | 4652.76 µs/frame | 4907.85 µs/frame |

Context: the game runs 200–360 fps ⇒ ~2.8–5.0 ms/frame total. Light
levels use ~1.2–1.4% of that budget on either backend; the fingerprint
cost (the gate's overhead, not the game's) is ~4.5–5 µs/frame.
`stress1` at ~1 ms/frame is a Lua-bound, deliberately pathological level
— its cost is in the Lua it executes, and it costs ~the same on both
backends. **Luau (interpreter) is within ~8% of LuaJIT (JIT) on light
content in debug; the gap's release-mode size is not measured here**
(LuaJIT's JIT advantage typically grows in release with hot loops —
see §7).

---

## 5. The Android result (task §4, REQUIRED)

- `source tools/android-env.sh && cargo check -p oh-app --target
  aarch64-linux-android --no-default-features --features backend-luau`
  → **PASSES** (NDK 27.2.12479018, API 28; whole `oh-app` graph incl.
  egui/egui-winit/egui_glow, Luau included).
- Same command with the default (LuaJIT) backend → **PASSES**
  (regression check for the plumbing).

Luau builds for `aarch64-linux-android` end-to-end. A web-only Luau is
*not* what we have: one VM is viable on all three targets (desktop,
Android, emscripten).

---

## 6. The recommendation

**Recommendation: keep `backend-luajit` as the default (no unification
now); treat `backend-luau` as a first-class, gate-covered alternative —
the switch, if ever chosen, is a one-line default change, and the
measurement that would have been needed to make it safely now exists and
re-runs in the gate every time.**

Reasoning, from the measured data:

1. **Nothing in the measurement forces a move.** The shipped corpus
   simulates byte-identically on both backends, both build for all three
   targets, and Luau's per-frame cost is within ~8% on light content.
   There is no parity gap to close by unifying.
2. **LuaJIT stays the C++-parity backend.** Its error shapes, number
   spellings, and stdlib surface are the ones the C++ build (and hence
   the published third-party pack ecosystem, written against LuaJIT 5.1)
   exposes. Unifying on Luau changes all of that at once: `tostring` of
   wide numbers, `error()` text, the disappearance of `io`/`load*`/
   `jit`/`bit`/`module`/`dofile`, and 31-bit integer width at the Rust
   boundary. None of that breaks shipped content (proven), but it is a
   compatibility surface for the *third-party* pack ecosystem — which
   this lane did not and could not test (no user-pack corpus exists in
   the repo).
3. **What unification would delete** (if the verdict were "go"):
   `extlibs/luajit-src` + the `[patch.crates-io]` (029k1),
   `ffi_stub.c`, the vendored FFI-off build, `crates/oh-android/cshim.c`
   (the `__clear_cache` shim), the per-target backend split, and — most
   importantly — **K3** (the arm64 `lj_asm_trace` ANR) and **K4** (the
   LuaJIT version skew) become moot by construction. That is a real
   simplification worth remembering.
4. **What it would cost:** the §3 surface changes for third-party
   scripts; i32 saturation at the integer boundary; a measured-but-
   unproven-in-release interpreter instead of a JIT; and the loss of the
   C++-parity error/spelling behavior.
5. **The pairs-order finding actually argues for waiting:** Luau's
   stable cross-process iteration order is a genuine robustness *gain*;
   but the same measurement shows the decision is not urgent, and the
   gate now catches any future behavioral drift either way.

If the operator's strategic goal is a single VM everywhere and the team
accepts the dialect surface (and the i32 boundary), the measurements
support that choice — nothing measured here contradicts it. The
measurement does *not* motivate doing it now.

---

## 7. What could NOT be established (explicit)

- **Release-mode (opt-level 3) performance on both backends.** Only
  debug-profile numbers are reported (§4). LuaJIT's JIT advantage in
  release (hot-loop `onUpdate`s) is unmeasured; the phone budget
  analysis must treat the debug numbers as conservative for the engine
  but *not* for the Lua-side cost of a JIT.
- **On-device (aarch64 phone) performance.** The Android target was
  `cargo check`-verified to compile — not run. No device confirmation of
  either backend's frame time.
- **Per-process pairs-order stability beyond 3–4 processes per
  backend** (LuaJIT varied in 3 observations — enough to be real;
  Luau was identical in 4 — a sample, not a proof of Luau's internal
  seed handling; the C++-side LuaJIT hash options were not inspected).
- **Third-party / user-pack content.** The corpus is `_RELEASE/Packs`
  only (42 shipped scripts). Packs relying on `io.open`/`load`/`jit`/
  `bit`/`module`/`dofile` or on 64-bit integers at the Lua boundary
  would behave differently on Luau — by design, not by defect — and no
  such corpus exists in the repo to measure against.
- **Integer edge semantics beyond the probed set** (`//` floored
  division, `%` on values outside i32, `tostring` of integral floats
  above 2^53): the probed `math.floor`/`%`/`tostring(42.0)` are
  identical, and the corpus exercises the common paths identically, but
  the full integer-operator matrix was not diffed.
- **The C++ third column** (port(Luau) ↔ C++ transitively): the C++
  reference build is blocked on system X11/GL dev packages (root;
  task 030b, see its status note) — the A/B this lane delivers is
  backend↔backend, and the C++ column stays a 030b oracle-vector job.
- **The wasm branch's rule in a full emscripten build** with these
  features: the composition was verified by construction (features
  first, target default after — `oh-lua`/`oh-app` edges + the wasm
  lane's unchanged target rules) and by native/Android builds, but this
  lane did not run an emscripten toolchain.

---

## 8. Durability (task §7 — it is a test module, not an experiment)

- `crates/oh-parity/tests/lua_engine_parity.rs` +
  `crates/oh-parity/tests/lua_vm_probes.rs` — permanent test targets.
- Artifacts under `target/lua-parity/<backend>/run-<k>/` (never
  committed), each carrying its fingerprint/probe definition.
- The comparator fails with **case id + first divergent frame**
  (fingerprints) and prints the per-probe diff table (probes).
- Case generator is seeded (reproducible case sets).
- `tools/lua-parity.sh` (both backends × N separate VM processes,
  A/B + repeats + probes, every `cargo` invocation `timeout`-bounded)
  is **stage 5 of `tools/gate.sh`** — it runs on every gate, on both
  backends, forever.
