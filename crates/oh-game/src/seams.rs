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
//!   `getLocalScore`, `setLocalScore`) — `oh-assets` ports the profile
//!   *loading* only (its editor/mutation API is out of that task's scope).
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

/// No-profile stand-in (`currentProfilePtr == nullptr`).
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

    // -- direct variable access the loop performs --------------------------
    /// C++ `lua.readVariable<float>(name)` (the `scoreOverridden` path in
    /// `update`).
    fn read_variable_f32(&self, name: &str) -> Result<f32, String>;
    /// C++ `lua.doesVariableExist(name)` (`updateText` tracked variables).
    fn variable_exists(&self, name: &str) -> bool;
    /// C++ `lua.readVariable<std::string>(name)` (`updateText`
    /// `scoreOverride` / tracked variables).
    fn read_variable_string(&self, name: &str) -> Result<String, String>;
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
    fn read_variable_f32(&self, name: &str) -> Result<f32, String> {
        Err(format!("variable '{name}' does not exist (no Lua context)"))
    }
    fn variable_exists(&self, _: &str) -> bool {
        false
    }
    fn read_variable_string(&self, name: &str) -> Result<String, String> {
        Err(format!("variable '{name}' does not exist (no Lua context)"))
    }
}

/// C++ `Joystick` global state (`Core/Joystick.hpp` — fed by the platform
/// window in a later issue; `updateInput_UpdateJoystickControls` only runs
/// when a window is present).
pub trait Joystick: Send + Sync + 'static {
    /// C++ `Joystick::update(deadzone)`.
    fn update(&self, deadzone: f32);
    /// C++ `Joystick::pressed(Jdir::Left|Right)`.
    fn pressed_dir(&self, dir: Jdir) -> bool;
    /// C++ `Joystick::pressed(Jid::Swap|Focus|…)`.
    fn pressed_id(&self, id: Jid) -> bool;
    /// C++ `Joystick::risingEdge(Jid::Exit|ForceRestart|Restart|Replay)`.
    fn rising_edge_id(&self, id: Jid) -> bool;
}

/// The always-idle joystick (no platform backend attached yet).
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
