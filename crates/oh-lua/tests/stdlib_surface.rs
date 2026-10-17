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
//! Lane 030a: the same file also pins the `backend-luau` arm (mutually
//! exclusive `backend-luajit` / `backend-luau` features — see
//! `crates/oh-lua/Cargo.toml`). Every documented divergence between the
//! two backends is called out inline with a `DIVERGENCE (documented)`
//! comment and is summarized in
//! `.agents/plans/001-rust-port/30a-lua-parity.md`.
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
    if cfg!(feature = "backend-luau") {
        // DIVERGENCE (documented): the Luau interpreter prefixes the
        // chunk location (its `require` is a built-in that raises from the
        // chunk, so the message carries the `[string "chunk"]:1: ` prefix
        // the LuaJIT `lj_cf_package_require` error does not).
        assert!(
            err.starts_with("[string \"chunk\"]:1: module 'ffi' not found"),
            "unexpected error: {err}"
        );
    } else {
        assert!(
            err.starts_with("module 'ffi' not found"),
            "unexpected error: {err}"
        );
    }

    // Same for the pre-sandbox spelling with double quotes (chunk-name /
    // quoting must not change the outcome).
    let err = run_err(&lua, r#"require("ffi")"#).expect_err("require(\"ffi\") must fail");
    if cfg!(feature = "backend-luau") {
        assert!(
            err.starts_with("[string \"chunk\"]:1: module 'ffi' not found"),
            "unexpected error: {err}"
        );
    } else {
        assert!(
            err.starts_with("module 'ffi' not found"),
            "unexpected error: {err}"
        );
    }

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
    if cfg!(feature = "backend-luau") {
        // DIVERGENCE (documented): the Luau interpreter's `package` has no
        // `preload` field at all (its `require` resolves built-in and
        // preloaded modules through its own internal search, not a
        // `package.preload` table) — the C++'s empty `package.preload` is
        // therefore absent, not empty.
        let t: String = lua
            .execute_code_typed("return type(package.preload)")
            .unwrap();
        assert_eq!(t, "nil", "Luau package.preload must be nil");
        return;
    }
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

    if cfg!(feature = "backend-luau") {
        // DIVERGENCE (documented): the Luau interpreter has no `jit`
        // library at all (there is no user-togglable JIT in the Luau
        // interpreter — it is always off; the K3 on-device knob does not
        // exist on this backend).
        assert_eq!(
            lua.execute_code_typed::<String>("return type(jit)")
                .unwrap(),
            "nil",
            "Luau must not expose a jit library"
        );
        return;
    }

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
fn disable_jit_turns_the_engine_off() {
    // K3 / issue 33: `LuaContext::new()` applies this automatically on arm64
    // Android, where the trace assembler hangs (see deviations.md K3/F29 for
    // the on-device stack traces). This pins the seam's contract on the host.
    let lua = LuaContext::new();

    if cfg!(feature = "backend-luau") {
        // DIVERGENCE (documented, as in the test above): the Luau interpreter
        // exposes no `jit` library, so there is nothing to turn off and the
        // call is expected to fail — which is also why `LuaContext::new`'s
        // Android mitigation is feature-gated to the LuaJIT backend.
        assert!(lua.disable_jit().is_err());
        return;
    }

    assert!(
        lua.execute_code_typed::<bool>("return jit.status()")
            .unwrap(),
        "the host starts with the JIT on (the C++ default)"
    );
    lua.disable_jit().unwrap();
    assert_eq!(
        lua.execute_code_typed::<(bool, bool)>("return false, jit.status()")
            .unwrap(),
        (false, false),
        "disable_jit() must leave the engine off"
    );
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

    if cfg!(feature = "backend-luau") {
        // DIVERGENCE (documented): the Luau interpreter's pre-sandbox
        // `package.loaded` is its own built-in set — no `io` (the library
        // itself is absent), no `jit`, `bit32` instead of `bit`, and the
        // Luau-only `buffer`/`coroutine`/`utf8`/`vector` entries; `debug`
        // IS present pre-sandbox (the sandbox clears it, exactly as in
        // the C++). Pin the exact set so a Luau version bump that changes
        // the built-in surface fails loudly.
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
        return;
    }

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

    if cfg!(feature = "backend-luau") {
        // DIVERGENCE (documented): the Luau interpreter's `require` is a
        // built-in that does not consult `package.loaders` at all (its
        // lookup is preloaded modules + built-in modules), so the C++'s
        // "'package.loaders' must be a table" error cannot occur here —
        // instead every non-builtin `require` fails with the chunk-prefixed
        // "module 'X' not found". The pack-script-visible outcome is the
        // same: `require` is effectively dead after the sandbox.
        for m in ["ffi", "anything_else"] {
            let err = run_err(&lua, &format!("require('{m}')"))
                .expect_err("require must fail post-sandbox");
            assert!(
                err.starts_with(&format!("[string \"chunk\"]:1: module '{m}' not found")),
                "unexpected error: {err}"
            );
        }
        return;
    }

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
    if cfg!(feature = "backend-luau") {
        // DIVERGENCE (documented): no `package.preload` field on Luau
        // (see `package_preload_is_empty_like_the_cpp`) — it stays nil.
        assert_eq!(
            lua.execute_code_typed::<String>("return type(package.preload)")
                .unwrap(),
            "nil"
        );
    } else {
        assert_eq!(
            lua.execute_code_typed::<i32>("#package.preload").unwrap(),
            0
        );
    }
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
    if cfg!(feature = "backend-luau") {
        // DIVERGENCE (documented): the Luau interpreter ships no `io`
        // library at all — the entire read-only subset check is replaced
        // by "io is nil".
        assert_eq!(
            lua.execute_code_typed::<String>("return type(io)").unwrap(),
            "nil"
        );
    } else {
        assert_eq!(
            lua.execute_code_typed::<(bool, bool, bool, bool, bool)>(
                "return type(io.open) == 'function', io.popen == nil, io.flush == nil, io.write == nil, io.setvbuf == nil"
            )
            .unwrap(),
            (true, true, true, true, true)
        );
    }
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
    if cfg!(feature = "backend-luau") {
        // DIVERGENCE (documented): no `jit` library on Luau at all (see
        // `jit_library_is_reachable_and_toggleable`) — nothing to toggle.
        assert_eq!(
            lua.execute_code_typed::<String>("return type(jit)")
                .unwrap(),
            "nil"
        );
    } else {
        assert!(lua
            .execute_code_typed::<bool>("return jit.status()")
            .unwrap());
        lua.execute_code("jit.off()").unwrap();
        assert!(!lua
            .execute_code_typed::<bool>("return jit.status()")
            .unwrap());
    }
}
