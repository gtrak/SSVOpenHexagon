# lib-luajit — LuaJIT (the C++ baseline VM) and Luau (the port VM, D-037)

## The two VMs (D-037, 2026-10-17 — the unification)

- **C++ baseline** (unchanged — the C++ tree still runs its own LuaJIT; this is a
  C++-side fact, not a build input of the port):
  `vittorioromeo/LuaJIT @ 4dc5ffbf60b16e54274c470fcc022165685c96fe` (CPM,
  `CMakeLists.txt:123-134`), built with `LUAJIT_DISABLE_FFI=ON`
  (CMakeLists.txt:127). Despite the repo name, the pin is **not a fork with
  special commits** — every commit in that tree (including the ARM64 fixes and
  the CMake support) exists in upstream `LuaJIT/LuaJIT`; it is plain
  **upstream v2.1 as of 2023-04-02** + CMake (029k1 verified by commit-level
  diff; see the K3 row in `deviations.md`).
- **Rust port (D-037)**: **Luau** — `mlua 0.10.5` on the `luau` feature,
  statically compiled from source by `luau0-src 0.12.3` (Luau 0.663). The
  port once ran a second VM (upstream LuaJIT @ `a4f56a4`, 2025-01-13, vendored
  as `extlibs/luajit-src` + `[patch.crates-io]` + the `ffi_stub.c` link shim +
  the Android `cshim.c` `__clear_cache` shim + the per-target
  `backend-luajit`/`backend-luau` selection) — D-037 **deleted all of it**
  (lane 030d). The C++ parity guarantee holds because the oracle fingerprints
  **game state, not Lua internals**, and 030c proved the shipping
  configuration against the committed C++ column (3627/3627 frames,
  `crates/oh-parity/tests/parity_cpp_fingerprints.rs`, re-proven per gate).
- The measured dialect differences between the two VMs (number spellings,
  error chunk prefixes, sandbox surface, integer width, `pairs` order) are
  D-037's **named residual risks** — pinned by the gate's probe pins
  (`crates/oh-parity/tests/lua_vm_probes.rs`), the dialect census
  (`dialect_census.rs`) and the prelude tests (`lua51_compat_prelude.rs`);
  the pre-unification A/B table lives in
  `.agents/plans/001-rust-port/30a-lua-parity.md` §3 (the historical record).

## The port's 5.1 compat surface (D-037)

Luau's base library drops the seven 5.1-isms third-party pack scripts could
rely on (`setfenv`, `getfenv`, `loadstring`, `unpack`, `newproxy`, `module`,
the `collectgarbage("gcinfo")` option). This Luau build (0.663) turns out to
have most of them **natively** (`unpack` re-installed as a 5.1-compat global;
`getfenv`/`setfenv` with real environment semantics; `newproxy`; the global
`gcinfo()`) — 030c classified each ism against the vendored source and the
running VM. The port ships the remainder:

- **`loadstring` — not emulatable at Lua level**: Luau's base library has no
  chunk compiler at all (no `load` either). The one-line Rust binding in
  `LuaContext::new` (`crates/oh-lua/src/context.rs`) exposes mlua's own
  `Lua::load` as the global `load` in EVERY context (the C++'s LuaJIT has
  `loadstring` everywhere — dropping it would be a parity regression for
  third-party content).
- **the 5.1 compat prelude** (`crates/oh-lua/lua51_prelude.lua`, 030c's file,
  installed by `api::init_lua_api` via `install_lua51_prelude`): the
  `loadstring = load` alias, the `collectgarbage` wrapper (adds the 5.1
  `"gcinfo"` option, forwards the native options unmodified), and the
  `module` shim (defining-chunk redirect via the native `setfenv`, read side
  via `__index = _G` — a deliberate lenient superset of 5.1's self-index).
  Every shim guards on the target global's absence (install-nothing property,
  idempotent). Pinned end to end by
  `crates/oh-parity/tests/lua51_compat_prelude.rs`
  (`loadstring('return 6*7')() == 42`).

## Numbers are all doubles (C++ LuaJIT, 5.1-compat mode)

- No integer subtyping in 5.1 mode: `type(7)` → `"number"`, never `"integer"`.
  `Value::Integer` in mlua is a Rust-side representation; the VM stores a
  double.
- Consequence: a `lua_pushinteger` / `Value::Integer` round-trip is exact
  **only for values representable in a 53-bit mantissa**. Verified:
  `i64::MAX/4` (= 2305843009213693951) comes back as `...952`. This is C++
  parity — the game's `writeVariable(i64)` loses the same bits.
- `lua_tointeger` truncates toward zero (C cast); `lua_tonumber` on an
  integer value widens to double.
- **Luau (the port VM) differs**: `lua_Integer` is 32-bit (`i32`) — integers
  wider than 31 bits saturate on writes/reads through the Lua boundary
  (pinned by the `mlua_integer_width` probe: `i32::MAX+1` and `2^62` both
  round-trip as `2147483647`); the engine's score/seed paths use f32/f64 and
  i64 *in Rust*, so the shipped corpus proves no divergence (D-037 residual
  risk 2).

## Number formatting — TWO spellings exist (the C++-side fact, still the parity pin)

- **C API** `lua_tostring` on a number = `lua_number2str` =
  `snprintf("%.14g")` (`LUA_NUMBER_FMT` / `LUA_PREC 14` in `luaconf.h`).
  Examples: `1e20` → `"1e+20"`, `0.5` → `"0.5"`, `1234567.89` →
  `"1234567.89"`, `0.0001` → `"1e-04"`, `9999999999999.999` →
  `"100000000000000"`. This is what the C++ `readVariable<std::string>` /
  `FunctionToPush` string reads produce — and what `oh_lua::read_string`
  reproduces (`convert.rs` `lua_number2str`, unit-tested).
- **Lua-side** `tostring(1e20)` (the builtin) prints up to **17 significant
  digits**: on LuaJIT `"100000000000000000000"`; on Luau the same value also
  spells `"100000000000000000000"` but wide literals PARSE differently
  (`9999999999999.999` → LuaJIT `1e13`, Luau `…9999999999999.998` — 030a
  measured; a value difference, not just a spelling one). Don't mix the two
  when C++ parity matters.
- `string.format('%.14g', n)` IS the C spelling (C library underneath) — and
  030a measured it **byte-identical on both VMs** (both C-printf-backed); it
  is pinned in the gate (`lua_vm_probes.rs` `probe_pins`), as is the port's
  number→string path (the two probe paths that can reach game-visible text).

## 5.1 language facts (things 5.2+ intuition gets wrong — the C++ baseline)

- Truthiness: **only `nil` and `false` are falsy** — `0`, `""`, empty table
  all read as `true` through `lua_toboolean`. (Luau agrees on the port's
  conversion path: `oh_lua::convert` pins it.)
- No `lightuserdata()` constructor (that's 5.2+); make a userdata with
  `newproxy(false)`. `lua_isuserdata` (C++ pre-check in every `_read`) catches
  both full and light userdata.
- `tonumber("0x1A")` → `26` (5.1 hex literals); `tonumber("inf")` → `inf`
  (5.1 spellings; modern `inf`/`-inf`/`nan`).
- `error('x')` appends `"\nstack traceback:\n\t[C]: in ?\n..."` to the error
  object itself (LuaJIT behavior) — so a C++ `executeCode` failure message
  for `error('x')` is NOT bare `"x"`. Tests must match on the prefix.
  **Luau (port VM) diverges**: it prefixes the chunk location
  (`[string "chunk"]:1: x`) — D-037 residual risk 3 (error text; logs/console
  only, never gameplay or replays; pinned by the `error_x_shape` probe).
- `io` has the 5.1 set **plus LuaJIT extensions**: `io.popen`, `io.setvbuf`
  exist (the sandbox removes them; `lib-mlua.md` / `LuaScripting.cpp`
  teardown). `utf8` does not.
- No `integer` type, no goto, no bitwise operators in the 5.1 language (only
  the `bit.*` library, if built in). (Luau has all of these — the shipped
  pack scripts target the engine API, not the language, and the dialect
  census pins the 5.1-ism surface; D-037.)

## File I/O / text mode (test-platform note — C++ side)

`io.open(filename, mode)`: text vs binary mode (`"r"` vs `"rb"`) differs
**only on Windows** (CRLF translation is a C-filesystem thing, absent on
POSIX). On Linux/macOS both read identical bytes — so a parity test for the
`redefineIoOpen` coercion must observe the *mode string routing*, not byte
translation. On the port VM (Luau) there is **no `io` library at all** —
the C++'s `luaopen_io == NULL` short-circuit (replicated by
`redefine_io_open`'s existence check) is the exact C++ behavior an io-less VM
gets; the pre-unification LuaJIT build pinned the mode routing end to end
with a spy (the 30a record).

## Sandbox-relevant stdlib surface (post-teardown — the C++ reference, and the port's documented divergences)

The C++ `hg::LuaScripting::init` (`src/SSVOpenHexagon/Core/LuaScripting.cpp:1585+`)
applies, in order, after registering the `u_rnd*` shims:

1. `redefineIoOpen` (lines 141-154) — verbatim:
   ```lua
   local open = io.open; io.open = function(filename, mode) return
   open(filename, mode == "rb" and mode or "r"); end
   ```
   (any mode except exactly `"rb"` becomes `"r"` — read-only text).
2. `redefineRandom` (156-175) — routes `math.random` through
   `u_rndSwitch(0|1|2, ...)`.
3. `destroyMaliciousFunctions` (178-218) — clears, in order: `os`,
   `io.popen`, `io.flush`, `io.write`, `io.setvbuf`, `debug`,
   `math.randomseed`, `package.loadlib`, `package.searchpath`,
   `package.loaded.os`, `package.loaded.debug`, `package.loadlib` (again —
   deliberate double), `package.loaders`.

Result (C++): `os`/`debug` gone; `io` is a read-only subset; `math.random`
is the engine-RNG shim; `package` keeps `require` + `package.path` working
(the C++ comment explains `require` needs the surviving parts).
`oh_lua::sandbox` ports all three verbatim (`crates/oh-lua/src/sandbox.rs`);
the post-sandbox shape the port observes on Luau (no `io` at all, no `jit`,
no `package.preload`, `require` a built-in that raises
`[string "chunk"]:1: module 'X' not found` for anything non-builtin instead
of the LuaJIT `'package.loaders' must be a table`) is pinned by
`crates/oh-lua/tests/stdlib_surface.rs` — the pack-script-visible outcome
(reachability of what the C++ cleared) is the same on both sides.

## Dotted-name semantics (`_getGlobal`/`_setGlobal`, `src/SSVOpenHexagon/Utils/LuaWrapper.cpp:96-220`)

- `_getGlobal("a.b")`: split on dots; each step after the first requires the
  current stack value to be a table (else `VariableDoesntExistException` —
  the **full dotted name**); after fetching **every** step (including the
  last) a nil value throws `VariableDoesntExistException(mVarName)`. So in
  the C++ original, `doesVariableExist("t.u")` on a missing `u` **throws**
  (it calls `_getGlobal`), it does not return false. Plain (dot-free) names
  never throw: missing → `nil` pushed.
- `_setGlobal("a.b.c")`: resolves the table part `"a.b"` via `_getGlobal`
  (missing/non-table table part throws the table-part name), then requires
  it to be a table (else `VariableDoesntExistException` with the **full name**
  `"a.b.c"`), then `lua_settable`. Intermediate tables are **not** created.
- `oh_lua` mirrors all of this: `get_path_value`/`set_path` in `context.rs`;
  `does_variable_exist` throws on dotted paths exactly like the C++. (VM-
  independent — holds on both the C++'s LuaJIT and the port's Luau.)
