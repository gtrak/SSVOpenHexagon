# Issue 06 — `oh-lua` runtime: LuaContext analogue, conversions, sandboxing

## Objective

Port the Lua scripting *runtime* layer: the `Lua::LuaContext` analogue from
`Utils/LuaWrapper.*`, the type-conversion rules, error/exception mapping, and the
sandboxing setup (`LuaScripting.cpp` non-API parts: `redefineIoOpen`,
`redefineRandom`, `destroyMaliciousFunctions`) so that issue 07 can register the
~140 game functions.

## Key decisions

- Lua backend (D-004 Lua entry): **`mlua`** with the `luajit` feature (vendored),
  matching the C++ `LuaJIT` pin. If `mlua`'s LuaJIT feature cannot be vendored on
  Windows/Linux cleanly, fall back to PUC-Lua 5.1 semantics and document the
  divergence — LuaJIT is required for byte-identical `math`-free behavior only if
  scripts rely on it; re-check before falling back.
- Model the C++ API shape: a `LuaContext` struct with
  `execute_code(&str)`, `write_variable(name, value)`, `read_variable::<T>(name)`,
  nested-name access (`"a.b.c"`), `clear_variable(name)`, and the three exception
  types (`ExecutionErrorException`, `VariableDoesntExistException`,
  `SyntaxErrorException`, `WrongTypeException`).
- Conversion rules must match `LuaWrapper.hpp` exactly for
  int/float/double/bool/string/tables/vectors/colors, including how Lua numbers
  cross as `float` vs `double`.
- `addLuaFn` + `LuaMetadataProxy` + `LuaMetadata`: port the metadata collector so
  `-printLuaDocs` (issue 14) works, but the registration itself is issue 07.

## Files

| File | Change |
|---|---|
| `crates/oh-lua/Cargo.toml` | new — deps `mlua` (luajit, vendored) |
| `crates/oh-lua/src/{context,convert,error,metadata,sandbox}.rs` | new |
| `crates/oh-lua/tests/runtime.rs` | new — conversion + sandbox tests |
| `.agents/skills/lib-luajit.md` | new — from the real LuaJIT source + LuaWrapper usage |
| `.agents/skills/lib-mlua.md` | new — from the crate's real source |

## Steps

1. Port the exception types and the `_load`/`_call` mechanism (`executeCode` with
   and without a return value).
2. Port `writeVariable` (including nested keys `"a.b"` creating intermediate
   tables) and `clearVariable`.
3. Port `readVariable<T>` with the nested-name path walk and the exact exceptions.
4. Port the conversion helpers from `LuaWrapper.hpp` (`_push`, `_get`, arrays,
   tables, `std::vector`, `sf::Vector2f`, `sf::Color` if bound).
5. Port sandboxing: `destroyMaliciousFunctions` (clear `os`, `io.popen/flush/write/
   setvbuf`, `debug`, `math.randomseed`?), `redefineIoOpen`, `redefineRandom`
   (Lua snippets verbatim — they call `u_rndSwitch`).
6. Port `LuaMetadata` / `LuaMetadataProxy` (arg names, docs, `TypeWrapper`
   signature extraction equivalent — in Rust use an explicit builder since there is
   no template reflection).

## Verification

- Conversion round-trip tests for each type in both directions.
- A test that `executeCode("error('x')")` raises `ExecutionErrorException` with the
  Lua message.
- A test that `readVariable("nope")` raises `VariableDoesntExistException`.
- A test that after `destroyMaliciousFunctions`, `os` is nil and `io.write` is nil.
- A test that `math.random()` routes to the injected `u_rndSwitch` (inject a stub).
- A test that `io.open(name, "rb")` passes mode through unchanged (the redefine).
- `tools/gate.sh` green.

## Notes / risks

- The C++ uses `lua_exception` handlers and prints/logs failures; keep the logging
  surface (`ssvu::lo("...")` → a Rust logger) so issue 07's `luaExceptionLippincottHandler`
  can reuse it.
- `mlua`'s `Lua` is `!Sync`; the game references the context from a single thread —
  mirror that (no cross-thread sharing), and note it.
- Nested-variable semantics: verify whether `writeVariable("a.b")` overwrites or
  merges; copy the C++ exactly.
