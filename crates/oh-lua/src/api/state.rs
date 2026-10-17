//! The state the Lua game API touches + the engine backend interface
//! (issue 007; shared storage moved to `oh_game::lua_state` by issue 009,
//! DECISIONS.md D-013).
//!
//! Because the 285 lambdas and the engine update loop must mutate **one**
//! store of `levelStatus` / `status` / `walls` / `rng` / timelines, the
//! data lives in `oh-game` (this crate depends on it, so the other
//! direction would be a cargo cycle):
//!
//! * [`SharedState`] (`oh_game::lua_state`) — the exact data every C++ API
//!   lambda reads or writes (RNG, `LevelStatus`, `HexagonGameStatus`,
//!   `StyleData`, the timelines, the custom-wall manager, the wall vector,
//!   difficulty multiplier, `firstPlay`, the in-level-console tracking
//!   vectors, the exec-script pack-path context stack, pack identity,
//!   `MusicData`). Re-exported here for the pre-009 import path.
//! * [`GameApiState`] — thin wrapper `{ shared, run_lua_file, assets,
//!   backend }` with `Deref<Target = SharedState>`, so every 007 lambda
//!   (`lock(&st).level_status.x`, `st.backend.x`, `st.assets.x`, ...)
//!   resolves UNCHANGED.
//! * [`GameApi`] — the engine operations the lambdas delegate to (defined
//!   in `oh_game::lua_state`, re-exported; issue 009 supplies the real
//!   implementation against the game loop).
//! * [`AssetsApi`] — the `HGAssets` lookups the lambdas perform (stays in
//!   this crate: its signatures return `oh-lua`'s `LuaError`, which
//!   `oh-game` cannot name).
//!
//! Every registered closure locks the state (`Rc<ReentrantMutex<GameApiState>>`)
//! before touching it, exactly mirroring the C++ lambdas capturing
//! `HexagonGame&`/`this`. The lock is reentrant because `t_eval`/`e_eval`/
//! `ct_eval` run Lua *while the state guard is held* and lambdas call
//! `lock(&st).backend.*`, whose implementation re-locks the same state on
//! the same thread (D-013).

use std::ops::{Deref, DerefMut};
use std::sync::Arc;

use oh_data::pack_data::PackData;
use oh_data::style_data::StyleData;
use oh_data::MusicData;

use crate::error::LuaError;
use crate::LuaContext;

/// Re-exports of the D-013 shared-state building blocks (the pre-009
/// import path `oh_lua::api::state::*` keeps working).
pub use oh_game::lua_state::{
    GameApi, ReentrantMutex, ReentrantMutexGuard, SharedState, SharedStore,
};

/// C++ `ssvu::getFTToSeconds(ft)`: frametime (1.0 = 1/60 s) -> seconds.
#[must_use]
pub fn ft_to_seconds(ft: f64) -> f64 {
    ft / 60.0
}

// ---------------------------------------------------------------------------
// SharedLua — 'static captures of the Lua context
// ---------------------------------------------------------------------------

/// A `Send + Sync` handle to a `LuaContext` for `'static` closures: the
/// deferred timeline actions (`t_eval`, `e_eval`, `ct_eval`) capture the Lua
/// context the same way the C++ lambdas capture `lua`/`this`, and the
/// `run_lua_file` / `run_lua_code` callbacks handed to the engine take one.
///
/// `mlua::Lua` is `!Sync`, so a `&LuaContext` cannot be stored in a
/// `Send + Sync` closure; this handle carries the reference through a
/// deliberate, documented lifetime extension.
///
/// # Contract (same as the C++ capture)
///
/// The `LuaContext` must outlive every closure / deferred action that holds
/// a `SharedLua` to it — i.e. the engine drops its state (timelines and all
/// registered Lua functions) before it destroys the context, on the same
/// single thread. This mirrors the C++ object layout where `HexagonGame::lua`
/// and its timelines die together.
#[derive(Clone, Copy, Debug)]
pub struct SharedLua {
    ctx: *const LuaContext,
}

// The reference is only ever dereferenced on the single thread that owns the
// LuaContext (single-stack Lua usage contract, see `LuaContext` docs).
unsafe impl Send for SharedLua {}
unsafe impl Sync for SharedLua {}

impl SharedLua {
    /// Extends the borrow of `ctx` to `'static` (see the type-level contract).
    #[must_use]
    pub fn new(ctx: &LuaContext) -> Self {
        // SAFETY: lifetime extension — valid under the documented
        // drop-order contract (type docs).
        unsafe { std::mem::transmute(ctx) }
    }

    /// The borrowed context (single-thread contract).
    #[must_use]
    pub fn context(&self) -> &LuaContext {
        // SAFETY: same contract as `new`.
        unsafe { &*self.ctx }
    }

    /// `Utils::runLuaCode` minus its logging: load + `pcall`, chunk name
    /// `"chunk"`, return values discarded.
    pub fn execute_code(&self, code: &str) -> Result<(), LuaError> {
        self.context().execute_code(code)
    }
}

// ---------------------------------------------------------------------------
// AssetsApi — HGAssets lookups the API performs
// ---------------------------------------------------------------------------

/// C++ `HGAssets` surface used by the Lua API lambdas (mapping/lua-api.md
/// §1, §6). Issue 009/10 back this with the real `oh-assets` registry.
///
/// Stays in this crate (unlike `GameApi`, which moved to
/// `oh_game::lua_state`): the signatures return `LuaError`, an `oh-lua`
/// type `oh-game` cannot reference (D-013 note).
pub trait AssetsApi: Send + Sync {
    /// C++ `assets.getMusicData(packId, id)` (copied into
    /// `HexagonGame::musicData`); throws when the id is unknown.
    fn get_music_data(&self, pack_id: &str, id: &str) -> Result<MusicData, LuaError>;

    /// C++ `assets.getStyleData(packId, id)`; throws when the id is unknown.
    fn get_style_data(&self, pack_id: &str, id: &str) -> Result<StyleData, LuaError>;

    /// C++ `assets.findPackData(disambiguator, name, author)`.
    fn find_pack_data(&self, disambiguator: &str, name: &str, author: &str) -> Option<PackData>;

    /// C++ `assets.getShaderIdByPath(path)` — `None` when the path is not a
    /// loaded shader (the `shdr_*` lambdas log + return `size_t(-1)`).
    fn get_shader_id_by_path(&self, path: &str) -> Option<u64>;

    /// C++ `assets.isValidShaderId(id)` (`id < shadersById.size()`).
    fn is_valid_shader_id(&self, shader_id: u64) -> bool;

    /// C++ `shader.setUniformUnsafe(name, a)` on the shader with `shader_id`.
    fn set_shader_uniform_f(&self, shader_id: u64, name: &str, a: f32);

    /// C++ `shader.setUniformUnsafe(name, sf::Glsl::Vec2{a, b})`.
    fn set_shader_uniform_f_vec2(&self, shader_id: u64, name: &str, a: f32, b: f32);

    /// C++ `shader.setUniformUnsafe(name, sf::Glsl::Vec3{a, b, c})`.
    fn set_shader_uniform_f_vec3(&self, shader_id: u64, name: &str, a: f32, b: f32, c: f32);

    /// C++ `shader.setUniformUnsafe(name, sf::Glsl::Vec4{a, b, c, d})`.
    fn set_shader_uniform_f_vec4(&self, shader_id: u64, name: &str, a: f32, b: f32, c: f32, d: f32);

    /// C++ `shader.setUniformUnsafe(name, a)` (integer uniform).
    fn set_shader_uniform_i(&self, shader_id: u64, name: &str, a: i32);

    /// C++ `shader.setUniformUnsafe(name, sf::Glsl::Ivec2{a, b})`.
    fn set_shader_uniform_i_vec2(&self, shader_id: u64, name: &str, a: i32, b: i32);

    /// C++ `shader.setUniformUnsafe(name, sf::Glsl::Ivec3{a, b, c})`.
    fn set_shader_uniform_i_vec3(&self, shader_id: u64, name: &str, a: i32, b: i32, c: i32);

    /// C++ `shader.setUniformUnsafe(name, sf::Glsl::Ivec4{a, b, c, d})`.
    fn set_shader_uniform_i_vec4(&self, shader_id: u64, name: &str, a: i32, b: i32, c: i32, d: i32);
}

// ---------------------------------------------------------------------------
// GameApiState
// ---------------------------------------------------------------------------

/// C++ `fRunLuaFile` closure type (the `GameApiState::run_lua_file` field
/// and the engine callbacks): takes the pack-relative path + the shared
/// context (execution happens in the same Lua state). `Send + Sync`
/// (issue-007 contract, unchanged) so `GameApiState` stays `Send + Sync`
/// for the `Arc`-shared `State` handle.
pub type RunLuaFile = Arc<dyn Fn(&str, &SharedLua) -> Result<(), LuaError> + Send + Sync + 'static>;

/// Everything the 285 registered functions read or write.
///
/// The gameplay data moved to [`SharedState`] (issue 009 / D-013) so the
/// engine update loop in `oh-game` can share it; this wrapper keeps the
/// composition-only fields (`run_lua_file`, `assets`, `backend`) and
/// derefs into the shared store, so every lambda from issue 007
/// compiles UNCHANGED.
pub struct GameApiState {
    pub shared: SharedState,
    // C++ `fRunLuaFile` (`HexagonGame::runLuaFile` — file cache +
    // error -> `goToMenu`). The engine supplies it; it takes the shared
    // context because executing the file happens in the same Lua state.
    pub run_lua_file: RunLuaFile,
    pub assets: Arc<dyn AssetsApi>,
    pub backend: Arc<dyn GameApi>,
}

impl Deref for GameApiState {
    type Target = SharedState;

    fn deref(&self) -> &SharedState {
        &self.shared
    }
}

impl DerefMut for GameApiState {
    fn deref_mut(&mut self) -> &mut SharedState {
        &mut self.shared
    }
}

/// The D-015 engine accessor: the composition's wrapper is a REAL
/// (non-reflexive) wrapper, so its `SharedStore` impl just points at the
/// inner `SharedState`. `HexagonGame<GameApiState>` uses `shared()`/
/// `shared_mut()` for all engine access; the `Deref` impls above only
/// serve the 007 lambdas (`lock(&st).level_status.x`).
impl SharedStore for GameApiState {
    fn shared(&self) -> &SharedState {
        &self.shared
    }

    fn shared_mut(&mut self) -> &mut SharedState {
        &mut self.shared
    }
}

impl GameApiState {
    /// `GameApiState::new(...)`. Every field takes its C++ initial value
    /// (`difficultyMult = 1`, `firstPlay = true`, empty timelines/walls/logs).
    /// One argument per C++-captured value (issue 009 supplies them); a
    /// builder would add no semantics, so the count exceeds clippy's limit.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn new(
        rng: oh_rng::RandomNumberGenerator,
        level_status: oh_game::LevelStatus,
        status: oh_game::HexagonGameStatus,
        style_data: StyleData,
        music_data: MusicData,
        in_menu: bool,
        headless: bool,
        pack_path: String,
        level_name: String,
        pack_id: String,
        pack_data: PackData,
        run_lua_file: RunLuaFile,
        assets: Arc<dyn AssetsApi>,
        backend: Arc<dyn GameApi>,
    ) -> Self {
        Self {
            shared: SharedState::new(
                rng,
                level_status,
                status,
                style_data,
                music_data,
                in_menu,
                headless,
                pack_path,
                level_name,
                pack_id,
                pack_data,
            ),
            run_lua_file,
            assets,
            backend,
        }
    }

    /// C++ `HexagonGame::raiseWarning(functionName, additionalInfo)`: warns
    /// once per function per level load, appends to `ilcCmdLog` (body lives
    /// on `SharedState`; this forwards the `backend` log sink).
    pub fn raise_warning(&mut self, function_name: &str, additional_info: &str) {
        self.shared
            .raise_warning(&*self.backend, function_name, additional_info)
    }

    /// C++ `HexagonGame::runLuaFile` body executed by the engine-provided
    /// `run_lua_file` callback; this is the *dependency/script path* helper
    /// of `Utils::getDependentScriptFilename` (mapping §6.2):
    /// `<context>/Scripts/<name>` where the context is the innermost active
    /// dependency folder, or the level's own pack path if none is active.
    #[must_use]
    pub fn get_dependent_script_filename(&self, script_name: &str) -> String {
        self.shared.get_dependent_script_filename(script_name)
    }

    /// C++ `Utils::getDependentShaderFilename`.
    #[must_use]
    pub fn get_dependent_shader_filename(&self, shader_name: &str) -> String {
        self.shared.get_dependent_shader_filename(shader_name)
    }

    /// C++ `Utils::withDependencyAssetFilename`'s
    /// `catch(const std::runtime_error& err)` (Utils.cpp:193-200): the
    /// "Fatal error while looking for Lua dependency" line, logged
    /// BEFORE the rethrow (the port returns the error to the Lua call
    /// frame instead of unwinding a C++ exception — the script still
    /// aborts, exactly the C++ behaviour). The `catch(...)` arm
    /// (Utils.cpp:201-207, "Fatal unknown error ...") has no port
    /// analogue: the only error flowing through the dependency lookup is
    /// the `LuaError` (the C++ `std::runtime_error` class).
    pub fn log_dependency_lookup_failure(&self, err: &LuaError) {
        self.backend.log(
            "hg::Utils::withDependencyAssetFilename",
            &format!("Fatal error while looking for Lua dependency\nError: {err}\n"),
        );
    }

    /// C++ `Utils::findDependencyPackDataOrThrow(assets, currentPack, ...)`.
    ///
    /// Faithful to the C++ quirk: `currentPack` is the level's OWN pack
    /// (`getPackData()`), even when called from inside a dependency's script.
    /// Returns the dependency's `PackData` or the C++ `runtime_error` message
    /// (the port surfaces it as a Lua runtime error instead of unwinding a
    /// C++ exception through the C API stack).
    pub fn find_dependency_pack_data_or_throw(
        &self,
        disambiguator: &str,
        name: &str,
        author: &str,
    ) -> Result<PackData, LuaError> {
        let current = &self.pack_data;
        let dep = current
            .dependencies
            .iter()
            .find(|d| {
                d.disambiguator == disambiguator && d.name == name && d.author == author
            })
            .ok_or_else(|| {
                LuaError::ExecutionError {
                    message: format!(
                        "Pack with disambiguator '{disambiguator}', name '{name}', author: '{author}' is not a dependency of '{}'\n",
                        current.name
                    ),
                }
            })?;

        let dependency_data = self
            .assets
            .find_pack_data(disambiguator, name, author)
            .ok_or_else(|| LuaError::ExecutionError {
                message: format!(
                    "Could not find dependency pack with disambiguator '{disambiguator}', name '{name}', author: '{author}'\n"
                ),
            })?;

        if dependency_data.version < dep.min_version {
            return Err(LuaError::ExecutionError {
                message: format!(
                    "Dependency pack with disambiguator '{disambiguator}', name '{name}', author: '{author}' has version '{}' but at least '{}' is required\n",
                    dependency_data.version, dep.min_version
                ),
            });
        }

        Ok(dependency_data)
    }
}
