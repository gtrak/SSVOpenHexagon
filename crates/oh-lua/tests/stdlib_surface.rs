//! issue 029k1 (K3): what a pack script can actually reach from Lua — the
//! stdlib surface of the port's Lua VM, compared against the C++ baseline's
//! semantics.
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
//! Port under test (D-037 — the unification on Luau): `mlua 0.10.5` on the
//! `luau` feature (Luau 0.663, statically built from `luau0-src`), opened
//! with mlua's "safe" set (`Lua::new()` — the interpreter's standard
//! libraries: no `debug`, no `ffi`, no `jit` — each via `luaL_requiref`,
//! never `luaL_openlibs`), plus the one-line Rust `load` binding
//! (`LuaContext::new`) and the 5.1 compat prelude (`install_lua51_prelude`,
//! from `api::init_lua_api`), then the same verbatim sandbox.
//!
//! Every documented divergence between the C++'s LuaJIT and the port's Luau
//! is called out inline with a `DIVERGENCE (documented)` comment and is
//! summarized in `.agents/plans/001-rust-port/DECISIONS.md` D-037 (the
//! residual risks) — the pre-unification A/B record lives in
//! `.agents/plans/001-rust-port/30a-lua-parity.md`.
//!
//! These tests pin the REACHABLE surface: `require("ffi")`, `load` /
//! `loadstring` (the D-037 one-line Rust binding + the prelude's alias),
//! `package.loaded`, `package.preload` — and assert the C++-parity
//! invariants for each.

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
/// full pcall error, which the Luau interpreter decorates with a stack
/// traceback).
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
    // `module 'ffi' not found: ...`. The port must fail identically:
    // nothing ever registers an `ffi` loader, so `require` must not be
    // able to find one.
    let lua = LuaContext::new();
    let err = run_err(&lua, "require('ffi')")
        .expect_err("require('ffi') must fail (C++ parity: FFI disabled at compile time)");
    // DIVERGENCE (documented, D-037 residual risk 3): the Luau interpreter
    // prefixes the chunk location (its `require` is a built-in that raises
    // from the chunk, so the message carries the `[string "chunk"]:1: `
    // prefix the LuaJIT `lj_cf_package_require` error does not).
    assert!(
        err.starts_with("[string \"chunk\"]:1: module 'ffi' not found"),
        "unexpected error: {err}"
    );

    // Same for the pre-sandbox spelling with double quotes (chunk-name /
    // quoting must not change the outcome).
    let err = run_err(&lua, r#"require("ffi")"#).expect_err("require(\"ffi\") must fail");
    assert!(
        err.starts_with("[string \"chunk\"]:1: module 'ffi' not found"),
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
    // DIVERGENCE (documented, D-037 residual risk 3): the Luau
    // interpreter's `package` has no `preload` field at all (its `require`
    // resolves built-in and preloaded modules through its own internal
    // search, not a `package.preload` table) — the C++'s empty
    // `package.preload` is therefore absent, not empty.
    let lua = LuaContext::new();
    let t: String = lua
        .execute_code_typed("return type(package.preload)")
        .unwrap();
    assert_eq!(t, "nil", "Luau package.preload must be nil");
}

#[test]
fn load_and_loadstring_are_the_rust_binding_plus_prelude() {
    // D-037's one preserved capability: the C++ (LuaJIT) has `loadstring`
    // (and no 5.1 `load`); Luau's base library has no chunk compiler at
    // all — not even `load`. The port therefore installs, in
    // `LuaContext::new`, the one-line Rust binding (mlua's `Lua::load`
    // exposed as the global `load`); the 5.1 spelling is the prelude's
    // `loadstring = load` line (installed by `api::init_lua_api` — here we
    // install the prelude the same way `install_lua51_prelude` does, so
    // this test pins the shipped file end to end).
    let lua = LuaContext::new();
    // (1) the Rust binding: `load` exists on every context, compiles a
    // chunk from a string, and the chunk runs.
    let v: i64 = lua
        .execute_code_typed("return load('return 6*7')()")
        .expect("the Rust `load` binding must compile + run a chunk");
    assert_eq!(v, 42, "load('return 6*7')() must be 42 (the D-037 binding)");

    // (2) the prelude activates the 5.1 spelling:
    lua.execute_code(include_str!("../lua51_prelude.lua"))
        .expect("the shipped prelude must install cleanly (idempotent guards)");
    let v: i64 = lua
        .execute_code_typed("return loadstring('return 6*7')()")
        .expect("loadstring('return 6*7')() must run");
    assert_eq!(
        v, 42,
        "loadstring('return 6*7')() == 42 (the pin of record)"
    );

    // (3) the chunk name the binding sets (the 030c-demonstrated spelling —
    // a loadstring chunk's error carries it):
    let err = run_err(&lua, "loadstring('error(\"x\")')()").expect_err("must error");
    assert!(
        err.contains("loadstring-chunk") && err.contains("x"),
        "the loadstring chunk must carry its binding's chunk name: {err}"
    );
}

#[test]
fn package_loaded_shape() {
    // C++ pre-sandbox: `luaL_openlibs` -> `luaL_register` puts every opened
    // library into `package.loaded` — including `debug` (the C++ removes it
    // later via the sandbox) and NOT including the unnamed base library.
    // DIVERGENCE (documented, D-037 residual risk 3): the Luau
    // interpreter's pre-sandbox `package.loaded` is its own built-in set —
    // no `io` (the library itself is absent), no `jit`, `bit32` instead of
    // `bit`, and the Luau-only `buffer`/`coroutine`/`utf8`/`vector`
    // entries; `debug` IS present pre-sandbox (the sandbox clears it,
    // exactly as in the C++). Pin the exact set so a Luau version bump that
    // changes the built-in surface fails loudly.
    let lua = LuaContext::new();
    let names: Vec<String> = lua
        .execute_code_typed("return (function() local t = {} for k in pairs(package.loaded) do t[#t+1] = k end return t end)()")
        .unwrap();
    let has = |n: &str| names.iter().any(|x| x == n);

    let mut sorted = names.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(
        sorted,
        vec![
            "_G",
            "bit32",
            "buffer",
            "coroutine",
            "debug",
            "math",
            "os",
            "package",
            "string",
            "table",
            "utf8",
            "vector",
        ],
        "unexpected Luau pre-sandbox package.loaded set"
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
    // The port's sandbox is verbatim, so `require` must still fail for
    // `require("ffi")` — and for any other non-loaded module.
    // DIVERGENCE (documented, D-037 residual risk 3): the Luau
    // interpreter's `require` is a built-in that does not consult
    // `package.loaders` at all (its lookup is preloaded modules + built-in
    // modules), so the C++'s "'package.loaders' must be a table" error
    // cannot occur here — instead every non-builtin `require` fails with
    // the chunk-prefixed "module 'X' not found". The pack-script-visible
    // outcome is the same: `require` is effectively dead after the sandbox.
    let lua = sandboxed();
    for m in ["ffi", "anything_else"] {
        let err =
            run_err(&lua, &format!("require('{m}')")).expect_err("require must fail post-sandbox");
        assert!(
            err.starts_with(&format!("[string \"chunk\"]:1: module '{m}' not found")),
            "unexpected error: {err}"
        );
    }
}

#[test]
fn post_sandbox_surface_pins_the_cpp_shape() {
    let lua = sandboxed();

    // `os` / `debug` gone (C++ clears both; the port's safe set never had
    // `debug` open — the interpreter registers it pre-sandbox and the
    // sandbox's verbatim clear removes it, exactly as in the C++).
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
    // `package.preload` absent on Luau (the C++'s empty table is a
    // LuaJIT/PUC structure; DIVERGENCE (documented) — see
    // `package_preload_is_empty_like_the_cpp`): it stays nil.
    assert_eq!(
        lua.execute_code_typed::<String>("return type(package.preload)")
            .unwrap(),
        "nil"
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
    // `io` is the read-only subset in the C++; DIVERGENCE (documented):
    // the Luau interpreter ships no `io` library at all — the entire
    // read-only subset check is replaced by "io is nil" (the C++'s own
    // `luaopen_io == NULL` short-circuit in `redefineIoOpen` is the exact
    // C++ behavior this port replicates for an io-less VM).
    assert_eq!(
        lua.execute_code_typed::<String>("return type(io)").unwrap(),
        "nil"
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
    // The `jit` surface: the C++ (LuaJIT) exposes `jit` and the C++ never
    // sandboxes it. DIVERGENCE (documented, D-037): the Luau interpreter
    // has no `jit` library at all (there is no user-togglable JIT in the
    // Luau interpreter — it is always off; the K3 on-device knob does not
    // exist on this VM). Pin the absence: a future VM bump that grows a
    // `jit` surface must fail here loudly.
    assert_eq!(
        lua.execute_code_typed::<String>("return type(jit)")
            .unwrap(),
        "nil",
        "Luau must not expose a jit library (D-037: the K3 class is gone by construction — there is no JIT to toggle)"
    );
}
