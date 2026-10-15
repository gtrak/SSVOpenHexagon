# Task spec — issue 006: `oh-lua` runtime (LuaContext analogue, conversions, sandbox)

You are a worker. Implement exactly this issue. Do not redesign or widen scope.

## Context

Repo: `/home/gary/dev/SSVOpenHexagon`. Literal 1:1 Rust port. Issues 001–004
(workspace/`oh-math`/`oh-rng`/`oh-replay`/`oh-data`/`oh-config`) are LANDED;
`bash tools/gate.sh` passes 4/4.

Read first:
1. `.agents/plans/001-rust-port/PLAN.md`, `DECISIONS.md`
2. `.agents/plans/001-rust-port/06-lua-runtime.md` (the contract)
3. `.agents/plans/001-rust-port/mapping/lua-api.md` §3 (conversion rules),
   §4 (RNG mapping), §8 (subtleties) — the authoritative transcription
4. `include/SSVOpenHexagon/Utils/LuaWrapper.hpp` + `src/.../LuaWrapper.cpp`
   (the exact C++ binding mechanism)
5. `src/SSVOpenHexagon/Core/LuaScripting.cpp` — the non-API parts:
   `redefineIoOpen`, `redefineRandom`, `destroyMaliciousFunctions`
6. `.agents/skills/README.md`

## Working agreement

- **Skills are truth.** Write `.agents/skills/lib-luajit.md` (the LuaJIT version
   used, from `CMakeLists.txt`'s pin, plus every raw Lua C API call the wrapper
   makes) and `.agents/skills/lib-mlua.md` (from the crate's REAL source under
   `~/.cargo/registry/src/**/mlua-*/`). Update the skills README index.
- **Write first**: scaffold `crates/oh-lua` immediately, compile early.
- **Exactness**: the wrapper's conversion rules (int/float/double/bool/string,
   tables, nested names) and the sandbox teardown must match the C++.
- **Commit at the first green build.**

## What to build

1. A `LuaContext` analogue over `mlua`: `execute_code`, `write_variable`
   (including nested `"a.b.c"` creating intermediate tables), `clear_variable`,
   `read_variable::<T>` with the path walk.
2. The exception types and error mapping (`ExecutionError`,
   `VariableDoesntExist`, `SyntaxError`, `WrongType`).
3. The conversion helpers for the types the game binds (ints, floats, doubles,
   bools, strings, arrays/tables).
4. The metadata collector (`LuaMetadata` / the `addLuaFn` doc/arg builder) so
   issue 14's `printDocs` and issue 07's registrations work.
5. The sandbox: `destroy_malicious_functions` (clear `os`, `io.popen/flush/write/
   setvbuf`, `debug`, `math.randomseed`), `redefine_io_open`, `redefine_random`
   (Lua snippets verbatim — they call `u_rndSwitch`).

## Verification

1. `bash tools/gate.sh` — paste all `GATE_EXIT=` lines.
2. Conversion round-trips both directions for each type.
3. `execute_code("error('x')")` raises the execution error with the Lua message.
4. `read_variable("nope")` raises `VariableDoesntExist`.
5. After the sandbox runs, `os` is nil and `io.write` is nil.
6. `math.random()` routes to an injected `u_rndSwitch` stub (assert the args).
7. `io.open(name, "rb")` passes the mode through unchanged.

## Report format

Module map, design summary, gate outputs, skill corrections, deviations, known gaps.
