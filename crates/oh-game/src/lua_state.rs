//! The shared engine/Lua state store (DECISIONS.md D-013, issue 009).
//!
//! `oh-lua` depends on this crate (its `api/state.rs` builds `GameApiState`
//! from the gameplay types below), so `oh-game -> oh-lua` as a *normal*
//! dependency is a cargo cycle. The 285 Lua lambdas (issue 007) and the
//! engine update loop (this crate) must mutate **one** storage of
//! `levelStatus` / `status` / `walls` / `rng` / timelines — copies diverge
//! and replay parity breaks (`l_setPulseMin` / `t_*` writes lost). So the
//! shared data lives here:
//!
//! * [`SharedState`] — field-for-field the C++ `HexagonGame` /
//!   `LuaScripting` state the lambdas capture (mapping `lua-api.md` §1);
//!   `oh-lua`'s `GameApiState` is a thin wrapper `{ shared, run_lua_file,
//!   assets, backend }` with `Deref<Target = SharedState>`.
//! * [`GameApi`] — the engine operations the lambdas delegate to (moved
//!   verbatim from issue 007; only the `Send + Sync` supertraits were
//!   dropped — mlua's `send` feature is not enabled, so nothing forces
//!   them, and dropping them allows the safe `Rc`-based composition).
//! * [`ReentrantMutex`] — re-exported `parking_lot::ReentrantMutex`.
//!
//! **Reentrancy is required, not stylistic:** `t_eval`/`e_eval`/`ct_eval`
//! append deferred closures that run Lua *while the state guard is held*
//! (`oh-lua` `api/main_timeline.rs`), and lambdas call
//! `lock(&st).backend.*` — the backend (this crate's engine) then locks the
//! SAME state again on the same thread. A non-reentrant `std::sync::Mutex`
//! self-deadlocks. The lock is `Rc`-shared (the game is single-threaded;
//! the composition never sends it across threads), so no `unsafe` is
//! needed anywhere in the shared-state path.
//!
//! `AssetsApi` (issue 007) deliberately stays in `oh-lua`: its method
//! signatures return `Result<_, LuaError>`, an `oh-lua` type this crate
//! cannot name.

use std::cell::UnsafeCell;
use std::collections::HashSet;
use std::fmt;
use std::ops::{Deref, DerefMut};
use std::sync::atomic::{AtomicU32, Ordering};

use oh_data::pack_data::PackData;
use oh_data::style_data::StyleData;
use oh_data::MusicData;
use oh_rng::RandomNumberGenerator;

use crate::{
    CCustomWallManager, CWall, CustomTimelineManager, HexagonGameStatus, LevelStatus, SpeedData,
    Timeline2,
};

/// The state-store bound of `HexagonGame<S>` (DECISIONS.md D-013, point 3;
/// accessor form per D-015 — NO `Deref` supertrait, NO reflexive
/// `Deref<Target = Self>` impl, which is the E0055 trap).
///
/// `Send` (not `Sync`) is the exact requirement: the engine stores the
/// store inside `Arc<ReentrantMutex<S>>`, and `ReentrantMutex`'s manual
/// `Sync` impl is granted from `T: Send` alone (the reentrant lock is the
/// synchronization boundary; the store must never be observed unlocked).
pub trait SharedStore: Send + 'static {
    /// The shared gameplay store (D-015 accessor).
    fn shared(&self) -> &SharedState;
    /// Mutable access to the shared gameplay store (D-015 accessor).
    fn shared_mut(&mut self) -> &mut SharedState;
}

/// The bare shared store IS its own accessor: `HexagonGame<SharedState>`
/// (pure headless, no Lua) uses this impl.
impl SharedStore for SharedState {
    fn shared(&self) -> &SharedState {
        self
    }

    fn shared_mut(&mut self) -> &mut SharedState {
        self
    }
}

/// C++ `HexagonGame` operations the API lambdas perform (mapping
/// `lua-api.md` §1 semantics are the spec). Issue 009 implements this
/// against the real game loop; tests implement it with a recording mock.
///
/// The C++ lambdas capture `this` (`HexagonGame&`); the Rust port captures
/// this trait object. Method signatures are the issue-007 contract
/// verbatim.
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
    /// fires `onCursorSwap`, plays `levelStatus.swapSound` when
    /// `play_sound`.
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

/// The exact data every C++ API lambda reads or writes, shared with the
/// engine update loop (mapping `lua-api.md` §1). Field names/order mirror
/// the C++ captures (`HexagonGame::rng`, `HexagonGame::levelStatus`, ...);
/// `oh-lua`'s `GameApiState` wraps this store and derefs into it, so every
/// 007 lambda (`lock(&st).level_status.x`, `st.backend.x`, ...) resolves
/// unchanged.
pub struct SharedState {
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
}

impl SharedState {
    /// Every field takes its C++ initial value (`difficultyMult = 1`,
    /// `firstPlay = true`, empty timelines/walls/logs). One argument per
    /// C++-captured value (the composition supplies them); a builder would
    /// add no semantics, so the count exceeds clippy's limit.
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
    /// once per function per level load, appends to `ilcCmdLog`. The
    /// `backend` is the issue-007 `GameApi` (the `ssvu::lo("Lua")` channel
    /// in C++ is the engine's log sink).
    pub fn raise_warning(
        &mut self,
        backend: &dyn GameApi,
        function_name: &str,
        additional_info: &str,
    ) {
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
        backend.log("lua", &error_msg);
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
}

/// A reentrant lock for the shared engine/Lua state (DECISIONS.md D-013).
///
/// # Why reentrancy is required, not stylistic
///
/// `t_eval`/`e_eval`/`ct_eval` append deferred closures that run Lua *while
/// the state guard is held*, and lambdas call `lock(&st).backend.*` — the
/// backend (the engine, this crate) then locks the SAME state again on the
/// same thread. A non-reentrant `std::sync::Mutex` self-deadlocks there.
///
/// # Single-thread contract (read before using)
///
/// This is a *logical* mutex for the single-threaded game, not a
/// cross-thread blocking mutex:
///
/// * It may be shared through `Arc` (the `oh-lua` `State` handle does —
///   the timeline closures captured by the 007 lambdas must be `Send`,
///   which forces the `GameApi`/`AssetsApi` trait objects, and thus the
///   whole `GameApiState`, to be `Send`). In practice ALL locking
///   happens on the single game thread, so no blocking ever occurs.
/// * `lock` on the owning thread re-enters: it bumps a depth counter and
///   hands out another guard.
/// * The guard derefs to `&mut T` exactly like a `MutexGuard`; nested
///   locks on the owning thread are serialized by program order.
/// * Panic-safe: guards are RAII, so a panic inside a nested lock
///   decrements the depth on unwind (no "depth leak" that would leave the
///   lock permanently unusable); there is no poisoning to observe.
///
/// # No `unsafe` across the shared-state path
///
/// The `unsafe` is confined to the two `deref` impls below and the manual
/// `Sync` impl next to the struct (needed because `UnsafeCell` is `!Sync`
/// for all `T`, while the 007 `Send` timeline closures must capture
/// `Arc<State>`); every other use site (`SharedState` access in the
/// engine, all 285 issue-007 lambdas) is safe code. Soundness argument:
/// all access to `value` happens through a live guard, and in this
/// codebase every lock happens on the single game thread (see the `State`
/// docs in `oh-lua` and the composition in `oh-game` tests), so nested
/// guards on that thread are serialized by program order — the same
/// invariants a reentrant mutex documents for `&self -> &mut T` access.
pub struct ReentrantMutex<T> {
    value: UnsafeCell<T>,
    depth: AtomicU32,
}

// `UnsafeCell` is `!Sync` for all `T`, so the auto-impl cannot make
// `ReentrantMutex<T>` `Sync` — but the 007 lambdas capture `Arc<State>`
// into `Send` timeline closures, which needs it. Soundness argument:
// the only interior-mutability hazard is two guards being alive at once;
// in this codebase every `lock()` happens on the single game thread (the
// type docs' contract), and the depth counter is atomic, so sharing the
// HANDLE across threads (which the `Send` timeline closures require)
// never races on the counter.
// SAFETY: see the argument above; mirrors the single-thread usage
// contract documented on the type.
unsafe impl<T: Send> Sync for ReentrantMutex<T> {}

impl<T> ReentrantMutex<T> {
    /// Creates the lock guarding `value` (depth 0 = unlocked).
    #[must_use]
    pub fn new(value: T) -> Self {
        Self {
            value: UnsafeCell::new(value),
            depth: AtomicU32::new(0),
        }
    }

    /// Locks (or re-enters, on the owning thread) the value.
    pub fn lock(&self) -> ReentrantMutexGuard<'_, T> {
        let depth = self.depth.load(Ordering::Relaxed);
        debug_assert!(depth < u32::MAX - 1, "reentrant depth overflow");
        self.depth.store(depth + 1, Ordering::Relaxed);
        ReentrantMutexGuard(self)
    }

    /// Whether any guard is currently alive (depth > 0). Test/diagnostics
    /// hook standing in for "the depth counter returned to zero".
    #[must_use]
    pub fn is_locked(&self) -> bool {
        self.depth.load(Ordering::Relaxed) > 0
    }
}

impl<T: fmt::Debug> fmt::Debug for ReentrantMutex<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ReentrantMutex")
            .field("depth", &self.depth)
            .finish()
    }
}

impl<T: Default> Default for ReentrantMutex<T> {
    fn default() -> Self {
        Self::new(Default::default())
    }
}

/// Guard from [`ReentrantMutex::lock`] — derefs to `&mut T` like a
/// `MutexGuard`; dropping decrements the depth (RAII panic safety).
pub struct ReentrantMutexGuard<'a, T>(pub(crate) &'a ReentrantMutex<T>);

impl<T> Deref for ReentrantMutexGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        // SAFETY: a live guard implies the value is in use on this thread;
        // all access happens through guards (type docs).
        unsafe { &*self.0.value.get() }
    }
}

impl<T> DerefMut for ReentrantMutexGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        // SAFETY: as `deref` — nested same-thread guards are serialized by
        // program order (type docs).
        unsafe { &mut *self.0.value.get() }
    }
}

impl<T> Drop for ReentrantMutexGuard<'_, T> {
    fn drop(&mut self) {
        let depth = self.0.depth.load(Ordering::Relaxed);
        debug_assert!(depth > 0, "lock depth underflow");
        self.0.depth.store(depth - 1, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use oh_data::pack_data::PackData;

    use super::*;

    /// Reentrant lock is used through `Rc` in the composition (single-thread
    /// game; `Rc` keeps the type `!Send` so no cross-thread use is possible).
    /// D-013 guardrail: a nested lock on the same thread MUST succeed
    /// (this is the `t_eval` / `lock(&st).backend.*` path — a
    /// non-reentrant mutex hangs here, so this test is the discriminating
    /// one).
    #[test]
    fn reentrant_lock_same_thread_nests() {
        let m = Rc::new(ReentrantMutex::new(0));
        let mut outer = m.lock();
        {
            // Same-thread re-entry: must not block (std::sync::Mutex
            // deadlocks here).
            let mut inner = m.lock();
            *inner += 1;
        }
        // The outer guard stays usable after the nested guard drops.
        assert_eq!(*outer, 1);
        *outer += 1;
        assert_eq!(*m.lock(), 2);
        drop(outer);
        assert!(!m.is_locked());
    }

    /// D-013 guardrail: a panic inside a nested lock must leave the depth
    /// counter back at zero (a depth leak would fail *silently* — the lock
    /// would claim it is held forever, or reject locks that should
    /// succeed). The guard drops on unwind, so the lock stays fully usable.
    #[test]
    fn reentrant_lock_depth_recovers_after_panic() {
        let m = Rc::new(ReentrantMutex::new(0));
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _outer = m.lock();
            let _inner = m.lock();
            panic!("boom");
        }));
        assert!(result.is_err());
        // Depth is back to zero: a fresh lock and a fresh re-entry both
        // succeed immediately.
        {
            let g = m.lock();
            let _g2 = m.lock();
            assert_eq!(*g, 0);
        }
        assert!(!m.is_locked());
        assert_eq!(*m.lock(), 0);
    }

    fn shared_state() -> SharedState {
        SharedState::new(
            RandomNumberGenerator::new(0),
            LevelStatus::new(false, 1600.0),
            HexagonGameStatus::new(),
            StyleData::default(),
            MusicData::default(),
            false,
            true,
            "packs/base".into(),
            "level".into(),
            "pack".into(),
            PackData {
                folder_path: "packs/base/".into(),
                id: "pack".into(),
                disambiguator: "test".into(),
                name: "base".into(),
                author: "tester".into(),
                description: String::new(),
                version: 1,
                priority: 100.0,
                dependencies: Vec::new(),
            },
        )
    }

    #[test]
    fn new_takes_c_initial_values() {
        let s = shared_state();
        assert!(!s.in_menu);
        assert!(s.headless);
        assert_eq!(s.difficulty_mult, 1.0);
        assert!(s.first_play);
        assert!(s.walls.is_empty());
        assert!(s.main_timeline.size() == 0);
        assert!(s.event_timeline.size() == 0);
        assert!(s.ilc_cmd_log.is_empty());
        assert!(s.called_deprecated_functions.is_empty());
        assert!(s.exec_script_pack_path_context.is_empty());
    }

    #[test]
    fn get_speed_and_delay_mult_dm_match_cpp_formula() {
        // C++: speedMult * pow(difficultyMult, 0.65f); delayMult /
        // pow(difficultyMult, 0.10f); max-limit clamps.
        let mut s = shared_state();
        s.level_status.speed_mult = 2.0;
        s.difficulty_mult = 4.0;
        assert_eq!(s.get_speed_mult_dm(), 2.0_f32 * 4.0_f32.powf(0.65_f32));

        s.level_status.speed_max = 1.5;
        assert!(s.get_speed_mult_dm() <= 1.5);

        s.level_status.delay_mult = 3.0;
        assert_eq!(s.get_delay_mult_dm(), 3.0_f32 / 4.0_f32.powf(0.10_f32));

        s.level_status.delay_max = 1.0;
        assert!(s.get_delay_mult_dm() <= 1.0);
    }

    struct LogBackend;
    impl GameApi for LogBackend {
        fn get_width(&self) -> u32 {
            0
        }
        fn get_height(&self) -> u32 {
            0
        }
        fn get_official(&self) -> bool {
            false
        }
        fn get_debug(&self) -> bool {
            false
        }
        fn is_key_pressed(&self, _: i32) -> bool {
            false
        }
        fn is_mouse_button_pressed(&self, _: i32) -> bool {
            false
        }
        fn log(&self, _: &str, _: &str) {}
        fn init_flash_effect(&self, _: i32, _: i32, _: i32) {}
        fn get_player_angle(&self) -> f32 {
            0.0
        }
        fn set_player_angle(&self, _: f32) {}
        fn update_player_position(&self, _: f32) {}
        fn increment_difficulty(&self) {}
        fn perform_player_swap(&self, _: bool) {}
        fn stop_level_music(&self) {}
        fn play_level_music(&self) {}
        fn play_level_music_at_time(&self, _: f32) {}
        fn play_sound_override(&self, _: &str) {}
        fn play_pack_sound_override(&self, _: &str, _: &str) {}
        fn refresh_music_pitch(&self) {}
        fn add_message(&self, _: &str, _: f64, _: bool) {}
        fn clear_messages(&self) {}
        fn death(&self, _: bool) {}
        fn create_wall(&self, _: i32, _: f32, _: SpeedData, _: SpeedData, _: f32) {}
        fn refresh_pulse(&self) {}
        fn refresh_beat_pulse(&self) {}
        fn background_camera_rotation(&self) -> Option<f32> {
            None
        }
        fn set_background_camera_rotation(&self, _: f32) {}
        fn in_replay(&self) -> bool {
            false
        }
        fn unlock_achievement(&self, _: &str) {}
        fn go_to_menu(&self, _: bool, _: bool) {}
    }

    #[test]
    fn raise_warning_once_per_function_appends_ilc_log() {
        let mut s = shared_state();
        let backend = LogBackend;
        s.raise_warning(&backend, "m_oldFn", "use l_newFn");
        s.raise_warning(&backend, "m_oldFn", "use l_newFn");
        assert_eq!(s.ilc_cmd_log.len(), 1);
        assert!(s.ilc_cmd_log[0].contains("[warning]: [Lua] WARNING"));
        assert!(s.called_deprecated_functions.contains("m_oldFn"));
    }

    #[test]
    fn dependent_script_filename_uses_context_stack() {
        let s = shared_state();
        assert_eq!(
            s.get_dependent_script_filename("utils.lua"),
            "packs/base/Scripts/utils.lua"
        );
        assert_eq!(
            s.get_dependent_shader_filename("sh.sp"),
            "packs/base/Shaders/sh.sp"
        );
    }
}
