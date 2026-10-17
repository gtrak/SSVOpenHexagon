//! issue 029k1 (K3): what a pack script can actually reach from Lua — the
//! stdlib surface of the port's vendored LuaJIT, compared against the C++
//! baseline's semantics.
//!
//! C++ reference semantics (`src/SSVOpenHexagon/Core/LuaScripting.cpp` +
//! the pinned `vittorioromeo/LuaJIT @ 4dc5ffbf` CMake build):
//!
//! * the VM is built with `LUAJIT_DISABLE_FFI=ON` (`CMakeLists.txt`), so
//!   `LJ_HASFFI == 0` and `luaL_openlibs` never registers `ffi` in
//!   `package.preload` (LuaJIT `lib_init.c`: the preload array is guarded by
//!   `#if LJ_HASFFI`). `require("ffi")` therefore FAILS there.
//! * the context is created with `luaL_openlibs` (safe set + `debug` + the
//!   `ffi` absence), then sandboxed by `LuaScripting::init`:
//!   `redefineIoOpen`, `redefineRandom`, `destroyMaliciousFunctions` (which
//!   clears `os`, `debug`, `package.loadlib`/`searchpath`,
//!   `package.loaded.os`/`.debug` and — last — `package.loaders`).
//! * consequence: after the sandbox, ANY `require("x")` whose module is not
//!   already in `package.loaded` raises Lua's own error
//!   `package.loaders must be a table` (LuaJIT `lib_package.c`
//!   `lj_cf_package_require`), because the loaders table is gone.
//!
//! Port under test: `mlua 0.10.5` `luajit`+`vendored` (upstream LuaJIT
//! 2.1.0-beta3 lineage, `luajit-src`), opened with mlua's "safe" set
//! (`Lua::new()` — every standard library except `debug` and `ffi`, each
//! via `luaL_requiref`, never `luaL_openlibs`), then the same verbatim
//! sandbox.
//!
//! These tests pin the REACHABLE surface: `require("ffi")`, `jit.status` /
//! `jit.off` / `jit.on`, `package.loaded`, `package.preload` — and assert
//! the C++-parity invariants for each.

use oh_lua::{destroy_malicious_functions, redefine_io_open, redefine_random, LuaContext};

/// Apply the full C++ `LuaScripting::init` sandbox in the verbatim order
/// (the `u_rndSwitch` registration is issue 07's job; the sandbox snippets
/// only reference it lazily, so it is not needed here).
fn sandboxed() -> LuaContext {
    let lua = LuaContext::new();
    redefine_io_open(&lua).expect("redefineIoOpen");
    redefine_random(&lua).expect("redefineRandom");
    destroy_malicious_functions(&lua).expect("destroyMaliciousFunctions");
    lua
}

/// Run a snippet, return `Ok(())` or the Lua error message (prefix of the
/// full pcall error, which LuaJIT decorates with a stack traceback).
fn run_err(lua: &LuaContext, code: &str) -> Result<(), String> {
    match lua.execute_code(code) {
        Ok(()) => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

// ---------------------------------------------------------------------------
// Pre-sandbox surface (the C++'s `luaL_openlibs` state, FFI absent)
// ---------------------------------------------------------------------------

#[test]
fn require_ffi_fails_like_the_cpp() {
    // C++: `LUAJIT_DISABLE_FFI` at compile time -> `luaL_openlibs` never
    // registers `ffi` in `package.preload` -> `require("ffi")` raises
    // `module 'ffi' not found: ...`. The port must fail identically: its
    // LuaJIT may have the FFI C code compiled in, but mlua's safe set never
    // opens it and nothing registers a `ffi` loader, so `require` must not
    // be able to find one.
    let lua = LuaContext::new();
    let err = run_err(&lua, "require('ffi')")
        .expect_err("require('ffi') must fail (C++ parity: FFI disabled at compile time)");
    assert!(
        err.starts_with("module 'ffi' not found"),
        "unexpected error: {err}"
    );

    // Same for the pre-sandbox spelling with double quotes (chunk-name /
    // quoting must not change the outcome).
    let err = run_err(&lua, r#"require("ffi")"#).expect_err("require(\"ffi\") must fail");
    assert!(
        err.starts_with("module 'ffi' not found"),
        "unexpected error: {err}"
    );

    // `ffi` is not a global either (it never was opened).
    assert!(
        lua.execute_code_typed::<bool>("return ffi == nil").unwrap(),
        "ffi must not exist as a global"
    );
}

#[test]
fn package_preload_is_empty_like_the_cpp() {
    // C++: in a `LUAJIT_DISABLE_FFI` build, `luaL_openlibs` registers NOTHING
    // into `package.preload` (the only LuaJIT preload entry is `ffi`,
    // `lib_init.c`). So the C++'s `package.preload` is an empty table.
    // The port (mlua opens each library via `luaL_requiref`, never
    // `luaL_openlibs`) must show the same: an empty `package.preload`.
    let lua = LuaContext::new();
    let (is_table, n) = lua
        .execute_code_typed::<(bool, i32)>(
            "return type(package.preload) == 'table', #package.preload",
        )
        .unwrap();
    assert!(is_table, "package.preload must be a table");
    assert_eq!(n, 0, "package.preload must be empty (C++ parity)");
}

#[test]
fn jit_library_is_reachable_and_toggleable() {
    // `jit` is a standard LuaJIT library in BOTH builds (the C++ never
    // sandboxes it). `jit.status()` reports the JIT engine on/off state;
    // `jit.off()` / `jit.on()` toggle it. This is the knob behind the K3
    // "does `jit.off()` make babysteps playable" experiment, so pin it.
    let lua = LuaContext::new();

    assert_eq!(
        lua.execute_code_typed::<(bool, bool, bool)>(
            "return type(jit) == 'table', type(jit.status) == 'function', type(jit.off) == 'function'"
        )
        .unwrap(),
        (true, true, true),
        "jit / jit.status / jit.off must exist"
    );

    // Fresh context: JIT enabled.
    assert!(lua
        .execute_code_typed::<bool>("return jit.status()")
        .unwrap());

    // `jit.off()` -> `jit.status()` false; `jit.on()` -> true again.
    assert!(lua.execute_code("jit.off()").is_ok());
    assert_eq!(
        lua.execute_code_typed::<(bool, bool)>("return false, jit.status()")
            .unwrap(),
        (false, false),
        "jit.status() must be false after jit.off()"
    );
    assert!(lua.execute_code("jit.on()").is_ok());
    assert!(lua
        .execute_code_typed::<bool>("return jit.status()")
        .unwrap());
}

#[test]
fn package_loaded_shape() {
    // C++ pre-sandbox: `luaL_openlibs` -> `luaL_register` puts every opened
    // library into `package.loaded` — including `debug` (the C++ removes it
    // later via the sandbox) and NOT including the unnamed base library.
    // Port: mlua's safe set registers `table, io, os, string, math, bit,
    // package, jit` (plus `"_G"` for base) and never `debug` (the safe set
    // excludes it; the sandbox would have cleared it in the C++ too).
    // Assert the invariants both sides share, and document the two known
    // pre-sandbox deltas (`debug` absent, `"_G"` present).
    let lua = LuaContext::new();
    let names: Vec<String> = lua
        .execute_code_typed("return (function() local t = {} for k in pairs(package.loaded) do t[#t+1] = k end return t end)()")
        .unwrap();
    let has = |n: &str| names.iter().any(|x| x == n);

    for lib in [
        "table", "io", "os", "string", "math", "bit", "package", "jit",
    ] {
        assert!(has(lib), "package.loaded must contain '{lib}' (C++ parity)");
    }
    // Port delta: base registered under "_G" (mlua's `luaL_requiref`),
    // C++ delta: "debug" present pre-sandbox (absent here — the safe set).
    assert!(has("_G"), "port registers base under package.loaded['_G']");
    assert!(
        !has("debug"),
        "the safe set must not register debug (C++ sandboxes it away)"
    );
    assert!(!has("ffi"), "ffi must never be in package.loaded");
}

// ---------------------------------------------------------------------------
// Post-sandbox surface (the state pack scripts actually run in)
// ---------------------------------------------------------------------------

#[test]
fn require_anything_fails_with_loaders_error_like_the_cpp() {
    // C++ post-sandbox: `destroyMaliciousFunctions` clears `package.loaders`
    // (LuaScripting.cpp:217), so `lj_cf_package_require` raises its own
    // error before any loader runs: `'package.loaders' must be a table`
    // (LuaJIT `LUA_QL` quoting).
    // The port's sandbox is verbatim, so the same error must surface for
    // `require("ffi")` — and for any other non-loaded module.
    let lua = sandboxed();

    let err = run_err(&lua, "require('ffi')").expect_err("require must fail post-sandbox");
    assert!(
        err.contains("'package.loaders' must be a table"),
        "unexpected error: {err}"
    );

    let err =
        run_err(&lua, "require('anything_else')").expect_err("require must fail post-sandbox");
    assert!(
        err.contains("'package.loaders' must be a table"),
        "unexpected error: {err}"
    );
}

#[test]
fn post_sandbox_surface_pins_the_cpp_shape() {
    let lua = sandboxed();

    // `os` / `debug` gone (C++ clears both; the port never had `debug`).
    assert_eq!(
        lua.execute_code_typed::<(bool, bool)>("return os == nil, debug == nil")
            .unwrap(),
        (true, true)
    );
    // `package.loadlib` / `searchpath` / `loaders` cleared.
    assert_eq!(
        lua.execute_code_typed::<(bool, bool, bool)>(
            "return package.loadlib == nil, package.searchpath == nil, package.loaders == nil"
        )
        .unwrap(),
        (true, true, true)
    );
    // `package.loaded.os` / `.debug` cleared (the C++'s restore paths).
    assert_eq!(
        lua.execute_code_typed::<(bool, bool)>(
            "return package.loaded.os == nil, package.loaded.debug == nil"
        )
        .unwrap(),
        (true, true)
    );
    // `package.preload` still an empty table (nothing was ever registered).
    assert_eq!(
        lua.execute_code_typed::<i32>("#package.preload").unwrap(),
        0
    );
    // `require` and `package.path` still exist (the C++ comment says they
    // are needed — even though the cleared `loaders` makes require raise,
    // the FUNCTION itself survives, exactly as in the C++).
    assert_eq!(
        lua.execute_code_typed::<(bool, bool)>(
            "return type(require) == 'function', type(package.path) == 'string'"
        )
        .unwrap(),
        (true, true)
    );
    // `io` is the read-only subset: `open` survives, the rest is gone.
    assert_eq!(
        lua.execute_code_typed::<(bool, bool, bool, bool, bool)>(
            "return type(io.open) == 'function', io.popen == nil, io.flush == nil, io.write == nil, io.setvbuf == nil"
        )
        .unwrap(),
        (true, true, true, true, true)
    );
    // `math.random` is the engine-RNG shim (calls `u_rndSwitch`, which is
    // registered by the game API in issue 07 — pin the wrapper shape).
    assert!(
        lua.execute_code_typed::<bool>("return type(math.random) == 'function'")
            .unwrap(),
        "math.random must be the engine-RNG shim function"
    );
    assert!(
        lua.execute_code_typed::<bool>("return math.randomseed == nil")
            .unwrap(),
        "math.randomseed must be cleared"
    );
    // The JIT surface is untouched by the sandbox (the C++ never touches
    // `jit`): `jit.status()` / `jit.off()` remain usable — this is what the
    // K3 on-device experiment toggles.
    assert!(lua
        .execute_code_typed::<bool>("return jit.status()")
        .unwrap());
    lua.execute_code("jit.off()").unwrap();
    assert!(!lua
        .execute_code_typed::<bool>("return jit.status()")
        .unwrap());
}
