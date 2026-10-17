//! Composition seams for `HexagonGame` (issue 009).
//!
//! `HexagonGame` mirrors the C++ `hg::HexagonGame`, whose constructor takes
//! `(steamManager, discordManager, assets, audio, window, hexagonClient)`.
//! Everything except `assets` is a platform/network service that this issue
//! leaves unbacked; each one is a small trait the *composition* (a later
//! app-level issue, or a test) implements:
//!
//! * [`Audio`] — C++ `Audio* audio` (nullable).
//! * [`Profiles`] — the C++ `HGAssets` local-profile/score surface
//!   (`anyLocalProfileActive`, `getCurrentLocalProfile().getName()`,
//!   `getLocalScore`, `setLocalScore`) — the full write/state surface is
//!   now ported in `oh-assets` (issue 029c1); the shipping compositions
//!   back this seam with `compose::RealProfiles` over the shared
//!   `Arc<oh_assets::Assets>` the menu also holds (the C++'s single
//!   `HGAssets`); [`NullProfiles`] is the test stand-in.
//! * [`ReplayStore`] — C++ `death_saveReplay`'s `Replays/<levelId>/<diff>x/`
//!   file write (the C++ server-send half always fails headless:
//!   `hexagonClient == nullptr`).
//! * [`ScriptHost`] — the C++ `Lua::LuaContext` callback surface
//!   (`runLuaFunctionIfExists`/`readVariable`/`doesVariableExist`/
//!   `runLuaFile`/`initLua`); the composition wires it to `oh-lua`
//!   (issue 007 `callbacks::*` + `init_level_api`). A normal
//!   `oh-game -> oh-lua` dependency is a cargo cycle (DECISIONS.md D-013).
//! * [`Joystick`] — C++ `Joystick::update/pressed/risingEdge` global state
//!   (fed by the platform window in a later issue).
//! * [`CosmeticRng`] — C++ `ssvu::getRndR/getRndI` (the SSVUtils *global*
//!   thread-local `pcg32_fast`, NOT the game `random_number_generator`).
//!   Only the windowed cosmetic-particle paths draw from it.
//!
//! Every trait is `Send + Sync` because the engine stores the objects as
//! `Arc<dyn …>` and the engine itself must be `Send + Sync` (it is the
//! `GameApi` backend `Arc` the 007 lambdas call through, step (b)).

use std::time::{SystemTime, UNIX_EPOCH};

use oh_replay::CompressedReplayFile;
use oh_rng::{uniform_int, uniform_real, Pcg32Fast};

use crate::lua_state::ReentrantMutex;
use oh_config::{Jdir, Jid};

/// C++ `Audio` (only the methods `HexagonGame`/`HGUpdate` call).
pub trait Audio: Send + Sync + 'static {
    /// C++ `audio->stopSounds()`.
    fn stop_sounds(&self);
    /// C++ `audio->resumeMusic()`.
    fn resume_music(&self);
    /// C++ `audio->pauseMusic()`.
    fn pause_music(&self);
    /// C++ `audio->stopMusic()`.
    fn stop_music(&self);
    /// C++ `audio->setMusicPlayingOffsetSeconds(seconds)`.
    fn set_music_playing_offset_seconds(&self, seconds: f32);
    /// C++ `audio->getMusicPlayingOffsetSeconds()`.
    fn get_music_playing_offset_seconds(&self) -> f32;
    /// C++ `audio->playSoundOverride(id)`.
    fn play_sound_override(&self, id: &str);
    /// C++ `audio->playPackSoundOverride(packId, id)`.
    fn play_pack_sound_override(&self, pack_id: &str, id: &str);
    /// C++ `audio->playSoundAbort(id)`.
    fn play_sound_abort(&self, id: &str);
    /// C++ `audio->loadAndPlayMusic(packId, id, playingOffsetSeconds)`.
    fn load_and_play_music(&self, pack_id: &str, id: &str, offset_seconds: f32) -> bool;
    /// C++ `audio->setCurrentMusicPitch(pitch)`.
    fn set_current_music_pitch(&self, pitch: f32);
}

/// The `audio == nullptr` stand-in (every C++ caller guards on `audio !=
/// nullptr`; the composition may also pass `None` for the `Option`).
pub struct NoAudio;

impl Audio for NoAudio {
    fn stop_sounds(&self) {}
    fn resume_music(&self) {}
    fn pause_music(&self) {}
    fn stop_music(&self) {}
    fn set_music_playing_offset_seconds(&self, _: f32) {}
    fn get_music_playing_offset_seconds(&self) -> f32 {
        0.0
    }
    fn play_sound_override(&self, _: &str) {}
    fn play_pack_sound_override(&self, _: &str, _: &str) {}
    fn play_sound_abort(&self, _: &str) {}
    fn load_and_play_music(&self, _: &str, _: &str, _: f32) -> bool {
        false
    }
    fn set_current_music_pitch(&self, _: f32) {}
}

/// The C++ `HGAssets` local-profile/score surface (mapping `data-json.md`
/// profile section): `currentProfilePtr` is non-null iff a profile is
/// active, and scores live inside the active `ProfileData`.
///
/// The shipping compositions (oh-app `compose::RealProfiles`, issue
/// 029c1) back this with the SAME `Arc<oh_assets::Assets>` the menu's
/// `RealMenuAssets` holds — the C++'s single `HGAssets` (main.cpp:381/
/// :434), so a score set here during play is visible in the menu
/// immediately and persists through the `mainClient` exit save.
/// [`NullProfiles`] remains the no-profile stand-in for tests (the C++'
/// `currentProfilePtr == nullptr` state).
pub trait Profiles: Send + Sync + 'static {
    /// C++ `assets.anyLocalProfileActive()`.
    fn any_local_profile_active(&self) -> bool;
    /// C++ `assets.getCurrentLocalProfile().getName()` (C++ asserts a
    /// profile is active; callers check `any_local_profile_active` first).
    fn current_local_profile_name(&self) -> String;
    /// C++ `assets.getLocalScore(id)` (`ProfileData::getScore` — missing id
    /// is `0.f`).
    fn get_local_score(&self, level_id: &str) -> f32;
    /// C++ `assets.setLocalScore(id, score)` (`ProfileData::setScore`).
    fn set_local_score(&self, level_id: &str, score: f32);
}

/// No-profile stand-in (`currentProfilePtr == nullptr`) — the test
/// equivalent of the C++'s never-`pSetCurrent`ed state (the shipping
/// compositions use `compose::RealProfiles` over the shared `Assets` store
/// instead, issue 029c1).
pub struct NullProfiles;

impl Profiles for NullProfiles {
    fn any_local_profile_active(&self) -> bool {
        false
    }
    fn current_local_profile_name(&self) -> String {
        String::new()
    }
    fn get_local_score(&self, _: &str) -> f32 {
        0.0
    }
    fn set_local_score(&self, _: &str, _: f32) {}
}

/// C++ `death_saveReplay`'s filesystem half: write the compressed replay
/// bytes to `<dir_path>/<filename>` via `std::filesystem::create_directories`
/// and `crf.serialize_to_file`. The C++ server-send half (`death_sendReplay`)
/// requires `hexagonClient`, which is `nullptr` in every headless path and
/// is not a seam of this issue — the engine logs the same
/// "Failure sending replay" message the C++ produces for a null client.
pub trait ReplayStore: Send + Sync + 'static {
    /// Returns C++ `crf.serialize_to_file(p)`'s success flag.
    fn save(&self, dir_path: &str, filename: &str, crf: &CompressedReplayFile) -> bool;
}

/// In-memory no-op store (always "succeeds"; nothing is written).
pub struct NullReplayStore;

impl ReplayStore for NullReplayStore {
    fn save(&self, _: &str, _: &str, _: &CompressedReplayFile) -> bool {
        true
    }
}

/// The C++ `Lua::LuaContext` callback surface the update loop and lifecycle
/// drive (`mapping/lua-api.md` §5). The composition implements this on top
/// of `oh-lua`'s `LuaContext` + `api::callbacks::*`; errors are surfaced as
/// the C++ exception `what()` string (the engine performs the
/// `luaExceptionLippincottHandler` behavior: print + `goToMenu(false, true)`
/// when not in debug mode — see `HexagonGame::host_call`).
/// The C++ exception classes a console command can surface (issue 014 step
/// (a); the console's catch structure at `HGUpdate.cpp:1512-1537`
/// distinguishes `catch(std::runtime_error&)` — carries `what()` — from
/// `catch(...)` — "unknown").
///
/// Every other `ScriptHost` method reports failures as a plain `String`
/// (its caller performs the Lippincott behavior or a `runLuaFile`-style
/// print); only `execute_code` needs the class, because the interpreter's
/// two nested catches treat the two cases differently.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScriptError {
    /// C++ `std::runtime_error` (every `Lua::LuaContext` exception derives
    /// from it: `ExecutionErrorException`, `SyntaxErrorException`, ...).
    /// Carries the C++ `what()`.
    Runtime(String),
    /// C++ `catch(...)` — a non-`runtime_error` exception.
    Unknown,
}

pub trait ScriptHost: Send + Sync + 'static {
    /// C++ `lua = Lua::LuaContext{}; initLua();` (called in `newGame` before
    /// the level script runs).
    fn init_lua(&self);
    /// C++ `runLuaFile(levelData->luaScriptPath)` (cached in headless;
    /// composition's choice — `oh-lua`'s `run_lua_file` closure).
    fn run_lua_file(&self, path: &str) -> Result<(), String>;

    // -- the 12 script callbacks (mapping/lua-api.md §5) -------------------
    /// C++ `runLuaFunctionIfExists<void>("onInit")`.
    fn run_on_init(&self) -> Result<(), String>;
    /// C++ `runLuaFunctionIfExists<void>("onLoad")`.
    fn run_on_load(&self) -> Result<(), String>;
    /// C++ `runLuaFunctionIfExists<float>("onUpdate", mFT)` (the float
    /// return is discarded by the engine).
    fn run_on_update(&self, ft: f32) -> Result<(), String>;
    /// C++ `runLuaFunctionIfExists<void>("onStep")`.
    fn run_on_step(&self) -> Result<(), String>;
    /// C++ `runLuaFunctionIfExists<bool, float, int, bool, bool>("onInput",
    /// mFT, movement, focused, swap)` — `Ok(Some(true))` is the input VETO;
    /// `Ok(None)` means the callback does not exist; a `nil` return coerces
    /// to `false` (C++ `_read<bool>`), i.e. `Ok(Some(false))`.
    fn run_on_input(
        &self,
        ft: f32,
        movement: i32,
        focused: bool,
        swap: bool,
    ) -> Result<Option<bool>, String>;
    /// C++ `runLuaFunctionIfExists<void>("onPreDeath")`.
    fn run_on_pre_death(&self) -> Result<(), String>;
    /// C++ `runLuaFunctionIfExists<void>("onDeath")`.
    fn run_on_death(&self) -> Result<(), String>;
    /// C++ `runLuaFunctionIfExists<void>("onIncrement")`.
    fn run_on_increment(&self) -> Result<(), String>;
    /// C++ `runLuaFunctionIfExists<void>("onPreUnload")`.
    fn run_on_pre_unload(&self) -> Result<(), String>;
    /// C++ `runLuaFunctionIfExists<void>("onUnload")`.
    fn run_on_unload(&self) -> Result<(), String>;
    /// C++ `runLuaFunctionIfExists<void>("onCursorSwap")`.
    fn run_on_cursor_swap(&self) -> Result<(), String>;
    /// C++ `runLuaFunctionIfExists<int, float>("onRenderStage", rs, 60.f /
    /// window->getFPS())` (`HGGraphics.cpp:74-77`, inside the per-stage
    /// `getRenderStates` lambda) — the level's `onRenderStage(rs, ft)`
    /// side-effect hook, `rs` = 0–8 (`RenderStage`), `ft` = `60 / FPS`.
    /// The `int` return is discarded by the engine. Dispatched by the
    /// draw bridge (issue 15 step (a)) — NOT by the update loop.
    fn run_on_render_stage(&self, stage: i32, frametime: f32) -> Result<(), String>;

    // -- direct variable access the loop performs --------------------------
    /// C++ `lua.readVariable<float>(name)` (the `scoreOverridden` path in
    /// `update`).
    fn read_variable_f32(&self, name: &str) -> Result<f32, String>;
    /// C++ `lua.doesVariableExist(name)` (`updateText` tracked variables).
    fn variable_exists(&self, name: &str) -> bool;
    /// C++ `lua.readVariable<std::string>(name)` (`updateText`
    /// `scoreOverride` / tracked variables).
    fn read_variable_string(&self, name: &str) -> Result<String, String>;

    // -- the headless Lua console (issue 014 step (a)) ---------------------
    /// C++ `lua.executeCode(std::string_view)` (`LuaWrapper.hpp:189`) — the
    /// command fallback (`u_log(<cmd>)\n`, then bare `<cmd>\n`) and the
    /// tracked-snippet loop call exactly this. Error class per
    /// [`ScriptError`] (the C++ catch structure, `HGUpdate.cpp:1512-1537`).
    fn execute_code(&self, code: &str) -> Result<(), ScriptError>;
    /// C++ `LuaScripting::getAllFunctionNames()`
    /// (`LuaScripting.cpp:1632-1652`) — the tab-completion candidate list.
    /// (In C++ a `LuaScripting` static over the process-global metadata
    /// store; the composition's host delegates to `oh_lua::docs`.)
    fn all_function_names(&self) -> Vec<String>;
    /// C++ `LuaScripting::getDocsForFunction(name)`
    /// (`LuaScripting.cpp:1654-1688`) — the `?fn` command's docs; returns
    /// `"UNKNOWN FUNCTION"` when no entry matches (the shipped flag-off
    /// build, D-023).
    fn docs_for_function(&self, name: &str) -> String;
    /// C++ `LuaScripting::printDocs()` (`LuaScripting.cpp:1613-1630`) to a
    /// writer (the C++ writes `std::cout`; the `-printLuaDocs` CLI entry is
    /// issue 15). Always emits the 11 category headers, entries or not.
    fn print_docs(&self, out: &mut dyn std::io::Write) -> std::io::Result<()>;
}

/// The no-Lua host (`HexagonGame::<SharedState>` pure headless runs): every
/// callback is "does not exist", every read is an error (C++ would throw
/// `VariableDoesntExistException`).
pub struct NoScriptHost;

impl ScriptHost for NoScriptHost {
    fn init_lua(&self) {}
    fn run_lua_file(&self, _: &str) -> Result<(), String> {
        Ok(())
    }
    fn run_on_init(&self) -> Result<(), String> {
        Ok(())
    }
    fn run_on_load(&self) -> Result<(), String> {
        Ok(())
    }
    fn run_on_update(&self, _: f32) -> Result<(), String> {
        Ok(())
    }
    fn run_on_step(&self) -> Result<(), String> {
        Ok(())
    }
    fn run_on_input(&self, _: f32, _: i32, _: bool, _: bool) -> Result<Option<bool>, String> {
        Ok(None)
    }
    fn run_on_pre_death(&self) -> Result<(), String> {
        Ok(())
    }
    fn run_on_death(&self) -> Result<(), String> {
        Ok(())
    }
    fn run_on_increment(&self) -> Result<(), String> {
        Ok(())
    }
    fn run_on_pre_unload(&self) -> Result<(), String> {
        Ok(())
    }
    fn run_on_unload(&self) -> Result<(), String> {
        Ok(())
    }
    fn run_on_cursor_swap(&self) -> Result<(), String> {
        Ok(())
    }
    fn run_on_render_stage(&self, _: i32, _: f32) -> Result<(), String> {
        // No Lua context → the function cannot exist → the C++
        // `runLuaFunctionIfExists` skips it silently.
        Ok(())
    }
    fn read_variable_f32(&self, name: &str) -> Result<f32, String> {
        Err(format!("variable '{name}' does not exist (no Lua context)"))
    }
    fn variable_exists(&self, _: &str) -> bool {
        false
    }
    fn read_variable_string(&self, name: &str) -> Result<String, String> {
        Err(format!("variable '{name}' does not exist (no Lua context)"))
    }

    // -- the headless Lua console ------------------------------------------
    fn execute_code(&self, _: &str) -> Result<(), ScriptError> {
        // Stand-in: the C++ engine always owns a live `Lua::LuaContext`
        // (default-constructed, default libs); calling `u_log` on it before
        // `initLua()` throws `ExecutionErrorException` (a `runtime_error`).
        // The exact `what()` is Lua-version dependent and not pinned, so the
        // port carries a stand-in message; the interpreter's catch behavior
        // is what is under test.
        Err(ScriptError::Runtime(
            "no Lua context (NoScriptHost)".to_string(),
        ))
    }
    fn all_function_names(&self) -> Vec<String> {
        // No registrations ever happened → the C++ store is empty → the C++
        // `getAllFunctionNames()` returns an empty vector.
        Vec::new()
    }
    fn docs_for_function(&self, _: &str) -> String {
        // C++ `getDocsForFunction` with an empty store: always the fallback.
        "UNKNOWN FUNCTION".to_string()
    }
    fn print_docs(&self, out: &mut dyn std::io::Write) -> std::io::Result<()> {
        // C++ `printDocs` prints the 11 category headers even when the store
        // is empty (flag-off build / never-initialized context); the entries
        // (none, here) would follow each header.
        for header in crate::lua_console::PREFIX_HEADERS {
            // C++ byte format: `"\n" + prefixHeaders[i] + "\n\n"`.
            writeln!(out, "\n{header}\n")?;
        }
        Ok(())
    }
}

/// C++ `Joystick` global state (`Core/Joystick.hpp` — the desktop
/// composition feeds it from the controller backend, `oh_app::joystick::
/// RealJoystick`; `updateInput_UpdateJoystickControls` only runs when a
/// window is present; the headless / Android / test compositions keep the
/// always-idle [`NoJoystick`]).
pub trait Joystick: Send + Sync + 'static {
    /// C++ `Joystick::update(deadzone)` — the poll (the desktop backend
    /// reads the controller here; the C++'s SFML poll is inside the call).
    fn update(&self, deadzone: f32);
    /// C++ `Joystick::pressed(Jdir::Left|Right)`.
    fn pressed_dir(&self, dir: Jdir) -> bool;
    /// C++ `Joystick::pressed(Jid::Swap|Focus|…)`.
    fn pressed_id(&self, id: Jid) -> bool;
    /// C++ `Joystick::risingEdge(Jid::Exit|ForceRestart|Restart|Replay)`.
    fn rising_edge_id(&self, id: Jid) -> bool;
    /// C++ `sf::Joystick::isConnected(0) ?
    /// sf::Joystick::getIdentification(0).vendorId : 0` (Config.cpp:1577-
    /// 1581 — the `getJoystickBindName` vendor source).
    fn vendor_id(&self) -> u32;
}

/// The always-idle joystick (no controller connected — the C++'s
/// `isConnected(0) == false` state: every query false, vendor id 0).
pub struct NoJoystick;

impl Joystick for NoJoystick {
    fn update(&self, _: f32) {}
    fn pressed_dir(&self, _: Jdir) -> bool {
        false
    }
    fn pressed_id(&self, _: Jid) -> bool {
        false
    }
    fn rising_edge_id(&self, _: Jid) -> bool {
        false
    }
    fn vendor_id(&self) -> u32 {
        0
    }
}

/// C++ `ssvu::getRndR/getRndI` — the SSVUtils **global** thread-local
/// `pcg32_fast` (`extlibs/SSVUtils/Core/Utils/Rnd.hpp`), seeded from
/// `std::random_device`. Used ONLY by the windowed cosmetic-particle paths
/// (never by the game `random_number_generator`, so it cannot affect
/// replay determinism).
pub trait CosmeticRng: Send + Sync + 'static {
    /// C++ `ssvu::getRndR(min, max)` —
    /// `std::uniform_real_distribution<float>(min, max)` on the global
    /// pcg32_fast.
    fn get_rnd_r(&self, min: f32, max: f32) -> f32;
    /// C++ `ssvu::getRndI(min, max)` — `std::uniform_int_distribution<i32>
    /// {min, max - 1}` (EXCLUSIVE of max).
    fn get_rnd_i(&self, min: i32, max: i32) -> i32;
}

/// Default cosmetic RNG: a `pcg32_fast` (C++ `pcg32_fast`) with the exact
/// libstdc++ distribution algorithms (`oh-rng` `uniform_real`/`uniform_int`).
/// The C++ engine is a thread-local seeded by `seed_seq_from<
/// std::random_device>`; the port seeds from process entropy at
/// construction — both are non-deterministic by design, so the streams are
/// not (and are not expected to be) comparable.
#[derive(Debug)]
pub struct PcgCosmeticRng {
    rng: ReentrantMutex<Pcg32Fast>,
}

impl PcgCosmeticRng {
    /// Seeds from process entropy (SystemTime + pid): the std-Rust
    /// replacement for C++ `seed_seq_from<std::random_device>` (no
    /// `random_device` in std; no `rand` dependency in this issue).
    #[must_use]
    pub fn new() -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0);
        let seed = now ^ (u64::from(std::process::id())).rotate_left(32);
        Self {
            rng: ReentrantMutex::new(Pcg32Fast::from_seed(seed)),
        }
    }
}

impl Default for PcgCosmeticRng {
    fn default() -> Self {
        Self::new()
    }
}

impl CosmeticRng for PcgCosmeticRng {
    fn get_rnd_r(&self, min: f32, max: f32) -> f32 {
        uniform_real(&mut self.rng.lock(), min, max)
    }
    fn get_rnd_i(&self, min: i32, max: i32) -> i32 {
        // C++ `getRndI`: `uniform_int_distribution{min, max - 1}`.
        uniform_int(&mut self.rng.lock(), min, max - 1)
    }
}

/// C++ `hg::Steam::steam_manager* steamManager` — the C++ `HexagonGame`'s
/// FIRST constructor argument (main.cpp:415) / `MenuGame`'s value argument
/// (main.cpp:434); the composition's Steam surface.
///
/// The composition wires `oh_steam::SteamManager` in here (issue 020):
/// the `steam` cargo feature selects the real-SDK backend, and without it
/// (or without a Steam client) the manager's `NoSteam` backend makes every
/// call a logged no-op — the C++'s graceful-degradation model
/// (`steamAttempt`/`steamHung` on `HexagonGame`, HexagonGame.hpp:90-92;
/// the hung counter lives in the manager in the port, so the sink's
/// `update_rich_presence_callbacks` carries it). The C++'s
/// `steamManager == nullptr` null-equivalent (every call site is
/// `steamManager != nullptr`-gated, e.g. HGUpdate.cpp:148,450,480,498)
/// is [`NoneSteamSink`].
///
/// The methods are exactly the C++ call sites the composition drives:
/// the engine's three (below, via the engine's `steam_manager` field) and
/// the menu's two (`oh-menu` is extend-only — the composition calls them
/// at the `MenuGame::init` lifecycle point, MenuGame.cpp:793-794).
pub trait SteamSink: Send + Sync + 'static {
    /// C++ `steamManager->set_rich_presence_in_game(nameStr, diffStr,
    /// timeStr)` (HGUpdate.cpp:147-149).
    fn set_rich_presence_in_game(&self, level_name: &str, difficulty_mult: &str, time: &str);
    /// C++ `HexagonGame::updateRichPresenceCallbacks()`'s Steam half
    /// (HexagonGame.cpp:477-495 — the C++'s `steamManager->
    /// run_callbacks()` + `steamAttempt`/`steamHung` accounting).
    fn update_rich_presence_callbacks(&self);
    /// C++ `steamManager->unlock_achievement(id)` (HGUpdate.cpp:450/480/
    /// 498 — `a22_swapdeath`; HexagonGame.cpp:1063 — `a24_onlinescore`,
    /// online-client-only, inert in this port; HGScripting.cpp:850 — the
    /// Lua `unlockAchievement` API, via the `GameApi` backend).
    fn unlock_achievement(&self, achievement_id: &str);
    /// C++ `steamManager.set_rich_presence_in_menu()` — called at the
    /// `MenuGame::init` lifecycle point (MenuGame.cpp:793).
    fn set_rich_presence_in_menu(&self);
    /// C++ `steamManager.update_hardcoded_achievements()` — called at the
    /// `MenuGame::init` lifecycle point (MenuGame.cpp:794).
    fn update_hardcoded_achievements(&self);
}

/// C++ `steamManager == nullptr` — the engine's null-manager equivalent
/// (every call site is a no-op; the C++ gates them with
/// `steamManager != nullptr`).
#[derive(Debug, Default, Clone, Copy)]
pub struct NoneSteamSink;

impl SteamSink for NoneSteamSink {
    fn set_rich_presence_in_game(&self, _level_name: &str, _difficulty_mult: &str, _time: &str) {}
    fn update_rich_presence_callbacks(&self) {}
    fn unlock_achievement(&self, _achievement_id: &str) {}
    fn set_rich_presence_in_menu(&self) {}
    fn update_hardcoded_achievements(&self) {}
}
