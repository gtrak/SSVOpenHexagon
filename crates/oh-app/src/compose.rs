//! The headless engine composition (issue 15 step (b)): a REAL
//! `oh_game::seams::ScriptHost` over `oh-lua`, a real `AssetsApi` adapter
//! over `oh_assets::Assets`, and the two-phase `HexagonGame` build.
//!
//! Promoted from the issue 15 step (a) test tree
//! (`crates/oh-app/tests/common/mod.rs`) so the `open-hexagon` binary
//! (step (b)) and the step (a) tests share ONE composition — the spec's
//! dedup. `crates/oh-game/tests/common/mod.rs` keeps its own step-(c-era)
//! copy (out of this issue's scope).
//!
//! # Two-phase build (DECISIONS.md D-013)
//!
//! `HexagonGame` stores its `ScriptHost` as `Arc<dyn ScriptHost>` at
//! construction, but the `GameApiState` the host drives needs
//! `Arc<dyn GameApi>` = a clone of that very engine. So:
//!
//! 1. build the host with its `State` slot empty (`OnceLock`),
//! 2. build the engine with the host,
//! 3. clone the engine into `Arc<dyn GameApi>` (the engine IS the backend —
//!    `impl GameApi for HexagonGame<S>`),
//! 4. build `GameApiState` / `State` with that backend + the `run_lua_file`
//!    closure (the C++ `fRunLuaFile` lambda, `HGScripting.cpp:1107`),
//! 5. `host.attach_state(state)` + `game.attach_state(state)` — after that
//!    the engine and the 285 registered lambdas mutate ONE store.
//!
//! # `ScriptHost: Send + Sync` given `mlua::Lua: !Send`
//!
//! `mlua`'s `send` feature is deliberately OFF (D-013), so a struct that
//! *owns* a `LuaContext` can never be `Send`. The bridge is
//! `oh_lua::api::state::SharedLua`: a `Copy` handle holding a raw
//! `*const LuaContext` with documented `unsafe impl Send/Sync`, whose
//! contract is "the `LuaContext` must outlive every closure holding a
//! `SharedLua`, and it is only dereferenced on the single thread that owns
//! it." The composition heap-pins the `LuaContext` in a `Box` held by
//! [`Composed`] (a field declared LAST, so it drops AFTER the engine/state/
//! host that carry `SharedLua`), never moves it (the `SharedLua` captured
//! at composition time points at the box's slot — a moved context would
//! dangle), and every Lua call happens on the composing thread — exactly
//! the C++ object layout where `HexagonGame::lua` and its timelines die
//! together.
//!
//! # One `LuaContext` per engine (documented bridge behaviour)
//!
//! The C++ `newGame` does `lua = Lua::LuaContext{}; initLua();` — a FRESH
//! context per game. The composition reuses ONE context and re-runs
//! `init_lua_api` + `init_level_api` on each `new_game`: redefining the 285
//! globals is idempotent (every registered closure is stateless — it locks
//! the shared `State`), and the engine clears `main_timeline` /
//! `event_timeline` / `custom_timelines` / `walls` /
//! `called_deprecated_functions` before the level script re-runs, so the
//! re-executed script observes the same fresh state the C++ fresh context
//! would. Chunk-local Lua state is the one difference (it survives a
//! re-`newGame` here) — no shipped level relies on chunk locals persisting
//! or resetting in a way the replay property (D-018) observes.

use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock, RwLock};

use oh_config::Config;
use oh_data::{MusicData, PackData, StyleData};
use oh_discord::Discord;
use oh_game::hexagon_game::HexagonGame;
use oh_game::lua_state::{GameApi, ReentrantMutex};
use oh_game::renderer::Renderer;
use oh_game::seams::{
    Audio, NoJoystick, NullReplayStore, PcgCosmeticRng, Profiles, ReplayStore, ScriptError,
    ScriptHost, SteamSink,
};
use oh_game::{HexagonGameStatus, LevelStatus};
use oh_lua::api::callbacks;
use oh_lua::api::state::{AssetsApi, GameApiState, RunLuaFile, SharedLua};
use oh_lua::api::{init_level_api, init_lua_api, State};
use oh_lua::{LuaContext, LuaError};
use oh_rng::RandomNumberGenerator;

/// The `HGAssets` lookups the 007 lambdas perform, backed by the real
/// `oh_assets::Assets` registry (mapping/lua-api.md §1/§6).
///
/// `get_music_data` / `get_style_data`: the C++ `HGAssetsImpl::get*Data`
/// never throws (missing id → log + fail-soft fallback to the smallest-key
/// entry); the oh-lua seam contract instead returns `Err` when the lookup
/// yields nothing (the fail-soft fallback already ran inside `Assets`, so
/// `None` means the whole map is empty) with the C++ log line's text.
///
/// The `set_shader_uniform_*` methods are no-ops: C++ routes them to
/// `sf::Shader::setUniformUnsafe` on the GL backend, which does not exist
/// in the headless composition (issue 10 owns the GL surface).
pub struct RealAssetsApi {
    assets: Arc<oh_assets::Assets>,
}

impl RealAssetsApi {
    #[must_use]
    pub fn new(assets: Arc<oh_assets::Assets>) -> Self {
        Self { assets }
    }
}

impl AssetsApi for RealAssetsApi {
    fn get_music_data(&self, pack_id: &str, id: &str) -> Result<MusicData, LuaError> {
        self.assets
            .get_music_data(pack_id, id)
            .ok_or_else(|| LuaError::ExecutionError {
                message: format!("Asset '{pack_id}_{id}' not found\n"),
            })
    }

    fn get_style_data(&self, pack_id: &str, id: &str) -> Result<StyleData, LuaError> {
        self.assets
            .get_style_data(pack_id, id)
            .ok_or_else(|| LuaError::ExecutionError {
                message: format!("Asset '{pack_id}_{id}' not found\n"),
            })
    }

    fn find_pack_data(&self, disambiguator: &str, name: &str, author: &str) -> Option<PackData> {
        self.assets
            .find_pack_data(disambiguator, name, author)
            .cloned()
    }

    fn get_shader_id_by_path(&self, path: &str) -> Option<u64> {
        self.assets.get_shader_id_by_path(path).map(|id| id as u64)
    }

    fn is_valid_shader_id(&self, shader_id: u64) -> bool {
        usize::try_from(shader_id).is_ok_and(|id| self.assets.is_valid_shader_id(id))
    }

    fn set_shader_uniform_f(&self, _shader_id: u64, _name: &str, _a: f32) {}
    fn set_shader_uniform_f_vec2(&self, _shader_id: u64, _name: &str, _a: f32, _b: f32) {}
    fn set_shader_uniform_f_vec3(&self, _shader_id: u64, _name: &str, _a: f32, _b: f32, _c: f32) {}
    fn set_shader_uniform_f_vec4(
        &self,
        _shader_id: u64,
        _name: &str,
        _a: f32,
        _b: f32,
        _c: f32,
        _d: f32,
    ) {
    }
    fn set_shader_uniform_i(&self, _shader_id: u64, _name: &str, _a: i32) {}
    fn set_shader_uniform_i_vec2(&self, _shader_id: u64, _name: &str, _a: i32, _b: i32) {}
    fn set_shader_uniform_i_vec3(&self, _shader_id: u64, _name: &str, _a: i32, _b: i32, _c: i32) {}
    fn set_shader_uniform_i_vec4(
        &self,
        _shader_id: u64,
        _name: &str,
        _a: i32,
        _b: i32,
        _c: i32,
        _d: i32,
    ) {
    }
}

/// The C++ `Lua::LuaContext` callback surface, driven by `oh-lua`'s
/// `api::callbacks::*` (mapping/lua-api.md §5) on a `SharedLua` handle.
pub struct LuaScriptHost {
    lua: SharedLua,
    state: OnceLock<State>,
    headless: bool,
    /// The process-CWD equivalent (the `_RELEASE` dir, or a synthetic pack
    /// root): C++ level-script paths are CWD-relative.
    root: PathBuf,
    config: Arc<RwLock<Config>>,
    /// C++ `assets.getLuaFileCache()` — a per-assets `unordered_map<string,
    /// string>`. oh-assets defers that member, so the host owns the
    /// equivalent map (one host per one `Assets` in this tree, so the
    /// "per-assets" scope is preserved).
    file_cache: ReentrantMutex<HashMap<String, String>>,
}

impl LuaScriptHost {
    pub fn new(
        ctx: &LuaContext,
        headless: bool,
        root: PathBuf,
        config: Arc<RwLock<Config>>,
    ) -> Self {
        Self {
            lua: SharedLua::new(ctx),
            state: OnceLock::new(),
            headless,
            root,
            config,
            file_cache: ReentrantMutex::new(HashMap::new()),
        }
    }

    /// Two-phase build: the state exists only after the engine does.
    pub fn attach_state(&self, state: State) {
        if self.state.set(state).is_err() {
            panic!("LuaScriptHost state attached twice");
        }
    }

    fn state(&self) -> &State {
        self.state
            .get()
            .expect("LuaScriptHost used before attach_state")
    }

    /// C++ `HexagonGame::runLuaFile` (`HGScripting.cpp:1127-1147`) MINUS its
    /// catch: headless or `Config::getUseLuaFileCache()` takes the *cached*
    /// path, else the uncached one. `Err(String)` carries the C++ exception
    /// `what()` — the caller performs the Lippincott behavior (the engine's
    /// `host_error` for the `newGame` path; the `run_lua_file` closure for
    /// `u_execScript`).
    pub fn execute_file(&self, shared: &SharedLua, path: &str) -> Result<(), String> {
        let full = self.root.join(path);
        let use_cache = self.headless
            || self
                .config
                .read()
                .expect("config poisoned")
                .get_use_lua_file_cache();
        if use_cache {
            self.run_lua_file_cached(shared, path, &full)
        } else {
            self.run_lua_file_uncached(shared, path, &full)
        }
    }

    /// C++ `Utils::runLuaFileCached` (`Utils.cpp:49-78`): read the file bytes
    /// once into the per-assets cache, then `execute_code` on the CACHED
    /// string (a second call for the same path never re-reads).
    fn run_lua_file_cached(
        &self,
        shared: &SharedLua,
        path: &str,
        full: &Path,
    ) -> Result<(), String> {
        let code = {
            let mut cache = self.file_cache.lock();
            if !cache.contains_key(path) {
                // C++: `std::ifstream t(...)` + `seekg`/`tellg`/`read` into
                // the map — a missing file makes `tellg()` fail, and
                // `buffer.resize(-1)` throws `std::bad_alloc` (no shipped
                // level hits this path; the message is the libstdc++ text
                // the C++ catch would see).
                let bytes = std::fs::read(full).map_err(|_| "std::bad_alloc".to_string())?;
                cache.insert(
                    path.to_string(),
                    String::from_utf8_lossy(&bytes).into_owned(),
                );
            }
            cache.get(path).expect("just inserted").clone()
        };
        shared.execute_code(&code).map_err(|e| e.to_string())
    }

    /// C++ `Utils::runLuaFile` (`Utils.cpp:80-110`): open failure ⇒
    /// `runtime_error("Fatal Lua error\nCould not open file: <name>\n")`
    /// (printed to the `hg::Utils::runLuaFile` channel, then thrown);
    /// execute error ⇒ print `"Fatal Lua error\nFilename: <name>\nError:
    /// <what>\n"` and rethrow (the propagated `what()` is the Lua error).
    fn run_lua_file_uncached(
        &self,
        shared: &SharedLua,
        path: &str,
        full: &Path,
    ) -> Result<(), String> {
        let bytes = std::fs::read(full).map_err(|_| {
            let msg = format!("Fatal Lua error\nCould not open file: {path}\n");
            // C++ `ssvu::lo("hg::Utils::runLuaFile") << errorStr`.
            println!("[hg::Utils::runLuaFile] {msg}");
            msg
        })?;
        let code = String::from_utf8_lossy(&bytes).into_owned();
        match shared.execute_code(&code) {
            Ok(()) => Ok(()),
            Err(e) => {
                // C++ `ssvu::lo("hg::Utils::runLuaFile") << "Fatal Lua
                // error\n" << "Filename: " << mFileName << '\n' << "Error: "
                // << mError.what() << '\n'` — then `throw;` (same `what()`).
                println!("[hg::Utils::runLuaFile] Fatal Lua error\nFilename: {path}\nError: {e}\n");
                Err(e.to_string())
            }
        }
    }
}

impl ScriptHost for LuaScriptHost {
    /// C++ `HexagonGame::initLua` = `LuaScripting::init` (the shared
    /// registration) + the ten per-game `initLua_*` groups.
    fn init_lua(&self) {
        let st = self.state();
        init_lua_api(self.lua.context(), st).expect("init_lua_api failed");
        init_level_api(self.lua.context(), st).expect("init_level_api failed");
    }

    fn run_lua_file(&self, path: &str) -> Result<(), String> {
        self.execute_file(&self.lua, path)
    }

    fn run_on_init(&self) -> Result<(), String> {
        callbacks::call_on_init(self.lua.context()).map_err(|e| e.to_string())
    }

    fn run_on_load(&self) -> Result<(), String> {
        callbacks::call_on_load(self.lua.context()).map_err(|e| e.to_string())
    }

    fn run_on_update(&self, ft: f32) -> Result<(), String> {
        callbacks::call_on_update(self.lua.context(), ft).map_err(|e| e.to_string())
    }

    fn run_on_step(&self) -> Result<(), String> {
        callbacks::call_on_step(self.lua.context()).map_err(|e| e.to_string())
    }

    fn run_on_input(
        &self,
        ft: f32,
        movement: i32,
        focused: bool,
        swap: bool,
    ) -> Result<Option<bool>, String> {
        // `call_on_input` already applies the C++ nil→false coercion; a
        // missing callback yields `false` here (`Some(false)`), which the
        // engine treats exactly like C++'s `std::nullopt` (only
        // `Some(true)` vetoes the default input).
        callbacks::call_on_input(self.lua.context(), ft, movement, focused, swap)
            .map(Some)
            .map_err(|e| e.to_string())
    }

    fn run_on_pre_death(&self) -> Result<(), String> {
        callbacks::call_on_pre_death(self.lua.context()).map_err(|e| e.to_string())
    }

    fn run_on_death(&self) -> Result<(), String> {
        callbacks::call_on_death(self.lua.context()).map_err(|e| e.to_string())
    }

    fn run_on_increment(&self) -> Result<(), String> {
        callbacks::call_on_increment(self.lua.context()).map_err(|e| e.to_string())
    }

    fn run_on_pre_unload(&self) -> Result<(), String> {
        callbacks::call_on_pre_unload(self.lua.context()).map_err(|e| e.to_string())
    }

    fn run_on_unload(&self) -> Result<(), String> {
        callbacks::call_on_unload(self.lua.context()).map_err(|e| e.to_string())
    }

    fn run_on_cursor_swap(&self) -> Result<(), String> {
        callbacks::call_on_cursor_swap(self.lua.context()).map_err(|e| e.to_string())
    }

    fn run_on_render_stage(&self, stage: i32, frametime: f32) -> Result<(), String> {
        // C++ `runLuaFunctionIfExists<int, float>("onRenderStage", rs, ft)`
        // (`HGGraphics.cpp:74-77`); the int return is discarded.
        callbacks::call_on_render_stage(self.lua.context(), stage, frametime)
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    fn read_variable_f32(&self, name: &str) -> Result<f32, String> {
        self.lua
            .context()
            .read_variable(name)
            .map_err(|e| e.to_string())
    }

    fn variable_exists(&self, name: &str) -> bool {
        // The engine only queries plain globals (`scoreOverride`, tracked
        // names), for which C++ `doesVariableExist` never throws.
        self.lua
            .context()
            .does_variable_exist(name)
            .unwrap_or(false)
    }

    fn read_variable_string(&self, name: &str) -> Result<String, String> {
        self.lua
            .context()
            .read_variable(name)
            .map_err(|e| e.to_string())
    }

    // -- the headless Lua console (issue 014 step (a)) ----------------------
    fn execute_code(&self, code: &str) -> Result<(), ScriptError> {
        // The composition's `LuaContext` failures are all
        // `std::runtime_error`-class (C++ `ExecutionErrorException` & co.),
        // so the console's outer `catch(...)` is unreachable from here.
        self.lua
            .execute_code(code)
            .map_err(|e| ScriptError::Runtime(e.to_string()))
    }
    fn all_function_names(&self) -> Vec<String> {
        oh_lua::docs::get_all_function_names()
    }
    fn docs_for_function(&self, name: &str) -> String {
        oh_lua::docs::get_docs_for_function(name)
    }
    fn print_docs(&self, out: &mut dyn Write) -> std::io::Result<()> {
        oh_lua::docs::print_docs(out)
    }
}

/// The engine's `Profiles` seam backed by the shared `oh_assets::Assets`
/// (issue 029c1 — the C++'s ONE `HGAssets`, main.cpp:381/:434: the engine
/// and the menu observe the SAME profile state, so a score the engine's
/// death path sets during play is visible in the menu immediately, and the
/// `mainClient` exit guard's `pSaveAll` persists it. The port shares ONE
/// `Arc<Assets>` between this seam and the menu's `RealMenuAssets` — the
/// same single-object semantics; `NullProfiles` remains the
/// no-profile stand-in for tests).
#[derive(Debug, Clone)]
pub struct RealProfiles(Arc<oh_assets::Assets>);

impl RealProfiles {
    #[must_use]
    pub fn new(assets: Arc<oh_assets::Assets>) -> Self {
        Self(assets)
    }
}

impl Profiles for RealProfiles {
    fn any_local_profile_active(&self) -> bool {
        self.0.any_local_profile_active()
    }
    fn current_local_profile_name(&self) -> String {
        self.0.current_local_profile_name().unwrap_or_default()
    }
    fn get_local_score(&self, level_id: &str) -> f32 {
        self.0.get_local_score(level_id)
    }
    fn set_local_score(&self, level_id: &str, score: f32) {
        self.0.set_local_score(level_id, score);
    }
}

/// One fully composed engine: `HexagonGame<GameApiState>` + its
/// `LuaContext` + the shared `State` + the host.
///
/// Drop order is part of the `SharedLua` contract: the fields drop in
/// declaration order, so `game` (whose `Inner` holds a `State` `Arc`),
/// `host` and `state` (both carrying `SharedLua`s, directly or through
/// captured closures) all drop BEFORE `lua` — no closure holding a
/// `SharedLua` ever outlives the `LuaContext` it points into.
pub struct Composed {
    pub game: HexagonGame<GameApiState>,
    /// Public so the binary and tests can drive the seam directly (e.g.
    /// `host.init_lua()` to register the API before `print_docs`). Still
    /// held for the whole struct lifetime: the host/state fields own the
    /// rest of the drop-order chain.
    pub host: Arc<LuaScriptHost>,
    // Held only for its drop-order position (the `State` `Arc` keeps the
    // shared store — and the `run_lua_file` closure capturing the host's
    // `SharedLua` — alive until `lua` drops; see the struct docs). Never
    // read directly, so allow the warning.
    #[allow(dead_code)]
    state: State,
    // Heap-pinned (`Box`): the `SharedLua` handle captured from this
    // `LuaContext` at `compose` time points at its HEAP slot — the context
    // must never move (a stack local moved into this struct would leave the
    // `SharedLua` pointing at the dead local's slot). Last field, so it
    // drops AFTER every `SharedLua` carrier (the `SharedLua` contract).
    lua: Box<LuaContext>, // last field — dropped last
}

impl Composed {
    /// The engine's `LuaContext` (read back globals after a run; the
    /// single-thread contract — call it from the composing thread).
    #[must_use]
    pub fn lua(&self) -> &LuaContext {
        &self.lua
    }
}

/// Builds the composition for one engine instance (C++ `mainClient` /
/// `mainPrintLuaDocs`: one `HexagonGame` per entrypoint, the `HGAssets`
/// shared through the process where the C++ shares it).
///
/// `window` mirrors the C++ `mGameWindow != nullptr` (the step-(b) binary
/// passes `false` — the windowed path is step (c)); `root` is the
/// process-CWD equivalent the level-script paths resolve against;
/// `profiles` is the engine's `Profiles` seam (the shipped compositions
/// pass [`RealProfiles`] over the same `Arc<Assets>` the menu's
/// `RealMenuAssets` holds — the C++'s single `HGAssets`; the step-(a)
/// tests pass their `testProfile` stand-in); `steam` is the engine's Steam
/// seam (issue
/// 020 — `mainClient` passes the composition's `oh_steam::SteamManager`;
/// `mainPrintLuaDocs`'s `nullptr /* steamManager */` is
/// `NoneSteamSink`).
#[allow(clippy::too_many_arguments)]
#[must_use]
pub fn compose_with(
    assets: Arc<oh_assets::Assets>,
    root: PathBuf,
    window: bool,
    profiles: Arc<dyn Profiles + 'static>,
    steam: Arc<dyn SteamSink + 'static>,
    discord: Option<Arc<dyn Discord + 'static>>,
) -> Composed {
    compose_with_config(
        assets,
        root,
        window,
        profiles,
        Arc::new(RwLock::new(Config::default())),
        steam,
        discord,
        None,                      // the headless composition's renderer (the C++'s `nullptr`)
        None, // the headless composition's audio (the C++'s `audio == nullptr`)
        Arc::new(NullReplayStore), // issue 029a1 — headless: disk-silent (the compose_with_config docs)
        Arc::new(NoJoystick), // 29j1 — the headless composition's joystick (the C++'s no-controller / headless state)
    )
}

/// `compose_with` with the engine's `Config` supplied by the caller — the
/// C++'s single process-global `hg::Config` (one `Config` shared by the
/// game and the menu; the windowed composition passes its shared
/// `Arc<RwLock<Config>>` so the menu's config writes — e.g. the debug
/// toggle the `Tid::Pause` trigger body reads — reach the engine).
///
/// `renderer` — the engine's `Renderer` seam (the C++ `HexagonGame`
/// ctor's renderer argument): the windowed composition passes the
/// window's touch sink (issue 25b SG5 — the `getFingerDownPositions`
/// data path); the headless composition passes `None` (the C++'s
/// headless `renderer == nullptr`).
///
/// `audio` — the engine's `Audio` seam (the C++ `Audio* audio`;
/// `main.cpp:392-401` creates `hg::Audio` and hands it to both `MenuGame`
/// and `HexagonGame`): the windowed composition passes the real
/// `oh_audio::Audio` (issue 025e); the headless/server composition passes
/// `None` (the C++'s `audio == nullptr`).
///
/// `replay_store` — the engine's `ReplayStore` seam (the C++
/// `death_saveReplay`'s filesystem half, HexagonGame.cpp:1067-1097;
/// issue 029a1): the WINDOWED composition passes
/// [`crate::replay_store::RealReplayStore`] rooted at `root` (the C++'
/// CWD — both windowed entries chdir to the game-data root before
/// composing), so a death persists `Replays/<levelId>/<diff>x/<name>.z`;
/// the headless client / server / print-lua-docs compositions keep
/// `NullReplayStore` — the C++ headless modes never write a replay file
/// (replay playback's `inReplay()` gates the save, HexagonGame.cpp:970 +
/// `shouldSaveScore`'s rejects, :1178-1183/:1220-1227; the server's
/// executor is the same `runReplayUntilDeathAndGetScore` path), so the
/// no-op matches the C++ there (disk-silent) and keeps the headless
/// determinism tests from writing files.
///
/// `joystick` — the engine's `Joystick` seam (29j1): the WINDOWED
/// desktop composition passes the shared `crate::joystick::RealJoystick`
/// (the gilrs poller — the C++ `sf::Joystick` controller-0 read; the
/// same object the menu's `MenuJoystick` seam views, one global);
/// the Android / wasm / headless compositions keep `NoJoystick` (the
/// C++'s `isConnected(0) == false` / headless state).
#[allow(clippy::too_many_arguments)]
#[must_use]
pub fn compose_with_config(
    assets: Arc<oh_assets::Assets>,
    root: PathBuf,
    window: bool,
    profiles: Arc<dyn Profiles + 'static>,
    config: Arc<RwLock<Config>>,
    steam: Arc<dyn SteamSink + 'static>,
    discord: Option<Arc<dyn Discord + 'static>>,
    renderer: Option<Arc<dyn Renderer + 'static>>,
    audio: Option<Arc<dyn Audio + 'static>>,
    replay_store: Arc<dyn ReplayStore + 'static>,
    joystick: Arc<dyn oh_game::seams::Joystick + 'static>,
) -> Composed {
    // Heap-pinned before the `SharedLua` capture (see the `lua` field docs
    // on `Composed`): the host's `SharedLua` points at this box's slot.
    let lua = Box::new(LuaContext::new());
    let host = Arc::new(LuaScriptHost::new(&lua, !window, root, config.clone()));

    // Phase 1: the engine, with the (state-less) host.
    let game = HexagonGame::<GameApiState>::new(
        steam,
        config.clone(),
        assets.clone(),
        audio, // C++ `Audio* audio` — `oh_audio::Audio` windowed; `None` headless (issue 025e)
        profiles,
        replay_store, // issue 029a1 — the C++ `death_saveReplay`'s filesystem half (the fn docs)
        host.clone() as Arc<dyn ScriptHost>,
        joystick, // 29j1 — the C++ `Joystick` global's poller (the fn docs)
        Arc::new(PcgCosmeticRng::new()),
        window,
        renderer, // issue 25b SG5 (the window's touch sink) / the C++ headless `nullptr`
        discord,  // C++ `discordManager` (main.cpp:264-269,416) — `None` headless
    );

    // Phase 2: the shared state with the engine itself as backend.
    let backend: Arc<dyn GameApi> = Arc::new(game.clone());
    let backend2 = backend.clone();
    let config2 = config.clone();
    let host2 = host.clone();
    // C++ `fRunLuaFile` = `[this](filename) { runLuaFile(filename); }`
    // (HGScripting.cpp:1107): the catch lives INSIDE `runLuaFile` — on error
    // `goToMenu(false, true)` unless debug, and the Lua call returns void.
    let run_lua_file: RunLuaFile = Arc::new(move |path, shared| {
        if host2.execute_file(shared, path).is_err()
            && !config2.read().expect("config poisoned").get_debug()
        {
            backend2.go_to_menu(false, true);
        }
        Ok(())
    });
    let state = Arc::new(ReentrantMutex::new(GameApiState::new(
        RandomNumberGenerator::new(0),
        LevelStatus::new(true, 1600.0), // C++ `LevelStatus()` NSDMIs
        HexagonGameStatus::new(),
        StyleData::default(),
        MusicData::default(),
        false,   /* in_menu */
        !window, /* headless */
        String::new(),
        String::new(),
        String::new(),
        PackData {
            folder_path: String::new(),
            id: String::new(),
            disambiguator: String::new(),
            name: String::new(),
            author: String::new(),
            description: String::new(),
            version: 0,
            priority: 0.0,
            dependencies: Vec::new(),
        },
        run_lua_file,
        Arc::new(RealAssetsApi::new(assets)),
        backend,
    )));

    host.attach_state(state.clone());
    game.attach_state(state.clone());
    Composed {
        game,
        host,
        state,
        lua,
    }
}
