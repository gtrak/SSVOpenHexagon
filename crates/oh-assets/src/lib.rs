//! `oh-assets` — port of SSVOpenHexagon's `Global/Assets` +
//! `Global/AssetStorage` (plan 001 issue 05).
//!
//! One Rust module per C++ concern (ground rule 1; D-000/D-002):
//!
//! - `pack_scan` — the `ssvufs::scan` call sites and the discovery-order rule
//!   (byte-sorted full-path order; see `.agents/skills/lib-assets.md`);
//! - `asset_storage` — `AssetStorage` (texture/font/sound registry with the
//!   emplace-once / null-entry-after-failure / `has*` semantics);
//! - `shaders` — the shader registry (id allocation, lookups; GLSL is issue 10);
//! - `assets` — `Assets` (pack/level/style/music discovery + loading,
//!   `LoadInfo` error accumulation, `assets.json`, dependency checks, the
//!   level-selection data structures, priority ordering).
//!
//! Everything is threaded through an explicit `root` path (the process CWD in
//! C++, the `_RELEASE` directory in tests) rather than a hidden global
//! singleton. The actual SFML/OpenGL asset handles are deferred to the
//! render/audio backend (issue 10); this crate records the registry state
//! (ids, loaded?, paths) the game logic observes.

pub mod asset_storage;
pub mod assets;
pub mod pack_scan;
pub mod shaders;

pub use asset_storage::{AssetRef, AssetStorage};
pub use assets::{Assets, FatalLoadError, WorkshopPacks};
pub use shaders::{LoadedShader, ShaderRegistry, ShaderType};
