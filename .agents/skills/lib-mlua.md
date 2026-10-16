# lib-mlua — mlua (Rust)

Pinned version: **mlua 0.10.5** with features `luajit`, `vendored` (`crates/oh-lua/Cargo.toml`), backend **luajit-src 210.5.12+a4f56a4** (LuaJIT 2.1.0-beta3, built from vendored C — `Cargo.lock`). Read from checked-out source:

`~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/mlua-0.10.5/src/`

Companion skill: `.agents/skills/lib-luajit.md` (the C backend's observable behavior; both skills were written against the same pinned source).

## `Lua` state (`src/state.rs`)

- `Lua::new()` → `Lua` (panics on allocation failure — NOT `Result`). Loads the safe stdlib set: on `luajit` that is every `luaL_openlibs` library **except `debug` and `ffi`**. The C++ build compiles LuaJIT with `LUAJIT_DISABLE_FFI`, and `oh_lua::destroy_malicious_functions` clears `debug` — post-sandbox stdlib surface is identical (see `crates/oh-lua/src/context.rs` `new()` docs).
- `Lua` is **`Clone` but not `Sync`** — each `Lua` handle is one thread's view of one state, exactly like the C++ raw `lua_State*`. `oh_lua::LuaContext` is likewise single-threaded by contract.
- `lua.globals()` → `Table` view of `_G`.
- `lua.create_function(|lua: &Lua, arg| -> Result<...>)` → `Function`. The closure ALWAYS takes the state as first argument; return values may be a bare `Ok(value)` or `Ok(MultiValue)` for multi-return (see `src/function.rs`).
- `lua.create_table()` / `lua.create_string(bytes)` → `Table` / `mlua::String`. `mlua::String` is interned (like a C++ `lua_pushstring` copy); `s.to_string_lossy()` decodes.
- `lua.load(code)` → `Chunk`; `.set_name("chunk")` sets the chunk name **before** the first eval (C++ uses the literal `"chunk"`); `.eval::<T: FromLuaMulti>()` loads AND runs under a protected `pcall` (this is the `executeCode`/`executeCode<T>` primitive).
- `Table::get::<V: FromLua>(key)` / `Table::set(key, value)` / `Table::len()` → `Result<Integer>` (1-based array length, same as C++ `lua_objlen` on the array part).
- `Value::to_string()` (e.g. `value.to_string()`) for `Number` goes through **LuaJIT's Lua-side `tostring`** (up to 17 significant digits: `1e20` → `"100000000000000000000"`). That is NOT the 5.1 C-API spelling — see "Number formatting" below and `crates/oh-lua/src/convert.rs` `lua_number2str`.

## `Value` (`src/value.rs:36-60`)

```rust
pub enum Value {
    Nil,
    Boolean(bool),
    Integer(i64),
    Number(f64),
    String(String),      // mlua::String
    Table(Table),
    Function(Function),
    Thread(Thread),
    LightUserData(*const c_void),
    UserData(UserData),
    TableMetatable(Table),
}
```

`as_*` accessors and `type_name()` (`Value::Integer(_)` → `"integer"`, `Number` → `"number"`).

## Error mapping (`src/error.rs`)

`mlua::Error` variants relevant to `oh_lua::LuaError` (mapping at `crates/oh-lua/src/error.rs` `from_mlua`):

- `Error::SyntaxError { message, location }` — chunk compile/parse failure → `LuaError::SyntaxError`.
- `Error::RuntimeError(String)` — a `pcall`ed Lua error object, i.e. what `error('x')` produced → `LuaError::ExecutionError { message }`. **LuaJIT appends a `stack traceback:` section to the error object inside `error()` itself** (so the payload is `x\nstack traceback:\n...` — the C++ build shows the same string; see `lib-luajit.md`).
- `Error::MemoryError`, `Error::StackOverflow`, `Error::CallbackError`, `Error::ExternalError`, `Error::FromLuaConversionError { from, to, message }` (e.g. `get::<T>` type mismatch), `Error::AssertFailed` (from `luaL_check*`), `Error::IncompatibleUserData`, ...

## Number formatting (parity-relevant)

The C++ baseline reads numbers with the 5.1 C API: `lua_tointeger`/`lua_tonumber`/`lua_tostring` where `lua_tostring` = `lua_number2str` = **C `%.14g`** (`LUA_NUMBER_FMT`, `luaconf.h`; `lib-luajit.md`). mlua's own `Value::to_string()` instead uses LuaJIT's Lua-side `tostring` (17-significant-digit form). `oh-lua` therefore re-implements `%.14g` in Rust (`crates/oh-lua/src/convert.rs` `lua_number2str`, unit-tested against known C spellings) and uses it in `read_string`. **Do not** route number→string conversion through `value.to_string()` if C++ parity matters.

Coercion tables in `oh_lua` (matching C++ `_read`, `LuaWrapper.hpp:930-1000`):

| Lua value | int read | float read | bool read | string read |
|---|---|---|---|---|
| nil | 0 | 0.0 | false | `WrongType` (C++ was UB) |
| 0 / any number | `T((i64)f)` truncate toward zero | `T(f)` | **true** (5.1: only nil/false falsy) | `%.14g` |
| "41" / "0x1A" | `tonumber` truncate | `tonumber` | true | verbatim |
| true / false | 1 / 0 | 1.0 / 0.0 | as-is | "true" / "false" |
| userdata (any) | `WrongType` | `WrongType` | `WrongType` | `WrongType` |
| table / function / thread | 0 | 0.0 | true | `WrongType` |

String→number parsing (`convert.rs` `parse_lua_number`) is Lua 5.1 `tonumber`: decimal with `inf`/`nan` spellings, and `0x`/`0X` hex (signed), NOT Rust `f64::from_str` (which would accept `inf`, reject `0x1A`).

## `FunctionToPush` port (C++ quirk, preserved)

`LuaContext::add_lua_fn` (`context.rs`) reproduces the C++ call semantics of `FunctionToPush::operator()` (`LuaWrapper.hpp:795-830`), including:

- C++ checks `lua_gettop(state) < count` where the **callee itself** occupies a stack slot → a call with `count - 2` args raises `this function requires at least {count} parameter(s)`; a call with exactly `count - 1` args makes the last read hit the callee (userdata) → `Trying to cast a lua variable to an invalid type`.
- A call with *extra* args reads the **last** `count` slots (`lua_gettop - count .. -1`), not the first — `add_lua_fn` mirrors this window.
- Zero-arg functions: `count == 0` — both guards must be skipped (usize `count - 1` underflows).
- `Ret: PushValue` maps `void`→`MultiValue::new()`, scalars→single value, tuples→`MultiValue::from_vec` (multi-return, C++ `_push(Ts&&...)`).

`Args: ArgTuple` (`context.rs` after the `()` impl): implemented for `()`, tuples of `FromLuaValue` up to 17 elements (largest game signature `cw_setVertexColor4`), **and the scalar `FromLuaValue` types** so a single-arg closure can be written `|a: i32|` rather than `|(a,): (i32,)|`.

## What NOT to do

- Don't hold a `Table`/`Function` across a second eval from another thread — the state is one-thread-owned (C++ parity).
- Don't call `mlua::Value::to_string()` for number→string where C++ `lua_tostring` output matters (formatting divergence above).
- `Lua::new()` panics on OOM; `oh_lua::LuaContext::new()` documents that as the fatal-creation contract (like a C++ `bad_alloc` from `lua_newstate`).

## WASM / web (the D-028a gate)

Phase 0 (issue 26, 2026-10-16) ran the Luau go/no-go (DECISIONS.md D-028a) on
both installed wasm targets. The script/backend is **not** the blocker — the
**wasm C++ toolchain** is. Versions read: `mlua-sys 0.6.8`
(`~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/mlua-sys-0.6.8/`,
build scripts `build/main_inner.rs` + `build/find_vendored.rs` + `Cargo.toml`),
`luau0-src 0.12.3+luau663` (registry; **not** in `Cargo.lock`, since
`oh-lua` uses `luajit`+`vendored` — it is pulled only when the `luau` feature
is on), `cc 1.8.0` (`Cargo.lock:230-232`), `getrandom 0.2.17`
(`Cargo.lock:815-819`), `discord-rich-presence 1.1.0` (`Cargo.lock:437-439`),
`uuid 0.8.2` (`Cargo.lock:3070-3074`).

- **`wasm32-unknown-unknown`, `vendored` OFF and ON → both FAIL:**
  `mlua-sys ... cc-rs: failed to find tool "clang++"` (via
  `luau0-src 0.12.3+luau663`). `vendored` is a **no-op for Luau**:
  `mlua-sys/build/main_inner.rs` routes
  `cfg(any(feature = "luau", feature = "vendored"))` to `find_vendored.rs`, and
  `mlua-sys/Cargo.toml` has `vendored = ["lua-src", "luajit-src"]` (does **not**
  gate `luau0-src`) while `luau = ["luau0-src"]` — so `find_vendored.rs`
  builds Luau from `luau0_src::Build::new()...build()` (a `cc` C++ compile)
  regardless of `vendored`. Enabling `vendored` changes nothing for Luau.
- **`wasm32-wasip1` (installed) with a host clang 22.1.8** (which does list a
  `wasm32` backend) → still FAILS at the definitive point:
  `fatal error: 'iterator' file not found` compiling
  `luau/Ast/include/Luau/Ast.h` via `clang++ --target=wasm32-wasip1 -std=c++17`
  — i.e. **no wasm C++ standard library** (no libc++/wasi-libc C++ headers).
- **Native control (`cargo run`) → PASS** (`third=0.33333333333333`, table loop
  `sum=20`): the script/backend is fine; only the wasm C++ toolchain is missing.
- **`oh-lua` transitive blockers (on `wasm32-unknown-unknown`):**
  (1) `getrandom 0.2.17` `compile_error!` at
  `getrandom-0.2.17/src/lib.rs:346` — "the wasm\*-unknown-unknown targets are
  not supported by default, you may need to enable the `js` feature" — reached
  via `oh-lua → oh-game → oh-discord → discord-rich-presence 1.1.0 →
  uuid 0.8.2 → getrandom 0.2.17`; (2) `libz-sys` clang (via `oh-replay`'s
  flate2 `zlib` feature — see `.agents/skills/lib-flate2.md`); plus a latent (3):
  `lua-src`/`luajit-src` (LuaJIT C) would need a wasm C compiler.

**Toolchain requirement (the bottom line):** Luau **cannot** build on
`wasm32-unknown-unknown` at all (no libc/stdlib; would require
**emscripten/emsdk**). The viable path is **`wasm32-wasip1` + wasi-sdk**
(clang++ + wasi-libc + libc++). In this environment: node v24.13.0 (node:wasi
present); `wasmtime`/`wasmer` **not** installed; `emcc`/`emsdk`/`wasi-sdk`
**not** installed; host clang 22.1.8 at `C:\Program Files\LLVM`. wasi-sdk
prebuilt releases were **not reachable here** (an environment limitation, not a
code limitation) — so the Luau gate is **toolchain-gated**, not yet PASS/FAIL on
compatibility. See `.agents/skills/lib-web-stack.md` for the full matrix.
