# lib-luajit — LuaJIT (C backend of the port)

## Two pins, one runtime

- **C++ baseline**: `vittorioromeo/LuaJIT @ 4dc5ffbf60b16e54274c470fcc022165685c96fe` (CPM, `CMakeLists.txt:123-134`), built with `LUAJIT_DISABLE_FFI=ON`. This is a fork of **LuaJIT 2.1** exposing the **Lua 5.1 C API**.
- **Rust port**: `mlua 0.10.5` with `luajit`+`vendored` → **luajit-src 210.5.12** (upstream LuaJIT 2.1.0-beta3, `Cargo.lock`). Compiled from C at build time (`cc` + no system library).

Both are LuaJIT 2.1 in 5.1-compat API mode; the observable behaviors below hold for both (the Rust-side facts were verified empirically through the vendored build — see `crates/oh-lua/tests/runtime.rs`).

## Numbers are all doubles

- No integer subtyping in 5.1 mode: `type(7)` → `"number"`, never `"integer"`. `Value::Integer` in mlua is a Rust-side representation; the VM stores a double.
- Consequence: a `lua_pushinteger` / `Value::Integer` round-trip is exact **only for values representable in 53-bit mantissa**. Verified: `i64::MAX/4` (= 2305843009213693951) comes back as `...952`. This is C++ parity — the game's `writeVariable(i64)` loses the same bits.
- `lua_tointeger` truncates toward zero (C cast); `lua_tonumber` on an integer value widens to double.

## Number formatting — TWO spellings exist

- **C API** `lua_tostring` on a number = `lua_number2str` = `snprintf("%.14g")` (`LUA_NUMBER_FMT` / `LUA_PREC 14` in `luaconf.h`). Examples: `1e20` → `"1e+20"`, `0.5` → `"0.5"`, `1234567.89` → `"1234567.89"`, `0.0001` → `"1e-04"`, `9999999999999.999` → `"100000000000000"`. This is what the C++ `readVariable<std::string>`/`FunctionToPush` string reads produce — and what `oh_lua::read_string` reproduces (`convert.rs` `lua_number2str`, unit-tested).
- **Lua-side** `tostring(1e20)` (the builtin) prints up to **17 significant digits**: `"100000000000000000000"`. mlua's `Value::to_string()` uses this path. Don't mix the two when C++ parity matters.
- `string.format('%.14g', n)` IS the C spelling (C library underneath) — an alternative way to obtain it in-Lua.

## 5.1 language facts (things 5.2+ intuition gets wrong)

- Truthiness: **only `nil` and `false` are falsy** — `0`, `""`, empty table all read as `true` through `lua_toboolean`.
- No `lightuserdata()` constructor (that's 5.2+); make a userdata with `newproxy(false)`. `lua_isuserdata` (C++ pre-check in every `_read`) catches both full and light userdata.
- `tonumber("0x1A")` → `26` (5.1 hex literals); `tonumber("inf")` → `inf` (5.1 spellings; modern `inf`/`-inf`/`nan`).
- `error('x')` appends `"\nstack traceback:\n\t[C]: in ?\n..."` to the error object itself (LuaJIT behavior) — so a C++ `executeCode` failure message for `error('x')` is NOT bare `"x"`. Tests must match on the prefix.
- `io` has the 5.1 set **plus LuaJIT extensions**: `io.popen`, `io.setvbuf` exist (the sandbox removes them; `lib-mlua.md` / `LuaScripting.cpp` teardown). `utf8` does not.
- No `integer` type, no goto, no bitwise operators in the 5.1 language (only the `bit.*` library, if built in).

## File I/O / text mode (test-platform note)

`io.open(filename, mode)`: text vs binary mode (`"r"` vs `"rb"`) differs **only on Windows** (CRLF translation is a C-filesystem thing, absent on POSIX). On Linux/macOS both read identical bytes — so a parity test for the `redefineIoOpen` coercion must observe the *mode string routing*, not byte translation (`tests/runtime.rs` `io_open_mode_is_passed_through_or_coerced` does this with a spy wrapper).

## Sandbox-relevant stdlib surface (post-teardown)

The C++ `hg::LuaScripting::init` (`src/SSVOpenHexagon/Core/LuaScripting.cpp:1585+`) applies, in order, after registering the `u_rnd*` shims:

1. `redefineIoOpen` (lines 141-154) — verbatim:
   ```lua
   local open = io.open; io.open = function(filename, mode) return
   open(filename, mode == "rb" and mode or "r"); end
   ```
   (any mode except exactly `"rb"` becomes `"r"` — read-only text).
2. `redefineRandom` (156-175) — routes `math.random` through `u_rndSwitch(0|1|2, ...)`.
3. `destroyMaliciousFunctions` (178-218) — clears, in order: `os`, `io.popen`, `io.flush`, `io.write`, `io.setvbuf`, `debug`, `math.randomseed`, `package.loadlib`, `package.searchpath`, `package.loaded.os`, `package.loaded.debug`, `package.loadlib` (again — deliberate double), `package.loaders`.

Result: `os`/`debug` gone; `io` is a read-only subset; `math.random` is the engine-RNG shim; `package` keeps `require` + `package.path` working (the C++ comment explains `require` needs the surviving parts). `oh_lua::sandbox` ports all three verbatim (`crates/oh-lua/src/sandbox.rs`).

## Dotted-name semantics (`_getGlobal`/`_setGlobal`, `src/SSVOpenHexagon/Utils/LuaWrapper.cpp:96-220`)

- `_getGlobal("a.b")`: split on dots; each step after the first requires the current stack value to be a table (else `VariableDoesntExistException(mVarName)` — the **full dotted name**); after fetching **every** step (including the last) a nil value throws `VariableDoesntExistException(mVarName)`. So in the C++ original, `doesVariableExist("t.u")` on a missing `u` **throws** (it calls `_getGlobal`), it does not return false. Plain (dot-free) names never throw: missing → `nil` pushed.
- `_setGlobal("a.b.c")`: resolves the table part `"a.b"` via `_getGlobal` (missing/non-table table part throws the table-part name), then requires it to be a table (else `VariableDoesntExistException` with the **full name** `"a.b.c"`), then `lua_settable`. Intermediate tables are **not** created.
- `oh_lua` mirrors all of this: `get_path_value`/`set_path` in `context.rs`; `does_variable_exist` throws on dotted paths exactly like the C++.
