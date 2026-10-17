//! The sandbox, ported from the non-API parts of
//! `src/SSVOpenHexagon/Core/LuaScripting.cpp`: `redefineIoOpen`
//! (lines 141-154), `redefineRandom` (156-176) and
//! `destroyMaliciousFunctions` (178-219).
//!
//! The C++ `init` (LuaScripting.cpp:1585) applies them in this order, after
//! the RNG shims (`u_rndReal`, `u_rndIntUpper`, `u_rndInt`, `u_rndSwitch`,
//! `u_getAttemptRandomSeed`) have been registered — `math.random` is
//! redefined in terms of `u_rndSwitch`, so that function must already exist
//! in the context. The port keeps the same contract:
//!
//! ```ignore
//! lua.add_lua_fn("u_rndSwitch", ...);   // issue 07: the u_rnd* group
//! redefine_io_open(&lua)?;
//! redefine_random(&lua)?;
//! destroy_malicious_functions(&lua)?;
//! ```

use crate::context::LuaContext;
use crate::error::LuaError;
use crate::snippets::{REDEFINE_IO_OPEN, REDEFINE_RANDOM};

/// `redefineIoOpen` — the Lua snippet verbatim from LuaScripting.cpp:145-148
/// (the single source of truth: [`crate::snippets::REDEFINE_IO_OPEN`],
/// compile-tested by 29v1's table in `snippets.rs`). Wraps `io.open` so
/// that any mode other than `"rb"` is coerced to `"r"`: all file opens
/// become read-only text, except true binary-read.
pub fn redefine_io_open(lua: &LuaContext) -> Result<(), LuaError> {
    lua.execute_code(REDEFINE_IO_OPEN)
}

/// `redefineRandom` — the Lua snippet verbatim (the C++ raw string,
/// LuaScripting.cpp:159-168; the single source of truth:
/// [`crate::snippets::REDEFINE_RANDOM`], compile-tested by 29v1's table in
/// `snippets.rs`). Routes `math.random` through `u_rndSwitch` so
/// pack scripts consume the same engine RNG the C++ game logic uses:
/// `math.random()` -> `u_rndSwitch(0, 0, 0)` (`u_rndReal`),
/// `math.random(n)` -> `u_rndSwitch(1, 0, n)` (`u_rndIntUpper`),
/// `math.random(a, b)` -> `u_rndSwitch(2, a, b)` (`u_rndInt`).
pub fn redefine_random(lua: &LuaContext) -> Result<(), LuaError> {
    lua.execute_code(REDEFINE_RANDOM)
}

/// `destroyMaliciousFunctions` — the exact teardown, in the exact C++ order
/// (LuaScripting.cpp:178-219), including the double `package.loadlib` clear
/// (lines 207 and 217). After this: `os` and `debug` are gone, `io` is
/// read-only (`io.popen`/`io.flush`/`io.write`/`io.setvbuf` removed;
/// `io.open` is the read-only wrapper from `redefine_io_open`),
/// `math.randomseed` is gone, and `package` survives with `require` /
/// `package.path` working but `loadlib`/`searchpath`/`loaders` and the
/// `package.loaded.os`/`package.loaded.debug` restore paths removed.
pub fn destroy_malicious_functions(lua: &LuaContext) -> Result<(), LuaError> {
    // This destroys the "os" library completely. This library is capable of
    // file manipulation, running shell commands, and messing up the replay
    // system completely.
    lua.clear_variable("os")?;

    // This destroys some of the "io" functions completely.
    lua.clear_variable("io.popen")?;
    lua.clear_variable("io.flush")?;
    lua.clear_variable("io.write")?;
    lua.clear_variable("io.setvbuf")?;

    // This destroys the "debug" library completely.
    lua.clear_variable("debug")?;

    // This function allows pack developers to set the seed in Lua.
    lua.clear_variable("math.randomseed")?;

    // These functions are being deleted as they can assist in restoring
    // destroyed modules. However, we cannot destroy the whole library as
    // the other functions are needed for the "require" function to work
    // properly.
    lua.clear_variable("package.loadlib")?;
    lua.clear_variable("package.searchpath")?;

    // This removes some other ways in which the "os" and "debug" libraries
    // can be accessed.
    lua.clear_variable("package.loaded.os")?;
    lua.clear_variable("package.loaded.debug")?;

    // This removes some potentially dangerous packages that could be used
    // to inject malicious code.
    lua.clear_variable("package.loadlib")?;
    lua.clear_variable("package.loaders")?;

    Ok(())
}
