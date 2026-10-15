//! The state the Lua game API touches + the engine backend interface
//! (issue 007).
//!
//! This crate must not depend on the not-yet-existing game loop (issue 009),
//! so the API is registered against this interface instead of a concrete
//! `HexagonGame`:
//!
//! * [`GameApiState`] — the exact data every C++ API lambda reads or writes
//!   (RNG, `LevelStatus`, `HexagonGameStatus`, `StyleData`, the timelines,
//!   the custom-wall manager, the wall vector, difficulty multiplier,
//!   `firstPlay`, the in-level-console tracking vectors, the exec-script
//!   pack-path context stack, pack identity, `MusicData`). All C++-concrete
//!   types come from `oh-game` / `oh-data` / `oh-rng`; the deferred-action
//!   semantics (`t_*`/`e_*`/`ct_*`/`w_*` schedule onto these timelines) are
//!   reproduced by appending closures onto exactly these fields.
//! * [`GameApi`] — the engine operations the lambdas delegate to (`death`,
//!   `createWall`, `performPlayerSwap`, audio, messages, input, ...).
//!   Issue 009 supplies the real implementation; the C++ lambdas' exact
//!   behavior (`.agents/plans/001-rust-port/mapping/lua-api.md`) is the spec.
//! * [`AssetsApi`] — the `HGAssets` lookups the lambdas perform (music/style
//!   data, dependency-pack discovery, shader registry).
//!
//! Every registered closure locks the state (`Arc<Mutex<GameApiState>>`)
//! before touching it, exactly mirroring the C++ lambdas capturing
//! `HexagonGame&`/`this`.

use std::collections::HashSet;
use std::sync::Arc;

use oh_data::pack_data::PackData;
use oh_data::style_data::StyleData;
use oh_data::MusicData;
use oh_game::{
    CCustomWallManager, CWall, CustomTimelineManager, HexagonGameStatus, LevelStatus, SpeedData,
    Timeline2,
};
use oh_rng::RandomNumberGenerator;

use crate::error::LuaError;
use crate::LuaContext;

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
// GameApi — the engine operations the API lambdas delegate to
// ---------------------------------------------------------------------------

/// C++ `HexagonGame` operations the API lambdas perform (mapping/lua-api.md
/// §1 semantics are the spec). Issue 009 implements this against the real
/// game loop; tests implement it with a recording mock.
pub trait GameApi: Send + Sync {
    // -- Config -----------------------------------------------------------
    /// C++ `Config::getWidth()`.
    fn get_width(&self) -> u32;
    /// C++ `Config::getHeight()`.
    fn get_height(&self) -> u32;
    /// C++ `Config::getOfficial()`.
    fn get_official(&self) -> bool;
    /// C++ `Config::getDebug()`.
    fn get_debug(&self) -> bool;

    // -- input (C++ `window != nullptr && window->getInputState()[...]`) --
    /// False when headless.
    fn is_key_pressed(&self, key_code: i32) -> bool;
    /// False when headless.
    fn is_mouse_button_pressed(&self, button_code: i32) -> bool;

    // -- logging (C++ `ssvu::lo(...)` / `std::cout` channels) -------------
    fn log(&self, channel: &str, message: &str);

    // -- flash ------------------------------------------------------------
    /// C++ `HexagonGame::initFlashEffect(r, g, b)`.
    fn init_flash_effect(&self, r: i32, g: i32, b: i32);

    // -- player -----------------------------------------------------------
    /// C++ `player.getPlayerAngle()` (radians).
    fn get_player_angle(&self) -> f32;
    /// C++ `player.setPlayerAngle(angle)`.
    fn set_player_angle(&self, angle: f32);
    /// C++ `player.updatePosition(radius)` (from `l_forceSetBeatPulse`).
    fn update_player_position(&self, radius: f32);

    // -- difficulty / swap --------------------------------------------------
    /// C++ `HexagonGame::incrementDifficulty()` — also fires `onIncrement`.
    fn increment_difficulty(&self);
    /// C++ `HexagonGame::performPlayerSwap(playSound)` — `playerSwap()`,
    /// fires `onCursorSwap`, plays `levelStatus.swapSound` when `play_sound`.
    fn perform_player_swap(&self, play_sound: bool);

    // -- audio (C++ HexagonGame music/sound methods) ----------------------
    fn stop_level_music(&self);
    fn play_level_music(&self);
    /// C++ `playLevelMusicAtTime(seconds)`.
    fn play_level_music_at_time(&self, seconds: f32);
    /// C++ `playSoundOverride(id)`.
    fn play_sound_override(&self, sound_id: &str);
    /// C++ `playPackSoundOverride(packId, fileName)`.
    fn play_pack_sound_override(&self, pack_id: &str, file_name: &str);
    /// C++ `HexagonGame::refreshMusicPitch()`.
    fn refresh_music_pitch(&self);

    // -- messages ----------------------------------------------------------
    /// C++ `HexagonGame::addMessage(message, duration, soundToggle)`.
    fn add_message(&self, message: &str, duration: f64, sound_toggle: bool);
    /// C++ `HexagonGame::clearMessages()`.
    fn clear_messages(&self);

    // -- death -------------------------------------------------------------
    /// C++ `HexagonGame::death(force)` (fires `onPreDeath`/`onDeath`).
    fn death(&self, force: bool);

    // -- walls -------------------------------------------------------------
    /// C++ `HexagonGame::createWall(side, thickness, speed, curve, hueMod)`.
    fn create_wall(
        &self,
        side: i32,
        thickness: f32,
        speed: SpeedData,
        curve: SpeedData,
        hue_mod: f32,
    );

    // -- pulse -------------------------------------------------------------
    /// C++ `HexagonGame::refreshPulse()`.
    fn refresh_pulse(&self);
    /// C++ `HexagonGame::refreshBeatPulse()`.
    fn refresh_beat_pulse(&self);

    // -- background camera --------------------------------------------------
    /// `Some(rotation)` iff C++ `backgroundCamera.has_value()`; the value is
    /// `backgroundCamera->getRotation()` (degrees).
    fn background_camera_rotation(&self) -> Option<f32>;
    /// C++ `backgroundCamera->setRotation(value)` (no-op when no camera; the
    /// callers check `background_camera_rotation().is_some()` first).
    fn set_background_camera_rotation(&self, value: f32);

    // -- replay / steam ------------------------------------------------------
    /// C++ `HexagonGame::inReplay()`.
    fn in_replay(&self) -> bool;
    /// C++ `steamManager->unlock_achievement(id)` (no-op when there is no
    /// steam manager; the caller already checked `in_replay` / official).
    fn unlock_achievement(&self, achievement_id: &str);

    // -- error exit ----------------------------------------------------------
    /// C++ `goToMenu(sendScores, mError)`.
    fn go_to_menu(&self, send_scores: bool, error: bool);
}

// ---------------------------------------------------------------------------
// GameApiState
// ---------------------------------------------------------------------------

/// C++ `fRunLuaFile` closure type (the `GameApiState::run_lua_file` field
/// and the engine callbacks): takes the pack-relative path + the shared
/// context (execution happens in the same Lua state).
pub type RunLuaFile = Arc<dyn Fn(&str, &SharedLua) -> Result<(), LuaError> + Send + Sync>;

/// Everything the 285 registered functions read or write.
///
/// Field-for-field mirror of the C++ `HexagonGame`/`LuaScripting` state the
/// lambdas capture (`mapping/lua-api.md` §1). Wrapped in
/// `Arc<Mutex<GameApiState>>` for the `'static` closures `add_lua_fn`
/// requires (C++ captured `this` / references).
pub struct GameApiState {
    // C++ captured flags (menu: `inMenu == true`, static `rng{0}`, static
    // status; game: `inMenu == false`, `headless == (window == nullptr)`).
    pub in_menu: bool,
    pub headless: bool,

    // C++ `HexagonGame::rng` (shared with the engine's per-frame advances).
    pub rng: RandomNumberGenerator,

    // C++ `HexagonGame::levelStatus` / `status` / `styleData` / `musicData`.
    pub level_status: LevelStatus,
    pub status: HexagonGameStatus,
    pub style_data: StyleData,
    pub music_data: MusicData,

    // C++ `HexagonGame::cwManager` / `timeline` / `eventTimeline` /
    // `_customTimelineManager` / `walls`.
    pub cw_manager: CCustomWallManager,
    pub main_timeline: Timeline2,
    pub event_timeline: Timeline2,
    pub custom_timelines: CustomTimelineManager,
    pub walls: Vec<CWall>,

    // C++ `HexagonGame::difficultyMult` / `firstPlay`.
    pub difficulty_mult: f32,
    pub first_play: bool,

    // C++ `HexagonGame::ilcLuaTrackedResults` / `ilcCmdLog` /
    // `calledDeprecatedFunctions`.
    pub ilc_lua_tracked_results: Vec<String>,
    pub ilc_cmd_log: Vec<String>,
    pub called_deprecated_functions: HashSet<String>,

    // C++ `HexagonGame::execScriptPackPathContext` (dependency-scope stack).
    pub exec_script_pack_path_context: Vec<String>,

    // C++ `levelData->packPath` / `levelData->name` / `levelData->packId`
    // (`getPackId()`) / `getPackData()` (the level's OWN pack — the C++
    // dependency check always validates against the level pack, not the
    // innermost dependency's pack).
    pub pack_path: String,
    pub level_name: String,
    pub pack_id: String,
    pub pack_data: PackData,

    // C++ `fRunLuaFile` (`HexagonGame::runLuaFile` — file cache +
    // error -> `goToMenu`). The engine supplies it; it takes the shared
    // context because executing the file happens in the same Lua state.
    pub run_lua_file: RunLuaFile,
    pub assets: Arc<dyn AssetsApi>,
    pub backend: Arc<dyn GameApi>,
}

impl GameApiState {
    /// `GameApiState::new(...)`. Every field takes its C++ initial value
    /// (`difficultyMult = 1`, `firstPlay = true`, empty timelines/walls/logs).
    /// One argument per C++-captured value (issue 009 supplies them); a
    /// builder would add no semantics, so the count exceeds clippy's limit.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn new(
        rng: RandomNumberGenerator,
        level_status: LevelStatus,
        status: HexagonGameStatus,
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
            in_menu,
            headless,
            rng,
            level_status,
            status,
            style_data,
            music_data,
            cw_manager: CCustomWallManager::new(),
            main_timeline: Timeline2::new(),
            event_timeline: Timeline2::new(),
            custom_timelines: CustomTimelineManager::new(),
            walls: Vec::new(),
            difficulty_mult: 1.0,
            first_play: true,
            ilc_lua_tracked_results: Vec::new(),
            ilc_cmd_log: Vec::new(),
            called_deprecated_functions: HashSet::new(),
            exec_script_pack_path_context: Vec::new(),
            pack_path,
            level_name,
            pack_id,
            pack_data,
            run_lua_file,
            assets,
            backend,
        }
    }

    /// C++ `HexagonGame::getSpeedMultDM()` — pure function of
    /// `levelStatus` + `difficultyMult` (verbatim formula, no reordering).
    #[must_use]
    pub fn get_speed_mult_dm(&self) -> f32 {
        let res = self.level_status.speed_mult * self.difficulty_mult.powf(0.65);
        if !self.level_status.has_speed_max_limit() {
            return res;
        }
        if res < self.level_status.speed_max {
            res
        } else {
            self.level_status.speed_max
        }
    }

    /// C++ `HexagonGame::getDelayMultDM()`.
    #[must_use]
    pub fn get_delay_mult_dm(&self) -> f32 {
        let res = self.level_status.delay_mult / self.difficulty_mult.powf(0.10);
        if !self.level_status.has_delay_max_limit() {
            return res;
        }
        if res < self.level_status.delay_max {
            res
        } else {
            self.level_status.delay_max
        }
    }

    /// C++ `HexagonGame::raiseWarning(functionName, additionalInfo)`: warns
    /// once per function per level load, appends to `ilcCmdLog`.
    pub fn raise_warning(&mut self, function_name: &str, additional_info: &str) {
        if !self
            .called_deprecated_functions
            .insert(function_name.to_string())
        {
            return;
        }
        let error_msg = format!(
            "[Lua] WARNING: The function \"{}\" (used in level \"{}\") is deprecated. {}",
            function_name, self.level_name, additional_info
        );
        self.backend.log("lua", &error_msg);
        self.ilc_cmd_log.push(format!("[warning]: {error_msg}\n"));
    }

    /// C++ `HexagonGame::runLuaFile` body executed by the engine-provided
    /// `run_lua_file` callback; this is the *dependency/script path* helper
    /// of `Utils::getDependentScriptFilename` (mapping §6.2):
    /// `<context>/Scripts/<name>` where the context is the innermost active
    /// dependency folder, or the level's own pack path if none is active.
    #[must_use]
    pub fn get_dependent_script_filename(&self, script_name: &str) -> String {
        self.get_dependent_asset_filename("Scripts", script_name)
    }

    /// C++ `Utils::getDependentShaderFilename`.
    #[must_use]
    pub fn get_dependent_shader_filename(&self, shader_name: &str) -> String {
        self.get_dependent_asset_filename("Shaders", shader_name)
    }

    /// C++ `getDependentAssetFilename(subfolder, ctx, currentPackPath, name)`.
    fn get_dependent_asset_filename(&self, subfolder: &str, name: &str) -> String {
        let context = self
            .exec_script_pack_path_context
            .last()
            .map_or(&self.pack_path, |s| s);
        format!("{context}/{subfolder}/{name}")
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
