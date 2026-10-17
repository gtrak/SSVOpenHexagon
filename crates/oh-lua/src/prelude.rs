//! The Lua 5.1 compat prelude (D-037 — the unification on Luau).
//!
//! Luau's base library drops the seven 5.1-isms third-party pack scripts
//! could rely on (`setfenv`, `getfenv`, `loadstring`, `unpack`, `newproxy`,
//! `module`, the `collectgarbage("gcinfo")` option). 030c classified each
//! ism against the exact Luau this port builds (0.663, `luau0-src 0.12.3`
//! via mlua 0.10.5): most are NATIVE on that build, `loadstring` is the one
//! construct no Lua-level code can emulate (there is no chunk compiler in
//! the language surface at all — the one-line Rust `load` binding in
//! `context.rs` is its companion), and `module` needs a shim. The prelude
//! (`../lua51_prelude.lua`, 030c's file, unchanged) installs that set:
//! every shim is guarded by the target global's absence, so on a VM that
//! already has a construct it installs nothing for it.
//!
//! The prelude is a PORT ADDITION, not a C++ port: the C++'s LuaJIT has
//! every 5.1-ism natively and needs nothing. It exists because D-037's
//! residual risk (1) is third-party content — packs that never ran through
//! this repo's census (030c: zero 5.1-isms across all 42 shipped pack
//! scripts) — and because the C++ column's parity proof (030b/030c)
//! compares game STATE, which the prelude leaves untouched (no shipped
//! script calls any shimmed construct; it only activates for third-party
//! code that would otherwise fail with a nil global).
//!
//! Installed once per context by `api::init_lua_api` (the shared game/menu
//! `hg::LuaScripting::init` port — the seam through which level scripts
//! run), before the C++ registration order begins.

use crate::context::LuaContext;
use crate::error::LuaError;

/// The shipped prelude (single source of truth: `crates/oh-lua/lua51_prelude.lua` —
/// the file 030c's tests in `crates/oh-parity/tests/lua51_compat_prelude.rs`
/// include by path).
pub(crate) const LUA51_PRELUDE: &str = include_str!("../lua51_prelude.lua");

/// Install the 5.1 compat prelude in the context (idempotent: every shim
/// guards on the target global's absence, so a second install changes
/// nothing — pinned by `prelude_is_idempotent` in the parity tests).
pub(crate) fn install_lua51_prelude(ctx: &LuaContext) -> Result<(), LuaError> {
    ctx.execute_code(LUA51_PRELUDE)
}
