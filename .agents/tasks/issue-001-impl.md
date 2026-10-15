# Task spec — issue 001: workspace scaffold, `oh-math` + `oh-rng`

You are a worker. Implement exactly this issue. Do not redesign, do not widen
scope, do not decide process.

## Context

Repo: `/home/gary/dev/SSVOpenHexagon` — a C++20 game being ported **literally 1:1**
to Rust (plan `.agents/plans/001-rust-port/PLAN.md`). This is the first issue: it
creates the Cargo workspace and ports the small pure helpers plus the RNG.

Read first, in order:
1. `.agents/plans/001-rust-port/PLAN.md` (architecture, ground rules)
2. `.agents/plans/001-rust-port/DECISIONS.md` (D-000 arithmetic, D-001 layout, D-003 RNG)
3. `.agents/plans/001-rust-port/01-workspace-math-rng.md` (the contract)
4. `.agents/plans/001-rust-port/mapping/oracle-vectors.md` (**golden RNG vectors**)
5. `.agents/skills/README.md` (skill rules)
6. The real C++ source you are porting (below) — **read it, do not guess**.

## Working agreement

- **Skills are truth.** `.agents/skills/*.md` are authoritative. For anything not
  covered there, read the real source under `extlibs/` or `src/`. Do **not** fetch
  from the web, docs.rs, or `~/.cargo/registry` for C++ semantics.
- **Write first.** Create the workspace and all module skeletons in your first
  handful of tool calls, then compile and iterate on named errors.
- **Arithmetic exactness (D-000).** `float`→`f32`, `double`→`f64`. Never reorder
  floating-point expressions. Copy constants verbatim.
- **Skills from real source.** You must write `.agents/skills/lib-pcg.md` and
  `.agents/skills/lib-ssvutils.md` from the real source you read (see below), and
  update `.agents/skills/README.md`'s index. If code contradicts any skill, code
  wins and you correct the skill minimally and list it under "skill corrections".
- **Commit at the first green build**, then keep going (amend or add commits).
  The deliverable is the commit.

## What to build

### 1. Workspace

- `Cargo.toml` (workspace root) with members `crates/oh-math`, `crates/oh-rng`
  (add more members only as later issues need them — leave a comment).
- `rust-toolchain.toml` pinning the installed stable.
- Workspace-resolved common settings: `edition = "2021"`, and a
  `[workspace.dependencies]` table with `flate2`, `serde`, `serde_json`,
  `thiserror` pinned (used by later issues; harmless now).
- `.gitignore` already ignores `target/`.

### 2. `crates/oh-rng` — bit-exact RNG

Port `hg::random_number_generator` (`include/SSVOpenHexagon/Core/RandomNumberGenerator.hpp`)
backed by `ssvu::pcg32_fast` = `pcg_engines::mcg_xsh_rs_64_32`
(`extlibs/SSVUtils/include/SSVUtils/Internal/PCG/pcg_random.hpp`).

Exact semantics (already characterized in `oracle-vectors.md`; re-derive from
source and confirm):
- 64-bit LCG state, multiplier `6364136223846793005`, **increment 0** (MCG / `no_stream`).
- Seed ctor: `state = seed | 3`.
- `output_previous == true`: each call returns `output(state)` then advances
  `state = state * M`.
- output `xsh_rs` (itype 64, xtype 32): `rshift = (internal >> 61) & 7;
  internal ^= internal >> 22; result = (internal >> (22 + rshift)) as u32`.
- `advance(delta)` = standard LCG jump-ahead with `plus = 0`.
- `seed_type` = `unsigned long long` → `u64`.
- `get_int(min,max)` / `get_real(min,max)` must match libstdc++
  `std::uniform_int_distribution` / `std::uniform_real_distribution` on this engine
  for the types the game uses (`int`, `float`, `double`; also unsigned variants).

Tests (`crates/oh-rng/tests/` or `#[cfg(test)]`) must reproduce **every** value in
`oracle-vectors.md`: the raw stream for seed 12345, `get_int(0,5)` stream,
`get_real(0f,1f)` stream, `advance(100)`, and edge seeds 0 and 4294967295. Test
names must say which property they pin.

### 3. `crates/oh-math` — helpers

Port only what the game actually uses. Read the real sources:

- `extlibs/SSVUtils/include/SSVUtils/Core/Common/Frametime.hpp` — `FT`(=f32),
  `getFTToSeconds`, `getSecondsToFT`, `getFTToFPS`, `secondsFTRatio`.
- `extlibs/SSVUtils/include/SSVUtils/Core/Utils/Math.hpp` — `getRndR`/`getRndI`,
  `getMod`, `getByModIdx`, `getClamped`, `clamp`, `clampMax`, `getSign`, `contains`,
  `tau`, `pi`, `toRad`, `toDeg`, `rotate`.
- `extlibs/SSVUtils/include/SSVUtils/Core/Utils/Containers.hpp` — `eraseRemoveIf`
  (as a generic helper if used; otherwise skip and note it).
- `extlibs/SSVUtils/include/SSVUtils/Core/Utils/String.hpp` — `toLower`,
  `isPunctuation`, `isAlphanumeric`, `toStr`/`toNum`/`toInt`/`toFloat` equivalents.
- `extlibs/SSVStart/include/SSVStart/Utils/Vector2.hpp` — the vector helpers:
  `getOrbitRad`, `getVecFromRad`, `getNormalized`, `getMagSquared`, `getRad`,
  `getDistEuclidean`, `zeroVec`. Provide a `Vec2` type (`f32` fields `x`,`y`)
  mirroring `sf::Vector2f` usage.
- `include/SSVOpenHexagon/Utils/Color.hpp` — `getColorFromHue`, `transformHue`, and any
  hue math (port the formulas verbatim).
- `include/SSVOpenHexagon/Utils/Geometry.hpp` + `src/SSVOpenHexagon/Utils/Geometry.cpp` — `getLineCircleIntersection` and
  friends (exact return semantics: the int code and out-params).
- `include/SSVOpenHexagon/Utils/Easing.hpp` — every curve used, verbatim.
- `include/SSVOpenHexagon/Utils/MoveTowards.hpp`, `Split.hpp`, `String.hpp`,
  `Concat.hpp`, `Timestamp.hpp` + `src/SSVOpenHexagon/Utils/Timestamp.cpp` (`formatTimepoint` uses `put_time` with
  `%Y%m%d_%H%M%S`), `BuildPackId.hpp` + `src/.../BuildPackId.cpp`, `LevelValidator.hpp`.

For each item: if the game does not use it, do not port it; say so in the report.

### 4. `tools/gate.sh`

A shell script that runs, in order, with output to a log and a clear exit status
(never pipe to `tail`):

```
cargo fmt --all -- --check
cargo build --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Print `GATE_EXIT=$?` for each stage; exit non-zero if any fails. Make it executable.

### 5. Skills

- `.agents/skills/lib-pcg.md` — the PCG engine, written from
  `pcg_random.hpp`: exact typedefs, multiplier, increment, seeding, `advance`,
  output function, and the `uniform_*` distributions used. Include the oracle
  vectors as the pin.
- `.agents/skills/lib-ssvutils.md` — the SSVUtils subset ported (Frametime, Math,
  String, Containers), exact signatures/semantics quoted from source.
- Update `.agents/skills/README.md` index rows for both.

## Verification (run these, paste the output in your report)

1. `bash tools/gate.sh` — must end green. Paste `GATE_EXIT=` lines.
2. `cargo test -p oh-rng -- --nocapture` — show the oracle-vector tests passing.
3. A **discrimination check**: temporarily change the PCG multiplier in a scratch
   copy (do NOT modify the reviewed tree) and confirm the oracle test fails. Report
   what you observed. (If you cannot do this safely, state the one-line experiment
   you would run and why it would fail.)
4. `cargo build --workspace --target x86_64-pc-windows-gnu` if the target is
   installed (`rustup target list --installed`); if not, say so.

## Report format

- **Module map**: files created, one line each.
- **Design summary**: how `Vec2`/`FT`/the RNG are represented; any deviation.
- **Gate outputs**: the `GATE_EXIT=` lines and the RNG test output.
- **Skill corrections**: any, with the real-source quote.
- **Deviations**: anything not done as specified, and why.
- **Known gaps**: what later issues must add.
