//! Port of `include/SSVOpenHexagon/Core/HexagonGame.hpp` +
//! `src/SSVOpenHexagon/Core/HexagonGame.cpp` — the `hg::HexagonGame`
//! lifecycle (constructor / `newGame` / `start` / death family /
//! difficulty / swap / replay record+playback).
//!
//! The per-frame `update` loop lives in [`crate::update`] (the C++
//! `HGUpdate.cpp`), mirroring the file split 1:1.
//!
//! # State ownership (DECISIONS.md D-013)
//!
//! The gameplay state the 285 Lua lambdas (issue 007) read/write
//! (`levelStatus`, `status`, `styleData`, `musicData`, `walls`, `rng`,
//! the timelines, `cwManager`, `difficultyMult`, `firstPlay`, …) lives in
//! the one [`SharedState`] store (see [`crate::lua_state`]), held inside a
//! [`ReentrantMutex`] that the composition shares with the lambdas
//! (`oh_lua::api::state::State`). `HexagonGame<S>` is generic over that
//! store (`S: [`SharedStore`]`): bare `SharedState` for pure-headless
//! runs, the composition's `GameApiState` when Lua is involved.
//!
//! Everything else (player, cameras, particles, input flags, replay
//! record/last-seed bookkeeping) stays engine-local in
//! [`EngineState`], behind the engine's own reentrant lock — the C++
//! captures the `HexagonGame&` directly, and the Rust engine's methods
//! are `&self` (the engine is the `GameApi` backend `Arc` the lambdas
//! call through), so its mutable state needs interior mutability. The
//! lock is reentrant because a Lua callback reached through `st.backend`
//! re-enters the engine on the same thread (D-013).
//!
//! # Headless
//!
//! The C++ `window != nullptr` gates are modeled by the `window: bool`
//! constructor argument (the C++ `mGameWindow != nullptr`); the C++
//! `ReplayExecution` tests run with a null window, and this issue's
//! tests do the same. A [`crate::renderer::Renderer`] may additionally be
//! attached as a draw sink (see that module) — it does NOT re-enable the
//! windowed paths.

use std::ops::{Deref, DerefMut};
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

use oh_config::{Config, TIME_STEP};
use oh_data::{get_level_validator, LevelData, PackData, StyleData};
use oh_math::hg::{concat::format_ostream_f64, string::to_uppercase};
use oh_math::ssvs::Vec2;
use oh_replay::{compress_replay_file, InputBits, ReplayData, ReplayFile};
use oh_rng::{uniform_int, Pcg32Fast, RandomNumberGenerator, Seed};

use crate::camera::Camera;
use crate::lua_state::{ReentrantMutex, ReentrantMutexGuard, SharedState, SharedStore};
use crate::renderer::Renderer;
use crate::seams::{Audio, CosmeticRng, Joystick, Profiles, ReplayStore, ScriptHost};
use crate::{
    CPlayer, CWall, FastVertexVectorTris, HexagonGameStatus, LevelStatus, SpeedData, StateChange,
    Timeline2, Timeline2Runner,
};

/// C++ `HexagonGame::GameExecutionResult`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GameExecutionResult {
    pub played_time_seconds: f64,
    pub paused_time_seconds: f64,
    pub total_time_seconds: f64,
    pub custom_score: f32,
}

/// C++ `HexagonGame::ActiveReplay`.
///
/// The C++ `replayPlayer` is a `replay_player` (a non-owning cursor over
/// `replayFile._data`); the cursor is inlined here as `replay_player_index`
/// (a `&ReplayData` self-borrow cannot be expressed in one struct).
#[derive(Debug, Clone, PartialEq)]
pub struct ActiveReplay {
    pub replay_file: ReplayFile,
    replay_player_index: usize,
    pub replay_pack_name: String,
    pub replay_level_name: String,
}

impl ActiveReplay {
    /// C++ `ActiveReplay(const replay_file&)` (names default-empty;
    /// `newGame` fills them).
    #[must_use]
    pub fn new(replay_file: ReplayFile) -> Self {
        Self {
            replay_file,
            replay_player_index: 0,
            replay_pack_name: String::new(),
            replay_level_name: String::new(),
        }
    }

    /// C++ `replayPlayer.get_current_and_move_forward()`.
    #[must_use]
    pub(crate) fn get_current_and_move_forward(&mut self) -> InputBits {
        if self.replay_file.data.size() <= self.replay_player_index {
            return InputBits::none();
        }
        let current = self.replay_file.data.at(self.replay_player_index);
        self.replay_player_index += 1;
        current
    }

    /// C++ `replayPlayer.done()`.
    fn done(&self) -> bool {
        self.replay_player_index == self.replay_file.data.size()
    }

    /// C++ `replayPlayer.reset()`.
    fn reset(&mut self) {
        self.replay_player_index = 0;
    }
}

/// C++ `HexagonGame::PreShakeCenters`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PreShakeCenters {
    pub background: Vec2,
    pub overlay: Vec2,
}

/// C++ `HexagonGame::SwapParticleSpawnInfo`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SwapParticleSpawnInfo {
    pub ready: bool,
    pub position: Vec2,
    pub angle: f32,
}

/// The minimal `sf::Sprite` surface the particle update paths touch
/// (position/rotation/scale/color/origin; the texture pointer itself is a
/// render-backend concern — issues 10/11).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpriteState {
    pub position: Vec2,
    pub rotation_deg: f32,
    pub scale: Vec2,
    pub color: oh_math::hg::color::Color,
    pub origin: Vec2,
}

/// C++ `HexagonGame::Particle`.
#[derive(Debug, Clone)]
pub struct Particle {
    pub sprite: SpriteState,
    pub velocity: Vec2,
    pub angular_velocity: f32,
}

/// C++ `HexagonGame::TrailParticle`.
#[derive(Debug, Clone)]
pub struct TrailParticle {
    pub sprite: SpriteState,
    pub angle: f32,
}

/// C++ `HexagonGame::SwapParticle`.
#[derive(Debug, Clone)]
pub struct SwapParticle {
    pub sprite: SpriteState,
    pub velocity: Vec2,
}

/// C++ `ssvu::TimelineManager` (legacy FT-based manager) — the
/// `effectTimelineManager` member. No game code ever calls `create()` on
/// it (only `update`/`clear` in the update loop and `newGame`), so this is
/// the minimal faithful stand-in: a set of FT-delayed callbacks.
#[derive(Default)]
pub struct EffectTimelineManager {
    entries: Vec<(f32, Box<dyn Fn() + Send>)>,
}

impl EffectTimelineManager {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// C++ `create()` + a single delayed action (`MonoManager<Timeline>`
    /// semantics, one action per timeline).
    pub fn add(&mut self, delay_ft: f32, f: impl Fn() + Send + 'static) {
        self.entries.push((delay_ft, Box::new(f)));
    }

    /// C++ `update(FT)` (fire + drop finished entries).
    pub fn update(&mut self, m_ft: f32) {
        let mut i = 0;
        while i < self.entries.len() {
            self.entries[i].0 -= m_ft;
            if self.entries[i].0 <= 0.0 {
                (self.entries[i].1)();
                drop(self.entries.remove(i));
            } else {
                i += 1;
            }
        }
    }

    /// C++ `clear()`.
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Count of pending entries (test access).
    #[must_use]
    pub fn entries_count(&self) -> usize {
        self.entries.len()
    }
}

/// The engine-local (non-Lua-captured) `HexagonGame` fields — one C++
/// field per Rust field, C++ spelling in the doc. A single reentrant guard
/// covers the whole struct: engine methods are `&self` (the engine is a
/// shared `GameApi` backend), and nested engine calls re-enter on the same
/// thread (D-013).
///
/// `pub(crate)` visibility: the update loop (`update.rs`) drives these
/// fields through short-lived guards; no `&mut` borrow into the guard's
/// contents may outlive a reentrant call (stacked-borrows discipline).
pub struct EngineState {
    // C++ `packId` / `levelId` (the newGame arguments; distinct from the
    // live `levelData->packId`).
    pub(crate) pack_id: String,
    pub(crate) level_id: String,
    // C++ `const LevelData* levelData` — the C++ holds a pointer into the
    // `HGAssets` registry; the port owns a clone (the registry object is
    // immutable after load, so the copy is exact).
    pub(crate) level_data: Option<Arc<LevelData>>,

    // C++ `CPlayer player`.
    pub(crate) player: CPlayer,

    // C++ `std::optional<ssvs::Camera> backgroundCamera / overlayCamera`.
    pub(crate) background_camera: Option<Camera>,
    pub(crate) overlay_camera: Option<Camera>,
    // C++ `std::optional<PreShakeCenters> preShakeCenters`.
    pub(crate) pre_shake_centers: Option<PreShakeCenters>,

    // C++ `ssvu::TimelineManager effectTimelineManager`.
    pub(crate) effect_timeline_manager: EffectTimelineManager,
    // C++ `Utils::FastVertexVectorTris flashPolygon`.
    pub(crate) flash_polygon: FastVertexVectorTris,

    // C++ `Utils::timeline2 messageTimeline` + runner (engine-local, NOT
    // Lua-shared: `addMessage`/`clearMessages` only).
    pub(crate) message_timeline: Timeline2,
    pub(crate) message_timeline_runner: Timeline2Runner,
    // C++ `timelineRunner` / `eventTimelineRunner` (the LAMBDA-shared
    // timelines `timeline`/`eventTimeline` live in `SharedState`; their
    // runners are engine-local, exactly as in the C++).
    pub(crate) main_timeline_runner: Timeline2Runner,
    pub(crate) event_timeline_runner: Timeline2Runner,

    // C++ particles.
    pub(crate) particles: Vec<Particle>,
    pub(crate) trail_particles: Vec<TrailParticle>,
    pub(crate) swap_particles: Vec<SwapParticle>,
    pub(crate) must_spawn_pb_particles: bool,
    pub(crate) swap_particles_spawn_info: Option<SwapParticleSpawnInfo>,
    pub(crate) next_pb_particle_spawn: f32,
    pub(crate) pb_text_growth: f32,

    // C++ `sf::Text messageText / pbText / (os -> status) / fpsText /
    // timeText / replayText` — the string content only (fonts/layout are
    // render-backend concerns, issue 11).
    pub(crate) message_text: String,
    pub(crate) pb_text: String,
    pub(crate) status_text: String,
    pub(crate) fps_text: String,
    pub(crate) time_text: String,
    pub(crate) replay_text: String,

    // C++ flags.
    pub(crate) must_change_sides: bool,
    pub(crate) must_start: bool,
    // Consumed in the windowed `draw()` (render lane); set by the ilc
    // console command in this crate.
    #[allow(dead_code)]
    pub(crate) must_take_screenshot: bool,
    pub(crate) input_focused: bool,
    pub(crate) input_swap: bool,
    pub(crate) death_input_ignore: f32,

    // C++ `std::optional<ActiveReplay> activeReplay` + last-replay
    // bookkeeping.
    pub(crate) active_replay: Option<ActiveReplay>,
    pub(crate) last_seed: Seed,
    pub(crate) last_replay_data: ReplayData,
    pub(crate) last_first_play: bool,
    pub(crate) last_played_score: f64,

    // C++ restart bookkeeping.
    pub(crate) restart_id: String,
    pub(crate) restart_first_time: bool,

    // C++ input impl.
    pub(crate) input_impl_last_movement: i32,
    pub(crate) input_movement: i32,
    pub(crate) input_impl_cw: bool,
    pub(crate) input_impl_ccw: bool,
    pub(crate) player_now_ready_to_swap: bool,

    // C++ test utilities.
    pub(crate) execute_random_inputs: bool,
    pub(crate) always_spin_right: bool,
    // C++ `debugPause` (ImGui console toggle, issue 14).
    pub(crate) debug_pause: bool,
    // C++ `ilcShowConsole` (HexagonGame.hpp:114) — the console window's
    // visibility; toggled by the `post_update` gate (issue 014 step (a))
    // and read by `imgui_lua_console_has_input`.
    pub(crate) ilc_show_console: bool,
    // C++ `sf::Clock ilcDeltaClock` (HexagonGame.hpp:108) — read ONLY by the
    // windowed ImGui frame (`ImGui::SFML::Update(*window,
    // ilcDeltaClock.restart())`, `HGUpdate.cpp:1305`). Nothing in the
    // headless port reads it; the read site is step (b). Kept field-for-
    // field per the spec; `Instant` is the port's clock-helper stand-in
    // (`clock.rs` carries the game-time-point alias, not a ticking clock).
    #[allow(dead_code)]
    pub(crate) ilc_delta_clock: std::time::Instant,
    // C++ `std::vector<std::string> ilcHistory` (HexagonGame.hpp:110).
    pub(crate) ilc_history: Vec<String>,
    // C++ `int ilcHistoryPos{-1}` (HexagonGame.hpp:111).
    pub(crate) ilc_history_pos: i32,
    // C++ `std::string ilcCmdBuffer` (HexagonGame.hpp:112).
    pub(crate) ilc_cmd_buffer: String,
    // C++ `std::string ilcTrackBuffer` (HexagonGame.hpp:113).
    pub(crate) ilc_track_buffer: String,
    // C++ `bool ilcShowConsoleNext{false}` (HexagonGame.hpp:115) — the
    // pending `Tid::LuaConsole` toggle.
    pub(crate) ilc_show_console_next: bool,
    // C++ `std::vector<std::string> ilcLuaTracked / ilcLuaTrackedNames`
    // (HexagonGame.hpp:116-117). (`ilcCmdLog` / `ilcLuaTrackedResults` live
    // on `SharedState` — the 007 lambdas write them.)
    pub(crate) ilc_lua_tracked: Vec<String>,
    pub(crate) ilc_lua_tracked_names: Vec<String>,
    // C++ `random_input_rng` equivalent: the C++
    // `static std::mt19937 en(std::random_device{})` in
    // `updateInput` (process-static, test-only, non-deterministic). The
    // port keeps a per-engine `pcg32_fast` (see `draw_random_bool`); both
    // are non-deterministic by design.
    pub(crate) random_input_rng: Pcg32Fast,

    // C++ fast-forward / tick-advance test hooks.
    pub(crate) fast_forward_target: Option<f64>,
    pub(crate) advance_tick_count: Option<i32>,

    // C++ `timeUntilRichPresenceUpdate` (rich presence; the steam/discord
    // managers are always null in this port).
    pub(crate) time_until_rich_presence_update: f32,
}

fn entropy_seed() -> u64 {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    now ^ (u64::from(std::process::id())).rotate_left(32)
}

/// C++ `HexagonGame` (the `window` argument is `mGameWindow != nullptr`).
///
/// Cheap to clone (an `Arc` over the engine core); the clone is how the
/// composition registers the engine as the `GameApi` backend before
/// attaching the shared state handle (see [`HexagonGame::attach_state`]).
pub struct HexagonGame<S: SharedStore = SharedState>(pub(crate) Arc<Inner<S>>);

/// Cheap to clone (an `Arc` over the engine core). Explicit (not
/// `#[derive]`): the derive would add a `S: Clone` bound and `clone()`
/// would then silently resolve to `(&HexagonGame<S>)::clone` (a reference
/// copy) for stores that are not `Clone`.
impl<S: SharedStore> Clone for HexagonGame<S> {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

pub(crate) struct Inner<S: SharedStore> {
    // The shared engine/Lua store (D-013). Attached after construction
    // because the composition's `GameApiState` needs `Arc<dyn GameApi>` =
    // this engine, which needs to exist first (two-phase, see
    // `attach_state`).
    pub(crate) state: std::sync::OnceLock<Arc<ReentrantMutex<S>>>,
    // C++ `HGAssets& assets` (composition-owned).
    pub(crate) assets: Arc<oh_assets::Assets>,
    // C++ `Config` (static singleton in C++; the composition shares its
    // loaded instance — `isEligibleForScore` mutates a cached
    // `uneligibilityReason`, hence the lock).
    pub(crate) config: Arc<RwLock<Config>>,
    // C++ `Audio* audio` (nullable).
    pub(crate) audio: Option<Arc<dyn Audio + 'static>>,
    // C++ `HGAssets` local-profile/score surface.
    pub(crate) profiles: Arc<dyn Profiles + 'static>,
    // C++ `death_saveReplay` file-write half.
    pub(crate) replay_store: Arc<dyn ReplayStore + 'static>,
    // C++ `Lua::LuaContext lua` / `initLua` / `runLuaFile`.
    pub(crate) script_host: Arc<dyn ScriptHost + 'static>,
    // C++ `Joystick` global (platform-fed; issue 10).
    pub(crate) joystick: Arc<dyn Joystick + 'static>,
    // C++ `ssvu::getRndR/getRndI` global cosmetic RNG.
    pub(crate) cosmetic_rng: Arc<dyn CosmeticRng + 'static>,
    // C++ `ssvs::GameWindow* mGameWindow != nullptr`.
    pub(crate) window: bool,
    // Draw sink (issues 10/11); `None` in the C++ headless tests.
    pub(crate) renderer: Option<Arc<dyn Renderer + 'static>>,
    // C++ `std::function<void(const replay_file&)> onDeathReplayCreated`.
    pub(crate) on_death_replay_created: std::sync::RwLock<Option<DeathReplayCallback>>,
    pub(crate) est: ReentrantMutex<EngineState>,
}

/// Guard over the shared store that derefs straight to [`SharedState`]
/// (D-015: the engine never derefs `S` itself — access goes through
/// [`SharedStore::shared`] / [`SharedStore::shared_mut`], so the bare
/// `SharedState` and the composition's `GameApiState` wrapper behave
/// identically without any `Deref` impl on `SharedState`).
pub(crate) struct SharedGuard<'a, S: SharedStore>(ReentrantMutexGuard<'a, S>);

impl<S: SharedStore> Deref for SharedGuard<'_, S> {
    type Target = SharedState;

    fn deref(&self) -> &SharedState {
        self.0.shared()
    }
}

impl<S: SharedStore> DerefMut for SharedGuard<'_, S> {
    fn deref_mut(&mut self) -> &mut SharedState {
        self.0.shared_mut()
    }
}

/// C++ `std::function<void(const replay_file&)>` capture (type alias so the
/// `Inner` field stays under clippy's type-complexity budget).
pub type DeathReplayCallback = Arc<dyn Fn(&ReplayFile) + Send + Sync + 'static>;

/// C++ `HexagonGame::SaveScoreIfNeededResult` (private enum).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SaveScoreIfNeededResult {
    NoWindow,
    ShouldNotSave,
    NotPersonalBest,
    PersonalBest,
}

impl<S: SharedStore> HexagonGame<S> {
    /// C++ `HexagonGame(steamManager, discordManager, assets, audio,
    /// mGameWindow, hexagonClient)` — the steam/discord/client managers
    /// have no Rust backing in this issue (their call sites are null-gated
    /// and therefore inert).
    ///
    /// When `window` is true the constructor emplaces the two cameras
    /// exactly like the C++ (background: center (0,0), size
    /// `(width*zoom, height*zoom)`; overlay: center `(w/2, h/2)`, size
    /// `(w, h)`).
    ///
    /// The shared state handle is NOT yet attached: call
    /// [`Self::attach_state`] before using the game (the composition builds
    /// the store with `Arc<dyn GameApi>` = a clone of this engine).
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        config: Arc<RwLock<Config>>,
        assets: Arc<oh_assets::Assets>,
        audio: Option<Arc<dyn Audio + 'static>>,
        profiles: Arc<dyn Profiles + 'static>,
        replay_store: Arc<dyn ReplayStore + 'static>,
        script_host: Arc<dyn ScriptHost + 'static>,
        joystick: Arc<dyn Joystick + 'static>,
        cosmetic_rng: Arc<dyn CosmeticRng + 'static>,
        window: bool,
        renderer: Option<Arc<dyn Renderer + 'static>>,
    ) -> Self {
        let (background_camera, overlay_camera) = if window {
            let c = config.read().expect("config poisoned");
            let width = c.get_width() as f32;
            let height = c.get_height() as f32;
            let zoom = c.get_zoom_factor();
            (
                Some(Camera::new(
                    Vec2::new(0.0, 0.0),
                    Vec2::new(width * zoom, height * zoom),
                )),
                Some(Camera::new(
                    Vec2::new(width / 2.0, height / 2.0),
                    Vec2::new(width, height),
                )),
            )
        } else {
            (None, None)
        };

        // C++ `player{ssvs::zeroVec2f, getSwapCooldown(), ...}` — read
        // against the default-initialized `levelStatus` (NSDMI
        // `swapCooldownMult{1.f}`), so `max(36.f * 1.f, 8.f)`.
        let est = EngineState {
            pack_id: String::new(),
            level_id: String::new(),
            level_data: None,
            player: CPlayer::new(
                &Vec2::new(0.0, 0.0),
                f32::max(36.0_f32 * 1.0_f32, 8.0_f32),
                {
                    let c = config.read().expect("config poisoned");
                    c.get_player_size()
                },
                {
                    let c = config.read().expect("config poisoned");
                    c.get_player_speed()
                },
                {
                    let c = config.read().expect("config poisoned");
                    c.get_player_focus_speed()
                },
            ),
            background_camera,
            overlay_camera,
            pre_shake_centers: None,
            effect_timeline_manager: EffectTimelineManager::new(),
            flash_polygon: FastVertexVectorTris::new(),
            message_timeline: Timeline2::new(),
            message_timeline_runner: Timeline2Runner::new(),
            main_timeline_runner: Timeline2Runner::new(),
            event_timeline_runner: Timeline2Runner::new(),
            particles: Vec::new(),
            trail_particles: Vec::new(),
            swap_particles: Vec::new(),
            must_spawn_pb_particles: false,
            swap_particles_spawn_info: None,
            next_pb_particle_spawn: 0.0,
            pb_text_growth: 0.0,
            message_text: String::new(),
            pb_text: String::new(),
            status_text: String::new(),
            fps_text: String::new(),
            time_text: String::new(),
            replay_text: String::new(),
            must_change_sides: false,
            must_start: false,
            must_take_screenshot: false,
            input_focused: false,
            input_swap: false,
            death_input_ignore: 0.0,
            active_replay: None,
            last_seed: 0,
            last_replay_data: ReplayData::default(),
            last_first_play: false,
            last_played_score: 0.0,
            restart_id: String::new(),
            restart_first_time: true,
            input_impl_last_movement: 0,
            input_movement: 0,
            input_impl_cw: false,
            input_impl_ccw: false,
            player_now_ready_to_swap: false,
            execute_random_inputs: false,
            always_spin_right: false,
            debug_pause: false,
            ilc_show_console: false,
            ilc_delta_clock: std::time::Instant::now(),
            ilc_history: Vec::new(),
            ilc_history_pos: -1,
            ilc_cmd_buffer: String::new(),
            ilc_track_buffer: String::new(),
            ilc_show_console_next: false,
            ilc_lua_tracked: Vec::new(),
            ilc_lua_tracked_names: Vec::new(),
            random_input_rng: Pcg32Fast::from_seed(entropy_seed()),
            fast_forward_target: None,
            advance_tick_count: None,
            time_until_rich_presence_update: 0.0,
        };

        Self(Arc::new(Inner {
            state: std::sync::OnceLock::new(),
            assets,
            config,
            audio,
            profiles,
            replay_store,
            script_host,
            joystick,
            cosmetic_rng,
            window,
            renderer,
            on_death_replay_created: std::sync::RwLock::new(None),
            est: ReentrantMutex::new(est),
        }))
    }

    /// Attaches the shared state handle (the `Arc<ReentrantMutex<S>>` the
    /// composition also gives to the 007 lambdas as
    /// `oh_lua::api::state::State`). Panics if called twice or if the game
    /// was used before attaching.
    pub fn attach_state(&self, state: Arc<ReentrantMutex<S>>) {
        if self.0.state.set(state).is_err() {
            panic!("HexagonGame state already attached");
        }
    }

    /// The attached handle (the same lock the 007 lambdas capture).
    pub fn state_handle(&self) -> &Arc<ReentrantMutex<S>> {
        self.0.state.get().expect("state not attached")
    }

    pub(crate) fn st(&self) -> SharedGuard<'_, S> {
        SharedGuard(self.state_handle().lock())
    }

    /// The engine-local state guard (reentrant).
    pub(crate) fn est(&self) -> ReentrantMutexGuard<'_, EngineState> {
        self.0.est.lock()
    }

    pub(crate) fn config(&self) -> std::sync::RwLockReadGuard<'_, Config> {
        self.0.config.read().expect("config poisoned")
    }

    /// C++ `window != nullptr`.
    pub fn window_present(&self) -> bool {
        self.0.window
    }

    fn level_data(&self) -> Arc<LevelData> {
        self.est()
            .level_data
            .clone()
            .expect("newGame not called yet (levelData unset)")
    }

    fn level_name(&self) -> String {
        self.st().level_name.clone()
    }

    // ------------------------------------------------------------------
    // `newGame` (HexagonGame.cpp:558, verbatim flow; window/audio/discord/
    // client branches are their null-gated headless forms)
    // ------------------------------------------------------------------

    /// C++ `newGame(mPackId, mId, mFirstPlay, mDifficultyMult,
    /// mExecuteLastReplay)`.
    pub fn new_game(
        &self,
        m_pack_id: &str,
        m_id: &str,
        m_first_play: bool,
        m_difficulty_mult: f32,
        m_execute_last_replay: bool,
    ) {
        // C++: save replay when restarting without having died.
        if !m_first_play {
            self.save_replay();
        }

        debug_assert!(self.0.assets.is_valid_pack_id(m_pack_id));
        debug_assert!(self.0.assets.is_valid_level_id(m_id));

        let mut g = self.st();
        let mut e = self.est();

        // C++ `packId = mPackId; levelId = mId;`
        e.pack_id = m_pack_id.to_string();
        e.level_id = m_id.to_string();

        if m_execute_last_replay {
            // C++ `replayPlayer.isActive()` — the active replay's firstPlay
            // wins; otherwise the argument.
            g.first_play = e
                .active_replay
                .as_ref()
                .map_or(m_first_play, |ar| ar.replay_file.first_play);
        } else {
            g.first_play = m_first_play;
        }

        self.set_level_data_inner(
            self.0
                .assets
                .get_level_data(m_id)
                .expect("SSVOH_ASSERT(assets.isValidLevelId(mId))"),
            m_first_play,
            &mut g,
            &mut e,
        );

        g.difficulty_mult = m_difficulty_mult;

        let temp_replay_score = get_replay_score(&g.status);
        // C++ `status = HexagonGameStatus{};`
        g.status = HexagonGameStatus::new();

        if !m_execute_last_replay {
            // C++ `rng = initializeRng();` — a fresh `pcg32_fast`-seeded
            // `random_number_generator` per non-replay game (see
            // `initialize_rng`).
            g.rng = initialize_rng();
            e.last_seed = g.rng.seed();
            e.last_replay_data = ReplayData::default();
            e.last_first_play = m_first_play;
            e.active_replay = None;
        } else {
            if e.active_replay.is_none() {
                e.last_played_score = temp_replay_score;
                // C++ `assets.getCurrentLocalProfile().getName()` — called
                // unconditionally (the `no_profile` fallback lives in
                // `death_createReplayFile`; NullProfiles reports "").
                let rf_name = self.0.profiles.current_local_profile_name();
                e.active_replay = Some(ActiveReplay::new(ReplayFile {
                    version: 0,
                    player_name: rf_name,
                    seed: e.last_seed,
                    data: e.last_replay_data.clone(),
                    pack_id: m_pack_id.to_string(),
                    level_id: m_id.to_string(),
                    first_play: e.last_first_play,
                    difficulty_mult: m_difficulty_mult,
                    played_score: e.last_played_score,
                }));
            }
            e.active_replay.as_mut().expect("active_replay").reset();

            debug_assert!(self.0.assets.is_valid_pack_id(m_pack_id));
            e.active_replay
                .as_mut()
                .expect("active_replay")
                .replay_pack_name = to_uppercase(
                self.0
                    .assets
                    .get_pack_data(m_pack_id)
                    .expect("SSVOH_ASSERT(assets.isValidPackId(mPackId))")
                    .name
                    .clone(),
            );
            e.active_replay
                .as_mut()
                .expect("active_replay")
                .replay_level_name =
                to_uppercase(e.level_data.as_ref().expect("level_data").name.clone());

            // C++ `rng = random_number_generator{activeReplay->replayFile._seed};`
            g.rng = RandomNumberGenerator::new(
                e.active_replay
                    .as_ref()
                    .expect("active_replay")
                    .replay_file
                    .seed,
            );
            g.first_play = e
                .active_replay
                .as_ref()
                .expect("active_replay")
                .replay_file
                .first_play;
        }

        // C++ audio cleanup block: `if(window && audio)` — audio never
        // starts in headless; ported for windowed compositions.
        if self.0.window {
            if let Some(a) = self.0.audio.as_ref() {
                a.stop_sounds();
                self.stop_level_music();
                if !self.config().get_no_music() {
                    self.play_level_music();
                    a.pause_music();
                    self.refresh_music_pitch();
                } else {
                    a.stop_music();
                }
            }
        }

        e.debug_pause = false;

        // Events cleanup.
        e.message_text.clear();
        e.pb_text.clear();
        g.event_timeline.clear();
        // C++ `eventTimelineRunner = {};` (HexagonGame.cpp:669). Resetting the
        // timeline WITHOUT its runner leaves a stale `wait_start_tp` /
        // `current_idx` (D-019) — the first `e_wait` of the next game then
        // measures against the PREVIOUS game's clock and the spawn schedule
        // shifts, so a live run no longer replays to the same death time.
        e.event_timeline_runner = Timeline2Runner::new();
        e.message_timeline.clear();
        e.message_timeline_runner = Timeline2Runner::new();
        g.custom_timelines.clear();

        // Manager cleanup.
        g.walls.clear();
        g.cw_manager.clear();
        e.player = CPlayer::new(
            &Vec2::new(0.0, 0.0),
            self.get_swap_cooldown(),
            {
                let c = self.config();
                c.get_player_size()
            },
            {
                let c = self.config();
                c.get_player_speed()
            },
            {
                let c = self.config();
                c.get_player_focus_speed()
            },
        );

        // Timeline cleanup.
        g.main_timeline.clear();
        // C++ `timelineRunner = {};` (HexagonGame.cpp:687) — see the
        // `event_timeline_runner` reset above for why the runner must be
        // reset alongside its timeline (D-019).
        e.main_timeline_runner = Timeline2Runner::new();
        e.effect_timeline_manager.clear();
        e.must_change_sides = false;
        e.must_start = false;

        // Particles cleanup.
        e.pb_text_growth = 0.0;
        e.must_spawn_pb_particles = false;
        e.swap_particles_spawn_info = None;
        e.next_pb_particle_spawn = 0.0;
        e.particles.clear();
        e.trail_particles.clear();
        e.swap_particles.clear();

        // Re-init default flash effect.
        self.init_flash_effect(255, 255, 255);

        // C++ window camera reset:
        if self.0.window {
            let c = self.config();
            let w = c.get_width() as f32;
            let h = c.get_height() as f32;
            let zoom = c.get_zoom_factor();
            let mut est = self.est();
            est.overlay_camera
                .as_mut()
                .expect("overlay camera")
                .set_view(Vec2::new(w / 2.0, h / 2.0), Vec2::new(w, h));
            est.background_camera
                .as_mut()
                .expect("background camera")
                .set_view(Vec2::new(0.0, 0.0), Vec2::new(w * zoom, h * zoom));
            est.background_camera
                .as_mut()
                .expect("background camera")
                .set_rotation(0.0);
            est.overlay_camera
                .as_mut()
                .expect("overlay camera")
                .set_skew(Vec2::new(1.0, 1.0));
            est.background_camera
                .as_mut()
                .expect("background camera")
                .set_skew(Vec2::new(1.0, 1.0));
        }

        e.input_impl_ccw = false;
        e.input_impl_cw = false;
        e.player_now_ready_to_swap = false;

        if !g.first_play {
            self.host_call_void("onPreUnload", self.0.script_host.run_on_pre_unload());
        }
        // C++ `lua = Lua::LuaContext{}; calledDeprecatedFunctions.clear();
        // initLua();`
        g.called_deprecated_functions.clear();
        self.0.script_host.init_lua();
        // C++ `runLuaFile(levelData->luaScriptPath);` (its own error path:
        // goToMenu(false, true) unless debug — no console print, unlike the
        // per-callback Lippincott handler).
        if let Err(msg) = self
            .0
            .script_host
            .run_lua_file(&self.level_data().lua_script_path)
        {
            self.host_error("runLuaFile", &msg, false);
        }

        if !g.first_play {
            self.host_call_void("onUnload", self.0.script_host.run_on_unload());
            self.play_sound_override("restart.ogg");
        } else {
            self.play_sound_override("select.ogg");
        }

        self.host_call_void("onInit", self.0.script_host.run_on_init());

        e.restart_id = m_id.to_string();
        e.restart_first_time = false;
        // C++ `setSides(levelStatus.sides)` — plays the beep + clamps >= 3.
        self.set_sides(g.level_status.sides);

        // Set initial values for some status fields from Lua.
        g.status.pulse_delay += g.level_status.pulse_initial_delay;
        g.status.beat_pulse_delay += g.level_status.beat_pulse_initial_delay;
        e.time_until_rich_presence_update = -1.0; // immediate update

        // C++ restart/replay input hint strings: windowed-only (and
        // Config bind-name plumbing, issues 10/15); headless leaves
        // `status.restartInput/replayInput` empty, exactly like the C++
        // headless tests.
    }

    /// C++ `setLevelData(mLevelData, mMusicFirstPlay)` (engine-internal
    /// here; the C++ method is only ever called from `newGame`).
    fn set_level_data_inner(
        &self,
        m_level_data: &LevelData,
        m_music_first_play: bool,
        g: &mut SharedState,
        e: &mut EngineState,
    ) {
        let cfg = self.config();
        // C++ `levelData = &mLevelData;`
        e.level_data = Some(Arc::new(m_level_data.clone()));
        // C++ `levelStatus = LevelStatus{Config::getMusicSpeedDMSync(),
        // Config::getSpawnDistance()};`
        g.level_status = LevelStatus::new(cfg.get_music_speed_dm_sync(), cfg.get_spawn_distance());
        // C++ `styleData = assets.getStyleData(levelData->packId,
        // levelData->styleId); styleData.computeColors();`
        let mut style: StyleData = self
            .0
            .assets
            .get_style_data(&m_level_data.pack_id, &m_level_data.style_id)
            .expect("assets.getStyleData(levelData->packId, levelData->styleId)")
            .clone();
        style.compute_colors();
        g.style_data = style;
        // C++ `musicData = assets.getMusicData(...); musicData.firstPlay =
        // mMusicFirstPlay;`
        let mut music_data = self
            .0
            .assets
            .get_music_data(&m_level_data.pack_id, &m_level_data.music_id)
            .expect("assets.getMusicData(levelData->packId, levelData->musicId)")
            .clone();
        music_data.first_play = m_music_first_play;
        g.music_data = music_data;

        // The 007 lambdas read `levelData->packPath/name/packId` and
        // `getPackData()`; the shared store snapshots them here (C++ reads
        // them live through the pointer).
        g.pack_path = m_level_data.pack_path.clone();
        g.level_name = m_level_data.name.clone();
        g.pack_id = m_level_data.pack_id.clone();
        g.pack_data = self
            .0
            .assets
            .get_pack_data(&m_level_data.pack_id)
            .cloned()
            .unwrap_or_else(|| PackData {
                folder_path: String::new(),
                id: String::new(),
                disambiguator: String::new(),
                name: String::new(),
                author: String::new(),
                description: String::new(),
                version: 0,
                priority: 0.0,
                dependencies: Vec::new(),
            });
    }

    // ------------------------------------------------------------------
    // `start` (HGUpdate.cpp:505, verbatim)
    // ------------------------------------------------------------------

    /// C++ `start()`.
    pub fn start(&self) {
        let mut g = self.st();
        g.status.start();
        self.est().message_text.clear();
        self.play_sound_override("go.ogg");

        if !self.must_replay_input() {
            // C++ builds nameStr/packStr/diffStr here for the (absent)
            // discordManager / hexagonClient — both are null in this port,
            // so no engine-side effect survives.
        }
        // else: discordManager null -> no-op.

        // C++ `if(audio != nullptr && !Config::getNoMusic())` — NOTE: no
        // window gate in the C++.
        if let Some(a) = self.0.audio.as_ref() {
            if !self.config().get_no_music() {
                a.resume_music();
            }
        }

        self.host_call_void("onLoad", self.0.script_host.run_on_load());
    }

    /// C++ `setMustStart(x)` (test hook).
    pub fn set_must_start(&self, x: bool) {
        self.est().must_start = x;
    }

    // ------------------------------------------------------------------
    // Sides / difficulty / swap / kill
    // ------------------------------------------------------------------

    /// C++ `setSides(mSides)`.
    pub fn set_sides(&self, mut m_sides: u32) {
        {
            let g = self.st();
            self.play_sound_override(&g.level_status.beep_sound);
        }
        if m_sides < 3 {
            m_sides = 3;
        }
        self.st().level_status.sides = m_sides;
    }

    /// C++ `incrementDifficulty()` (verbatim).
    pub fn increment_difficulty(&self) {
        self.play_sound_override("levelUp.ogg");

        let mut g = self.st();
        let sign_mult = if g.level_status.rotation_speed > 0.0_f32 {
            1.0_f32
        } else {
            -1.0_f32
        };

        g.level_status.rotation_speed += g.level_status.rotation_speed_inc * sign_mult;

        let rotation_speed_max = g.level_status.rotation_speed_max;
        if g.level_status.rotation_speed.abs() > rotation_speed_max {
            g.level_status.rotation_speed = rotation_speed_max * sign_mult;
        }

        g.level_status.rotation_speed *= -1.0_f32;
        g.status.fast_spin = g.level_status.fast_spin;
    }

    /// C++ `sideChange(mSideNumber)` (verbatim).
    pub fn side_change(&self, m_side_number: u32) {
        {
            let mut g = self.st();
            g.level_status.speed_mult += g.level_status.speed_inc;
            g.level_status.delay_mult += g.level_status.delay_inc;

            if g.level_status.rnd_side_changes_enabled {
                self.set_sides(m_side_number);
            }

            self.est().must_change_sides = false;

            self.play_sound_override(&g.level_status.level_up_sound);
        }
        self.host_call_void("onIncrement", self.0.script_host.run_on_increment());
    }

    /// C++ `getSwapCooldown()`.
    #[must_use]
    pub fn get_swap_cooldown(&self) -> f32 {
        let g = self.st();
        f32::max(36.0_f32 * g.level_status.swap_cooldown_mult, 8.0_f32)
    }

    /// C++ `performPlayerSwap(mPlaySound)` (verbatim).
    pub fn perform_player_swap(&self, m_play_sound: bool) {
        self.est().player.player_swap();
        self.host_call_void("onCursorSwap", self.0.script_host.run_on_cursor_swap());
        if m_play_sound {
            let g = self.st();
            self.play_sound_override(&g.level_status.swap_sound);
        }
    }

    /// C++ `performPlayerKill()` (verbatim).
    pub fn perform_player_kill(&self) {
        let fatal = {
            let cfg = self.config();
            let g = self.st();
            !cfg.get_invincible() && !g.level_status.tutorial_mode
        };
        self.est().player.kill(fatal);
        self.death(false);
    }

    // ------------------------------------------------------------------
    // Death family (HexagonGame.cpp:822ff / 943ff, verbatim)
    // ------------------------------------------------------------------

    /// C++ `death(mForce)` (verbatim; window/audio/client branches are
    /// their null-gated forms).
    pub fn death(&self, m_force: bool) {
        {
            let g = self.st();
            if g.status.has_died {
                return;
            }
        }
        self.est().death_input_ignore = 10.0_f32;

        {
            let g = self.st();
            self.play_sound_abort(&g.level_status.death_sound);
        }

        self.host_call_void("onPreDeath", self.0.script_host.run_on_pre_death());

        {
            let cfg = self.config();
            let g = self.st();
            if !m_force && (cfg.get_invincible() || g.level_status.tutorial_mode) {
                return;
            }
        }

        self.host_call_void("onDeath", self.0.script_host.run_on_death());
        self.death_shake_camera();
        self.death_update_rich_presence();
        self.stop_level_music();
        self.death_flash_effect();

        self.st().status.has_died = true;

        if !self.in_replay() {
            let rf = self.death_create_replay_file();
            // C++ `if(onDeathReplayCreated) onDeathReplayCreated(rf);`
            if let Some(cb) = &*self
                .0
                .on_death_replay_created
                .read()
                .expect("on_death_replay_created poisoned")
            {
                cb(&rf);
            }
            self.death_send_and_save_replay(&rf);
        }

        self.death_save_score_if_needed_and_show_pb_effects();

        // C++ `if(window && Config::getAutoRestart())`.
        if self.0.window && self.config().get_auto_restart() {
            self.st().status.must_state_change = StateChange::MustRestart;
        }
    }

    /// C++ `death_shakeCamera()`.
    fn death_shake_camera(&self) {
        if !self.0.window {
            return;
        }
        let mut e = self.est();
        let c = self.config();
        e.overlay_camera.as_mut().expect("overlay camera").set_view(
            Vec2::new(c.get_width() as f32 / 2.0, c.get_height() as f32 / 2.0),
            Vec2::new(c.get_width() as f32, c.get_height() as f32),
        );
        e.background_camera
            .as_mut()
            .expect("background camera")
            .set_center(Vec2::new(0.0, 0.0));
        self.st().status.camera_shake = 45.0_f32 * c.get_camera_shake_multiplier();
    }

    /// C++ `death_updateRichPresence()` — windowed-only AND both rich-
    /// presence managers are null in this port, so this is the C++ no-op.
    fn death_update_rich_presence(&self) {
        // (no-op: window gate + null managers, verbatim C++ behavior)
    }

    /// C++ `death_flashEffect()`.
    fn death_flash_effect(&self) {
        self.init_flash_effect(255, 255, 255);
        self.st().status.flash_effect = 255.0;
    }

    /// C++ `death_createReplayFile()` (verbatim).
    fn death_create_replay_file(&self) -> ReplayFile {
        let rf_name = if self.0.profiles.any_local_profile_active() {
            self.0.profiles.current_local_profile_name()
        } else {
            "no_profile".to_string()
        };
        let g = self.st();
        let e = self.est();
        ReplayFile {
            version: 0,
            player_name: rf_name,
            seed: e.last_seed,
            data: e.last_replay_data.clone(),
            pack_id: e.pack_id.clone(),
            level_id: e.level_id.clone(),
            first_play: g.first_play,
            difficulty_mult: g.difficulty_mult,
            played_score: get_replay_score(&g.status),
        }
    }

    /// C++ `death_sendAndSaveReplay(rf)` (verbatim; the server-send half
    /// always fails headless: `hexagonClient == nullptr`).
    fn death_send_and_save_replay(&self, rf: &ReplayFile) {
        let Some(crf) = compress_replay_file(rf) else {
            self.log(
                "Replay",
                "Failed to compress replay, will not save to file or send to server\n",
            );
            return;
        };

        // C++ send half: `death_sendReplay(levelValidator, crf)` —
        // `hexagonClient == nullptr` -> `return false` ->
        // "Failure sending replay".
        let _level_validator = get_level_validator(&rf.level_id, rf.difficulty_mult);
        self.log("Replay", "Failure sending replay\n");

        // C++ save half.
        let mut dir_path = format!(
            "Replays/{}/{}x/",
            rf.level_id,
            self.diff_format(rf.difficulty_mult)
        );
        let mut filename = rf.create_filename() + ".z";
        // C++ `for(const char c : {':', '*', '?', '"', '<', '>', '|'})
        // replace(...)`.
        for c in [':', '*', '?', '"', '<', '>', '|'] {
            dir_path = dir_path.replace(c, "_");
            filename = filename.replace(c, "_");
        }
        if !self.0.replay_store.save(&dir_path, &filename, &crf) {
            self.log("Replay", "Failure saving replay\n");
        }
    }

    /// C++ `death_saveScoreIfNeeded()` (verbatim).
    fn death_save_score_if_needed(&self) -> SaveScoreIfNeededResult {
        if !self.0.window {
            return SaveScoreIfNeededResult::NoWindow;
        }

        if !self.should_save_score() {
            return SaveScoreIfNeededResult::ShouldNotSave;
        }

        let level_data = self.level_data();
        let validator_without_pack_id = level_data
            .get_validator_without_pack_id(self.difficulty_mult())
            .expect("levelData->getValidatorWithoutPackId(difficultyMult)");

        let score = self.st().status.get_time_seconds();

        let is_personal_best =
            score > self.0.profiles.get_local_score(validator_without_pack_id) as f64;

        if !is_personal_best {
            return SaveScoreIfNeededResult::NotPersonalBest;
        }

        self.0
            .profiles
            .set_local_score(validator_without_pack_id, score as f32);
        SaveScoreIfNeededResult::PersonalBest
    }

    /// C++ `death_saveScoreIfNeededAndShowPBEffects()` (verbatim).
    fn death_save_score_if_needed_and_show_pb_effects(&self) {
        let r = self.death_save_score_if_needed();

        if matches!(r, SaveScoreIfNeededResult::NoWindow) {
            return;
        }

        if matches!(
            r,
            SaveScoreIfNeededResult::ShouldNotSave | SaveScoreIfNeededResult::NotPersonalBest
        ) {
            self.play_sound_abort("gameOver.ogg");
            return;
        }

        let mut e = self.est();
        e.pb_text = "NEW PERSONAL BEST!".to_string();
        e.must_spawn_pb_particles = true;
        self.play_sound_abort("personalBest.ogg");
    }

    /// C++ `HexagonGame::SaveScoreIfNeededResult` lives at module scope
    /// (Rust forbids type definitions inside impl blocks).
    /// C++ `shouldSaveScore()` (verbatim, including the logs).
    fn should_save_score(&self) -> bool {
        if !self.0.profiles.any_local_profile_active() {
            self.log(
                "hg::HexagonGame::shouldSaveScore()",
                "No local profile active, rejecting\n",
            );
            return false;
        }

        {
            let mut cfg = self.0.config.write().expect("config poisoned");
            if !cfg.is_eligible_for_score() {
                self.log(
                    "hg::HexagonGame::shouldSaveScore()",
                    &format!(
                        "Not saving score - not eligible - {} - {}",
                        cfg.get_uneligibility_reason(),
                        '\n'
                    ),
                );
                return false;
            }
        }

        let g = self.st();
        if g.status.score_invalid {
            self.log(
                "hg::HexagonGame::shouldSaveScore()",
                "Not saving score - score invalidated\n",
            );
            return false;
        }

        if g.level_status.tutorial_mode {
            self.log(
                "hg::HexagonGame::shouldSaveScore()",
                "Not saving score - in tutorial mode\n",
            );
            return false;
        }

        if self.level_data().unscored {
            self.log(
                "hg::HexagonGame::shouldSaveScore()",
                "Not saving score - unscored level\n",
            );
            return false;
        }

        if self.in_replay() {
            self.log(
                "hg::HexagonGame::shouldSaveScore()",
                "Not saving score - currently in replay\n",
            );
            return false;
        }

        true
    }

    // ------------------------------------------------------------------
    // Replay record / playback (HexagonGame.cpp:467 / 1100ff, verbatim)
    // ------------------------------------------------------------------

    /// C++ `setLastReplay(mReplayFile)` (verbatim).
    pub fn set_last_replay(&self, m_replay_file: ReplayFile) {
        let mut e = self.est();
        e.last_seed = m_replay_file.seed;
        e.last_replay_data = m_replay_file.data.clone();
        e.last_first_play = m_replay_file.first_play;
        e.last_played_score = m_replay_file.played_score;

        e.active_replay = Some(ActiveReplay::new(m_replay_file));
    }

    /// C++ `inReplay()`.
    #[must_use]
    pub fn in_replay(&self) -> bool {
        self.est().active_replay.is_some()
    }

    /// C++ `mustReplayInput()`.
    #[must_use]
    pub fn must_replay_input(&self) -> bool {
        let e = self.est();
        e.active_replay.as_ref().is_some_and(|ar| !ar.done())
    }

    /// C++ `mustShowReplayUI()`.
    #[must_use]
    pub fn must_show_replay_ui(&self) -> bool {
        self.in_replay()
    }

    /// The active replay file (for tests; C++ exposes it via the member).
    #[must_use]
    pub fn active_replay_file(&self) -> Option<ReplayFile> {
        self.est()
            .active_replay
            .as_ref()
            .map(|ar| ar.replay_file.clone())
    }

    /// `lastReplayData` (the recorded input so far; test hook).
    #[must_use]
    pub fn last_replay_data(&self) -> ReplayData {
        self.est().last_replay_data.clone()
    }

    /// The C++ `onDeathReplayCreated` test hook.
    pub fn set_on_death_replay_created(
        &self,
        cb: Arc<dyn Fn(&ReplayFile) + Send + Sync + 'static>,
    ) {
        *self
            .0
            .on_death_replay_created
            .write()
            .expect("on_death_replay_created poisoned") = Some(cb);
    }

    /// C++ `executeRandomInputs` test flag.
    pub fn set_execute_random_inputs(&self, value: bool) {
        self.est().execute_random_inputs = value;
    }

    /// C++ `alwaysSpinRight` test flag.
    pub fn set_always_spin_right(&self, value: bool) {
        self.est().always_spin_right = value;
    }

    /// The C++ `fastForwardTarget` hook (the ImGui `!ff <seconds>` command,
    /// issue 14; exposed directly for tests).
    pub fn set_fast_forward_target(&self, target: f64) {
        self.est().fast_forward_target = Some(target);
    }

    /// The C++ `advanceTickCount` hook (the ImGui `!advt <ticks>` command;
    /// exposed directly for tests).
    pub fn set_advance_tick_count(&self, n: i32) {
        self.est().advance_tick_count = Some(n);
    }

    // ------------------------------------------------------------------
    // Headless execution (HexagonGame.cpp:1100ff, verbatim)
    // ------------------------------------------------------------------

    /// C++ `executeGameUntilDeath(maxProcessingSeconds, timescale)`.
    #[must_use]
    pub fn execute_game_until_death(
        &self,
        max_processing_seconds: i32,
        timescale: f32,
    ) -> Option<GameExecutionResult> {
        let tp_begin = SystemTime::now();
        let exceeded_processing_time = || {
            SystemTime::now()
                .duration_since(tp_begin)
                .map(|d| d.as_secs_f64() > max_processing_seconds as f64)
                .unwrap_or(false)
        };

        while !self.st().status.has_died {
            self.update(TIME_STEP, timescale);
            self.post_update();

            if exceeded_processing_time() {
                return None;
            }
        }

        let g = self.st();
        Some(GameExecutionResult {
            // C++ `status.getPlayedAccumulatedFrametimeInSeconds()` etc.
            played_time_seconds: g.status.get_played_accumulated_frametime_in_seconds(),
            paused_time_seconds: g.status.get_paused_accumulated_frametime_in_seconds(),
            total_time_seconds: g.status.get_total_accumulated_frametime_in_seconds(),
            custom_score: g.status.get_custom_score(),
        })
    }

    /// C++ `runReplayUntilDeathAndGetScore(mReplayFile, maxProcessingSeconds,
    /// timescale)` (verbatim).
    #[must_use]
    pub fn run_replay_until_death_and_get_score(
        &self,
        m_replay_file: &ReplayFile,
        max_processing_seconds: i32,
        timescale: f32,
    ) -> Option<GameExecutionResult> {
        debug_assert!(self.0.assets.is_valid_pack_id(&m_replay_file.pack_id));
        debug_assert!(self.0.assets.is_valid_level_id(&m_replay_file.level_id));

        self.set_last_replay(m_replay_file.clone());

        self.new_game(
            &m_replay_file.pack_id,
            &m_replay_file.level_id,
            m_replay_file.first_play,
            m_replay_file.difficulty_mult,
            true, /* mExecuteLastReplay */
        );

        self.execute_game_until_death(max_processing_seconds, timescale)
    }

    // ------------------------------------------------------------------
    // goToMenu / saveReplay / invalidateScore / addMessage (verbatim)
    // ------------------------------------------------------------------

    /// C++ `goToMenu(mSendScores, mError)` (verbatim; `fnGoToMenu` is the
    /// app-level callback and is unset in this port — the C++ guard
    /// `if(fnGoToMenu)` skips it).
    pub fn go_to_menu(&self, m_send_scores: bool, m_error: bool) {
        if !self.0.window {
            self.log(
                "hg::HexagonGame::goToMenu",
                "Attempted to go back to menu without a game window\n",
            );
            return;
        }

        if let Some(a) = self.0.audio.as_ref() {
            a.stop_sounds();
        }

        self.st().called_deprecated_functions.clear();

        if !m_error {
            self.play_sound_override("beep.ogg");
        }

        if m_send_scores && !m_error {
            self.save_replay();
        }

        // Stop infinite feedback from occurring if the error is happening on
        // onUnload.
        if !m_error {
            self.host_call_void("onUnload", self.0.script_host.run_on_unload());
        }

        // C++ `if(fnGoToMenu) fnGoToMenu(mError);` — unset in this port.
    }

    /// C++ `saveReplay()` (verbatim).
    pub fn save_replay(&self) {
        if self.should_save_score() && !self.st().status.has_died {
            self.death_save_score_if_needed(); // Saves local best
            let rf = self.death_create_replay_file();
            self.log("Replay", "Attempting to send and save replay...\n");
            self.death_send_and_save_replay(&rf);
        }
    }

    /// C++ `invalidateScore(mReason)` (verbatim).
    pub fn invalidate_score(&self, m_reason: &str) {
        {
            let mut g = self.st();
            if g.status.score_invalid {
                return;
            }
            g.status.score_invalid = true;
            g.status.invalid_reason = m_reason.to_string();
        }
        self.log(
            "HexagonGame::invalidateScore",
            &format!("Invalidating official game ({m_reason})\n"),
        );
    }

    /// C++ `addMessage(mMessage, mDuration, mSoundToggle)` (verbatim).
    pub fn add_message(&self, m_message: &str, m_duration: f64, m_sound_toggle: bool) {
        if !self.config().get_show_messages() {
            return;
        }
        let mut message = m_message.to_string();
        oh_math::hg::string::uppercasify(&mut message);

        // The C++ lambdas capture `this`; the port captures the engine
        // handle (an `Arc` over the same core) so the action fires
        // identically when the message timeline reaches it.
        let handle = self.clone();
        let msg = message;
        self.est().message_timeline.append_do(move || {
            if m_sound_toggle {
                let g = handle.st();
                handle.play_sound_override(&g.level_status.beep_sound);
            }
            handle.est().message_text = msg.clone();
        });

        self.est()
            .message_timeline
            .append_wait_for_sixths(m_duration);
        let handle2 = self.clone();
        self.est().message_timeline.append_do(move || {
            handle2.est().message_text.clear();
        });
    }

    /// C++ `clearMessages()`.
    pub fn clear_messages(&self) {
        self.est().message_timeline.clear();
    }

    // ------------------------------------------------------------------
    // Music / sound (HexagonGame.cpp:515ff / 1379ff, verbatim)
    // ------------------------------------------------------------------

    /// C++ `shouldPlaySounds()`.
    fn should_play_sounds(&self) -> bool {
        self.0.window && self.0.audio.is_some() && !self.config().get_no_sound()
    }

    /// C++ `shouldPlayMusic()`.
    fn should_play_music(&self) -> bool {
        self.0.window && self.0.audio.is_some() && !self.config().get_no_music()
    }

    /// C++ `playSoundOverride(mId)`.
    pub fn play_sound_override(&self, m_id: &str) {
        if self.should_play_sounds() {
            self.0
                .audio
                .as_ref()
                .expect("audio")
                .play_sound_override(m_id);
        }
    }

    /// C++ `playSoundAbort(mId)`.
    pub fn play_sound_abort(&self, m_id: &str) {
        if self.should_play_sounds() {
            self.0.audio.as_ref().expect("audio").play_sound_abort(m_id);
        }
    }

    /// C++ `playPackSoundOverride(mPackId, mId)`.
    pub fn play_pack_sound_override(&self, m_pack_id: &str, m_id: &str) {
        if self.should_play_sounds() {
            self.0
                .audio
                .as_ref()
                .expect("audio")
                .play_pack_sound_override(m_pack_id, m_id);
        }
    }

    /// C++ `playLevelMusic()` (verbatim, incl. the `beatPulseDelay`
    /// gameplay effect of `playRandomSegment`).
    pub fn play_level_music(&self) {
        if !self.should_play_music() {
            return;
        }
        let mut g = self.st();
        let seg_index = if g.music_data.first_play {
            g.music_data.first_play = false;
            0
        } else {
            // C++ `ssvu::getRndI(std::size_t(0), segments.size())` — the
            // SSVUtils GLOBAL rng (not the game rng); the port draws from
            // its `CosmeticRng` seam (documented: seed streams are not
            // comparable between C++ and the port).
            let n = g.music_data.segments().len() as i32;
            if n == 0 {
                0
            } else {
                self.0.cosmetic_rng.get_rnd_i(0, n) as usize
            }
        };
        let seg = g
            .music_data
            .get_segment(seg_index)
            .expect("C++ playSegment(mIdx): segments[mIdx]");
        // C++ `playSeconds` -> `loadAndPlayMusic`.
        if let Some(a) = self.0.audio.as_ref() {
            if !a.load_and_play_music(&self.get_pack_id(), &g.music_data.id, seg.time) {
                self.log(
                    "MusicData::playSeconds",
                    &format!(
                        "Failed playing music '{}'_'{}'\n",
                        self.get_pack_id(),
                        g.music_data.id
                    ),
                );
            }
        }
        // TODO (P0) can this actually de-sync replays? beatpulse affects
        // gameplay — ported verbatim.
        g.status.beat_pulse_delay += seg.beat_pulse_delay_offset;
    }

    /// C++ `playLevelMusicAtTime(mSeconds)`.
    pub fn play_level_music_at_time(&self, m_seconds: f32) {
        if !self.should_play_music() {
            return;
        }
        if let Some(a) = self.0.audio.as_ref() {
            let id = {
                let g = self.st();
                g.music_data.id.clone()
            };
            if !a.load_and_play_music(&self.get_pack_id(), &id, m_seconds) {
                self.log(
                    "MusicData::playSeconds",
                    &format!("Failed playing music '{}'_'{id}'\n", self.get_pack_id()),
                );
            }
        }
    }

    /// C++ `stopLevelMusic()`.
    pub fn stop_level_music(&self) {
        if self.should_play_music() {
            self.0.audio.as_ref().expect("audio").stop_music();
        }
    }

    /// C++ `getMusicDMSyncFactor()`.
    #[must_use]
    pub fn get_music_dm_sync_factor(&self) -> f32 {
        let g = self.st();
        g.difficulty_mult.powf(0.12_f32)
    }

    /// C++ `getOptionalMusicDMSyncFactor()`.
    #[must_use]
    pub fn get_optional_music_dm_sync_factor(&self) -> f32 {
        let g = self.st();
        if g.level_status.sync_music_to_dm {
            self.get_music_dm_sync_factor()
        } else {
            1.0_f32
        }
    }

    /// C++ `refreshMusicPitch()` (verbatim — note: audio-null gate, NOT
    /// the window gate).
    pub fn refresh_music_pitch(&self) {
        if let Some(a) = self.0.audio.as_ref() {
            a.set_current_music_pitch(
                self.get_optional_music_dm_sync_factor() * self.config().get_music_speed_mult() * {
                    let g = self.st();
                    g.level_status.music_pitch
                },
            );
        }
    }

    // ------------------------------------------------------------------
    // Misc getters (HexagonGame.cpp, verbatim)
    // ------------------------------------------------------------------

    /// C++ `getReplayScore` accessor (the file-local `get_replay_score`).
    #[must_use]
    pub fn get_replay_score(&self) -> f64 {
        get_replay_score(&self.st().status)
    }

    /// C++ `getPackId()` — `levelData->packId`.
    #[must_use]
    pub fn get_pack_id(&self) -> String {
        self.level_data().pack_id.clone()
    }

    /// C++ `getPackName()` — `getPackData().name`.
    #[must_use]
    pub fn get_pack_name(&self) -> String {
        self.0
            .assets
            .get_pack_data(&self.get_pack_id())
            .expect("getPackData()")
            .name
            .clone()
    }

    /// C++ `getLevelData` accessor (the port owns a clone).
    #[must_use]
    pub fn get_level_data(&self) -> Arc<LevelData> {
        self.level_data()
    }

    /// C++ `getSides()`.
    #[must_use]
    pub fn get_sides(&self) -> u32 {
        self.st().level_status.sides
    }

    /// C++ `getRadius()`.
    #[must_use]
    pub fn get_radius(&self) -> f32 {
        self.st().status.radius
    }

    /// C++ `getRotationSpeed()`.
    #[must_use]
    pub fn get_rotation_speed(&self) -> f32 {
        self.st().level_status.rotation_speed
    }

    /// C++ `getWallSkewLeft()` / `getWallSkewRight()`.
    #[must_use]
    pub fn get_wall_skew_left(&self) -> f32 {
        self.st().level_status.wall_skew_left
    }

    #[must_use]
    pub fn get_wall_skew_right(&self) -> f32 {
        self.st().level_status.wall_skew_right
    }

    /// C++ `getWallAngleLeft()` / `getWallAngleRight()`.
    #[must_use]
    pub fn get_wall_angle_left(&self) -> f32 {
        self.st().level_status.wall_angle_left
    }

    #[must_use]
    pub fn get_wall_angle_right(&self) -> f32 {
        self.st().level_status.wall_angle_right
    }

    /// C++ `getInputMovement()`.
    #[must_use]
    pub fn get_input_movement(&self) -> i32 {
        self.est().input_movement
    }

    /// C++ `getInputFocused()`.
    #[must_use]
    pub fn get_input_focused(&self) -> bool {
        self.est().input_focused
    }

    /// C++ `getInputSwap()`.
    #[must_use]
    pub fn get_input_swap(&self) -> bool {
        self.est().input_swap
    }

    /// C++ `getPlayerSpeedMult()`.
    #[must_use]
    pub fn get_player_speed_mult(&self) -> f32 {
        self.st().level_status.player_speed_mult
    }

    /// C++ `getStatus()` (clone; the C++ returns a reference).
    #[must_use]
    pub fn get_status(&self) -> HexagonGameStatus {
        self.st().status.clone()
    }

    /// C++ `getLevelStatus()` (clone; the C++ returns a mutable reference).
    #[must_use]
    pub fn get_level_status(&self) -> LevelStatus {
        self.st().level_status.clone()
    }

    /// C++ `difficultyMult` (shared with the lambdas).
    #[must_use]
    pub fn difficulty_mult(&self) -> f32 {
        self.st().difficulty_mult
    }

    /// C++ `player.getPlayerAngle()`.
    #[must_use]
    pub fn get_player_angle(&self) -> f32 {
        self.est().player.get_player_angle()
    }

    /// C++ `player.setPlayerAngle(angle)`.
    pub fn set_player_angle(&self, angle: f32) {
        self.est().player.set_player_angle(angle);
    }

    /// C++ `player.updatePosition(radius)`.
    pub fn update_player_position(&self, radius: f32) {
        self.est().player.update_position(radius);
    }

    /// `player.getPosition()` (test/draw access).
    #[must_use]
    pub fn get_player_position(&self) -> Vec2 {
        self.est().player.get_position()
    }

    /// The flash-effect batch (C++ `flashPolygon`).
    #[must_use]
    pub fn flash_polygon(&self) -> FastVertexVectorTris {
        self.est().flash_polygon.clone()
    }

    // ------------------------------------------------------------------
    // Colors (HexagonGame.cpp:1423ff, verbatim)
    // ------------------------------------------------------------------

    /// C++ `getColorMain()`.
    #[must_use]
    pub fn get_color_main(&self) -> oh_math::hg::color::Color {
        let g = self.st();
        if self.config().get_black_and_white() {
            oh_math::hg::color::Color::new(255, 255, 255, g.style_data.get_main_color().a)
        } else {
            *g.style_data.get_main_color()
        }
    }

    /// C++ `getColorPlayer()`.
    #[must_use]
    pub fn get_color_player(&self) -> oh_math::hg::color::Color {
        let g = self.st();
        if self.config().get_black_and_white() {
            oh_math::hg::color::Color::new(255, 255, 255, g.style_data.get_player_color().a)
        } else {
            *g.style_data.get_player_color()
        }
    }

    /// C++ `getColorPlayerAdjustedForSwap()`.
    #[must_use]
    pub fn get_color_player_adjusted_for_swap(&self) -> oh_math::hg::color::Color {
        let g = self.st();
        if self.config().get_black_and_white() {
            return oh_math::hg::color::Color::new(
                255,
                255,
                255,
                g.style_data.get_player_color().a,
            );
        }
        if !self.config().get_show_swap_blinking_effect() {
            return self.get_color_player();
        }
        let color = self.get_color_player();
        self.est().player.get_color_adjusted_for_swap(color)
    }

    /// C++ `getColorPlayerTrail()`.
    #[must_use]
    pub fn get_color_player_trail(&self) -> oh_math::hg::color::Color {
        if self.config().get_player_trail_has_swap_color() {
            self.get_color_player_adjusted_for_swap()
        } else {
            self.get_color_player()
        }
    }

    /// C++ `getColorText()`.
    #[must_use]
    pub fn get_color_text(&self) -> oh_math::hg::color::Color {
        let g = self.st();
        if self.config().get_black_and_white() {
            oh_math::hg::color::Color::new(255, 255, 255, g.style_data.get_text_color().a)
        } else {
            *g.style_data.get_text_color()
        }
    }

    // ------------------------------------------------------------------
    // `initFlashEffect` (HGGraphics.cpp, mapping §17, verbatim)
    // ------------------------------------------------------------------

    /// C++ `initFlashEffect(r, g, b)` (verbatim: the screen quad + 100.f
    /// margin, initial alpha 0; `updateFlash` drives the alpha).
    pub fn init_flash_effect(&self, r: i32, g: i32, b: i32) {
        let c = self.config();
        let color = oh_math::hg::color::Color::new(r as u8, g as u8, b as u8, 0);

        let width = c.get_width() as f32;
        let height = c.get_height() as f32;
        let offset = 100.0_f32;

        let nw = Vec2::new(-offset, -offset);
        let sw = Vec2::new(-offset, height + offset);
        let se = Vec2::new(width + offset, height + offset);
        let ne = Vec2::new(width + offset, -offset);

        let poly = &mut self.est().flash_polygon;
        poly.clear();
        poly.reserve(6);
        poly.batch_unsafe_emplace_back_quad(color, nw, sw, se, ne);
    }

    // ------------------------------------------------------------------
    // `createWall` (HexagonGame.cpp:74, verbatim)
    // ------------------------------------------------------------------

    /// C++ `createWall(mSide, mThickness, mSpeed, mCurve, mHueMod)`.
    pub fn create_wall(
        &self,
        m_side: i32,
        m_thickness: f32,
        m_speed: &SpeedData,
        m_curve: &SpeedData,
        m_hue_mod: f32,
    ) {
        let mut g = self.st();
        let sides = g.level_status.sides;
        let (angle_l, angle_r, skew_l, skew_r, spawn) = (
            g.level_status.wall_angle_left,
            g.level_status.wall_angle_right,
            g.level_status.wall_skew_left,
            g.level_status.wall_skew_right,
            g.level_status.wall_spawn_distance,
        );
        g.walls.push(CWall::new(
            sides,
            angle_l,
            angle_r,
            skew_l,
            skew_r,
            &Vec2::new(0.0, 0.0), // centerPos
            m_side,
            m_thickness,
            spawn,
            m_speed,
            m_curve,
            m_hue_mod,
        ));
    }

    // ------------------------------------------------------------------
    // Logging + the Lippincott Lua-error handler
    // ------------------------------------------------------------------

    /// The `ssvu::lo(channel)` sink (the C++ prints to stdout; the port
    /// does the same, channel-prefixed).
    pub(crate) fn log(&self, channel: &str, message: &str) {
        println!("[{channel}] {message}");
    }

    /// The C++ `luaExceptionLippincottHandler` behavior for a failed
    /// callback: print, then `goToMenu(false, true)` unless debug.
    pub(crate) fn host_error(&self, name: &str, message: &str, print_level: bool) {
        if print_level {
            println!(
                "[runLuaFunctionIfExists] Runtime error on \"{name}\" with level \"{}\": \n{}\n",
                self.level_name(),
                message
            );
        }
        if !self.config().get_debug() {
            self.go_to_menu(false, true);
        }
    }

    /// Runs `result` through the Lippincott handler; `Ok(v)` -> `Some(v)`.
    fn host_call<R>(&self, name: &str, result: Result<R, String>) -> Option<R> {
        match result {
            Ok(v) => Some(v),
            Err(msg) => {
                self.host_error(name, &msg, true);
                None
            }
        }
    }

    /// The void-callback form of [`Self::host_call`].
    fn host_call_void(&self, name: &str, result: Result<(), String>) {
        self.host_call(name, result);
    }

    // ------------------------------------------------------------------
    // Formatting helpers (HexagonGame.cpp:251ff + HGGraphics.cpp:33,
    // verbatim)
    // ------------------------------------------------------------------

    /// C++ `nameFormat(name)` — capitalizes the first char (returns the
    /// formatted copy; the C++ mutates in place).
    #[must_use]
    pub(crate) fn name_format(&self, name: &str) -> String {
        let mut s = name.to_string();
        if let Some(first) = s.get_mut(0..1) {
            first.make_ascii_uppercase();
        }
        s
    }

    /// C++ `diffFormat(diff)` — `snprintf("%g")`: the C default 6-digit
    /// general format, which the codebase's oracle-pinned ostream
    /// formatter reproduces (mapping/oracle-vectors.md, format probe).
    #[must_use]
    pub(crate) fn diff_format(&self, diff: f32) -> String {
        format_ostream_f64(diff as f64)
    }

    /// C++ `timeFormat(time)` — `snprintf("%.3f")`.
    #[must_use]
    pub(crate) fn time_format(&self, time: f32) -> String {
        format!("{time:.3}")
    }

    /// C++ `formatTime(x)` (HGGraphics.cpp file-local) —
    /// `ssvu::toStr(std::floor(x * 1000) / 1000.f)`: floor the *product*,
    /// then divide (double / the float literal 1000.f promoted to double).
    #[must_use]
    pub(crate) fn format_time(&self, x: f64) -> String {
        format_ostream_f64((x * 1000.0).floor() / (1000.0_f32 as f64))
    }

    /// C++ `imguiLuaConsoleHasInput()` (`HexagonGame.cpp:270-274`):
    /// `ilcShowConsole && (Imgui::wantCaptureKeyboard() ||
    /// Imgui::wantCaptureMouse())`. The port returns `ilc_show_console`
    /// alone: the `wantCaptureKeyboard/Mouse` conjunct is the step (b) egui
    /// piece (issue 15), and headless the C++ conjuncts are false anyway —
    /// when the console IS shown (debug + non-official + F1), suppressing
    /// the game input is the behavior that matters (the C++ gate at
    /// `HGUpdate.cpp:649` then returns before any input is read).
    pub(crate) fn imgui_lua_console_has_input(&self) -> bool {
        self.est().ilc_show_console
    }

    /// Draws one random bool the way C++ `executeRandomInputs` does:
    /// `std::uniform_int_distribution<int>{0, 1}(en)` on a u32 engine =
    /// the top bit of one draw (libstdc++ Lemire path with n = 2).
    pub(crate) fn draw_random_bool(rng: &mut Pcg32Fast) -> bool {
        // `uniform_int` on [0, 1] draws exactly one u32 (see oh-rng).
        uniform_int(rng, 0, 1) == 1
    }
}

/// C++ file-local `getReplayScore(status)` (HexagonGame.cpp:50, verbatim).
#[must_use]
pub(crate) fn get_replay_score(status: &HexagonGameStatus) -> f64 {
    if status.get_custom_score() != 0.0_f32 {
        status.get_custom_score() as f64
    } else {
        status.get_played_accumulated_frametime()
    }
}

/// C++ `initializeRng()` (HexagonGame.cpp:55, ported): a fresh
/// `random_number_generator` seeded by ONE 32-bit draw of a thread-local
/// `pcg32_fast`.
///
/// The C++ seeds that thread-local engine with
/// `seed_seq_from<std::random_device>` (4 random u32s); the port seeds
/// from process entropy (`entropy_seed`) — both are non-deterministic by
/// design (this only seeds *live* games; replays always re-seed from the
/// recorded `lastSeed`).
fn initialize_rng() -> RandomNumberGenerator {
    thread_local! {
        static SEED_RNG: std::cell::Cell<Pcg32Fast> =
            std::cell::Cell::new(Pcg32Fast::from_seed(entropy_seed()));
    }
    let seed = SEED_RNG.with(|c| c.get().next_u32() as u64);
    // C++: `return random_number_generator{seed_rng()};` (u32 in a u64,
    // high 32 bits zero).
    RandomNumberGenerator::new(seed)
}

// ---------------------------------------------------------------------------
// `GameApi` — the engine IS the backend the 007 lambdas call through
// (mapping/lua-api.md §1; the C++ lambdas captured `this`).
// ---------------------------------------------------------------------------

impl<S: SharedStore> crate::GameApi for HexagonGame<S> {
    // -- Config -----------------------------------------------------------
    fn get_width(&self) -> u32 {
        self.config().get_width()
    }

    fn get_height(&self) -> u32 {
        self.config().get_height()
    }

    fn get_official(&self) -> bool {
        self.config().get_official()
    }

    fn get_debug(&self) -> bool {
        self.config().get_debug()
    }

    // -- input (C++ `window != nullptr && window->getInputState()[...]`) --
    fn is_key_pressed(&self, _key_code: i32) -> bool {
        // C++ `window->getInputState()[...]` — no window in this port.
        false
    }

    fn is_mouse_button_pressed(&self, _button_code: i32) -> bool {
        false
    }

    // -- logging ----------------------------------------------------------
    fn log(&self, channel: &str, message: &str) {
        self.log(channel, message);
    }

    // -- flash ------------------------------------------------------------
    fn init_flash_effect(&self, r: i32, g: i32, b: i32) {
        self.init_flash_effect(r, g, b);
    }

    // -- player -----------------------------------------------------------
    fn get_player_angle(&self) -> f32 {
        HexagonGame::get_player_angle(self)
    }

    fn set_player_angle(&self, angle: f32) {
        HexagonGame::set_player_angle(self, angle)
    }

    fn update_player_position(&self, radius: f32) {
        HexagonGame::update_player_position(self, radius)
    }

    // -- difficulty / swap --------------------------------------------------
    fn increment_difficulty(&self) {
        HexagonGame::increment_difficulty(self);
    }

    fn perform_player_swap(&self, play_sound: bool) {
        HexagonGame::perform_player_swap(self, play_sound);
    }

    // -- audio --------------------------------------------------------------
    fn stop_level_music(&self) {
        HexagonGame::stop_level_music(self);
    }

    fn play_level_music(&self) {
        HexagonGame::play_level_music(self);
    }

    fn play_level_music_at_time(&self, seconds: f32) {
        HexagonGame::play_level_music_at_time(self, seconds);
    }

    fn play_sound_override(&self, sound_id: &str) {
        HexagonGame::play_sound_override(self, sound_id);
    }

    fn play_pack_sound_override(&self, pack_id: &str, file_name: &str) {
        HexagonGame::play_pack_sound_override(self, pack_id, file_name);
    }

    fn refresh_music_pitch(&self) {
        HexagonGame::refresh_music_pitch(self);
    }

    // -- messages ----------------------------------------------------------
    fn add_message(&self, message: &str, duration: f64, sound_toggle: bool) {
        HexagonGame::add_message(self, message, duration, sound_toggle);
    }

    fn clear_messages(&self) {
        HexagonGame::clear_messages(self);
    }

    // -- death -------------------------------------------------------------
    fn death(&self, force: bool) {
        HexagonGame::death(self, force);
    }

    // -- walls -------------------------------------------------------------
    fn create_wall(
        &self,
        side: i32,
        thickness: f32,
        speed: SpeedData,
        curve: SpeedData,
        hue_mod: f32,
    ) {
        HexagonGame::create_wall(self, side, thickness, &speed, &curve, hue_mod);
    }

    // -- pulse -------------------------------------------------------------
    fn refresh_pulse(&self) {
        HexagonGame::refresh_pulse(self);
    }

    fn refresh_beat_pulse(&self) {
        HexagonGame::refresh_beat_pulse(self);
    }

    // -- background camera --------------------------------------------------
    fn background_camera_rotation(&self) -> Option<f32> {
        self.est()
            .background_camera
            .as_ref()
            .map(|c| c.get_rotation())
    }

    fn set_background_camera_rotation(&self, value: f32) {
        if let Some(c) = self.est().background_camera.as_mut() {
            c.set_rotation(value);
        }
    }

    // -- replay / steam ------------------------------------------------------
    fn in_replay(&self) -> bool {
        HexagonGame::in_replay(self)
    }

    fn unlock_achievement(&self, _achievement_id: &str) {
        // C++ `steamManager->unlock_achievement(...)` — steamManager is
        // always null in this port.
    }

    // -- error exit ----------------------------------------------------------
    fn go_to_menu(&self, send_scores: bool, error: bool) {
        HexagonGame::go_to_menu(self, send_scores, error);
    }
}
