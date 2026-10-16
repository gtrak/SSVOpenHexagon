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

## WASM / web (the D-028a gate) — **PASS on `wasm32-unknown-emscripten`**

The Luau go/no-go (DECISIONS.md D-028a) is **PASS** — verified on
`wasm32-unknown-emscripten` (2026-10-16, independently verified by the
supervisor). The earlier note ("Luau does NOT build for wasm / needs
wasi-sdk") is **superseded**: the working target is **emscripten**, NOT
`wasm32-unknown-unknown` or `wasm32-wasip1`.

**Evidence (the spike):** a throwaway crate (record the *recipe*, not the temp
path) with `mlua = { version = "0.10", features = ["luau", "vendored"] }` + a
`main.rs` that runs a Luau script (`lua.load(<source string>)` from an
in-memory chunk). `cargo build --target wasm32-unknown-emscripten` (clean ≈30 s)
then `node target/wasm32-unknown-emscripten/debug/luau-emsdk-spike.js` printed:

```
PASS: luau spike ok; continue_sum=12 floordiv(7/2)=3 sort='1,2,3,5,8,9' fmt='07-x-3.5' pcall_err=[string "src\main.rs:95:21"]:56: boom double(21)=42
```

and exited **0**. Debug `.wasm` ≈ 15,732 KB. The negative path (a failing
assert) prints `FAIL: ...` and exits 1.

**Versions:** mlua 0.10.5 (`features = ["luau","vendored"]`), mlua-sys 0.6.8,
luau0-src 0.12.3+luau663, rust 1.97.1, target `wasm32-unknown-emscripten`;
emsdk 6.0.12 (emcc 6.0.12, clang 24, node 24.19.0, python 3.13.3).

**Mandatory build recipe** (every env var is required):

- `EMSDK=<emsdk>`, `EM_CONFIG=<emsdk>\.emscripten`
- `CC_wasm32_unknown_emscripten=<emsdk>\upstream\emscripten\emcc.exe`
- `CXX_wasm32_unknown_emscripten=<emsdk>\upstream\emscripten\em++.exe`
- `AR_wasm32_unknown_emscripten=<emsdk>\upstream\bin\llvm-ar.exe`
- `CXXFLAGS_wasm32_unknown_emscripten=-fwasm-exceptions` ← **the key one**
- `PATH` must contain a dir with `emcc.bat`/`em++.bat` **name shims** (rustc's
  emscripten link step and the `cc` crate's archiver look for the `.bat` names;
  emsdk 6.0.12 ships only `emcc.exe`/`em++.exe`) **plus**
  `<emsdk>\upstream\emscripten`, `<emsdk>\upstream\bin`, `<emsdk>\node\...`
- crate `.cargo/config.toml`:

  ```toml
  [target.wasm32-unknown-emscripten]
  rustflags = ["-C", "link-arg=-sDEFAULT_TO_CXX"]
  ```

- `mlua = { version = "0.10", features = ["luau", "vendored"] }` — `vendored`
  is **REQUIRED** for wasm32 (`mlua-sys/build/find_normal.rs` panics without it).

**Why `-fwasm-exceptions` is mandatory:** rustc hard-links the emscripten
target with `WASM_EXCEPTIONS` (`-fwasm-exceptions`; it is an internal emcc
setting, NOT overridable from the CLI — `-sWASM_EXCEPTIONS=0` is rejected),
while `luau0-src` compiles Luau C++ with a bare `-fexceptions` (it has an
explicit emscripten branch adding exactly that) which in clang 24 selects
**legacy JS exception handling** → undefined `__resumeException` /
`llvm_eh_typeid_for` at link. Compiling all C++ with `-fwasm-exceptions`
aligns it with the link. **Treat this as the rule for ALL C++ in this target.**

**Staleness trap:** `mlua-sys`'s build script declares no
`rerun-if-env-changed` for the compiler env vars → after changing
`CXXFLAGS_wasm32_unknown_emscripten` you must run `cargo clean -p mlua-sys`
once (else stale JS-EH objects resurface as link errors).

**Caveats (record all):**

- Luau **string interpolation** `` `{x}` `` is **INERT** — `LANG_STRING_INTERPO`
  is not enabled in the vendored Luau (verified: `"hello {name}!"` stayed
  literal). **Immaterial for the current corpus**: a repo-wide scan of the 42
  shipped `.lua` files found NO interpolation (backticks appear only inside
  comments).
- `lua.load(<source string>)` (in-memory) **WORKS** — that is the path we need
  (`.lua` bytes from the vfs). `load_from_file`/file ops are assumed broken
  (mlua's own `// TODO: figure out why emscripten fails on file operations`).
- JIT/codegen is excluded (`luau0-src` panics on emscripten if codegen is
  enabled) — OH does not use it.
- This proves Luau runs on the emscripten target under **node**; running the
  same artifact **in a real browser** is still unproven (a 26c item).

See `.agents/skills/lib-web-stack.md` for the broader web-stack matrix.
