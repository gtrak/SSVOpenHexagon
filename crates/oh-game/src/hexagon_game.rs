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
use oh_discord::Discord;
use oh_math::hg::{concat::format_ostream_f64, string::to_uppercase};
use oh_math::ssvs::Vec2;
use oh_replay::{compress_replay_file, InputBits, ReplayData, ReplayFile};
use oh_rng::{uniform_int, Pcg32Fast, RandomNumberGenerator, Seed};

use crate::camera::Camera;
use crate::lua_state::{ReentrantMutex, ReentrantMutexGuard, SharedState, SharedStore};
use crate::renderer::Renderer;
use crate::seams::{Audio, CosmeticRng, Joystick, Profiles, ReplayStore, ScriptHost, SteamSink};
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
    // Consumed in the windowed `draw()` (render lane, via the issue 15
    // step (a) bridge); set by the ilc console command in this crate.
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
    // The port's stand-in for the C++ gate's `Imgui::wantCaptureKeyboard()
    // || Imgui::wantCaptureMouse()` conjunct (HexagonGame.cpp:272-273) —
    // the windowed egui layer (`oh-app`, issue 014b) rewrites it every
    // frame from the egui context's `egui_wants_keyboard_input() /
    // egui_wants_pointer_input()`. Default `true`: headless has no egui
    // context (the C++'s conjuncts would be `false`, but C++ headless
    // never shows the console — `window == nullptr` returns first,
    // HGUpdate.cpp:1290-1293 — so `ilc_show_console` is always false and
    // the conjunct is moot), and `true` keeps the landed 14a headless gate
    // byte-identical; a windowed frame without an egui pump yet (the first
    // shown frame, before its post-update pass) conservatively gates too.
    pub(crate) ilc_capture: bool,
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

    // C++ `timeUntilRichPresenceUpdate` (rich presence; the STEAM manager is
    // still null — issue 20; the Discord manager is the `discord` seam).
    pub(crate) time_until_rich_presence_update: f32,
    // C++ `discordHung` / `discordAttempt` (HexagonGame.hpp:89,91) — the
    // retry/hang state for the Discord `run_callbacks` (HexagonGame.cpp:
    // 495-506); `discordAttempt` starts at `1`, `discordHung` at `false`.
    pub(crate) discord_hung: bool,
    pub(crate) discord_attempt: i8,

    // 030b fingerprint (OFF BY DEFAULT, additive-only; see
    // .agents/plans/001-rust-port/parity/cpp-fingerprint-definition.md):
    // `fingerprint_file == None` = disabled (the default) — `update` takes
    // no extra path, so default behaviour AND output are unchanged.
    pub(crate) fingerprint_file: Option<std::fs::File>,
    pub(crate) fingerprint_frame: u64, // 0-based, reset by `new_game`
    pub(crate) fingerprint_last_draws: u64, // reset by `new_game`
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
    // C++ `hg::Steam::steam_manager* steamManager` (the C++'s FIRST
    // constructor argument, main.cpp:415) — the engine's Steam surface
    // (issue 020; [`SteamSink`] / [`NoneSteamSink`]).
    pub(crate) steam_manager: Arc<dyn SteamSink + 'static>,
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
    // C++ `Discord::discord_manager* discordManager` (HexagonGame.hpp:88) —
    // nullable: `main.cpp:264-269` emplaces it only when NOT headless, and
    // `main.cpp:416` passes `has_value() ? &*discordManager : nullptr`.
    pub(crate) discord: Option<Arc<dyn Discord + 'static>>,
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
    // C++ `std::function<void(const bool)> fnGoToMenu` (HexagonGame.hpp:445)
    // — the app-level "back to menu" callback, assigned by `mainClient`
    // AFTER construction (main.cpp:456-462) and called at the tail of
    // `goToMenu` (HexagonGame.cpp:1277-1280).
    pub(crate) fn_go_to_menu: std::sync::RwLock<Option<GoToMenuCallback>>,
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

/// C++ `std::function<void(const bool)>` capture (type alias so the
/// `Inner` field stays under clippy's type-complexity budget). `Send +
/// Sync` (the engine is `Arc<dyn GameApi>`, so its members must be):
/// the windowed `fnGoToMenu` closure (main.cpp:456-462) therefore only
/// sets a thread-atomic signal (an `Arc<AtomicU8>` — `Send + Sync`);
/// the `Rc`-based menu calls (`returnToLevelSelection` / `init`) +
/// `setGameState` are performed by the composition's game state adapter,
/// which consumes the signal at the start of the next tick (see
/// `oh_app::windowed`).
pub type GoToMenuCallback = Arc<dyn Fn(bool) + Send + Sync>;

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
    /// mGameWindow, hexagonClient)` — the discord/client managers have no
    /// Rust backing yet (their call sites stay null-gated and therefore
    /// inert; issues 17/21); `steam_manager` is the C++'s first argument
    /// (issue 020: the `oh_steam::SteamManager` the composition hands the
    /// engine, or the null-manager equivalent [`crate::seams::
    /// NoneSteamSink`]).
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
        steam_manager: Arc<dyn SteamSink + 'static>,
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
        discord: Option<Arc<dyn Discord + 'static>>,
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
            ilc_capture: true,
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
            // C++ `discordAttempt{1}` / `discordHung{false}` (HexagonGame.hpp:91,89).
            discord_hung: false,
            discord_attempt: 1,
            // 030b fingerprint: OFF by default (the C++ `fingerprintFile` is
            // nullptr until `setFingerprintPath`).
            fingerprint_file: None,
            fingerprint_frame: 0,
            fingerprint_last_draws: 0,
        };

        // C++ `HexagonGame` ctor's "Keyboard binds" section
        // (HexagonGame.cpp:343-345) — `Config::keyboardBindsSanityCheck()`
        // (Config.cpp:1392-1406): `resizeTrigger` REPAIR on the first 12
        // triggers (RotateCCW..Down) — agglomerate (drop unbound combos),
        // trim to `MAX_BINDS`, pad with cleared combos. A hand-edited /
        // legacy `config.json` with a malformed trigger (too many /
        // too few / scattered combos) is normalized before any bind is
        // read; the other 4 triggers (NextPack / PreviousPack /
        // LuaConsole / Pause) are NOT repaired (the C++'s exact scope).
        config
            .write()
            .expect("config poisoned")
            .keyboard_binds_sanity_check();

        // C++ `HexagonGame` ctor's "Joystick binds" section (HexagonGame.cpp:
        // 445-448) — `Config::loadAllJoystickBinds()`: load the 12 `j_*`
        // config values into the shared `Joystick` machine (`Config.cpp:
        // 1609-1615` → `Joystick::setJoystickBind` ×12). The desktop
        // composition's `RealJoystick` poller then feeds this machine the
        // controller's snapshot each frame (29j1); headless /
        // no-controller runs stay inert (every query false — the C++'
        // `isConnected(0) == false` state).
        config
            .read()
            .expect("config poisoned")
            .load_all_joystick_binds();

        Self(Arc::new(Inner {
            steam_manager,
            state: std::sync::OnceLock::new(),
            assets,
            config,
            audio,
            profiles,
            discord,
            replay_store,
            script_host,
            joystick,
            cosmetic_rng,
            window,
            renderer,
            on_death_replay_created: std::sync::RwLock::new(None),
            fn_go_to_menu: std::sync::RwLock::new(None),
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
        // 030b fingerprint (additive-only): the frame counter is per-game
        // (see .agents/plans/001-rust-port/parity/
        // cpp-fingerprint-definition.md). `st.rng` is replaced below with a
        // fresh wrapper (draws == 0), so the draw baseline resets with it.
        {
            let mut e = self.est();
            e.fingerprint_frame = 0;
            e.fingerprint_last_draws = 0;
        }

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

        let level_data = self
            .0
            .assets
            .get_level_data(m_id)
            .expect("SSVOH_ASSERT(assets.isValidLevelId(mId))");
        self.set_level_data_inner(&level_data, m_first_play, &mut g, &mut e);

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
                    player_name: rf_name.into(),
                    seed: e.last_seed,
                    data: e.last_replay_data.clone(),
                    pack_id: m_pack_id.into(),
                    level_id: m_id.into(),
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

        // C++ `newGame`'s "Prepare text for input hints on restart/
        // replay" (HexagonGame.cpp:758-821) — VERBATIM, window-gated
        // (`window != nullptr`): `getKeyboardBindNames(Tid::Restart/Replay)`
        // (:763-764), trimmed to the FIRST key (the `find(',')` +
        // `erase(commaPos)` half — "to avoid extremely long messages";
        // a comma at position 0 is NOT erased, the C++ `commaPos > 0`
        // gate), then `getJoystickBindName(Jid::Restart/Replay)` (:781/
        // :801) — the vendor-aware button name (the C++ reads
        // `isConnected(0) ? getIdentification(0).vendorId : 0` INSIDE the
        // call, Config.cpp:1577-1581; the port's seam takes the `vendor_id`
        // as a parameter — the `Joystick` seam's `vendor_id()`, 29j1,
        // `NoJoystick` = 0 = the C++'s not-connected read). Headless
        // leaves both strings empty, exactly like the C++.
        if self.0.window {
            let cfg = self.config();
            let mut restart_input = cfg.get_keyboard_bind_names(oh_config::Tid::Restart);
            let mut replay_input = cfg.get_keyboard_bind_names(oh_config::Tid::Replay);

            // C++ `status.restartInput.find(',')` — `std::string::find`
            // returns `npos` (= -1 as `int`) when absent; the port's
            // `None` arm is that same "not > 0" case.
            if let Some(comma_pos) = restart_input.find(',') {
                if comma_pos > 0 {
                    restart_input.truncate(comma_pos);
                }
            }
            if let Some(comma_pos) = replay_input.find(',') {
                if comma_pos > 0 {
                    replay_input.truncate(comma_pos);
                }
            }

            let joystick_button =
                cfg.get_joystick_bind_name(oh_config::Jid::Restart, self.0.joystick.vendor_id());
            if !restart_input.is_empty() {
                if !joystick_button.is_empty() {
                    restart_input.push_str(" OR JOYSTICK ");
                    restart_input.push_str(&joystick_button);
                }
                g.status.restart_input = format!("PRESS {restart_input} TO RESTART\n");
            } else if !joystick_button.is_empty() {
                g.status.restart_input = format!("PRESS JOYSTICK {joystick_button} TO RESTART\n");
            } else {
                g.status.restart_input = "NO RESTART BUTTON SET\n".to_string();
            }

            let joystick_button =
                cfg.get_joystick_bind_name(oh_config::Jid::Replay, self.0.joystick.vendor_id());
            if !replay_input.is_empty() {
                if !joystick_button.is_empty() {
                    replay_input.push_str(" OR JOYSTICK ");
                    replay_input.push_str(&joystick_button);
                }
                g.status.replay_input = format!("PRESS {replay_input} TO REPLAY\n");
            } else if !joystick_button.is_empty() {
                g.status.replay_input = format!("PRESS JOYSTICK {joystick_button} TO REPLAY\n");
            } else {
                g.status.replay_input = "NO REPLAY BUTTON SET\n".to_string();
            }
        }
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
            // C++ (HGUpdate.cpp:512-524): `nameStr = nameFormat(name)`,
            // `packStr = nameFormat(getPackName())`, `diffStr =
            // diffFormat(difficultyMult)`, and — when `discordManager !=
            // nullptr` — `set_rich_presence_in_game(nameStr + " [x" + diffStr
            // + "]", packStr)` (dead defaults to false).
            let name_str = self.name_format(&g.level_name);
            let pack_str = self.name_format(&self.get_pack_name());
            let diff_str = self.diff_format(g.difficulty_mult);
            if let Some(discord) = self.0.discord.as_ref() {
                discord.set_rich_presence_in_game(
                    &format!("{name_str} [x{diff_str}]"),
                    &pack_str,
                    false,
                );
            }
            // C++ `hexagonClient->trySendStartedGame` — the null client
            // (issue 17; `HexagonClient::State::LoggedIn_Ready` gate).
        } else {
            // C++ (HGUpdate.cpp:536-541): the replay branch —
            // `discordManager->set_rich_presence_on_replay()`.
            if let Some(discord) = self.0.discord.as_ref() {
                discord.set_rich_presence_on_replay();
            }
        }

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
            // C++ HexagonGame.cpp:980 — `ssvu::lo("Replay") <<
            // "Attempting to send and save replay...\n"`.
            self.log("Replay", "Attempting to send and save replay...\n");
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

    /// C++ `death_updateRichPresence()` (HexagonGame.cpp:847-886): the
    /// `window == nullptr` gate (849-852) and the `inReplay()` gate (854-857),
    /// then — when `discordManager != nullptr` —
    /// `set_rich_presence_in_game(nameStr + " [x" + diffStr + "]",
    /// "Survived " + timeStr + "s", true)` (878-886).
    fn death_update_rich_presence(&self) {
        // C++ `if(window == nullptr) return;` (HexagonGame.cpp:849-852).
        if !self.0.window {
            return;
        }
        // C++ `if(inReplay()) return;` (HexagonGame.cpp:854-857).
        if self.in_replay() {
            return;
        }
        if let Some(discord) = self.0.discord.as_ref() {
            let g = self.st();
            let name_str = self.name_format(&g.level_name);
            let diff_str = self.diff_format(g.difficulty_mult);
            let time_str = self.time_format(g.status.get_time_seconds() as f32);
            discord.set_rich_presence_in_game(
                &format!("{name_str} [x{diff_str}]"),
                &format!("Survived {time_str}s"),
                true,
            );
        }
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
            player_name: rf_name.into(),
            seed: e.last_seed,
            data: e.last_replay_data.clone(),
            pack_id: e.pack_id.clone().into(),
            level_id: e.level_id.clone().into(),
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
        let _level_validator =
            get_level_validator(&rf.level_id.to_string_lossy(), rf.difficulty_mult);
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

    /// C++ `hg.fnGoToMenu = ...` (main.cpp:456-462) — the app-level
    /// "back to menu" callback, invoked at the tail of [`Self::go_to_menu`]
    /// (HexagonGame.cpp:1277-1280: `if(fnGoToMenu) fnGoToMenu(mError);`).
    /// The C++ assigns it after construction, so the port sets it the same
    /// way (the member is unset — `std::function` default-empty — until
    /// then).
    pub fn set_fn_go_to_menu(&self, cb: Arc<dyn Fn(bool) + Send + Sync>) {
        *self
            .0
            .fn_go_to_menu
            .write()
            .expect("fn_go_to_menu poisoned") = Some(cb);
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

    /// 030b fingerprint — the C++ twin of `HexagonGame::setFingerprintPath`
    /// (OFF BY DEFAULT, additive-only; the definition:
    /// `.agents/plans/001-rust-port/parity/cpp-fingerprint-definition.md`).
    ///
    /// When `Some(path)`, the end-of-`update` hook writes one line per
    /// frame, `<frame> <hash> <draws>\n` (hash = FNV-1a64 over the exact
    /// v1 field order; floats as raw IEEE bits; the game-RNG state as its
    /// canonical MCG label). `None` closes any open file: the hook takes no
    /// extra path and default behaviour AND output are unchanged.
    ///
    /// Returns `false` (and leaves the file closed) when the path cannot be
    /// opened — mirroring the C++ `fopen` failure (null file, hook off).
    pub fn set_fingerprint_path(&self, path: Option<&std::path::Path>) -> bool {
        let mut e = self.est();
        e.fingerprint_file.take(); // drops (closes) the previous file (fclose)

        e.fingerprint_frame = 0;
        e.fingerprint_last_draws = 0;

        match path.and_then(|p| std::fs::File::create(p).ok()) {
            Some(file) => {
                e.fingerprint_file = Some(file);
                true
            }
            None => false,
        }
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
        debug_assert!(self
            .0
            .assets
            .is_valid_pack_id(&m_replay_file.pack_id.to_string_lossy()));
        debug_assert!(self
            .0
            .assets
            .is_valid_level_id(&m_replay_file.level_id.to_string_lossy()));

        self.set_last_replay(m_replay_file.clone());

        self.new_game(
            &m_replay_file.pack_id.to_string_lossy(),
            &m_replay_file.level_id.to_string_lossy(),
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

        // C++ `if(fnGoToMenu) fnGoToMenu(mError);` (HexagonGame.cpp:1277-1280)
        // — the app-level callback (main.cpp:456-462: `returnToLevelSelection`
        // + `init(error)` + `window->setGameState(mg->getGame())`). Unset in
        // the headless compositions (the C++'s default-empty `std::function`
        // guard). The `Arc` is cloned and the read-lock guard drops BEFORE
        // the call (the callback never re-locks this engine).
        if let Some(cb) = self
            .0
            .fn_go_to_menu
            .read()
            .expect("fn_go_to_menu poisoned")
            .clone()
        {
            cb(m_error);
        }
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

    /// C++ `getColorCap()` (HexagonGame.cpp:1475, verbatim).
    #[must_use]
    pub fn get_color_cap(&self) -> oh_math::hg::color::Color {
        let g = self.st();
        if self.config().get_black_and_white() {
            oh_math::hg::color::Color::BLACK
        } else {
            // C++ `styleData.getCapColorResult()` (never fails in C++);
            // the port's `None` (empty-colours `ByIndex`) falls back to
            // Black — the C++ `getColor(index)` there asserts in debug.
            g.style_data
                .get_cap_color_result()
                .unwrap_or(oh_math::hg::color::Color::BLACK)
        }
    }

    /// C++ `getColorWall()` (HexagonGame.cpp:1485, verbatim).
    #[must_use]
    pub fn get_color_wall(&self) -> oh_math::hg::color::Color {
        let g = self.st();
        if self.config().get_black_and_white() {
            oh_math::hg::color::Color::new(255, 255, 255, g.style_data.get_wall_color().a)
        } else {
            *g.style_data.get_wall_color()
        }
    }

    // ------------------------------------------------------------------
    // Draw-bridge accessors (issue 15 step (a), D-020)
    //
    // The smallest `pub` reader set the composition crate (`oh-app`)
    // needs to build `FrameInput`/`DrawState` from the engine — mirroring
    // the C++ member visibility (the C++ `draw()` reads these members
    // directly, `HGGraphics.cpp:52-316`). Read-only: none of these may
    // change simulation state (the D-019 determinism harness must see no
    // difference).
    // ------------------------------------------------------------------

    /// The engine's `Config` (the C++ static `Config`; the composition
    /// shares its loaded instance). Borrow lives as long as the guard.
    pub fn config_ref(&self) -> std::sync::RwLockReadGuard<'_, Config> {
        self.0.config.read().expect("config poisoned")
    }

    /// C++ `backgroundCamera` (`std::optional<ssvs::Camera>`; `None` when
    /// the engine was constructed headless — the C++ `window == nullptr`
    /// state).
    #[must_use]
    pub fn background_camera(&self) -> Option<Camera> {
        self.est().background_camera
    }

    /// C++ `overlayCamera`.
    #[must_use]
    pub fn overlay_camera(&self) -> Option<Camera> {
        self.est().overlay_camera
    }

    /// C++ `preShakeCenters` (the camera centres captured before a shake
    /// starts, `HexagonGame::PreShakeCenters`).
    #[must_use]
    pub fn pre_shake_centers(&self) -> Option<PreShakeCenters> {
        self.est().pre_shake_centers
    }

    /// C++ `mustTakeScreenshot` (set by the ilc `screenshot` command;
    /// consumed by the windowed `draw()`).
    #[must_use]
    pub fn must_take_screenshot(&self) -> bool {
        self.est().must_take_screenshot
    }

    /// The setter half of the C++ `mustTakeScreenshot = false` clear the
    /// draw bridge performs after consuming the flag (the engine field is
    /// behind the reentrant lock, so the bridge reads/writes it as a
    /// value).
    pub fn set_must_take_screenshot(&self, value: bool) {
        self.est().must_take_screenshot = value;
    }

    // -- keyboard-input trigger actions (issue 010b) ------------------------
    // The C++ bind bodies registered in the `HexagonGame` constructor
    // (`HexagonGame.cpp:349-460`): the windowed input manager (oh-app,
    // issue 010b) invokes these from the `Config::getTrigger(Tid::...)`
    // callbacks. Headless runs never invoke them (no input manager exists
    // there — the `updateInput` keyboard comment at `HGUpdate.cpp` / this
    // crate's `update_input` is the only reader of `input_impl_*`).

    /// C++ `add2StateInput(game, getTrigger(Tid::RotateCCW), inputImplCCW)`
    /// (`HexagonGame.cpp:358` + SSVStart `Utils/Input.hpp:209-219`).
    pub fn set_input_impl_ccw(&self, value: bool) {
        self.est().input_impl_ccw = value;
    }

    /// C++ `add2StateInput(..., Tid::RotateCW, inputImplCW)`.
    pub fn set_input_impl_cw(&self, value: bool) {
        self.est().input_impl_cw = value;
    }

    /// C++ `add2StateInput(..., Tid::Focus, inputFocused)`.
    pub fn set_input_focused(&self, value: bool) {
        self.est().input_focused = value;
    }

    /// C++ `add2StateInput(..., Tid::Swap, inputSwap)`.
    pub fn set_input_swap(&self, value: bool) {
        self.est().input_swap = value;
    }

    /// C++ `inputImplCCW` (test access — the replay-leak check: live input
    /// sets this even during a replay; `update_input` must not read it
    /// then, `must_replay_input` branch).
    #[must_use]
    pub fn get_input_impl_ccw(&self) -> bool {
        self.est().input_impl_ccw
    }

    /// C++ `debugPause` (test access — the `Tid::Pause` body's flag).
    #[must_use]
    pub fn debug_pause(&self) -> bool {
        self.est().debug_pause
    }

    /// C++ `imguiLuaConsoleHasInput()` (`HexagonGame.cpp:270-274`) — the
    /// public form of the `notInConsole` gate
    /// (`HexagonGame.cpp:367-375`).
    #[must_use]
    pub fn lua_console_has_input(&self) -> bool {
        self.imgui_lua_console_has_input()
    }

    /// C++ `Tid::ForceRestart` body (`HexagonGame.cpp:385-387`).
    pub fn trigger_force_restart(&self) {
        self.st().status.must_state_change = StateChange::MustRestart;
    }

    /// C++ `Tid::Restart` body (`HexagonGame.cpp:389-397`).
    pub fn trigger_restart(&self) {
        let (ignore, has_died) = {
            let g = self.st();
            let e = self.est();
            (e.death_input_ignore, g.status.has_died)
        };
        if ignore <= 0.0_f32 && has_died {
            self.st().status.must_state_change = StateChange::MustRestart;
        }
    }

    /// C++ `Tid::Replay` body (`HexagonGame.cpp:399-407`).
    pub fn trigger_replay(&self) {
        let (ignore, has_died) = {
            let g = self.st();
            let e = self.est();
            (e.death_input_ignore, g.status.has_died)
        };
        if (ignore <= 0.0_f32 && has_died) || self.in_replay() {
            self.st().status.must_state_change = StateChange::MustReplay;
        }
    }

    /// C++ `Tid::Screenshot` body (`HexagonGame.cpp:409-410`):
    /// `mustTakeScreenshot = true` (consumed by the windowed `draw()`).
    pub fn trigger_screenshot(&self) {
        self.est().must_take_screenshot = true;
    }

    /// C++ hardcoded-Escape / `Tid::Exit` bind body
    /// (`HexagonGame.cpp:379-383`): `goToMenu()` — the default-argument
    /// call `goToMenu(true /*mSendScores*/, false /*mError*/)`
    /// (HexagonGame.hpp:417).
    pub fn trigger_go_to_menu(&self) {
        self.go_to_menu(true, false);
    }

    /// C++ `Tid::LuaConsole` body (`HexagonGame.cpp:412-419`): the
    /// debug-gated `ilcShowConsoleNext = true`.
    pub fn trigger_lua_console(&self) {
        self.set_ilc_show_console_next();
    }

    /// C++ `Tid::Pause` body (`HexagonGame.cpp:421-441`): the debug-gated
    /// `debugPause` toggle + the `shouldPlayMusic` music pause/resume
    /// (`shouldPlayMusic`, `HexagonGame.cpp:515-518`).
    pub fn trigger_pause(&self) {
        if !self.config().get_debug() {
            return;
        }
        let should_play_music =
            self.0.window && self.0.audio.is_some() && !self.config().get_no_music();
        let audio = self.0.audio.clone();
        let mut e = self.est();
        e.debug_pause = !e.debug_pause;
        if e.debug_pause {
            if should_play_music {
                audio
                    .as_ref()
                    .expect("shouldPlayMusic checked")
                    .pause_music();
            }
        } else if !self.st().status.has_died && should_play_music {
            audio
                .as_ref()
                .expect("shouldPlayMusic checked")
                .resume_music();
        }
    }

    /// C++ `pbTextGrowth` (the PB-text size sawtooth driven by
    /// `updateText`).
    #[must_use]
    pub fn pb_text_growth(&self) -> f32 {
        self.est().pb_text_growth
    }

    /// C++ `styleData` (clone; the C++ reads the member in place).
    #[must_use]
    pub fn get_style_data(&self) -> StyleData {
        self.st().style_data.clone()
    }

    /// C++ `ssvu::getRndR(min, max)` — the NON-game (cosmetic) RNG the
    /// draw-time camera-shake jitter uses (`HGGraphics.cpp:88-96`).
    /// Draw-only: never advances the replay-relevant `status.rng`.
    #[must_use]
    pub fn get_rnd_r(&self, min: f32, max: f32) -> f32 {
        self.0.cosmetic_rng.get_rnd_r(min, max)
    }

    /// C++ `runLuaFunctionIfExists<int, float>("onRenderStage", rs, ft)`
    /// (`HGGraphics.cpp:74-77`, inside the `getRenderStates` lambda) —
    /// dispatches the level's `onRenderStage(rs, ft)` Lua function if
    /// defined. `Err` carries the C++ exception `what()`; the caller
    /// performs the Lippincott behavior (`HGScripting.cpp:1155`): print
    /// `"[runLuaFunctionIfExists] Runtime error on ..."` and
    /// `goToMenu(false, true)` unless debug.
    pub fn run_on_render_stage(&self, stage: i32, frametime: f32) -> Result<(), String> {
        self.0.script_host.run_on_render_stage(stage, frametime)
    }

    /// The C++ `draw()` scene-batch fill orchestration
    /// (`HGGraphics.cpp:127-146`), verbatim order, returning the four
    /// batches `draw_frame` consumes pre-filled (`wallQuads`,
    /// `pivotQuads`, `playerTris`, `capTris`):
    ///
    /// 1. `wallQuads.reserve_more_quad(walls.size() + cwManager.count())`
    ///    (`HGGraphics.cpp:133`);
    /// 2. `for(CWall& w : walls) w.draw(getColorWall(), wallQuads)`
    ///    (`HGGraphics.cpp:135-138`);
    /// 3. `cwManager.draw(wallQuads)` (`HGGraphics.cpp:140`);
    /// 4. `if(status.started) player.draw(getSides(), getColorMain(),
    ///    getColorPlayer(), pivotQuads, capTris, playerTris, getColorCap(),
    ///    Config::getAngleTiltIntensity(),
    ///    Config::getShowSwapBlinkingEffect())` (`HGGraphics.cpp:142-146`).
    ///
    /// (The C++ clears its member batches first, `HGGraphics.cpp:127-132`;
    /// the port returns fresh batches — the caller's per-frame scratch is
    /// refilled every call. The `status.started` gate is read under the
    /// shared-store lock, in the C++ position between the cw fill and the
    /// player fill. The 3D copies of these batches are built INSIDE
    /// `oh_render::graphics::draw_frame` — do not duplicate them here.)
    #[must_use]
    pub fn draw_scene_batches(
        &self,
    ) -> (
        FastVertexVectorTris,
        FastVertexVectorTris,
        FastVertexVectorTris,
        FastVertexVectorTris,
    ) {
        // C++ `getColorWall()` / `getColorMain()` / `getColorPlayer()` /
        // `getColorCap()` (HexagonGame.cpp:1423-1493).
        let color_wall = self.get_color_wall();
        let color_main = self.get_color_main();
        let color_player = self.get_color_player();
        let color_cap = self.get_color_cap();
        // C++ `Config::getAngleTiltIntensity()` / `getShowSwapBlinkingEffect()`.
        let (angle_tilt, swap_blinking) = {
            let c = self.config();
            (
                c.get_angle_tilt_intensity(),
                c.get_show_swap_blinking_effect(),
            )
        };
        // C++ `getSides()`.
        let sides = self.get_sides();

        let mut wall_quads = FastVertexVectorTris::new();
        let mut pivot_quads = FastVertexVectorTris::new();
        let mut cap_tris = FastVertexVectorTris::new();
        let mut player_tris = FastVertexVectorTris::new();

        // C++ `HGGraphics.cpp:133-140`.
        let must_draw_player = {
            let g = self.st();
            wall_quads.reserve_more_quad(g.walls.len() + g.cw_manager.count());
            for w in &g.walls {
                w.draw(color_wall, &mut wall_quads);
            }
            g.cw_manager.draw(&mut wall_quads);
            // C++ `status.started` (HGGraphics.cpp:142).
            g.status.started
        };

        if must_draw_player {
            // C++ `HGGraphics.cpp:144-145` — the 4th argument is the
            // PIVOT batch (C++ `player.draw(..., pivotQuads, capTris,
            // playerTris, ...)`).
            let e = self.est();
            e.player.draw(
                sides,
                color_main,
                color_player,
                &mut pivot_quads,
                &mut cap_tris,
                &mut player_tris,
                color_cap,
                angle_tilt,
                swap_blinking,
            );
        }

        (wall_quads, pivot_quads, player_tris, cap_tris)
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
    /// Imgui::wantCaptureMouse())`. The `wantCapture*` conjunct is the
    /// windowed egui piece (issue 014b, D-022): `oh-app` rewrites
    /// `ilc_capture` every frame from the egui context's focus state
    /// (after the console's own post-update pass — the C++'s one-frame
    /// lag: `ImGui::SFML::Update` computes `WantCapture*` DURING this
    /// frame's post-update, so this frame's `update_input` gate reads the
    /// PREVIOUS frame's values). Headless there is no egui context and the
    /// conjunct defaults to `true`, keeping the landed 14a gate
    /// byte-identical (C++ headless never shows the console, so the
    /// conjunct is moot there — see the field's docs).
    pub(crate) fn imgui_lua_console_has_input(&self) -> bool {
        let e = self.est();
        e.ilc_show_console && e.ilc_capture
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
    /// C++ `u_isKeyPressed` (HGScripting.cpp:89-98): the window's LIVE
    /// key state, read through the `Renderer` seam (the C++'s `window`
    /// stand-in — the windowed composition's renderer publishes the
    /// window's `InputState` per input event; the raw `int` code is the
    /// C++'s `ssvs::KKey(mKey)` subscript, bounds-checked in
    /// `InputState::is_key_pressed_code`). `false` when there is no
    /// renderer — the C++'s `window == nullptr` headless half (the port's
    /// defined headless behavior: the read is the C++'s `false` from
    /// `window != nullptr && ...`, deterministic — the C++'s window is
    /// always present in the port's windowed composition, so the only
    /// `false`-by-absence case is a renderer-less composition, i.e.
    /// headless).
    fn is_key_pressed(&self, key_code: i32) -> bool {
        self.0
            .renderer
            .as_ref()
            .map(|r| r.is_key_pressed(key_code))
            .unwrap_or(false)
    }

    /// C++ `u_isMouseButtonPressed` (HGScripting.cpp:135-141) — the same
    /// seam's button half (the `false`-when-headless note is the
    /// `is_key_pressed` one's).
    fn is_mouse_button_pressed(&self, button_code: i32) -> bool {
        self.0
            .renderer
            .as_ref()
            .map(|r| r.is_mouse_button_pressed(button_code))
            .unwrap_or(false)
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

    fn unlock_achievement(&self, achievement_id: &str) {
        // C++ `steamManager->unlock_achievement(id)` (HGScripting.cpp:850).
        self.0.steam_manager.unlock_achievement(achievement_id);
    }

    // -- error exit ----------------------------------------------------------
    fn go_to_menu(&self, send_scores: bool, error: bool) {
        HexagonGame::go_to_menu(self, send_scores, error);
    }
}
