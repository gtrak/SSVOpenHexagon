//! Issue 007 verification: the exact registered global-function name set
//! (the 285 names re-derived from the C++ `addLuaFn` /
//! `makeLuaAccessor` / `sdColorGetter` / `writeVariable` call sites in
//! `src/SSVOpenHexagon/Core/*.cpp`, per mapping/lua-api.md §1/§2) and the
//! spread-argument closure semantics of `add_lua_fn!`.

use oh_game::lua_state::ReentrantMutex;
use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};

use oh_data::{MusicData, PackData, StyleData};
use oh_game::{HexagonGameStatus, LevelStatus, SpeedData};
use oh_lua::api::state::{AssetsApi, GameApi, GameApiState};
use oh_lua::api::{init_level_api, init_lua_api, State};
use oh_lua::{add_lua_fn, FnMeta, LuaContext, LuaError};
use oh_rng::RandomNumberGenerator;

// ---------------------------------------------------------------------------
// Recording mock backends (issue 009 implements the real ones)
// ---------------------------------------------------------------------------

#[derive(Default)]
struct MockAssets;

impl AssetsApi for MockAssets {
    fn get_music_data(&self, _pack_id: &str, _id: &str) -> Result<MusicData, LuaError> {
        Err(LuaError::ExecutionError {
            message: "no music".into(),
        })
    }
    fn get_style_data(&self, _pack_id: &str, _id: &str) -> Result<StyleData, LuaError> {
        Err(LuaError::ExecutionError {
            message: "no style".into(),
        })
    }
    fn find_pack_data(&self, _d: &str, _n: &str, _a: &str) -> Option<PackData> {
        None
    }
    fn get_shader_id_by_path(&self, _path: &str) -> Option<u64> {
        None
    }
    fn is_valid_shader_id(&self, _shader_id: u64) -> bool {
        false
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

/// The `ssvu::lo` recorder's (channel, message) pairs.
type LogPair = (String, String);

/// A `GameApi` that records the `ssvu::lo` (channel, message) pairs —
/// `logs: None` (the derive `Default`) is the silent original.
/// `held_key` / `held_button` (issue 31 §1): the backend's LIVE key /
/// button state the `u_isKeyPressed` / `u_isMouseButtonPressed` reads
/// must return (the C++'s `window->getInputState()[...]` — a `None`
/// backend is the C++'s `window == nullptr` headless `false`).
#[derive(Default)]
struct MockGame {
    logs: Option<std::sync::Arc<std::sync::Mutex<Vec<LogPair>>>>,
    held_key: Option<i32>,
    held_button: Option<i32>,
}

impl GameApi for MockGame {
    fn get_width(&self) -> u32 {
        1920
    }
    fn get_height(&self) -> u32 {
        1080
    }
    fn get_official(&self) -> bool {
        false
    }
    fn get_debug(&self) -> bool {
        true
    }
    fn is_key_pressed(&self, key_code: i32) -> bool {
        self.held_key == Some(key_code)
    }
    fn is_mouse_button_pressed(&self, button_code: i32) -> bool {
        self.held_button == Some(button_code)
    }
    fn log(&self, channel: &str, message: &str) {
        if let Some(logs) = &self.logs {
            logs.lock()
                .expect("log recorder poisoned")
                .push((channel.to_string(), message.to_string()));
        }
    }
    fn init_flash_effect(&self, _r: i32, _g: i32, _b: i32) {}
    fn get_player_angle(&self) -> f32 {
        0.0
    }
    fn set_player_angle(&self, _angle: f32) {}
    fn update_player_position(&self, _radius: f32) {}
    fn increment_difficulty(&self) {}
    fn perform_player_swap(&self, _play_sound: bool) {}
    fn stop_level_music(&self) {}
    fn play_level_music(&self) {}
    fn play_level_music_at_time(&self, _seconds: f32) {}
    fn play_sound_override(&self, _sound_id: &str) {}
    fn play_pack_sound_override(&self, _pack_id: &str, _file_name: &str) {}
    fn refresh_music_pitch(&self) {}
    fn add_message(&self, _message: &str, _duration: f64, _sound_toggle: bool) {}
    fn clear_messages(&self) {}
    fn death(&self, _force: bool) {}
    fn create_wall(
        &self,
        _side: i32,
        _thickness: f32,
        _speed: SpeedData,
        _curve: SpeedData,
        _hue_mod: f32,
    ) {
    }
    fn refresh_pulse(&self) {}
    fn refresh_beat_pulse(&self) {}
    fn background_camera_rotation(&self) -> Option<f32> {
        None
    }
    fn set_background_camera_rotation(&self, _value: f32) {}
    fn in_replay(&self) -> bool {
        false
    }
    fn unlock_achievement(&self, _achievement_id: &str) {}
    fn go_to_menu(&self, _send_scores: bool, _error: bool) {}
}

/// The four globals registered only by the game context's
/// `init_level_api` groups (C++ `initLua_LevelControl` /
/// `initLua_Deprecated`), absent from the C++ menu context
/// (`MenuGame::initLua` runs only the shared init + menu overrides).
const GAME_ONLY: [&str; 4] = [
    "l_forceSetPulse",
    "l_forceSetBeatPulse",
    "u_impl_addTrackedResult",
    "u_swapPlayer",
];

/// A fresh game context (C++ `HexagonGame` state, `in_menu == false`,
/// headless so no window reads happen).
fn game_state(in_menu: bool) -> State {
    game_state_with(in_menu, Arc::new(MockGame::default()))
}

/// `game_state` with a chosen backend (the 29g1 tests pass a recording
/// `MockGame` to pin the `ssvu::lo` lines).
fn game_state_with(in_menu: bool, backend: std::sync::Arc<dyn GameApi>) -> State {
    Arc::new(ReentrantMutex::new(GameApiState::new(
        RandomNumberGenerator::new(42),
        LevelStatus::new(true, 800.0),
        HexagonGameStatus::default(),
        StyleData::default(),
        MusicData::default(),
        in_menu,
        true,
        "/tmp".to_string(),
        "TestLevel".to_string(),
        "TestPack".to_string(),
        PackData {
            folder_path: "/tmp/TestPack/".to_string(),
            id: "TestPack".to_string(),
            disambiguator: "TestPack".to_string(),
            name: "TestLevel".to_string(),
            author: "TestAuthor".to_string(),
            description: String::new(),
            version: 1,
            priority: 100.0,
            dependencies: Vec::new(),
        },
        Arc::new(|_pack_path, _shared| Ok(())),
        Arc::new(MockAssets),
        backend,
    )))
}

/// Every top-level Lua global name (pre- and post-registration snapshot).
fn snapshot_globals(lua: &LuaContext) -> BTreeSet<String> {
    let names: Vec<String> = lua
        .execute_code_typed(
            "local out = {} for k in pairs(_G) do out[#out + 1] = tostring(k) end return out",
        )
        .unwrap();
    names.into_iter().collect()
}

// The 285 registered global function names: 135 `addLuaFn` +
// 72 `makeLuaAccessor` variables × 2 (get/set) + 5 `sdColorGetter`
// + 1 raw `u_impl_addTrackedResult` write.
const REGISTERED_NAMES: [&str; 285] = [
    "a_overrideBeepSound",
    "a_overrideDeathSound",
    "a_overrideIncrementSound",
    "a_overrideSwapSound",
    "a_playPackSound",
    "a_playSound",
    "a_setMusic",
    "a_setMusicPitch",
    "a_setMusicSeconds",
    "a_setMusicSegment",
    "a_syncMusicToDM",
    "ct_create",
    "ct_eval",
    "ct_kill",
    "ct_stopTime",
    "ct_stopTimeS",
    "ct_wait",
    "ct_waitS",
    "ct_waitUntilS",
    "cw_clear",
    "cw_create",
    "cw_createDeadly",
    "cw_createNoCollision",
    "cw_destroy",
    "cw_getCollision",
    "cw_getDeadly",
    "cw_getKillingSide",
    "cw_getVertexPos",
    "cw_getVertexPos4",
    "cw_moveVertexPos",
    "cw_moveVertexPos4Same",
    "cw_setCollision",
    "cw_setDeadly",
    "cw_setKillingSide",
    "cw_setVertexColor",
    "cw_setVertexColor4",
    "cw_setVertexColor4Same",
    "cw_setVertexPos",
    "cw_setVertexPos4",
    "e_clearMessages",
    "e_eval",
    "e_eventStopTime",
    "e_eventStopTimeS",
    "e_eventWait",
    "e_eventWaitS",
    "e_eventWaitUntilS",
    "e_kill",
    "e_messageAdd",
    "e_messageAddImportant",
    "e_messageAddImportantSilent",
    "e_stopTime",
    "e_stopTimeS",
    "e_wait",
    "e_waitS",
    "e_waitUntilS",
    "l_addTracked",
    "l_clearTracked",
    "l_enableRndSideChanges",
    "l_forceSetBeatPulse",
    "l_forceSetPulse",
    "l_get3dRequired",
    "l_getBeatPulse",
    "l_getBeatPulseDelay",
    "l_getBeatPulseDelayMax",
    "l_getBeatPulseInitialDelay",
    "l_getBeatPulseMax",
    "l_getBeatPulseSpeedMult",
    "l_getCameraShake",
    "l_getCurrentIncrements",
    "l_getDarkenUnevenBackgroundChunk",
    "l_getDelayInc",
    "l_getDelayMax",
    "l_getDelayMin",
    "l_getDelayMult",
    "l_getFastSpin",
    "l_getIncEnabled",
    "l_getIncTime",
    "l_getLevelTime",
    "l_getManualBeatPulseControl",
    "l_getManualPulseControl",
    "l_getOfficial",
    "l_getPlayerSpeedMult",
    "l_getPulse",
    "l_getPulseDelay",
    "l_getPulseDelayMax",
    "l_getPulseDirection",
    "l_getPulseInitialDelay",
    "l_getPulseMax",
    "l_getPulseMin",
    "l_getPulseSpeed",
    "l_getPulseSpeedR",
    "l_getRadiusMin",
    "l_getRotation",
    "l_getRotationSpeed",
    "l_getRotationSpeedInc",
    "l_getRotationSpeedMax",
    "l_getShadersRequired",
    "l_getShowPlayerTrail",
    "l_getSides",
    "l_getSidesMax",
    "l_getSidesMin",
    "l_getSpeedInc",
    "l_getSpeedMax",
    "l_getSpeedMult",
    "l_getSwapCooldownMult",
    "l_getSwapEnabled",
    "l_getTutorialMode",
    "l_getWallAngleLeft",
    "l_getWallAngleRight",
    "l_getWallSkewLeft",
    "l_getWallSkewRight",
    "l_getWallSpawnDistance",
    "l_overrideScore",
    "l_removeTracked",
    "l_resetTime",
    "l_set3dRequired",
    "l_setBeatPulse",
    "l_setBeatPulseDelay",
    "l_setBeatPulseDelayMax",
    "l_setBeatPulseInitialDelay",
    "l_setBeatPulseMax",
    "l_setBeatPulseSpeedMult",
    "l_setCameraShake",
    "l_setCurrentIncrements",
    "l_setDarkenUnevenBackgroundChunk",
    "l_setDelayInc",
    "l_setDelayMax",
    "l_setDelayMin",
    "l_setDelayMult",
    "l_setFastSpin",
    "l_setIncEnabled",
    "l_setIncTime",
    "l_setManualBeatPulseControl",
    "l_setManualPulseControl",
    "l_setPlayerSpeedMult",
    "l_setPulse",
    "l_setPulseDelay",
    "l_setPulseDelayMax",
    "l_setPulseDirection",
    "l_setPulseInitialDelay",
    "l_setPulseMax",
    "l_setPulseMin",
    "l_setPulseSpeed",
    "l_setPulseSpeedR",
    "l_setRadiusMin",
    "l_setRotation",
    "l_setRotationSpeed",
    "l_setRotationSpeedInc",
    "l_setRotationSpeedMax",
    "l_setShadersRequired",
    "l_setShowPlayerTrail",
    "l_setSides",
    "l_setSidesMax",
    "l_setSidesMin",
    "l_setSpeedInc",
    "l_setSpeedMax",
    "l_setSpeedMult",
    "l_setSwapCooldownMult",
    "l_setSwapEnabled",
    "l_setTutorialMode",
    "l_setWallAngleLeft",
    "l_setWallAngleRight",
    "l_setWallSkewLeft",
    "l_setWallSkewRight",
    "l_setWallSpawnDistance",
    "m_clearMessages",
    "m_messageAdd",
    "m_messageAddImportant",
    "m_messageAddImportantSilent",
    "s_get3DOverrideColor",
    "s_get3dAlphaFalloff",
    "s_get3dAlphaMult",
    "s_get3dDarkenMult",
    "s_get3dDepth",
    "s_get3dPerspectiveMult",
    "s_get3dPulseMax",
    "s_get3dPulseMin",
    "s_get3dPulseSpeed",
    "s_get3dSkew",
    "s_get3dSpacing",
    "s_getBGColorOffset",
    "s_getBGRotationOffset",
    "s_getBGTileRadius",
    "s_getCapColorResult",
    "s_getColor",
    "s_getHueInc",
    "s_getHueIncrement",
    "s_getHueMax",
    "s_getHueMin",
    "s_getHuePingPong",
    "s_getMainColor",
    "s_getMaxSwapTime",
    "s_getPlayerColor",
    "s_getPulseInc",
    "s_getPulseIncrement",
    "s_getPulseMax",
    "s_getPulseMin",
    "s_getTextColor",
    "s_set3dAlphaFalloff",
    "s_set3dAlphaMult",
    "s_set3dDarkenMult",
    "s_set3dDepth",
    "s_set3dPerspectiveMult",
    "s_set3dPulseMax",
    "s_set3dPulseMin",
    "s_set3dPulseSpeed",
    "s_set3dSkew",
    "s_set3dSpacing",
    "s_setBGColorOffset",
    "s_setBGRotationOffset",
    "s_setBGTileRadius",
    "s_setCapColorByIndex",
    "s_setCapColorMain",
    "s_setCapColorMainDarkened",
    "s_setHueInc",
    "s_setHueIncrement",
    "s_setHueMax",
    "s_setHueMin",
    "s_setHuePingPong",
    "s_setMaxSwapTime",
    "s_setPulseInc",
    "s_setPulseIncrement",
    "s_setPulseMax",
    "s_setPulseMin",
    "s_setStyle",
    "shdr_getDependencyShaderId",
    "shdr_getShaderId",
    "shdr_resetActiveFragmentShader",
    "shdr_resetAllActiveFragmentShaders",
    "shdr_setActiveFragmentShader",
    "shdr_setUniformF",
    "shdr_setUniformFVec2",
    "shdr_setUniformFVec3",
    "shdr_setUniformFVec4",
    "shdr_setUniformI",
    "shdr_setUniformIVec2",
    "shdr_setUniformIVec3",
    "shdr_setUniformIVec4",
    "steam_unlockAchievement",
    "t_clear",
    "t_eval",
    "t_kill",
    "t_wait",
    "t_waitS",
    "t_waitUntilS",
    "u_clearWalls",
    "u_eventKill",
    "u_execDependencyScript",
    "u_execScript",
    "u_forceIncrement",
    "u_getAttemptRandomSeed",
    "u_getDelayMultDM",
    "u_getDifficultyMult",
    "u_getHeight",
    "u_getPlayerAngle",
    "u_getSpeedMultDM",
    "u_getVersionMajor",
    "u_getVersionMicro",
    "u_getVersionMinor",
    "u_getVersionString",
    "u_getWidth",
    "u_haltTime",
    "u_impl_addTrackedResult",
    "u_inMenu",
    "u_isFastSpinning",
    "u_isHeadless",
    "u_isKeyPressed",
    "u_isMouseButtonPressed",
    "u_kill",
    "u_log",
    "u_playPackSound",
    "u_playSound",
    "u_rndInt",
    "u_rndIntUpper",
    "u_rndReal",
    "u_rndSwitch",
    "u_setFlashColor",
    "u_setFlashEffect",
    "u_setPlayerAngle",
    "u_swapPlayer",
    "w_wall",
    "w_wallAcc",
    "w_wallAdj",
    "w_wallHModCurveData",
    "w_wallHModSpeedData",
];

#[test]
fn registered_name_set_matches_source() {
    let lua = LuaContext::new();
    let baseline = snapshot_globals(&lua);

    let st = game_state(false);
    init_lua_api(&lua, &st).expect("shared init");
    init_level_api(&lua, &st).expect("level init");

    let observed = snapshot_globals(&lua);
    let registered: BTreeSet<String> = observed.difference(&baseline).cloned().collect();

    // D-037: the shipping Luau context additionally carries the 5.1 compat
    // prelude's installed globals (the C++'s LuaJIT has them natively — no
    // registration, no diff): `loadstring` (aliasing the Rust-side `load`
    // binding) and `module` (the shim). Both are absent on Luau, so both
    // are installed.
    const PRELUDE_GLOBALS: [&str; 2] = ["loadstring", "module"];

    let expected: BTreeSet<String> = REGISTERED_NAMES
        .iter()
        .chain(PRELUDE_GLOBALS.iter())
        .map(|s| s.to_string())
        .collect();

    let missing: Vec<&String> = expected.difference(&registered).collect();
    let extra: Vec<&String> = registered.difference(&expected).collect();
    assert!(
        missing.is_empty(),
        "registered names missing from the C++ source set: {missing:?}"
    );
    assert!(
        extra.is_empty(),
        "globals registered beyond the C++ source set + the D-037 prelude globals: {extra:?}"
    );
    assert_eq!(
        registered.len(),
        REGISTERED_NAMES.len() + PRELUDE_GLOBALS.len(),
        "exact count"
    );

    // The pinned menu getters keep their shared-init types in the game
    // context: `u_getDifficultyMult` returns the level's 1.0, and a few
    // calls execute against the mock backend without error.
    let d: f64 = lua
        .execute_code_typed("return u_getDifficultyMult()")
        .unwrap();
    assert_eq!(d, 1.0);
    lua.execute_code("l_setSpeedMult(2)").unwrap();
    let sm: f64 = lua.execute_code_typed("return l_getSpeedMult()").unwrap();
    assert_eq!(sm, 2.0);
}

#[test]
fn menu_context_registers_the_same_name_set() {
    // C++ `MenuGame::initLua`: the shared init (in_menu == true), the
    // `u_log` override, 4 pinned getters, and the 71-entry no-op list
    // (mapping §8.10). The menu context drops the four game-only globals
    // (never registered outside `HexagonGame::initLua`) and gains
    // `u_timelineWait` (a historical name the C++ no-op list still writes).
    // D-037: plus the 5.1 compat prelude's installed globals (`loadstring`,
    // `module` — absent on the shipping Luau, installed by the shared
    // init; see `registered_name_set_matches_source`).
    let lua = LuaContext::new();
    let baseline = snapshot_globals(&lua);
    let st = game_state(true);
    oh_lua::api::init_menu_api(&lua, &st).expect("menu init");
    let registered: BTreeSet<String> = snapshot_globals(&lua)
        .difference(&baseline)
        .cloned()
        .collect();
    let expected: BTreeSet<String> = REGISTERED_NAMES
        .iter()
        .map(|s| s.to_string())
        .filter(|name| !GAME_ONLY.contains(&name.as_str()))
        .chain(std::iter::once("u_timelineWait".to_string()))
        .chain(["loadstring".to_string(), "module".to_string()])
        .collect();
    assert_eq!(registered, expected);

    // The pinned getters are plain constants in the menu context.
    let d: f64 = lua
        .execute_code_typed("return u_getDifficultyMult()")
        .unwrap();
    assert_eq!(d, 1.0);
    let d: f64 = lua.execute_code_typed("return u_getSpeedMultDM()").unwrap();
    assert_eq!(d, 1.0);
    let d: f64 = lua.execute_code_typed("return u_getDelayMultDM()").unwrap();
    assert_eq!(d, 1.0);
    let a: f64 = lua.execute_code_typed("return u_getPlayerAngle()").unwrap();
    assert_eq!(a, 0.0);
}

#[test]
fn registered_fn_arity_mismatch_and_window() {
    let lua = LuaContext::new();
    let seen = Arc::new(Mutex::new(Vec::<(i32, i32, i32)>::new()));
    let s2 = seen.clone();
    add_lua_fn!(
        &lua,
        "g3",
        FnMeta {
            ret: "void",
            arg_types: &["int", "int", "int"],
        },
        move |a: i32, b: i32, c: i32| {
            s2.lock().unwrap().push((a, b, c));
        },
    )
    .unwrap();

    // 0 or 1 args (gettop < count - 1): the C++ "at least N" error.
    for args in ["", ", 7"] {
        let code = format!("local _, e = pcall(g3{args}); return tostring(e)");
        let msg: String = lua.execute_code_typed(&code).unwrap();
        assert!(
            msg.contains("this function requires at least 3 parameter(s)"),
            "got: {msg}"
        );
    }

    // Exactly N - 1 args: the C++ last-slot read lands on the callee
    // itself -> the userdata cast error text.
    let msg: String = lua
        .execute_code_typed("local _, e = pcall(g3, 1, 2); return tostring(e)")
        .unwrap();
    assert!(
        msg.contains("Trying to cast a lua variable to an invalid type"),
        "got: {msg}"
    );

    // N args: exact; N + 1 args: the C++ LAST-N window (drops the first).
    lua.execute_code("g3(1, 2, 3)").unwrap();
    lua.execute_code("g3(0, 1, 2, 3)").unwrap();
    assert_eq!(*seen.lock().unwrap(), vec![(1, 2, 3), (1, 2, 3)]);
}

// ---------------------------------------------------------------------------
// Issue 31 §1 — the key / button state surface (HGScripting.cpp:89-98 /
// :135-141): `u_isKeyPressed` / `u_isMouseButtonPressed` read the
// backend's LIVE state (`window->getInputState()[...]`), not a stubbed
// `false`.
// ---------------------------------------------------------------------------

#[test]
fn the_lua_key_and_button_reads_consult_the_backend_state() {
    let lua = LuaContext::new();
    let st = game_state_with(
        false,
        Arc::new(MockGame {
            logs: None,
            held_key: Some(38),   // the C++'s LShift (the default focus bind)
            held_button: Some(3), // the C++'s XButton1 (the default swap bind)
        }),
    );
    init_lua_api(&lua, &st).expect("shared init");
    init_level_api(&lua, &st).expect("level init");

    let down: bool = lua.execute_code_typed("return u_isKeyPressed(38)").unwrap();
    assert!(
        down,
        "the backend's held LShift reads true through the Lua surface"
    );
    let up: bool = lua.execute_code_typed("return u_isKeyPressed(39)").unwrap();
    assert!(!up, "an unheld code reads false");

    let btn_down: bool = lua
        .execute_code_typed("return u_isMouseButtonPressed(3)")
        .unwrap();
    assert!(btn_down, "the backend's held XButton1 reads true");
    let btn_up: bool = lua
        .execute_code_typed("return u_isMouseButtonPressed(2)")
        .unwrap();
    assert!(!btn_up, "an unheld button code reads false");
}

#[test]
fn the_lua_key_reads_are_false_on_a_windowless_backend() {
    // The C++'s `window == nullptr` half: a backend with no held state is
    // the headless `false` (deterministic — the §1 defined behavior).
    let lua = LuaContext::new();
    let st = game_state(false);
    init_lua_api(&lua, &st).expect("shared init");
    init_level_api(&lua, &st).expect("level init");

    let k: bool = lua.execute_code_typed("return u_isKeyPressed(38)").unwrap();
    assert!(!k);
    let b: bool = lua
        .execute_code_typed("return u_isMouseButtonPressed(3)")
        .unwrap();
    assert!(!b);
}
