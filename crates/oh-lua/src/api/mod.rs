//! The complete Lua game API (issue 007): all 285 registered global function
//! names, the `LuaScripting::init` shared registration, the
//! `HexagonGame::initLua` per-game groups, the menu-context stubs, and the
//! callback-dispatch helpers (`onInput` veto and friends).
//!
//! # Authority
//!
//! * Name set + arities: `.agents/plans/001-rust-port/mapping/lua-api.md`
//!   §1/§2 (re-derived from `addLuaFn`/`makeLuaAccessor`/color-getter call
//!   sites in `src/SSVOpenHexagon/Core/{HGScripting,LuaScripting}.cpp` —
//!   see `tests/api.rs::registered_name_set_matches_source`);
//! * Semantics: the C++ lambdas themselves (`HGScripting.cpp`,
//!   `LuaScripting.cpp`, `Utils.cpp` dependency resolution, `MenuGame.cpp`
//!   menu stubs);
//! * Callback table: mapping §5.
//!
//! # Module map (one module per C++ init group)
//!
//! Shared (`init_lua_api`, C++ `LuaScripting::init`):
//! `random` (`u_rnd*` + seed) → `redefine_io_open` / `redefine_random` /
//! `destroy_malicious_functions` → `utils` (`u_inMenu`, `u_isHeadless`,
//! `u_getVersion*`) → `custom_walls` (`cw_*`) → `level_control` (`l_*`
//! accessors + direct) → `style_control` (`s_*` accessors + colors) →
//! `exec_script` (`u_execScript*`) → `shaders` (`shdr_*`) → `config`
//! (`u_getWidth`/`u_getHeight`).
//!
//! Game context (`init_level_api`, C++ `HexagonGame::initLua` groups, in
//! order): `level_utils` (`initLua_Utils`, incl. raw
//! `u_impl_addTrackedResult`) → `audio` (`a_*`) → `main_timeline` (`t_*`) →
//! `event_timeline` (`e_*`) → `custom_timelines` (`ct_*`) →
//! `level_control_level` (`initLua_LevelControl`) → `style_level`
//! (`initLua_StyleControl`) → `wall_creation` (`w_*`) → `steam`
//! (`initLua_Steam`) → `deprecated` (`initLua_Deprecated`, incl. `m_*`).
//!
//! Menu context (`init_menu_api`, C++ `MenuGame::initLua`): the shared init
//! with `in_menu == true` + `menu` (overrides `u_log`, pins 4 getters,
//! no-ops the unused list).
//!
//! `callbacks` holds the engine-side dispatch of the 12 script callbacks.

pub mod callbacks;
pub mod state;

mod audio;
mod config;
mod custom_timelines;
mod custom_walls;
mod deprecated;
mod event_timeline;
mod exec_script;
mod level_control;
mod level_control_level;
mod level_utils;
mod main_timeline;
mod menu;
mod random;
mod shaders;
mod steam;
mod style_control;
mod style_level;
mod utils;
mod wall_creation;

pub use state::{ft_to_seconds, AssetsApi, GameApi, GameApiState, SharedLua};

use std::sync::{Arc, Mutex, MutexGuard};

use crate::context::LuaContext;
use crate::error::LuaError;
use crate::{destroy_malicious_functions, redefine_io_open, redefine_random};

/// The shared state handle every registered closure locks (C++ captured
/// `this` / references; the `'static` closures need the `Arc`).
pub type State = Arc<Mutex<GameApiState>>;

/// Locks the shared state; a poisoned lock is recovered (same convention as
/// `get_metadata`).
fn lock(state: &State) -> MutexGuard<'_, GameApiState> {
    state
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// The C++ `hg::LuaScripting::init` shared registration (random shims →
/// `io.open`/`math.random` redefinition → malicious-function teardown →
/// utils → custom walls → level control → style control → exec-script →
/// shaders → config).
///
/// The `in_menu`/`headless` flags are read from `state` at registration
/// time (C++ captured the booleans into the lambdas).
pub fn init_lua_api(ctx: &LuaContext, state: &State) -> Result<(), LuaError> {
    let shared = SharedLua::new(ctx);

    random::register(ctx, state)?;
    redefine_io_open(ctx)?;
    redefine_random(ctx)?;
    destroy_malicious_functions(ctx)?;
    utils::register(ctx, state)?;
    custom_walls::register(ctx, state)?;
    level_control::register(ctx, state)?;
    style_control::register(ctx, state)?;
    exec_script::register(ctx, state, shared)?;
    shaders::register(ctx, state, shared)?;
    config::register(ctx, state)?;
    Ok(())
}

/// The C++ `HexagonGame::initLua` per-game groups, in registration order:
/// `initLua_Utils`, `initLua_AudioControl`, `initLua_MainTimeline`,
/// `initLua_EventTimeline`, `initLua_CustomTimelines`,
/// `initLua_LevelControl`, `initLua_StyleControl`, `initLua_WallCreation`,
/// `initLua_Steam`, `initLua_Deprecated`.
pub fn init_level_api(ctx: &LuaContext, state: &State) -> Result<(), LuaError> {
    let shared = SharedLua::new(ctx);

    level_utils::register(ctx, state)?;
    audio::register(ctx, state)?;
    main_timeline::register(ctx, state, shared)?;
    event_timeline::register(ctx, state, shared)?;
    custom_timelines::register(ctx, state, shared)?;
    level_control_level::register(ctx, state, shared)?;
    style_level::register(ctx, state)?;
    wall_creation::register(ctx, state)?;
    steam::register(ctx, state)?;
    deprecated::register(ctx, state, shared)?;
    Ok(())
}

/// The C++ `MenuGame::initLua` menu context: the shared init (with
/// `state.in_menu == true`) followed by the `u_log` override, the 4 pinned
/// getters, and the no-op "unused functions" list (mapping §8.10).
pub fn init_menu_api(ctx: &LuaContext, state: &State) -> Result<(), LuaError> {
    init_lua_api(ctx, state)?;
    menu::register(ctx, state)?;
    Ok(())
}
