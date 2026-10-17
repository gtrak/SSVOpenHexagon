//! `oh-lua` — the Lua runtime crate (issue 006).
//!
//! Rust analogue of `Utils/LuaWrapper.hpp` + the non-API parts of
//! `Core/LuaScripting.cpp` (sandbox teardown). Contracts:
//! `.agents/plans/001-rust-port/06-lua-runtime.md`,
//! `.agents/plans/001-rust-port/mapping/lua-api.md` (§3 conversion rules,
//! §4 RNG mapping, §8 subtleties),
//! `.agents/skills/lib-luajit.md`, `.agents/skills/lib-mlua.md`.
//!
//! Backend: `mlua` on the `luajit` + `vendored` features (LuaJIT 2.1, the
//! same runtime the C++ build bundles — CMake CPM pin
//! `vittorioromeo/LuaJIT @ 4dc5ffbf60b16e54274c470fcc022165685c96fe` with
//! `LUAJIT_DISABLE_FFI`).
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
