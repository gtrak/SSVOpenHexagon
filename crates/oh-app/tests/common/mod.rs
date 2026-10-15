//! Test-side composition for the draw-bridge tests (issue 15 step (a)).
//!
//! A minimal copy of `crates/oh-game/tests/common/mod.rs` — the real
//! `oh_game::seams::ScriptHost` over `oh-lua`, a real `AssetsApi` adapter
//! over `oh_assets::Assets`, and the C++ test's `testProfile` profile
//! seam. (Issue 15 step (b) dedupes the two copies when the `oh-app`
//! binary lands and both trees share a `tests/` home; kept intentionally
//! self-contained here so `oh-app`'s test tree stands alone.)
//!
//! The two-phase build (DECISIONS.md D-013) and the `SharedLua`
//! drop-order contract are documented in the original
//! (`crates/oh-game/tests/common/mod.rs`); they apply verbatim here.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock, RwLock};

use oh_config::Config;
use oh_data::{MusicData, PackData, StyleData};
use oh_game::hexagon_game::HexagonGame;
use oh_game::lua_state::{GameApi, ReentrantMutex};
use oh_game::seams::{
    NoJoystick, NullReplayStore, PcgCosmeticRng, Profiles, ScriptError, ScriptHost,
};
use oh_game::{HexagonGameStatus, LevelStatus};
use oh_lua::api::callbacks;
use oh_lua::api::state::{AssetsApi, GameApiState, RunLuaFile, SharedLua};
use oh_lua::api::{init_level_api, init_lua_api, State};
use oh_lua::{LuaContext, LuaError};
use oh_render::draw::RenderStates;
use oh_render::graphics::DrawTarget;
use oh_render::view::View;
use oh_replay::ReplayFile;
use oh_rng::RandomNumberGenerator;

/// The C++ `ReplayExecution.t.cpp` profile (see the original's docs).
pub struct TestProfiles;

impl Profiles for TestProfiles {
    fn any_local_profile_active(&self) -> bool {
        true
    }

    fn current_local_profile_name(&self) -> String {
        "testProfile".to_string()
    }

    fn get_local_score(&self, _level_id: &str) -> f32 {
        0.0
    }

    fn set_local_score(&self, _level_id: &str, _score: f32) {}
}

/// The `HGAssets` lookups the 007 lambdas perform (see the original's
/// docs for the `get_*_data` error semantics and the no-op
/// `set_shader_uniform_*` methods).
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
            .cloned()
            .ok_or_else(|| LuaError::ExecutionError {
                message: format!("Asset '{pack_id}_{id}' not found\n"),
            })
    }

    fn get_style_data(&self, pack_id: &str, id: &str) -> Result<StyleData, LuaError> {
        self.assets
            .get_style_data(pack_id, id)
            .cloned()
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

/// The C++ `Lua::LuaContext` callback surface (see the original's docs).
pub struct LuaScriptHost {
    lua: SharedLua,
    state: OnceLock<State>,
    headless: bool,
    root: PathBuf,
    config: Arc<RwLock<Config>>,
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

    /// C++ `HexagonGame::runLuaFile` (`HGScripting.cpp:1127-1147`) MINUS
    /// its catch (see the original's docs for the cached/uncached paths).
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

    fn run_lua_file_cached(
        &self,
        shared: &SharedLua,
        path: &str,
        full: &Path,
    ) -> Result<(), String> {
        let code = {
            let mut cache = self.file_cache.lock();
            if !cache.contains_key(path) {
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

    fn run_lua_file_uncached(
        &self,
        shared: &SharedLua,
        path: &str,
        full: &Path,
    ) -> Result<(), String> {
        let bytes = std::fs::read(full).map_err(|_| {
            let msg = format!("Fatal Lua error\nCould not open file: {path}\n");
            println!("[hg::Utils::runLuaFile] {msg}");
            msg
        })?;
        let code = String::from_utf8_lossy(&bytes).into_owned();
        match shared.execute_code(&code) {
            Ok(()) => Ok(()),
            Err(e) => {
                println!("[hg::Utils::runLuaFile] Fatal Lua error\nFilename: {path}\nError: {e}\n");
                Err(e.to_string())
            }
        }
    }
}

impl ScriptHost for LuaScriptHost {
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
        // See the original's docs: `call_on_input` already applies the
        // C++ nil→false coercion; a missing callback yields `Ok(None)`.
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
        // See the original's docs: the engine only queries plain globals.
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

    fn execute_code(&self, code: &str) -> Result<(), ScriptError> {
        // See the original's docs: the composition's failures are all
        // `std::runtime_error`-class.
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
    fn print_docs(&self, out: &mut dyn std::io::Write) -> std::io::Result<()> {
        oh_lua::docs::print_docs(out)
    }
}

/// One fully composed engine: `HexagonGame<GameApiState>` + its
/// `LuaContext` + the shared `State` + the host (see the original's docs
/// for the drop-order contract — `lua` MUST stay the LAST field).
pub struct Composed {
    pub game: HexagonGame<GameApiState>,
    /// Public so tests can drive the seam directly. `allow(dead_code)`:
    /// each test binary compiles this module separately and not all of
    /// them read every field.
    #[allow(dead_code)]
    pub host: Arc<LuaScriptHost>,
    #[allow(dead_code)]
    state: State,
    #[allow(dead_code)]
    lua: Box<LuaContext>, // last field — dropped last
}

impl Composed {
    /// The engine's `LuaContext` (read back globals after a run; the
    /// single-thread contract — call it from the test's own thread).
    #[must_use]
    #[allow(dead_code)]
    pub fn lua(&self) -> &LuaContext {
        &self.lua
    }
}

/// Builds the composition for one engine instance (C++ test: one
/// `HexagonGame` per `doTest` call, all sharing one `HGAssets`).
/// `window` mirrors the C++ `mGameWindow != nullptr`; `root` is the
/// process-CWD equivalent the level-script paths resolve against.
#[allow(clippy::too_many_arguments)]
pub fn compose(assets: Arc<oh_assets::Assets>, root: PathBuf, window: bool) -> Composed {
    let config = Arc::new(RwLock::new(Config::default()));
    // Heap-pinned before the `SharedLua` capture (see the original's docs
    // on the `lua` field): the host's `SharedLua` points at this box's slot.
    let lua = Box::new(LuaContext::new());
    let host = Arc::new(LuaScriptHost::new(&lua, !window, root, config.clone()));

    // Phase 1: the engine, with the (state-less) host.
    let game = HexagonGame::<GameApiState>::new(
        config.clone(),
        assets.clone(),
        None, // C++ `audio == nullptr`
        Arc::new(TestProfiles),
        Arc::new(NullReplayStore),
        host.clone() as Arc<dyn ScriptHost>,
        Arc::new(NoJoystick),
        Arc::new(PcgCosmeticRng::new()),
        window,
        None, // C++ headless tests: no renderer
    );

    // Phase 2: the shared state with the engine itself as backend.
    let backend: Arc<dyn GameApi> = Arc::new(game.clone());
    let backend2 = backend.clone();
    let config2 = config.clone();
    let host2 = host.clone();
    // C++ `fRunLuaFile` (`HGScripting.cpp:1107`): the catch lives INSIDE
    // `runLuaFile` — on error `goToMenu(false, true)` unless debug.
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

/// The `_RELEASE` root (the process-CWD equivalent; the same idiom as
/// `oh-assets/tests/release_tree.rs`).
#[must_use]
pub fn release_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../_RELEASE")
        .to_path_buf()
}

/// The C++ `initText` fonts (`HexagonGame.cpp:289-290`: the assets
/// registry's `OpenSquare-Regular.ttf` / `OpenSquare-Bold.ttf`).
#[must_use]
pub fn test_fonts() -> (oh_render::font::Font, oh_render::font::Font) {
    let root = release_dir().join("Assets");
    (
        oh_render::font::Font::load(&root.join("OpenSquare-Regular.ttf"))
            .expect("OpenSquare-Regular.ttf"),
        oh_render::font::Font::load(&root.join("OpenSquare-Bold.ttf"))
            .expect("OpenSquare-Bold.ttf"),
    )
}

// ---------------------------------------------------------------------
// A recording DrawTarget for the stage-sequence assertions — the same
// event model as `oh-render`'s private `Recording` target
// (`graphics.rs:1561`, not importable — private to that crate's test
// module; 15b dedupes when the test trees share a home).
// ---------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
pub enum DrawEvent {
    Clear,
    SetView(View),
    Vertices { count: usize, states: RenderStates },
    Sprite,
    Text { states: RenderStates },
    Rectangle { states: RenderStates },
    Screenshot(String),
}

#[derive(Default)]
pub struct Recording {
    pub events: Vec<DrawEvent>,
}

impl DrawTarget for Recording {
    fn clear_black(&mut self) {
        self.events.push(DrawEvent::Clear);
    }
    fn set_view(&mut self, view: View) {
        self.events.push(DrawEvent::SetView(view));
    }
    fn draw_vertices(&mut self, vertices: &[oh_render::draw::Vertex], states: RenderStates) {
        self.events.push(DrawEvent::Vertices {
            count: vertices.len(),
            states,
        });
    }
    fn draw_sprite(&mut self, _sprite: &oh_render::texture::Sprite) {
        self.events.push(DrawEvent::Sprite);
    }
    fn draw_text(&mut self, _text: &oh_render::font::Text, states: RenderStates) {
        self.events.push(DrawEvent::Text { states });
    }
    fn draw_rectangle(&mut self, _rect: &oh_render::types::RectangleShape, states: RenderStates) {
        self.events.push(DrawEvent::Rectangle { states });
    }
    fn save_screenshot(&mut self, path: &str) {
        self.events.push(DrawEvent::Screenshot(path.to_string()));
    }
}

/// Runs one engine to death (C++ `executeGameUntilDeath(1, 1.f)`) and
/// returns the C++ test's `(playedTimeSeconds, replay_file)` pair.
/// `allow(dead_code)`: the no-op test (this milestone) doesn't run a
/// live game yet — the full-draw test (next milestone) does.
#[must_use]
#[allow(dead_code)]
pub fn run_live_to_death(game: &HexagonGame<GameApiState>) -> (f64, ReplayFile) {
    let captured: std::sync::Arc<std::sync::Mutex<Option<ReplayFile>>> =
        std::sync::Arc::new(std::sync::Mutex::new(None));
    let cap = captured.clone();
    game.set_on_death_replay_created(std::sync::Arc::new(move |rf: &ReplayFile| {
        *cap.lock().expect("replay capture poisoned") = Some(rf.clone());
    }));

    game.set_must_start(true);
    let result = game
        .execute_game_until_death(1, 1.0)
        .expect("live run exceeded the C++ 1-second processing budget");
    let rf = captured
        .lock()
        .expect("replay capture poisoned")
        .take()
        .expect("on_death_replay_created never fired (in_replay was true?)");
    (result.played_time_seconds, rf)
}
