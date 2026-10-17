//! `oh-lua` — the Lua runtime crate (issue 006).
//!
//! Rust analogue of `Utils/LuaWrapper.hpp` + the non-API parts of
//! `Core/LuaScripting.cpp` (sandbox teardown). Contracts:
//! `.agents/plans/001-rust-port/06-lua-runtime.md`,
//! `.agents/plans/001-rust-port/mapping/lua-api.md` (§3 conversion rules,
//! §4 RNG mapping, §8 subtleties),
//! `.agents/skills/lib-luajit.md`, `.agents/skills/lib-mlua.md`.
//!
//! Backend (D-037, 2026-10-17 — the unification): `mlua` on the `luau`
//! feature — Luau, statically compiled from source by `luau0-src`. This is
//! the port's ONE Lua VM; the LuaJIT machinery the port once vendored
//! (`luajit-src` + `[patch.crates-io]`, the `ffi_stub.c` link shim, the
//! Android `__clear_cache` shim, the per-target backend selection) is
//! deleted by D-037. The C++ build still runs its own LuaJIT
//! (`vittorioromeo/LuaJIT @ 4dc5ffbf60b16e54274c470fcc022165685c96fe`
//! with `LUAJIT_DISABLE_FFI` — a C++-side fact, `.agents/skills/
//! lib-luajit.md`); the parity guarantee holds because the C++ oracle
//! fingerprints game STATE, not Lua internals, and 030c proved this exact
//! configuration against the C++ column (3627/3627 frames).
//!
//! # Modules
//!
//! * [`context`] — `LuaContext` (`execute_code`, `read_variable`,
//!   `write_variable`, `clear_variable`, dotted-name path walk, `add_lua_fn`)
//! * [`convert`] — the exact C++ `_push`/`_read` conversion rules
//! * [`error`] — the `Lua::` exception hierarchy as `LuaError`
//! * [`metadata`] — `LuaMetadata` / `LuaMetadataProxy` (`addLuaFn` docs)
//! * [`docs`] — the `LuaScripting.cpp:1613-1688` docs readers
//!   (`print_docs`, `get_all_function_names`, `get_docs_for_function`)
//! * [`sandbox`] — `redefine_io_open`, `redefine_random`,
//!   `destroy_malicious_functions` (the verbatim C++ Lua snippets — the
//!   single source of truth lives in [`snippets`], compile-tested by
//!   29v1's table)
//! * [`snippets`] — every embedded/spliced Lua string the crate loads
//!   (29v1: the per-snippet compile test is the mechanical guard against
//!   the 29s1 bug class — a hand-transcribed Lua typo inside a snippet)
//! * [`prelude`] — the Lua 5.1 compat prelude (D-037: the Luau-era safety
//!   net for third-party packs; installed from `api::init_lua_api`)
//! * [`api`] — the complete game API (issue 007): all 285 registered global
//!   function names + the 12-callback dispatch + the `GameApiState` /
//!   `GameApi` / `AssetsApi` state interface (issue 009 implements the real
//!   backend)
//!
//! # Threading
//!
//! Like the C++ context (single Lua stack, no mutex in the vendored
//! wrapper), a `LuaContext` is used from one thread at a time and is not
//! `Sync` (nor `Clone`: the C++ context is move-only).

pub mod api;
mod context;
mod convert;
pub mod docs;
mod error;
mod metadata;
mod prelude;
mod sandbox;
pub mod snippets;

pub use context::LuaContext;
pub use convert::{FnReturn, FromLuaValue, IntoLuaValue, PushValue};
pub use error::LuaError;
pub use metadata::{
    get_metadata, FnEntry, FnMeta, LuaMetadata, LuaMetadataProxy, NUM_CATEGORIES,
    PREFIX_CATEGORIES, PREFIX_HEADERS,
};
pub use sandbox::{destroy_malicious_functions, redefine_io_open, redefine_random};
