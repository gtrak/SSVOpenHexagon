//! Port of `include/SSVOpenHexagon/Global/Config.hpp` +
//! `src/SSVOpenHexagon/Global/Config.cpp`.
//!
//! The C++ is a namespace of free functions over function-local statics
//! (one `ssvuj::LinkedValue` per config key); the port is a `Config` struct
//! holding the same 101 `LinkedValue`s (JSON name + type + default, mapping
//! §6.2) plus the module-level state (`sizeX`/`sizeY`,
//! `uneligibilityReason`). Free-function names map to snake_case methods;
//! the window-backend operations of `setFullscreen` / `setVsync` /
//! `setMaxFPS` / `setAntialiasingLevel` and the desktop-mode reads of
//! `reapplyResolution` are left as pure-value seams (issue 10).
//!
//! Fidelity notes (mapping §10):
//! - the "official-locked" getters return the *default* while
//!   `getOfficial()` is true;
//! - `pulse_enabled` is persisted but never read by the game (no getter in
//!   C++ either — the field is still synced and saved);
//! - unknown keys in `config.json` stay in `root` and are re-saved;
//! - `getSpawnDistance()` returns the constant 1600 — there is NO config
//!   key for spawn distance (so `isEligibleForScore`'s spawn branch is
//!   effectively dead, kept for 1:1).

use std::path::{Path, PathBuf};

use oh_data::json::{self, JsonError, Obj};
use oh_data::LinkedValue;
use serde_json::Value as SerdeValue;

use crate::joystick::{self, Jid};
use crate::triggers::{Button, Combo, Key, Trigger};

/// C++ `inline constexpr float TICKS_PER_SECOND = 240.f;`.
pub const TICKS_PER_SECOND: f32 = 240.0_f32;
/// C++ `inline constexpr float TIME_STEP = 60.f / TICKS_PER_SECOND;`
/// (= 0.25f exactly).
pub const TIME_STEP: f32 = 60.0_f32 / TICKS_PER_SECOND;
/// C++ `inline constexpr float TIME_SLICE = 60.f / TICKS_PER_SECOND;`.
pub const TIME_SLICE: f32 = 60.0_f32 / TICKS_PER_SECOND;

/// C++ `constexpr float defaultSpawnDistance{1600}`.
const DEFAULT_SPAWN_DISTANCE: f32 = 1600.0_f32;

/// C++ `inline constexpr int maxBinds{4}`.
pub const MAX_BINDS: i32 = 4;

/// C++ `hg::Config::Tid`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(i32)]
pub enum Tid {
    Unknown = -1,
    RotateCCW = 0,
    RotateCW,
    Focus,
    Select,
    Exit,
    ForceRestart,
    Restart,
    Replay,
    Screenshot,
    Swap,
    Up,
    Down,
    NextPack,
    PreviousPack,
    LuaConsole,
    Pause,
    TriggersCount,
}

/// C++ `defaultServerLevelWhitelist()` — 174 level validator ids, verbatim
/// from `Config.cpp` (mapping §6.5).
#[must_use]
pub fn default_server_level_whitelist() -> Vec<String> {
    [
        // Vittorio Romeo - Cube
        "ohvrvanilla_vittorio_romeo_cube_1_apeirogon_m_0.35",
        "ohvrvanilla_vittorio_romeo_cube_1_apeirogon_m_1",
        "ohvrvanilla_vittorio_romeo_cube_1_apeirogon_m_1.6",
        "ohvrvanilla_vittorio_romeo_cube_1_commando_m_0.5",
        "ohvrvanilla_vittorio_romeo_cube_1_commando_m_1",
        "ohvrvanilla_vittorio_romeo_cube_1_commando_m_1.5",
        "ohvrvanilla_vittorio_romeo_cube_1_euclideanpc_m_0.5",
        "ohvrvanilla_vittorio_romeo_cube_1_euclideanpc_m_1",
        "ohvrvanilla_vittorio_romeo_cube_1_euclideanpc_m_1.8",
        "ohvrvanilla_vittorio_romeo_cube_1_flatteringshape_m_0.5",
        "ohvrvanilla_vittorio_romeo_cube_1_flatteringshape_m_1",
        "ohvrvanilla_vittorio_romeo_cube_1_flatteringshape_m_3",
        "ohvrvanilla_vittorio_romeo_cube_1_flatteringshape_m_4",
        "ohvrvanilla_vittorio_romeo_cube_1_goldenratio_m_0.5",
        "ohvrvanilla_vittorio_romeo_cube_1_goldenratio_m_1",
        "ohvrvanilla_vittorio_romeo_cube_1_goldenratio_m_2",
        "ohvrvanilla_vittorio_romeo_cube_1_labyrinth_m_0.5",
        "ohvrvanilla_vittorio_romeo_cube_1_labyrinth_m_1",
        "ohvrvanilla_vittorio_romeo_cube_1_labyrinth_m_1.5",
        "ohvrvanilla_vittorio_romeo_cube_1_pi_m_0.5",
        "ohvrvanilla_vittorio_romeo_cube_1_pi_m_1",
        "ohvrvanilla_vittorio_romeo_cube_1_pi_m_1.8",
        "ohvrvanilla_vittorio_romeo_cube_1_pointless_m_0.5",
        "ohvrvanilla_vittorio_romeo_cube_1_pointless_m_1",
        "ohvrvanilla_vittorio_romeo_cube_1_pointless_m_3",
        "ohvrvanilla_vittorio_romeo_cube_1_pointless_m_5",
        "ohvrvanilla_vittorio_romeo_cube_1_seconddimension_m_0.6",
        "ohvrvanilla_vittorio_romeo_cube_1_seconddimension_m_1",
        "ohvrvanilla_vittorio_romeo_cube_1_seconddimension_m_1.8",
        "ohvrvanilla_vittorio_romeo_cube_1_seconddimension_m_2.2",
        // Vittorio Romeo - Orthoplex
        "ohvrvanilla_vittorio_romeo_orthoplex_1_bipolarity_m_0.5",
        "ohvrvanilla_vittorio_romeo_orthoplex_1_bipolarity_m_1",
        "ohvrvanilla_vittorio_romeo_orthoplex_1_bipolarity_m_1.8",
        // Vittorio Romeo - Hypercube
        "ohvrvanilla_vittorio_romeo_hypercube_1_acceleradiant_m_0.85",
        "ohvrvanilla_vittorio_romeo_hypercube_1_acceleradiant_m_1",
        "ohvrvanilla_vittorio_romeo_hypercube_1_acceleradiant_m_1.8",
        "ohvrvanilla_vittorio_romeo_hypercube_1_acceleradiant_m_2.4",
        "ohvrvanilla_vittorio_romeo_hypercube_1_centrifugal_m_0.6",
        "ohvrvanilla_vittorio_romeo_hypercube_1_centrifugal_m_1",
        "ohvrvanilla_vittorio_romeo_hypercube_1_centrifugal_m_1.6",
        "ohvrvanilla_vittorio_romeo_hypercube_1_disc-o_m_0.5",
        "ohvrvanilla_vittorio_romeo_hypercube_1_disc-o_m_1",
        "ohvrvanilla_vittorio_romeo_hypercube_1_disc-o_m_2",
        "ohvrvanilla_vittorio_romeo_hypercube_1_disc-o_m_2.8",
        "ohvrvanilla_vittorio_romeo_hypercube_1_g-force_m_0.6",
        "ohvrvanilla_vittorio_romeo_hypercube_1_g-force_m_1",
        "ohvrvanilla_vittorio_romeo_hypercube_1_g-force_m_1.6",
        "ohvrvanilla_vittorio_romeo_hypercube_1_g-force_m_2.8",
        "ohvrvanilla_vittorio_romeo_hypercube_1_incongruence_m_0.5",
        "ohvrvanilla_vittorio_romeo_hypercube_1_incongruence_m_1",
        "ohvrvanilla_vittorio_romeo_hypercube_1_incongruence_m_1.8",
        "ohvrvanilla_vittorio_romeo_hypercube_1_massacre_m_0.5",
        "ohvrvanilla_vittorio_romeo_hypercube_1_massacre_m_1",
        "ohvrvanilla_vittorio_romeo_hypercube_1_massacre_m_1.6",
        "ohvrvanilla_vittorio_romeo_hypercube_1_polyhedrug_m_0.75",
        "ohvrvanilla_vittorio_romeo_hypercube_1_polyhedrug_m_1",
        "ohvrvanilla_vittorio_romeo_hypercube_1_polyhedrug_m_1.8",
        "ohvrvanilla_vittorio_romeo_hypercube_1_reppaws_m_0.5",
        "ohvrvanilla_vittorio_romeo_hypercube_1_reppaws_m_1",
        "ohvrvanilla_vittorio_romeo_hypercube_1_reppaws_m_1.8",
        "ohvrvanilla_vittorio_romeo_hypercube_1_slither_m_0.5",
        "ohvrvanilla_vittorio_romeo_hypercube_1_slither_m_1",
        "ohvrvanilla_vittorio_romeo_hypercube_1_slither_m_1.5",
        "ohvrvanilla_vittorio_romeo_hypercube_1_slither_m_2",
        // Vipre - Vanity [Steam Workshop]
        "vanitylevels_Vipre_Vanity_10006_haunted_m_1",
        "vanitylevels_Vipre_Vanity_10006_haunted_m_1.001",
        "vanitylevels_Vipre_Vanity_10006_mantra_m_1",
        "vanitylevels_Vipre_Vanity_10006_menace_m_1",
        "vanitylevels_Vipre_Vanity_10006_menace_m_1.001",
        "vanitylevels_Vipre_Vanity_10006_narcotics_m_1",
        "vanitylevels_Vipre_Vanity_10006_narcotics_m_1.001",
        "vanitylevels_Vipre_Vanity_10006_rocketscience_m_1",
        "vanitylevels_Vipre_Vanity_10006_rocketscience_m_1.001",
        "vanitylevels_Vipre_Vanity_10006_singlepulseremixv2remix_m_1",
        "vanitylevels_Vipre_Vanity_10006_squarepractice_m_1",
        "vanitylevels_Vipre_Vanity_10006_squarepractice_m_1.003",
        "vanitylevels_Vipre_Vanity_10006_squarepractice_m_1.005",
        "vanitylevels_Vipre_Vanity_10006_technology_m_1",
        "vanitylevels_Vipre_Vanity_10006_technology_m_1.001",
        "vanitylevels_Vipre_Vanity_10006_troglodyte_m_1",
        "vanitylevels_Vipre_Vanity_10006_troglodyte_m_1.001",
        "vanitylevels_Vipre_Vanity_10006_troglodyte_m_1.002",
        // Morxemplum - Rotationality Remastered [Steam Workshop]
        "rotationality_SkyMidnight_Rotationality_(Remastered)_2_dragonMayhem_m_1",
        "rotationality_SkyMidnight_Rotationality_(Remastered)_2_dragonMayhem_m_2",
        "rotationality_SkyMidnight_Rotationality_(Remastered)_2_dragonMayhem_m_3",
        "rotationality_SkyMidnight_Rotationality_(Remastered)_2_dragonMayhem_m_5",
        "rotationality_SkyMidnight_Rotationality_(Remastered)_2_dragonMayhem_m_6.66",
        "rotationality_SkyMidnight_Rotationality_(Remastered)_2_skyFasion_m_1",
        "rotationality_SkyMidnight_Rotationality_(Remastered)_2_skyFasion_m_2",
        "rotationality_SkyMidnight_Rotationality_(Remastered)_2_skyFasion_m_3",
        "rotationality_SkyMidnight_Rotationality_(Remastered)_2_skyFasion_m_5",
        // Syyrion - Travel [Steam Workshop]
        "1D2B-E31E-EC19_Syyrion_Travel_13103_ti84_m_0.8",
        "1D2B-E31E-EC19_Syyrion_Travel_13103_ti84_m_1",
        "1D2B-E31E-EC19_Syyrion_Travel_13103_ti84_m_1.2",
        "1D2B-E31E-EC19_Syyrion_Travel_13103_ti84_m_1.4",
        "1D2B-E31E-EC19_Syyrion_Travel_13103_spl_m_1",
        // NDigger - Delta [Steam Workshop]
        "ndeltalevels_NDigger_delta_1_escape_m_1",
        "ndeltalevels_NDigger_delta_1_escape_m_1.001",
        "ndeltalevels_NDigger_delta_1_fusionZ_m_1",
        "ndeltalevels_NDigger_delta_1_fusionZ_m_1.001",
        "ndeltalevels_NDigger_delta_1_fusionZ_m_1.002",
        "ndeltalevels_NDigger_delta_1_hyperbounce_m_1",
        "ndeltalevels_NDigger_delta_1_hyperbounce_m_1.001",
        "ndeltalevels_NDigger_delta_1_matrix_m_1",
        "ndeltalevels_NDigger_delta_1_matrix_m_1.001",
        "ndeltalevels_NDigger_delta_1_nocturnal_m_1",
        "ndeltalevels_NDigger_delta_1_nocturnal_m_1.001",
        "ndeltalevels_NDigger_delta_1_radioactive_m_1",
        "ndeltalevels_NDigger_delta_1_radioactive_m_1.001",
        "ndeltalevels_NDigger_delta_1_snowstorm_m_1",
        "ndeltalevels_NDigger_delta_1_snowstorm_m_1.001",
        "ndeltalevels_NDigger_delta_1_synergic_m_1",
        "ndeltalevels_NDigger_delta_1_synergic_m_1.001",
        "ndeltalevels_NDigger_delta_1_underworld_m_1",
        "ndeltalevels_NDigger_delta_1_underworld_m_1.001",
        "ndeltalevels_NDigger_delta_1_viper_m_1",
        "ndeltalevels_NDigger_delta_1_viper_m_1.001",
        // Hunter & others - Open Hexagon League 2 - Pack [Steam Workshop]
        "ohl2pack_Hunter_&_Maniac_Open_Hexagon_League_2_1_Automaton_m_1",
        "ohl2pack_Hunter_&_Maniac_Open_Hexagon_League_2_1_Automaton_m_1.01",
        "ohl2pack_Hunter_&_Maniac_Open_Hexagon_League_2_1_Ezekiel_m_1",
        "ohl2pack_Hunter_&_Maniac_Open_Hexagon_League_2_1_Ezekiel_m_1.01",
        "ohl2pack_Hunter_&_Maniac_Open_Hexagon_League_2_1_Frostbite_m_1",
        "ohl2pack_Hunter_&_Maniac_Open_Hexagon_League_2_1_Frostbite_m_1.01",
        "ohl2pack_Hunter_&_Maniac_Open_Hexagon_League_2_1_Insight_m_1",
        "ohl2pack_Hunter_&_Maniac_Open_Hexagon_League_2_1_Insight_m_1.01",
        "ohl2pack_Hunter_&_Maniac_Open_Hexagon_League_2_1_Neurodance_m_1",
        "ohl2pack_Hunter_&_Maniac_Open_Hexagon_League_2_1_Neurodance_m_1.01",
        "ohl2pack_Hunter_&_Maniac_Open_Hexagon_League_2_1_Partygoer_m_1",
        "ohl2pack_Hunter_&_Maniac_Open_Hexagon_League_2_1_Partygoer_m_1.01",
        "ohl2pack_Hunter_&_Maniac_Open_Hexagon_League_2_1_Technicolor_m_1",
        "ohl2pack_Hunter_&_Maniac_Open_Hexagon_League_2_1_Technicolor_m_1.01",
        "ohl2pack_Hunter_&_Maniac_Open_Hexagon_League_2_1_Toxine_m_1",
        "ohl2pack_Hunter_&_Maniac_Open_Hexagon_League_2_1_Toxine_m_1.01",
        "ohl2pack_Hunter_&_Maniac_Open_Hexagon_League_2_1_Vault_m_1",
        "ohl2pack_Hunter_&_Maniac_Open_Hexagon_League_2_1_Vault_m_1.01",
        "ohl2pack_Hunter_&_Maniac_Open_Hexagon_League_2_1_VaultFixed_m_1.02",
        // Hunter & others - Open Hexagon League 3 - Pack [Steam Workshop]
        "ohl3pack_Hunter_&_Maniac_Open_Hexagon_League_3_1_Blitz_m_1",
        "ohl3pack_Hunter_&_Maniac_Open_Hexagon_League_3_1_Blitz_m_1.01",
        "ohl3pack_Hunter_&_Maniac_Open_Hexagon_League_3_1_Bloodlines_m_1",
        "ohl3pack_Hunter_&_Maniac_Open_Hexagon_League_3_1_Bloodlines_m_1.01",
        "ohl3pack_Hunter_&_Maniac_Open_Hexagon_League_3_1_Cybereye_m_1",
        "ohl3pack_Hunter_&_Maniac_Open_Hexagon_League_3_1_Cybereye_m_1.01",
        "ohl3pack_Hunter_&_Maniac_Open_Hexagon_League_3_1_Cyclone_m_1",
        "ohl3pack_Hunter_&_Maniac_Open_Hexagon_League_3_1_Cyclone_m_1.01",
        "ohl3pack_Hunter_&_Maniac_Open_Hexagon_League_3_1_Cyclone_m_1.015",
        "ohl3pack_Hunter_&_Maniac_Open_Hexagon_League_3_1_Darkland_m_1",
        "ohl3pack_Hunter_&_Maniac_Open_Hexagon_League_3_1_Darkland_m_1.01",
        "ohl3pack_Hunter_&_Maniac_Open_Hexagon_League_3_1_Evirex_m_1",
        "ohl3pack_Hunter_&_Maniac_Open_Hexagon_League_3_1_Evirex_m_1.01",
        "ohl3pack_Hunter_&_Maniac_Open_Hexagon_League_3_1_Onyx_m_1",
        "ohl3pack_Hunter_&_Maniac_Open_Hexagon_League_3_1_Onyx_m_1.01",
        "ohl3pack_Hunter_&_Maniac_Open_Hexagon_League_3_1_Reactive_m_1",
        "ohl3pack_Hunter_&_Maniac_Open_Hexagon_League_3_1_Reactive_m_1.01",
        "ohl3pack_Hunter_&_Maniac_Open_Hexagon_League_3_1_Wilderness_m_1",
        "ohl3pack_Hunter_&_Maniac_Open_Hexagon_League_3_1_Wilderness_m_1.01",
        // Hunter & others - Open Hexagon League 3 - Pack [Steam Workshop]
        "ohl4pack_Hunter_&_Fractal_&_zX_Open_Hexagon_League_4_1_Catharsis_m_1",
        "ohl4pack_Hunter_&_Fractal_&_zX_Open_Hexagon_League_4_1_Catharsis_m_1.01",
        "ohl4pack_Hunter_&_Fractal_&_zX_Open_Hexagon_League_4_1_Heartbeat_m_1",
        "ohl4pack_Hunter_&_Fractal_&_zX_Open_Hexagon_League_4_1_Heartbeat_m_1.01",
        "ohl4pack_Hunter_&_Fractal_&_zX_Open_Hexagon_League_4_1_Joululumi_m_1",
        "ohl4pack_Hunter_&_Fractal_&_zX_Open_Hexagon_League_4_1_Joululumi_m_1.001",
        "ohl4pack_Hunter_&_Fractal_&_zX_Open_Hexagon_League_4_1_JoululumiNormal_m_1",
        "ohl4pack_Hunter_&_Fractal_&_zX_Open_Hexagon_League_4_1_JoululumiNormal_m_1.001",
        "ohl4pack_Hunter_&_Fractal_&_zX_Open_Hexagon_League_4_1_Killerbox_m_1",
        "ohl4pack_Hunter_&_Fractal_&_zX_Open_Hexagon_League_4_1_Killerbox_m_1.01",
        "ohl4pack_Hunter_&_Fractal_&_zX_Open_Hexagon_League_4_1_Luminara_m_1",
        "ohl4pack_Hunter_&_Fractal_&_zX_Open_Hexagon_League_4_1_Luminara_m_1.01",
        "ohl4pack_Hunter_&_Fractal_&_zX_Open_Hexagon_League_4_1_Sharpshooter_m_1",
        "ohl4pack_Hunter_&_Fractal_&_zX_Open_Hexagon_League_4_1_Sharpshooter_m_1.01",
        "ohl4pack_Hunter_&_Fractal_&_zX_Open_Hexagon_League_4_1_Signal_m_1",
        "ohl4pack_Hunter_&_Fractal_&_zX_Open_Hexagon_League_4_1_Signal_m_1.01",
        "ohl4pack_Hunter_&_Fractal_&_zX_Open_Hexagon_League_4_1_eclipse_m_1",
        "ohl4pack_Hunter_&_Fractal_&_zX_Open_Hexagon_League_4_1_eclipse_m_1.01",
        "ohl4pack_Hunter_&_Fractal_&_zX_Open_Hexagon_League_4_1_teraflect_m_1",
    ]
    .into_iter()
    .map(String::from)
    .collect()
}

fn sync_lv<T>(root: &mut Obj, lv: &LinkedValue<T>)
where
    T: oh_data::JsonConvertible + Clone,
{
    lv.sync_to(root);
}

fn reset_lv<T>(lv: &mut LinkedValue<T>)
where
    T: oh_data::JsonConvertible + Clone,
{
    lv.reset_to_default();
}

fn key_combo(key: Key) -> Combo {
    let mut c = Combo::new();
    c.add_key(key);
    c
}

fn btn_combo(btn: Button) -> Combo {
    let mut c = Combo::new();
    c.add_btn(btn);
    c
}

/// C++ `hg::Config` module — every config key as a `LinkedValue` (the C++
/// `X` macro's function-local statics), plus the module-level `sizeX`,
/// `sizeY` and `uneligibilityReason` state.
#[derive(Clone)]
pub struct Config {
    /// C++ `root()` — the source-of-truth JSON object (unknown keys kept).
    root: Obj,

    // ------------------------------------------------------------------
    // `X_LINKEDVALUES` — non-bind keys, in C++ declaration order
    // ------------------------------------------------------------------
    official: LinkedValue<bool>,
    no_pulse: LinkedValue<bool>,
    no_rotation: LinkedValue<bool>,
    no_background: LinkedValue<bool>,
    no_sound: LinkedValue<bool>,
    no_music: LinkedValue<bool>,
    black_and_white: LinkedValue<bool>,
    pulse_enabled: LinkedValue<bool>,
    _3d_enabled: LinkedValue<bool>,
    shaders_enabled: LinkedValue<bool>,
    _3d_multiplier: LinkedValue<f32>,
    _3d_max_depth: LinkedValue<u32>,
    invincible: LinkedValue<bool>,
    auto_restart: LinkedValue<bool>,
    sound_volume: LinkedValue<f32>,
    music_volume: LinkedValue<f32>,
    flash_enabled: LinkedValue<bool>,
    zoom_factor: LinkedValue<f32>,
    pixel_multiplier: LinkedValue<i32>,
    player_speed: LinkedValue<f32>,
    player_focus_speed: LinkedValue<f32>,
    player_size: LinkedValue<f32>,
    limit_fps: LinkedValue<bool>,
    vsync: LinkedValue<bool>,
    auto_zoom_factor: LinkedValue<bool>,
    fullscreen: LinkedValue<bool>,
    windowed_auto_resolution: LinkedValue<bool>,
    fullscreen_auto_resolution: LinkedValue<bool>,
    fullscreen_width: LinkedValue<u32>,
    fullscreen_height: LinkedValue<u32>,
    windowed_width: LinkedValue<u32>,
    windowed_height: LinkedValue<u32>,
    show_messages: LinkedValue<bool>,
    debug: LinkedValue<bool>,
    beat_pulse: LinkedValue<bool>,
    show_tracked_variables: LinkedValue<bool>,
    music_speed_dm_sync: LinkedValue<bool>,
    max_fps: LinkedValue<u32>,
    antialiasing_level: LinkedValue<u32>,
    show_fps: LinkedValue<bool>,
    music_speed_mult: LinkedValue<f32>,
    draw_text_outlines: LinkedValue<bool>,
    darken_uneven_background_chunk: LinkedValue<bool>,
    rotate_to_start: LinkedValue<bool>,
    joystick_deadzone: LinkedValue<f32>,
    text_padding: LinkedValue<f32>,
    text_scaling: LinkedValue<f32>,
    timescale: LinkedValue<f32>,
    show_key_icons: LinkedValue<bool>,
    key_icons_scale: LinkedValue<f32>,
    first_time_playing: LinkedValue<bool>,
    show_level_info: LinkedValue<bool>,
    show_timer: LinkedValue<bool>,
    show_status_text: LinkedValue<bool>,
    server_ip: LinkedValue<String>,
    server_port: LinkedValue<u16>,
    server_control_port: LinkedValue<u16>,
    server_level_whitelist: LinkedValue<Vec<String>>,
    save_last_login_username: LinkedValue<bool>,
    last_login_username: LinkedValue<String>,
    show_login_at_startup: LinkedValue<bool>,
    camera_shake_multiplier: LinkedValue<f32>,
    angle_tilt_intensity: LinkedValue<f32>,
    show_player_trail: LinkedValue<bool>,
    player_trail_alpha: LinkedValue<u32>,
    player_trail_scale: LinkedValue<f32>,
    player_trail_decay: LinkedValue<f32>,
    player_trail_has_swap_color: LinkedValue<bool>,
    show_swap_particles: LinkedValue<bool>,
    play_swap_ready_sound: LinkedValue<bool>,
    show_swap_blinking_effect: LinkedValue<bool>,
    use_lua_file_cache: LinkedValue<bool>,
    disable_game_rendering: LinkedValue<bool>,

    // `X_LINKEDVALUES_BINDS_JOYSTICK` — SFML button index on controller 0
    // (`33` is the "unbound" sentinel).
    joystick_select: LinkedValue<u32>,
    joystick_exit: LinkedValue<u32>,
    joystick_focus: LinkedValue<u32>,
    joystick_swap: LinkedValue<u32>,
    joystick_force_restart: LinkedValue<u32>,
    joystick_restart: LinkedValue<u32>,
    joystick_replay: LinkedValue<u32>,
    joystick_screenshot: LinkedValue<u32>,
    joystick_next_pack: LinkedValue<u32>,
    joystick_previous_pack: LinkedValue<u32>,
    joystick_add_to_favorites: LinkedValue<u32>,
    joystick_favorites_menu: LinkedValue<u32>,

    // `X_LINKEDVALUES_BINDS_TRIGGERS`.
    t_rotate_ccw: LinkedValue<Trigger>,
    t_rotate_cw: LinkedValue<Trigger>,
    t_focus: LinkedValue<Trigger>,
    t_select: LinkedValue<Trigger>,
    t_exit: LinkedValue<Trigger>,
    t_force_restart: LinkedValue<Trigger>,
    t_restart: LinkedValue<Trigger>,
    t_replay: LinkedValue<Trigger>,
    t_screenshot: LinkedValue<Trigger>,
    t_swap: LinkedValue<Trigger>,
    t_up: LinkedValue<Trigger>,
    t_down: LinkedValue<Trigger>,
    t_next: LinkedValue<Trigger>,
    t_previous: LinkedValue<Trigger>,
    t_lua_console: LinkedValue<Trigger>,
    t_pause: LinkedValue<Trigger>,

    // module-level state (C++ `sizeX`/`sizeY`/`uneligibilityReason`).
    size_x: f32,
    size_y: f32,
    uneligibility_reason: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            root: SerdeValue::Null,
            official: LinkedValue::new("official", true),
            no_pulse: LinkedValue::new("no_pulse", false),
            no_rotation: LinkedValue::new("no_rotation", false),
            no_background: LinkedValue::new("no_background", false),
            no_sound: LinkedValue::new("no_sound", false),
            no_music: LinkedValue::new("no_music", false),
            black_and_white: LinkedValue::new("black_and_white", false),
            pulse_enabled: LinkedValue::new("pulse_enabled", true),
            _3d_enabled: LinkedValue::new("3D_enabled", true),
            shaders_enabled: LinkedValue::new("shaders_enabled", true),
            _3d_multiplier: LinkedValue::new("3D_multiplier", 1.0_f32),
            _3d_max_depth: LinkedValue::new("3D_max_depth", 100),
            invincible: LinkedValue::new("invincible", false),
            auto_restart: LinkedValue::new("auto_restart", false),
            sound_volume: LinkedValue::new("sound_volume", 100.0_f32),
            music_volume: LinkedValue::new("music_volume", 100.0_f32),
            flash_enabled: LinkedValue::new("flash_enabled", true),
            zoom_factor: LinkedValue::new("zoom_factor", 1.27_f32),
            pixel_multiplier: LinkedValue::new("pixel_multiplier", 1),
            player_speed: LinkedValue::new("player_speed", 9.45_f32),
            player_focus_speed: LinkedValue::new("player_focus_speed", 4.625_f32),
            player_size: LinkedValue::new("player_size", 7.3_f32),
            limit_fps: LinkedValue::new("limit_fps", true),
            vsync: LinkedValue::new("vsync", false),
            auto_zoom_factor: LinkedValue::new("auto_zoom_factor", true),
            fullscreen: LinkedValue::new("fullscreen", false),
            windowed_auto_resolution: LinkedValue::new("windowed_auto_resolution", false),
            fullscreen_auto_resolution: LinkedValue::new("fullscreen_auto_resolution", false),
            fullscreen_width: LinkedValue::new("fullscreen_width", 1920),
            fullscreen_height: LinkedValue::new("fullscreen_height", 1080),
            windowed_width: LinkedValue::new("windowed_width", 800),
            windowed_height: LinkedValue::new("windowed_height", 600),
            show_messages: LinkedValue::new("show_messages", true),
            debug: LinkedValue::new("debug", false),
            beat_pulse: LinkedValue::new("beatpulse_enabled", true),
            show_tracked_variables: LinkedValue::new("show_tracked_variables", true),
            music_speed_dm_sync: LinkedValue::new("music_speed_dm_sync", true),
            max_fps: LinkedValue::new("max_fps", 200),
            antialiasing_level: LinkedValue::new("antialiasing_level", 4),
            show_fps: LinkedValue::new("show_fps", false),
            music_speed_mult: LinkedValue::new("music_speed_mult", 1.0_f32),
            draw_text_outlines: LinkedValue::new("draw_text_outlines", true),
            darken_uneven_background_chunk: LinkedValue::new(
                "darken_uneven_background_chunk",
                true,
            ),
            rotate_to_start: LinkedValue::new("rotate_to_start", false),
            joystick_deadzone: LinkedValue::new("joystick_deadzone", 5.0_f32),
            text_padding: LinkedValue::new("text_padding", 8.0_f32),
            text_scaling: LinkedValue::new("text_scaling", 1.0_f32),
            timescale: LinkedValue::new("timescale", 1.0_f32),
            show_key_icons: LinkedValue::new("show_key_icons", false),
            key_icons_scale: LinkedValue::new("key_icons_scale", 0.75_f32),
            first_time_playing: LinkedValue::new("first_time_playing", true),
            show_level_info: LinkedValue::new("show_level_info", false),
            show_timer: LinkedValue::new("show_timer", true),
            show_status_text: LinkedValue::new("show_status_text", true),
            server_ip: LinkedValue::new("server_ip", "139.162.199.162".to_string()),
            server_port: LinkedValue::new("server_port", 50505),
            server_control_port: LinkedValue::new("server_control_port", 50506),
            server_level_whitelist: LinkedValue::new(
                "server_level_whitelist",
                default_server_level_whitelist(),
            ),
            save_last_login_username: LinkedValue::new("save_last_login_username", true),
            last_login_username: LinkedValue::new("last_login_username", String::new()),
            show_login_at_startup: LinkedValue::new("show_login_at_startup", false),
            camera_shake_multiplier: LinkedValue::new("camera_shake_multiplier", 1.0_f32),
            angle_tilt_intensity: LinkedValue::new("angle_tilt_intensity", 1.0_f32),
            show_player_trail: LinkedValue::new("show_player_trail", true),
            player_trail_alpha: LinkedValue::new("player_trail_alpha", 35),
            player_trail_scale: LinkedValue::new("player_trail_scale", 0.9_f32),
            player_trail_decay: LinkedValue::new("player_trail_decay", 3.0_f32),
            player_trail_has_swap_color: LinkedValue::new("player_trail_has_swap_color", true),
            show_swap_particles: LinkedValue::new("show_swap_particles", true),
            play_swap_ready_sound: LinkedValue::new("play_swap_ready_sound", true),
            show_swap_blinking_effect: LinkedValue::new("show_swap_blinking_effect", true),
            use_lua_file_cache: LinkedValue::new("use_lua_file_cache", false),
            disable_game_rendering: LinkedValue::new("disable_game_rendering", false),

            joystick_select: LinkedValue::new("j_select", 1),
            joystick_exit: LinkedValue::new("j_exit", 2),
            joystick_focus: LinkedValue::new("j_focus", 4),
            joystick_swap: LinkedValue::new("j_swap", 5),
            joystick_force_restart: LinkedValue::new("j_force_restart", 3),
            joystick_restart: LinkedValue::new("j_restart", 0),
            joystick_replay: LinkedValue::new("j_replay", 6),
            joystick_screenshot: LinkedValue::new("j_screenshot", 7),
            joystick_next_pack: LinkedValue::new("j_next", 11),
            joystick_previous_pack: LinkedValue::new("j_previous", 10),
            joystick_add_to_favorites: LinkedValue::new("j_add_favorite", 8),
            joystick_favorites_menu: LinkedValue::new("j_favorite_menu", 9),

            t_rotate_ccw: LinkedValue::new(
                "t_rotate_ccw",
                Trigger::from_combos(vec![
                    key_combo(Key::A),
                    key_combo(Key::LEFT),
                    btn_combo(Button::LEFT),
                ]),
            ),
            t_rotate_cw: LinkedValue::new(
                "t_rotate_cw",
                Trigger::from_combos(vec![
                    key_combo(Key::D),
                    key_combo(Key::RIGHT),
                    btn_combo(Button::RIGHT),
                ]),
            ),
            t_focus: LinkedValue::new(
                "t_focus",
                Trigger::from_combos(vec![key_combo(Key::L_SHIFT), btn_combo(Button::X_BUTTON1)]),
            ),
            t_select: LinkedValue::new(
                "t_select",
                Trigger::from_combos(vec![key_combo(Key::SPACE), btn_combo(Button::MIDDLE)]),
            ),
            t_exit: LinkedValue::new(
                "t_exit",
                Trigger::from_combos(vec![key_combo(Key::T), btn_combo(Button::X_BUTTON2)]),
            ),
            t_force_restart: LinkedValue::new(
                "t_force_restart",
                Trigger::from_combos(vec![key_combo(Key::UP), key_combo(Key::R)]),
            ),
            t_restart: LinkedValue::new(
                "t_restart",
                Trigger::from_combos(vec![
                    key_combo(Key::SPACE),
                    key_combo(Key::ENTER),
                    btn_combo(Button::MIDDLE),
                ]),
            ),
            t_replay: LinkedValue::new("t_replay", Trigger::from_combos(vec![key_combo(Key::Y)])),
            t_screenshot: LinkedValue::new(
                "t_screenshot",
                Trigger::from_combos(vec![key_combo(Key::F12)]),
            ),
            t_swap: LinkedValue::new(
                "t_swap",
                Trigger::from_combos(vec![key_combo(Key::SPACE), btn_combo(Button::MIDDLE)]),
            ),
            t_up: LinkedValue::new("t_up", Trigger::from_combos(vec![key_combo(Key::W)])),
            t_down: LinkedValue::new("t_down", Trigger::from_combos(vec![key_combo(Key::S)])),
            t_next: LinkedValue::new(
                "t_next",
                Trigger::from_combos(vec![key_combo(Key::PAGE_DOWN)]),
            ),
            t_previous: LinkedValue::new(
                "t_previous",
                Trigger::from_combos(vec![key_combo(Key::PAGE_UP)]),
            ),
            t_lua_console: LinkedValue::new(
                "t_lua_console",
                Trigger::from_combos(vec![key_combo(Key::F1)]),
            ),
            t_pause: LinkedValue::new("t_pause", Trigger::from_combos(vec![key_combo(Key::F2)])),

            size_x: 1500.0_f32,
            size_y: 1500.0_f32,
            uneligibility_reason: String::new(),
        }
    }
}

impl Config {
    /// C++ free-function `hg::Config::loadConfig(overrideIds)`, with the
    /// hardcoded CWD paths (`"config.json"`, `"ConfigOverrides/"`) made
    /// explicit through `base_dir` so tests can point at `_RELEASE`.
    ///
    /// C++ semantics: `config.json` missing/unreadable → `root` stays the
    /// null object (defaults); each listed override (extension-less file
    /// name) is shallow-merged into `root` top-level-key by
    /// top-level-key; then every `LinkedValue` re-syncs and empty triggers
    /// are fixed up to four unbound slots.
    pub fn load_config(
        &mut self,
        base_dir: &Path,
        override_ids: &[impl AsRef<str>],
    ) -> Result<(), JsonError> {
        // C++ `root()`: "config.json" in the CWD if present (getFromFile
        // swallows parse failures -> root stays null).
        let config_path = base_dir.join("config.json");
        if config_path.is_file() {
            match std::fs::read_to_string(&config_path) {
                Ok(contents) => match json::parse(&contents) {
                    Ok(obj) => self.root = obj,
                    Err(_) => self.root = SerdeValue::Null,
                },
                Err(_) => self.root = SerdeValue::Null,
            }
        }

        if base_dir.join("ConfigOverrides").is_dir() {
            // ssvufs::scan: single level, by ext ".json" (case-sensitive),
            // sorted by full path.
            let mut files: Vec<PathBuf> = std::fs::read_dir(base_dir.join("ConfigOverrides"))
                .into_iter()
                .flatten()
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.is_file() && p.extension().is_some_and(|x| x == "json"))
                .collect();
            files.sort();
            for path in files {
                let name = path
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                if !override_ids.iter().any(|id| id.as_ref() == name) {
                    continue;
                }
                // C++ logs "applying config override '<name>'" and uses
                // getFromFile (parse failure -> skipped, root untouched).
                let (obj, err) = json::read_file_with_errors(&path);
                if !err.is_empty() {
                    continue;
                }
                if let SerdeValue::Object(overrides) = obj {
                    match &mut self.root {
                        SerdeValue::Object(root) => {
                            for (key, value) in overrides {
                                root.insert(key.clone(), value);
                            }
                        }
                        other => *other = SerdeValue::Object(overrides),
                    }
                }
            }
        }

        self.sync_all_from_obj()?;
        self.fixup_missing_triggers();
        Ok(())
    }

    /// C++ `syncAllFromObj()`: every `LinkedValue` re-syncs from `root`.
    fn sync_all_from_obj(&mut self) -> Result<(), JsonError> {
        let root = &self.root;
        self.official.sync_from(root)?;
        self.no_pulse.sync_from(root)?;
        self.no_rotation.sync_from(root)?;
        self.no_background.sync_from(root)?;
        self.no_sound.sync_from(root)?;
        self.no_music.sync_from(root)?;
        self.black_and_white.sync_from(root)?;
        self.pulse_enabled.sync_from(root)?;
        self._3d_enabled.sync_from(root)?;
        self.shaders_enabled.sync_from(root)?;
        self._3d_multiplier.sync_from(root)?;
        self._3d_max_depth.sync_from(root)?;
        self.invincible.sync_from(root)?;
        self.auto_restart.sync_from(root)?;
        self.sound_volume.sync_from(root)?;
        self.music_volume.sync_from(root)?;
        self.flash_enabled.sync_from(root)?;
        self.zoom_factor.sync_from(root)?;
        self.pixel_multiplier.sync_from(root)?;
        self.player_speed.sync_from(root)?;
        self.player_focus_speed.sync_from(root)?;
        self.player_size.sync_from(root)?;
        self.limit_fps.sync_from(root)?;
        self.vsync.sync_from(root)?;
        self.auto_zoom_factor.sync_from(root)?;
        self.fullscreen.sync_from(root)?;
        self.windowed_auto_resolution.sync_from(root)?;
        self.fullscreen_auto_resolution.sync_from(root)?;
        self.fullscreen_width.sync_from(root)?;
        self.fullscreen_height.sync_from(root)?;
        self.windowed_width.sync_from(root)?;
        self.windowed_height.sync_from(root)?;
        self.show_messages.sync_from(root)?;
        self.debug.sync_from(root)?;
        self.beat_pulse.sync_from(root)?;
        self.show_tracked_variables.sync_from(root)?;
        self.music_speed_dm_sync.sync_from(root)?;
        self.max_fps.sync_from(root)?;
        self.antialiasing_level.sync_from(root)?;
        self.show_fps.sync_from(root)?;
        self.music_speed_mult.sync_from(root)?;
        self.draw_text_outlines.sync_from(root)?;
        self.darken_uneven_background_chunk.sync_from(root)?;
        self.rotate_to_start.sync_from(root)?;
        self.joystick_deadzone.sync_from(root)?;
        self.text_padding.sync_from(root)?;
        self.text_scaling.sync_from(root)?;
        self.timescale.sync_from(root)?;
        self.show_key_icons.sync_from(root)?;
        self.key_icons_scale.sync_from(root)?;
        self.first_time_playing.sync_from(root)?;
        self.show_level_info.sync_from(root)?;
        self.show_timer.sync_from(root)?;
        self.show_status_text.sync_from(root)?;
        self.server_ip.sync_from(root)?;
        self.server_port.sync_from(root)?;
        self.server_control_port.sync_from(root)?;
        self.server_level_whitelist.sync_from(root)?;
        self.save_last_login_username.sync_from(root)?;
        self.last_login_username.sync_from(root)?;
        self.show_login_at_startup.sync_from(root)?;
        self.camera_shake_multiplier.sync_from(root)?;
        self.angle_tilt_intensity.sync_from(root)?;
        self.show_player_trail.sync_from(root)?;
        self.player_trail_alpha.sync_from(root)?;
        self.player_trail_scale.sync_from(root)?;
        self.player_trail_decay.sync_from(root)?;
        self.player_trail_has_swap_color.sync_from(root)?;
        self.show_swap_particles.sync_from(root)?;
        self.play_swap_ready_sound.sync_from(root)?;
        self.show_swap_blinking_effect.sync_from(root)?;
        self.use_lua_file_cache.sync_from(root)?;
        self.disable_game_rendering.sync_from(root)?;
        self.joystick_select.sync_from(root)?;
        self.joystick_exit.sync_from(root)?;
        self.joystick_focus.sync_from(root)?;
        self.joystick_swap.sync_from(root)?;
        self.joystick_force_restart.sync_from(root)?;
        self.joystick_restart.sync_from(root)?;
        self.joystick_replay.sync_from(root)?;
        self.joystick_screenshot.sync_from(root)?;
        self.joystick_next_pack.sync_from(root)?;
        self.joystick_previous_pack.sync_from(root)?;
        self.joystick_add_to_favorites.sync_from(root)?;
        self.joystick_favorites_menu.sync_from(root)?;
        self.t_rotate_ccw.sync_from(root)?;
        self.t_rotate_cw.sync_from(root)?;
        self.t_focus.sync_from(root)?;
        self.t_select.sync_from(root)?;
        self.t_exit.sync_from(root)?;
        self.t_force_restart.sync_from(root)?;
        self.t_restart.sync_from(root)?;
        self.t_replay.sync_from(root)?;
        self.t_screenshot.sync_from(root)?;
        self.t_swap.sync_from(root)?;
        self.t_up.sync_from(root)?;
        self.t_down.sync_from(root)?;
        self.t_next.sync_from(root)?;
        self.t_previous.sync_from(root)?;
        self.t_lua_console.sync_from(root)?;
        self.t_pause.sync_from(root)?;
        Ok(())
    }

    /// C++ `fixupMissingTriggers()`: any trigger whose combos vector is
    /// empty is resized to 4 unbound combos.
    fn fixup_missing_triggers(&mut self) {
        for trigger in [
            &mut self.t_rotate_ccw,
            &mut self.t_rotate_cw,
            &mut self.t_focus,
            &mut self.t_select,
            &mut self.t_exit,
            &mut self.t_force_restart,
            &mut self.t_restart,
            &mut self.t_replay,
            &mut self.t_screenshot,
            &mut self.t_swap,
            &mut self.t_up,
            &mut self.t_down,
            &mut self.t_next,
            &mut self.t_previous,
            &mut self.t_lua_console,
            &mut self.t_pause,
        ] {
            let combos = trigger.get_mut().get_combos_mut();
            if combos.is_empty() {
                combos.resize(MAX_BINDS as usize, Combo::cleared());
            }
        }
    }

    /// C++ `syncAllToObj()`: every `LinkedValue` writes itself into
    /// `root` (unknown keys already in `root` are kept — it is the source
    /// of truth).
    fn sync_all_to_obj(&mut self) {
        // C++ archives in X-macro order; the Obj is a byte-sorted map, so
        // the written order is the same.
        let root = &mut self.root;
        sync_lv(root, &self.official);
        sync_lv(root, &self.no_pulse);
        sync_lv(root, &self.no_rotation);
        sync_lv(root, &self.no_background);
        sync_lv(root, &self.no_sound);
        sync_lv(root, &self.no_music);
        sync_lv(root, &self.black_and_white);
        sync_lv(root, &self.pulse_enabled);
        sync_lv(root, &self._3d_enabled);
        sync_lv(root, &self.shaders_enabled);
        sync_lv(root, &self._3d_multiplier);
        sync_lv(root, &self._3d_max_depth);
        sync_lv(root, &self.invincible);
        sync_lv(root, &self.auto_restart);
        sync_lv(root, &self.sound_volume);
        sync_lv(root, &self.music_volume);
        sync_lv(root, &self.flash_enabled);
        sync_lv(root, &self.zoom_factor);
        sync_lv(root, &self.pixel_multiplier);
        sync_lv(root, &self.player_speed);
        sync_lv(root, &self.player_focus_speed);
        sync_lv(root, &self.player_size);
        sync_lv(root, &self.limit_fps);
        sync_lv(root, &self.vsync);
        sync_lv(root, &self.auto_zoom_factor);
        sync_lv(root, &self.fullscreen);
        sync_lv(root, &self.windowed_auto_resolution);
        sync_lv(root, &self.fullscreen_auto_resolution);
        sync_lv(root, &self.fullscreen_width);
        sync_lv(root, &self.fullscreen_height);
        sync_lv(root, &self.windowed_width);
        sync_lv(root, &self.windowed_height);
        sync_lv(root, &self.show_messages);
        sync_lv(root, &self.debug);
        sync_lv(root, &self.beat_pulse);
        sync_lv(root, &self.show_tracked_variables);
        sync_lv(root, &self.music_speed_dm_sync);
        sync_lv(root, &self.max_fps);
        sync_lv(root, &self.antialiasing_level);
        sync_lv(root, &self.show_fps);
        sync_lv(root, &self.music_speed_mult);
        sync_lv(root, &self.draw_text_outlines);
        sync_lv(root, &self.darken_uneven_background_chunk);
        sync_lv(root, &self.rotate_to_start);
        sync_lv(root, &self.joystick_deadzone);
        sync_lv(root, &self.text_padding);
        sync_lv(root, &self.text_scaling);
        sync_lv(root, &self.timescale);
        sync_lv(root, &self.show_key_icons);
        sync_lv(root, &self.key_icons_scale);
        sync_lv(root, &self.first_time_playing);
        sync_lv(root, &self.show_level_info);
        sync_lv(root, &self.show_timer);
        sync_lv(root, &self.show_status_text);
        sync_lv(root, &self.server_ip);
        sync_lv(root, &self.server_port);
        sync_lv(root, &self.server_control_port);
        sync_lv(root, &self.server_level_whitelist);
        sync_lv(root, &self.save_last_login_username);
        sync_lv(root, &self.last_login_username);
        sync_lv(root, &self.show_login_at_startup);
        sync_lv(root, &self.camera_shake_multiplier);
        sync_lv(root, &self.angle_tilt_intensity);
        sync_lv(root, &self.show_player_trail);
        sync_lv(root, &self.player_trail_alpha);
        sync_lv(root, &self.player_trail_scale);
        sync_lv(root, &self.player_trail_decay);
        sync_lv(root, &self.player_trail_has_swap_color);
        sync_lv(root, &self.show_swap_particles);
        sync_lv(root, &self.play_swap_ready_sound);
        sync_lv(root, &self.show_swap_blinking_effect);
        sync_lv(root, &self.use_lua_file_cache);
        sync_lv(root, &self.disable_game_rendering);
        sync_lv(root, &self.joystick_select);
        sync_lv(root, &self.joystick_exit);
        sync_lv(root, &self.joystick_focus);
        sync_lv(root, &self.joystick_swap);
        sync_lv(root, &self.joystick_force_restart);
        sync_lv(root, &self.joystick_restart);
        sync_lv(root, &self.joystick_replay);
        sync_lv(root, &self.joystick_screenshot);
        sync_lv(root, &self.joystick_next_pack);
        sync_lv(root, &self.joystick_previous_pack);
        sync_lv(root, &self.joystick_add_to_favorites);
        sync_lv(root, &self.joystick_favorites_menu);
        sync_lv(root, &self.t_rotate_ccw);
        sync_lv(root, &self.t_rotate_cw);
        sync_lv(root, &self.t_focus);
        sync_lv(root, &self.t_select);
        sync_lv(root, &self.t_exit);
        sync_lv(root, &self.t_force_restart);
        sync_lv(root, &self.t_restart);
        sync_lv(root, &self.t_replay);
        sync_lv(root, &self.t_screenshot);
        sync_lv(root, &self.t_swap);
        sync_lv(root, &self.t_up);
        sync_lv(root, &self.t_down);
        sync_lv(root, &self.t_next);
        sync_lv(root, &self.t_previous);
        sync_lv(root, &self.t_lua_console);
        sync_lv(root, &self.t_pause);
    }

    /// C++ `saveConfig()` → `writeToFile(root(), "config.json")` with the
    /// CWD path made explicit.
    pub fn save_config(&mut self, path: &Path) -> std::io::Result<()> {
        self.sync_all_to_obj();
        std::fs::write(path, json::write_to_string(&self.root))
    }

    /// `saveConfig`'s output as a string (the StyledStreamWriter format,
    /// byte-sorted keys, floats via the fork's `%#.16g` rules).
    #[must_use]
    pub fn save_to_string(&mut self) -> String {
        self.sync_all_to_obj();
        json::write_to_string(&self.root)
    }

    /// C++ `resetConfigToDefaults()` (the `resetAllFromDefault()` part;
    /// the `reapplyResolution()` part needs the desktop-mode seam, see
    /// [`Config::reapply_resolution`]).
    pub fn reset_config_to_defaults(&mut self) {
        reset_lv(&mut self.official);
        reset_lv(&mut self.no_pulse);
        reset_lv(&mut self.no_rotation);
        reset_lv(&mut self.no_background);
        reset_lv(&mut self.no_sound);
        reset_lv(&mut self.no_music);
        reset_lv(&mut self.black_and_white);
        reset_lv(&mut self.pulse_enabled);
        reset_lv(&mut self._3d_enabled);
        reset_lv(&mut self.shaders_enabled);
        reset_lv(&mut self._3d_multiplier);
        reset_lv(&mut self._3d_max_depth);
        reset_lv(&mut self.invincible);
        reset_lv(&mut self.auto_restart);
        reset_lv(&mut self.sound_volume);
        reset_lv(&mut self.music_volume);
        reset_lv(&mut self.flash_enabled);
        reset_lv(&mut self.zoom_factor);
        reset_lv(&mut self.pixel_multiplier);
        reset_lv(&mut self.player_speed);
        reset_lv(&mut self.player_focus_speed);
        reset_lv(&mut self.player_size);
        reset_lv(&mut self.limit_fps);
        reset_lv(&mut self.vsync);
        reset_lv(&mut self.auto_zoom_factor);
        reset_lv(&mut self.fullscreen);
        reset_lv(&mut self.windowed_auto_resolution);
        reset_lv(&mut self.fullscreen_auto_resolution);
        reset_lv(&mut self.fullscreen_width);
        reset_lv(&mut self.fullscreen_height);
        reset_lv(&mut self.windowed_width);
        reset_lv(&mut self.windowed_height);
        reset_lv(&mut self.show_messages);
        reset_lv(&mut self.debug);
        reset_lv(&mut self.beat_pulse);
        reset_lv(&mut self.show_tracked_variables);
        reset_lv(&mut self.music_speed_dm_sync);
        reset_lv(&mut self.max_fps);
        reset_lv(&mut self.antialiasing_level);
        reset_lv(&mut self.show_fps);
        reset_lv(&mut self.music_speed_mult);
        reset_lv(&mut self.draw_text_outlines);
        reset_lv(&mut self.darken_uneven_background_chunk);
        reset_lv(&mut self.rotate_to_start);
        reset_lv(&mut self.joystick_deadzone);
        reset_lv(&mut self.text_padding);
        reset_lv(&mut self.text_scaling);
        reset_lv(&mut self.timescale);
        reset_lv(&mut self.show_key_icons);
        reset_lv(&mut self.key_icons_scale);
        reset_lv(&mut self.first_time_playing);
        reset_lv(&mut self.show_level_info);
        reset_lv(&mut self.show_timer);
        reset_lv(&mut self.show_status_text);
        reset_lv(&mut self.server_ip);
        reset_lv(&mut self.server_port);
        reset_lv(&mut self.server_control_port);
        reset_lv(&mut self.server_level_whitelist);
        reset_lv(&mut self.save_last_login_username);
        reset_lv(&mut self.last_login_username);
        reset_lv(&mut self.show_login_at_startup);
        reset_lv(&mut self.camera_shake_multiplier);
        reset_lv(&mut self.angle_tilt_intensity);
        reset_lv(&mut self.show_player_trail);
        reset_lv(&mut self.player_trail_alpha);
        reset_lv(&mut self.player_trail_scale);
        reset_lv(&mut self.player_trail_decay);
        reset_lv(&mut self.player_trail_has_swap_color);
        reset_lv(&mut self.show_swap_particles);
        reset_lv(&mut self.play_swap_ready_sound);
        reset_lv(&mut self.show_swap_blinking_effect);
        reset_lv(&mut self.use_lua_file_cache);
        reset_lv(&mut self.disable_game_rendering);
    }

    /// C++ `resetBindsToDefaults()` — the 28 bind `LinkedValue`s only.
    pub fn reset_binds_to_defaults(&mut self) {
        reset_lv(&mut self.joystick_select);
        reset_lv(&mut self.joystick_exit);
        reset_lv(&mut self.joystick_focus);
        reset_lv(&mut self.joystick_swap);
        reset_lv(&mut self.joystick_force_restart);
        reset_lv(&mut self.joystick_restart);
        reset_lv(&mut self.joystick_replay);
        reset_lv(&mut self.joystick_screenshot);
        reset_lv(&mut self.joystick_next_pack);
        reset_lv(&mut self.joystick_previous_pack);
        reset_lv(&mut self.joystick_add_to_favorites);
        reset_lv(&mut self.joystick_favorites_menu);
        reset_lv(&mut self.t_rotate_ccw);
        reset_lv(&mut self.t_rotate_cw);
        reset_lv(&mut self.t_focus);
        reset_lv(&mut self.t_select);
        reset_lv(&mut self.t_exit);
        reset_lv(&mut self.t_force_restart);
        reset_lv(&mut self.t_restart);
        reset_lv(&mut self.t_replay);
        reset_lv(&mut self.t_screenshot);
        reset_lv(&mut self.t_swap);
        reset_lv(&mut self.t_up);
        reset_lv(&mut self.t_down);
        reset_lv(&mut self.t_next);
        reset_lv(&mut self.t_previous);
        reset_lv(&mut self.t_lua_console);
        reset_lv(&mut self.t_pause);
    }

    // ------------------------------------------------------------------
    // Score eligibility
    // ------------------------------------------------------------------

    /// C++ `isEligibleForScore()` — note it funnels through the
    /// official-locked getters (so e.g. "debug mode on" only fires when
    /// official mode is off), and sets `uneligibilityReason`.
    pub fn is_eligible_for_score(&mut self) -> bool {
        if !self.get_official() {
            self.uneligibility_reason = "official mode off".into();
            return false;
        }
        if self.get_spawn_distance() != DEFAULT_SPAWN_DISTANCE {
            self.uneligibility_reason = "spawn distance modified".into();
            return false;
        }
        if self.get_debug() {
            self.uneligibility_reason = "debug mode on".into();
            return false;
        }
        if !self.get_auto_zoom_factor() {
            self.uneligibility_reason = "modified zoom factor".into();
            return false;
        }
        if self.get_player_speed() != *self.player_speed.get_default() {
            self.uneligibility_reason = "player speed modified".into();
            return false;
        }
        if self.get_player_focus_speed() != *self.player_focus_speed.get_default() {
            self.uneligibility_reason = "player focus speed modified".into();
            return false;
        }
        if self.get_player_size() != *self.player_size.get_default() {
            self.uneligibility_reason = "player size modified".into();
            return false;
        }
        if self.get_invincible() {
            self.uneligibility_reason = "invincibility on".into();
            return false;
        }
        if self.get_no_pulse() {
            self.uneligibility_reason = "pulse off".into();
            return false;
        }
        if self.get_no_rotation() {
            self.uneligibility_reason = "rotation off".into();
            return false;
        }
        true
    }

    // ------------------------------------------------------------------
    // Resolution / window (pure-value parts; window ops are issue 10)
    // ------------------------------------------------------------------

    /// C++ `reapplyResolution()` with the desktop-mode read made a
    /// parameter (C++ `sf::VideoMode::getDesktopMode()`).
    pub fn reapply_resolution(&mut self, desktop_width: u32, desktop_height: u32) {
        if self.get_windowed_auto_resolution() {
            self.windowed_width.set(desktop_width);
            self.windowed_height.set(desktop_height);
        }
        if self.get_fullscreen_auto_resolution() {
            self.fullscreen_width.set(desktop_width);
            self.fullscreen_height.set(desktop_height);
        }
        self.recalculate_sizes();
    }

    /// C++ `recalculateSizes()`.
    pub fn recalculate_sizes(&mut self) {
        let max_side = self.get_width().max(self.get_height()) as f32;
        self.size_x = max_side * 1.3_f32;
        self.size_y = self.size_x;

        if !self.get_auto_zoom_factor() {
            return;
        }
        let factor_x = 1024.0_f32 / self.get_width() as f32;
        let factor_y = 768.0_f32 / self.get_height() as f32;
        self.zoom_factor.set(factor_x.max(factor_y));
    }

    /// C++ `setFullscreen(window, mFullscreen)` (window ops omitted).
    pub fn set_fullscreen(&mut self, fullscreen: bool) {
        self.fullscreen.set(fullscreen);
        self.recalculate_sizes();
    }

    /// C++ `setCurrentResolution(mWidth, mHeight)`.
    pub fn set_current_resolution(&mut self, width: u32, height: u32) {
        if self.get_fullscreen() {
            self.fullscreen_auto_resolution.set(false);
            self.fullscreen_width.set(width);
            self.fullscreen_height.set(height);
        } else {
            self.windowed_auto_resolution.set(false);
            self.windowed_width.set(width);
            self.windowed_height.set(height);
        }
        self.recalculate_sizes();
    }

    /// C++ `setCurrentResolutionAuto(window)` (window ops omitted), with
    /// the desktop-mode read made a parameter.
    pub fn set_current_resolution_auto(&mut self, desktop_width: u32, desktop_height: u32) {
        if *self.fullscreen.get() {
            self.fullscreen_auto_resolution.set(true);
            self.fullscreen_width.set(desktop_width);
            self.fullscreen_height.set(desktop_height);
        } else {
            self.windowed_auto_resolution.set(true);
            self.windowed_width.set(desktop_width);
            self.windowed_height.set(desktop_height);
        }
        self.recalculate_sizes();
    }

    /// C++ `setVsync(window, mValue)` (window op omitted).
    pub fn set_vsync(&mut self, value: bool) {
        self.vsync.set(value);
    }

    /// C++ `setLimitFPS(window, mValue)` (window op omitted).
    pub fn set_limit_fps(&mut self, value: bool) {
        self.limit_fps.set(value);
    }

    /// C++ `setMaxFPS(window, mValue)` (window op omitted).
    pub fn set_max_fps(&mut self, value: u32) {
        self.max_fps.set(value);
    }

    /// C++ `setAntialiasingLevel(window, mValue)` — only touches the value
    /// when it actually changes (window op omitted).
    pub fn set_antialiasing_level(&mut self, value: u32) {
        if value != self.get_antialiasing_level() {
            self.antialiasing_level.set(value);
        }
    }
}

impl Config {
    // ------------------------------------------------------------------
    // Getters (C++ names `getX` -> `get_x`; official-locked ones return
    // the default while `getOfficial()` is true — mapping §6.2)
    // ------------------------------------------------------------------

    #[must_use]
    pub fn get_official(&self) -> bool {
        *self.official.get()
    }

    #[must_use]
    pub fn get_uneligibility_reason(&self) -> &str {
        &self.uneligibility_reason
    }

    #[must_use]
    pub fn get_size_x(&self) -> f32 {
        self.size_x
    }

    #[must_use]
    pub fn get_size_y(&self) -> f32 {
        self.size_y
    }

    #[must_use]
    pub fn get_spawn_distance(&self) -> f32 {
        DEFAULT_SPAWN_DISTANCE
    }

    #[must_use]
    pub fn get_zoom_factor(&self) -> f32 {
        *self.zoom_factor.get()
    }

    #[must_use]
    pub fn get_pixel_multiplier(&self) -> i32 {
        *self.pixel_multiplier.get()
    }

    #[must_use]
    pub fn get_player_speed(&self) -> f32 {
        if self.get_official() {
            *self.player_speed.get_default()
        } else {
            *self.player_speed.get()
        }
    }

    #[must_use]
    pub fn get_player_focus_speed(&self) -> f32 {
        if self.get_official() {
            *self.player_focus_speed.get_default()
        } else {
            *self.player_focus_speed.get()
        }
    }

    #[must_use]
    pub fn get_player_size(&self) -> f32 {
        if self.get_official() {
            *self.player_size.get_default()
        } else {
            *self.player_size.get()
        }
    }

    #[must_use]
    pub fn get_no_pulse(&self) -> bool {
        if self.get_official() {
            *self.no_pulse.get_default()
        } else {
            *self.no_pulse.get()
        }
    }

    #[must_use]
    pub fn get_no_rotation(&self) -> bool {
        if self.get_official() {
            *self.no_rotation.get_default()
        } else {
            *self.no_rotation.get()
        }
    }

    #[must_use]
    pub fn get_no_background(&self) -> bool {
        if self.get_official() {
            *self.no_background.get_default()
        } else {
            *self.no_background.get()
        }
    }

    #[must_use]
    pub fn get_black_and_white(&self) -> bool {
        if self.get_official() {
            *self.black_and_white.get_default()
        } else {
            *self.black_and_white.get()
        }
    }

    #[must_use]
    pub fn get_no_sound(&self) -> bool {
        *self.no_sound.get()
    }

    #[must_use]
    pub fn get_no_music(&self) -> bool {
        *self.no_music.get()
    }

    #[must_use]
    pub fn get_sound_volume(&self) -> f32 {
        *self.sound_volume.get()
    }

    #[must_use]
    pub fn get_music_volume(&self) -> f32 {
        *self.music_volume.get()
    }

    #[must_use]
    pub fn get_limit_fps(&self) -> bool {
        *self.limit_fps.get()
    }

    #[must_use]
    pub fn get_vsync(&self) -> bool {
        *self.vsync.get()
    }

    #[must_use]
    pub fn get_auto_zoom_factor(&self) -> bool {
        if self.get_official() {
            *self.auto_zoom_factor.get_default()
        } else {
            *self.auto_zoom_factor.get()
        }
    }

    #[must_use]
    pub fn get_fullscreen(&self) -> bool {
        *self.fullscreen.get()
    }

    #[must_use]
    pub fn get_windowed_auto_resolution(&self) -> bool {
        *self.windowed_auto_resolution.get()
    }

    #[must_use]
    pub fn get_fullscreen_auto_resolution(&self) -> bool {
        *self.fullscreen_auto_resolution.get()
    }

    #[must_use]
    pub fn get_fullscreen_width(&self) -> u32 {
        *self.fullscreen_width.get()
    }

    #[must_use]
    pub fn get_fullscreen_height(&self) -> u32 {
        *self.fullscreen_height.get()
    }

    #[must_use]
    pub fn get_windowed_width(&self) -> u32 {
        *self.windowed_width.get()
    }

    #[must_use]
    pub fn get_windowed_height(&self) -> u32 {
        *self.windowed_height.get()
    }

    #[must_use]
    pub fn get_width(&self) -> u32 {
        if self.get_fullscreen() {
            self.get_fullscreen_width()
        } else {
            self.get_windowed_width()
        }
    }

    #[must_use]
    pub fn get_height(&self) -> u32 {
        if self.get_fullscreen() {
            self.get_fullscreen_height()
        } else {
            self.get_windowed_height()
        }
    }

    #[must_use]
    pub fn get_show_messages(&self) -> bool {
        *self.show_messages.get()
    }

    #[must_use]
    pub fn get_debug(&self) -> bool {
        if self.get_official() {
            *self.debug.get_default()
        } else {
            *self.debug.get()
        }
    }

    #[must_use]
    pub fn get_beat_pulse(&self) -> bool {
        if self.get_official() {
            *self.beat_pulse.get_default()
        } else {
            *self.beat_pulse.get()
        }
    }

    #[must_use]
    pub fn get_invincible(&self) -> bool {
        if self.get_official() {
            *self.invincible.get_default()
        } else {
            *self.invincible.get()
        }
    }

    #[must_use]
    pub fn get_3d(&self) -> bool {
        *self._3d_enabled.get()
    }

    #[must_use]
    pub fn get_shaders(&self) -> bool {
        *self.shaders_enabled.get()
    }

    #[must_use]
    pub fn get_3d_multiplier(&self) -> f32 {
        *self._3d_multiplier.get()
    }

    #[must_use]
    pub fn get_3d_max_depth(&self) -> u32 {
        *self._3d_max_depth.get()
    }

    #[must_use]
    pub fn get_auto_restart(&self) -> bool {
        *self.auto_restart.get()
    }

    #[must_use]
    pub fn get_flash(&self) -> bool {
        *self.flash_enabled.get()
    }

    #[must_use]
    pub fn get_show_tracked_variables(&self) -> bool {
        *self.show_tracked_variables.get()
    }

    #[must_use]
    pub fn get_music_speed_dm_sync(&self) -> bool {
        *self.music_speed_dm_sync.get()
    }

    #[must_use]
    pub fn get_max_fps(&self) -> u32 {
        *self.max_fps.get()
    }

    #[must_use]
    pub fn get_antialiasing_level(&self) -> u32 {
        *self.antialiasing_level.get()
    }

    #[must_use]
    pub fn get_show_fps(&self) -> bool {
        *self.show_fps.get()
    }

    #[must_use]
    pub fn get_music_speed_mult(&self) -> f32 {
        *self.music_speed_mult.get()
    }

    #[must_use]
    pub fn get_draw_text_outlines(&self) -> bool {
        *self.draw_text_outlines.get()
    }

    #[must_use]
    pub fn get_darken_uneven_background_chunk(&self) -> bool {
        *self.darken_uneven_background_chunk.get()
    }

    #[must_use]
    pub fn get_rotate_to_start(&self) -> bool {
        *self.rotate_to_start.get()
    }

    #[must_use]
    pub fn get_joystick_deadzone(&self) -> f32 {
        *self.joystick_deadzone.get()
    }

    #[must_use]
    pub fn get_text_padding(&self) -> f32 {
        *self.text_padding.get()
    }

    #[must_use]
    pub fn get_text_scaling(&self) -> f32 {
        *self.text_scaling.get()
    }

    #[must_use]
    pub fn get_timescale(&self) -> f32 {
        if self.get_official() {
            *self.timescale.get_default()
        } else {
            *self.timescale.get()
        }
    }

    #[must_use]
    pub fn get_show_key_icons(&self) -> bool {
        *self.show_key_icons.get()
    }

    #[must_use]
    pub fn get_key_icons_scale(&self) -> f32 {
        *self.key_icons_scale.get()
    }

    #[must_use]
    pub fn get_first_time_playing(&self) -> bool {
        *self.first_time_playing.get()
    }

    #[must_use]
    pub fn get_show_level_info(&self) -> bool {
        *self.show_level_info.get()
    }

    #[must_use]
    pub fn get_show_timer(&self) -> bool {
        if self.get_official() {
            *self.show_timer.get_default()
        } else {
            *self.show_timer.get()
        }
    }

    #[must_use]
    pub fn get_show_status_text(&self) -> bool {
        if self.get_official() {
            *self.show_status_text.get_default()
        } else {
            *self.show_status_text.get()
        }
    }

    #[must_use]
    pub fn get_server_ip(&self) -> &str {
        self.server_ip.get()
    }

    #[must_use]
    pub fn get_server_port(&self) -> u16 {
        *self.server_port.get()
    }

    #[must_use]
    pub fn get_server_control_port(&self) -> u16 {
        *self.server_control_port.get()
    }

    #[must_use]
    pub fn get_server_level_whitelist(&self) -> &Vec<String> {
        self.server_level_whitelist.get()
    }

    #[must_use]
    pub fn get_save_last_login_username(&self) -> bool {
        *self.save_last_login_username.get()
    }

    #[must_use]
    pub fn get_last_login_username(&self) -> &str {
        self.last_login_username.get()
    }

    #[must_use]
    pub fn get_show_login_at_startup(&self) -> bool {
        *self.show_login_at_startup.get()
    }

    #[must_use]
    pub fn get_camera_shake_multiplier(&self) -> f32 {
        *self.camera_shake_multiplier.get()
    }

    #[must_use]
    pub fn get_angle_tilt_intensity(&self) -> f32 {
        *self.angle_tilt_intensity.get()
    }

    #[must_use]
    pub fn get_show_player_trail(&self) -> bool {
        *self.show_player_trail.get()
    }

    #[must_use]
    pub fn get_player_trail_alpha(&self) -> u32 {
        *self.player_trail_alpha.get()
    }

    #[must_use]
    pub fn get_player_trail_scale(&self) -> f32 {
        *self.player_trail_scale.get()
    }

    #[must_use]
    pub fn get_player_trail_decay(&self) -> f32 {
        *self.player_trail_decay.get()
    }

    #[must_use]
    pub fn get_player_trail_has_swap_color(&self) -> bool {
        *self.player_trail_has_swap_color.get()
    }

    #[must_use]
    pub fn get_show_swap_particles(&self) -> bool {
        *self.show_swap_particles.get()
    }

    #[must_use]
    pub fn get_play_swap_ready_sound(&self) -> bool {
        *self.play_swap_ready_sound.get()
    }

    #[must_use]
    pub fn get_show_swap_blinking_effect(&self) -> bool {
        *self.show_swap_blinking_effect.get()
    }

    #[must_use]
    pub fn get_use_lua_file_cache(&self) -> bool {
        *self.use_lua_file_cache.get()
    }

    #[must_use]
    pub fn get_disable_game_rendering(&self) -> bool {
        *self.disable_game_rendering.get()
    }

    // ------------------------------------------------------------------
    // Setters (C++ `setX` -> `set_x`)
    // ------------------------------------------------------------------

    pub fn set_official(&mut self, official: bool) {
        self.official.set(official);
    }

    pub fn set_debug(&mut self, debug: bool) {
        self.debug.set(debug);
    }

    pub fn set_no_pulse(&mut self, no_pulse: bool) {
        self.no_pulse.set(no_pulse);
    }

    pub fn set_no_rotation(&mut self, no_rotation: bool) {
        self.no_rotation.set(no_rotation);
    }

    pub fn set_no_background(&mut self, no_background: bool) {
        self.no_background.set(no_background);
    }

    pub fn set_black_and_white(&mut self, black_and_white: bool) {
        self.black_and_white.set(black_and_white);
    }

    pub fn set_no_sound(&mut self, no_sound: bool) {
        self.no_sound.set(no_sound);
    }

    pub fn set_no_music(&mut self, no_music: bool) {
        self.no_music.set(no_music);
    }

    pub fn set_3d(&mut self, enabled: bool) {
        self._3d_enabled.set(enabled);
    }

    pub fn set_shaders(&mut self, enabled: bool) {
        self.shaders_enabled.set(enabled);
    }

    pub fn set_invincible(&mut self, invincible: bool) {
        self.invincible.set(invincible);
    }

    pub fn set_auto_restart(&mut self, auto_restart: bool) {
        self.auto_restart.set(auto_restart);
    }

    pub fn set_sound_volume(&mut self, volume: f32) {
        self.sound_volume.set(volume);
    }

    pub fn set_music_volume(&mut self, volume: f32) {
        self.music_volume.set(volume);
    }

    pub fn set_flash(&mut self, flash: bool) {
        self.flash_enabled.set(flash);
    }

    pub fn set_music_speed_dm_sync(&mut self, value: bool) {
        self.music_speed_dm_sync.set(value);
    }

    pub fn set_show_fps(&mut self, value: bool) {
        self.show_fps.set(value);
    }

    pub fn set_music_speed_mult(&mut self, value: f32) {
        self.music_speed_mult.set(value);
    }

    pub fn set_draw_text_outlines(&mut self, enabled: bool) {
        self.draw_text_outlines.set(enabled);
    }

    pub fn set_darken_uneven_background_chunk(&mut self, enabled: bool) {
        self.darken_uneven_background_chunk.set(enabled);
    }

    pub fn set_rotate_to_start(&mut self, enabled: bool) {
        self.rotate_to_start.set(enabled);
    }

    pub fn set_joystick_deadzone(&mut self, value: f32) {
        self.joystick_deadzone.set(value);
    }

    pub fn set_text_padding(&mut self, value: f32) {
        self.text_padding.set(value);
    }

    pub fn set_text_scaling(&mut self, value: f32) {
        self.text_scaling.set(value);
    }

    pub fn set_timescale(&mut self, value: f32) {
        self.timescale.set(value);
    }

    pub fn set_show_key_icons(&mut self, enabled: bool) {
        self.show_key_icons.set(enabled);
    }

    pub fn set_key_icons_scale(&mut self, value: f32) {
        self.key_icons_scale.set(value);
    }

    pub fn set_first_time_playing(&mut self, value: bool) {
        self.first_time_playing.set(value);
    }

    pub fn set_show_level_info(&mut self, value: bool) {
        self.show_level_info.set(value);
    }

    pub fn set_show_timer(&mut self, value: bool) {
        self.show_timer.set(value);
    }

    pub fn set_show_status_text(&mut self, value: bool) {
        self.show_status_text.set(value);
    }

    pub fn set_server_ip(&mut self, value: &str) {
        self.server_ip.set(value.to_string());
    }

    pub fn set_server_port(&mut self, value: u16) {
        self.server_port.set(value);
    }

    pub fn set_server_control_port(&mut self, value: u16) {
        self.server_control_port.set(value);
    }

    pub fn set_server_level_whitelist(&mut self, level_validators: Vec<String>) {
        self.server_level_whitelist.set(level_validators);
    }

    pub fn set_save_last_login_username(&mut self, value: bool) {
        self.save_last_login_username.set(value);
    }

    pub fn set_last_login_username(&mut self, value: &str) {
        self.last_login_username.set(value.to_string());
    }

    pub fn set_show_login_at_startup(&mut self, value: bool) {
        self.show_login_at_startup.set(value);
    }

    pub fn set_camera_shake_multiplier(&mut self, value: f32) {
        self.camera_shake_multiplier.set(value);
    }

    pub fn set_angle_tilt_intensity(&mut self, value: f32) {
        self.angle_tilt_intensity.set(value);
    }

    pub fn set_show_player_trail(&mut self, value: bool) {
        self.show_player_trail.set(value);
    }

    pub fn set_player_trail_alpha(&mut self, value: u32) {
        self.player_trail_alpha.set(value);
    }

    pub fn set_player_trail_scale(&mut self, value: f32) {
        self.player_trail_scale.set(value);
    }

    pub fn set_player_trail_decay(&mut self, value: f32) {
        self.player_trail_decay.set(value);
    }

    pub fn set_player_trail_has_swap_color(&mut self, value: bool) {
        self.player_trail_has_swap_color.set(value);
    }

    pub fn set_show_swap_particles(&mut self, value: bool) {
        self.show_swap_particles.set(value);
    }

    pub fn set_play_swap_ready_sound(&mut self, value: bool) {
        self.play_swap_ready_sound.set(value);
    }

    pub fn set_show_swap_blinking_effect(&mut self, value: bool) {
        self.show_swap_blinking_effect.set(value);
    }

    pub fn set_use_lua_file_cache(&mut self, value: bool) {
        self.use_lua_file_cache.set(value);
    }

    pub fn set_disable_game_rendering(&mut self, value: bool) {
        self.disable_game_rendering.set(value);
    }

    // ------------------------------------------------------------------
    // Joystick bind values
    // ------------------------------------------------------------------

    /// C++ `joystickTriggerGetters[i]()` by `Jid` index.
    #[must_use]
    pub fn joystick_bind(&self, jid: Jid) -> u32 {
        match jid {
            Jid::Select => *self.joystick_select.get(),
            Jid::Exit => *self.joystick_exit.get(),
            Jid::Focus => *self.joystick_focus.get(),
            Jid::Swap => *self.joystick_swap.get(),
            Jid::ForceRestart => *self.joystick_force_restart.get(),
            Jid::Restart => *self.joystick_restart.get(),
            Jid::Replay => *self.joystick_replay.get(),
            Jid::Screenshot => *self.joystick_screenshot.get(),
            Jid::NextPack => *self.joystick_next_pack.get(),
            Jid::PreviousPack => *self.joystick_previous_pack.get(),
            Jid::AddToFavorites => *self.joystick_add_to_favorites.get(),
            Jid::FavoritesMenu => *self.joystick_favorites_menu.get(),
            Jid::Unknown => 0,
            Jid::JoystickBindsCount => 0,
        }
    }

    /// C++ `joystickTriggerSetters[i](button)` by `Jid` index.
    pub fn set_joystick_bind_value(&mut self, jid: Jid, button: u32) {
        match jid {
            Jid::Select => self.joystick_select.set(button),
            Jid::Exit => self.joystick_exit.set(button),
            Jid::Focus => self.joystick_focus.set(button),
            Jid::Swap => self.joystick_swap.set(button),
            Jid::ForceRestart => self.joystick_force_restart.set(button),
            Jid::Restart => self.joystick_restart.set(button),
            Jid::Replay => self.joystick_replay.set(button),
            Jid::Screenshot => self.joystick_screenshot.set(button),
            Jid::NextPack => self.joystick_next_pack.set(button),
            Jid::PreviousPack => self.joystick_previous_pack.set(button),
            Jid::AddToFavorites => self.joystick_add_to_favorites.set(button),
            Jid::FavoritesMenu => self.joystick_favorites_menu.set(button),
            Jid::Unknown | Jid::JoystickBindsCount => {}
        }
    }

    // ------------------------------------------------------------------
    // Keyboard/mouse binds
    // ------------------------------------------------------------------

    /// C++ `getTrigger(tid)` via the `triggerGetters` array.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    pub fn get_trigger(&mut self, tid: Tid) -> &mut Trigger {
        match tid {
            Tid::RotateCCW => &mut self.t_rotate_ccw,
            Tid::RotateCW => &mut self.t_rotate_cw,
            Tid::Focus => &mut self.t_focus,
            Tid::Select => &mut self.t_select,
            Tid::Exit => &mut self.t_exit,
            Tid::ForceRestart => &mut self.t_force_restart,
            Tid::Restart => &mut self.t_restart,
            Tid::Replay => &mut self.t_replay,
            Tid::Screenshot => &mut self.t_screenshot,
            Tid::Swap => &mut self.t_swap,
            Tid::Up => &mut self.t_up,
            Tid::Down => &mut self.t_down,
            Tid::NextPack => &mut self.t_next,
            Tid::PreviousPack => &mut self.t_previous,
            Tid::LuaConsole => &mut self.t_lua_console,
            Tid::Pause => &mut self.t_pause,
            Tid::Unknown | Tid::TriggersCount => panic!("bad Tid {tid:?}"),
        }
        .get_mut()
    }

    /// Const view of a trigger by `Tid` (C++ `triggerGetters[i]() const`
    /// path used by `getKeyboardBindNames` and the BindControls).
    #[must_use]
    pub fn trigger_ref(&self, tid: Tid) -> &Trigger {
        self.trigger_ref_impl(tid)
    }

    /// C++ "`triggerGetter()` returns by value" — a copy of the trigger
    /// (used by `KeyboardBindControl::new` / `erase` / `newKeyboardBind`,
    /// which must not alias the config while mutating it).
    #[must_use]
    pub fn trigger_copy(&self, tid: Tid) -> Trigger {
        self.trigger_ref_impl(tid).clone()
    }

    fn trigger_ref_impl(&self, tid: Tid) -> &Trigger {
        match tid {
            Tid::RotateCCW => &self.t_rotate_ccw,
            Tid::RotateCW => &self.t_rotate_cw,
            Tid::Focus => &self.t_focus,
            Tid::Select => &self.t_select,
            Tid::Exit => &self.t_exit,
            Tid::ForceRestart => &self.t_force_restart,
            Tid::Restart => &self.t_restart,
            Tid::Replay => &self.t_replay,
            Tid::Screenshot => &self.t_screenshot,
            Tid::Swap => &self.t_swap,
            Tid::Up => &self.t_up,
            Tid::Down => &self.t_down,
            Tid::NextPack => &self.t_next,
            Tid::PreviousPack => &self.t_previous,
            Tid::LuaConsole => &self.t_lua_console,
            Tid::Pause => &self.t_pause,
            Tid::Unknown | Tid::TriggersCount => panic!("bad Tid {tid:?}"),
        }
        .get()
    }

    /// C++ `keyboardBindsSanityCheck()`: `resizeTrigger` on the first 12
    /// triggers (RotateCCW..Down) — NOT on NextPack/PreviousPack/
    /// LuaConsole/Pause.
    pub fn keyboard_binds_sanity_check(&mut self) {
        for tid in [
            Tid::RotateCCW,
            Tid::RotateCW,
            Tid::Focus,
            Tid::Select,
            Tid::Exit,
            Tid::ForceRestart,
            Tid::Restart,
            Tid::Replay,
            Tid::Screenshot,
            Tid::Swap,
            Tid::Up,
            Tid::Down,
        ] {
            resize_trigger(self.get_trigger(tid));
        }
    }

    /// C++ `getJoystickBindName(bindID)`; `vendorId` is
    /// `sf::Joystick::getIdentification(0).vendorId` when a controller is
    /// connected, else 0 (the SFML seam).
    #[must_use]
    pub fn get_joystick_bind_name(&self, bind_id: Jid, vendor_id: u32) -> String {
        const BUTTONS_NAMES: [[&str; 2]; 12] = [
            ["A", "SQUARE"],
            ["B", "CROSS"],
            ["X", "CIRCLE"],
            ["Y", "TRIANGLE"],
            ["LB", "L1"],
            ["RB", "R1"],
            ["BACK", "L2"],
            ["START", "R2"],
            ["LEFT STICK", "SELECT"],
            ["RIGHT STICK", "START"],
            ["LT", "LEFT STICK"],
            ["RT", "RIGHT STICK"],
        ];
        const MS_VENDOR_ID: u32 = 0x045E;
        const SONY_VENDOR_ID: u32 = 0x54C;

        let value = self.joystick_bind(bind_id);

        if value == 33 {
            return String::new();
        }

        if vendor_id == MS_VENDOR_ID {
            return if value >= 12 {
                String::new()
            } else {
                BUTTONS_NAMES[value as usize][0].to_string()
            };
        }

        if vendor_id == SONY_VENDOR_ID {
            return if value >= 12 {
                String::new()
            } else {
                BUTTONS_NAMES[value as usize][1].to_string()
            };
        }

        value.to_string()
    }

    /// C++ `loadAllJoystickBinds()` — feeds `Joystick::setJoystickBind`
    /// from the 12 config values.
    pub fn load_all_joystick_binds(&self) {
        for (i, jid) in [
            Jid::Select,
            Jid::Exit,
            Jid::Focus,
            Jid::Swap,
            Jid::ForceRestart,
            Jid::Restart,
            Jid::Replay,
            Jid::Screenshot,
            Jid::NextPack,
            Jid::PreviousPack,
            Jid::AddToFavorites,
            Jid::FavoritesMenu,
        ]
        .iter()
        .enumerate()
        {
            joystick::set_joystick_bind(self.joystick_bind(*jid), i as i32);
        }
    }

    /// C++ `joystickBindsSanityCheck()` — declared in `Config.hpp` but
    /// never defined in `Config.cpp` (dead API; calling it would be a link
    /// error in C++). The port keeps the name as a documented no-op.
    pub fn joystick_binds_sanity_check(&mut self) {
        // no-op: C++ has no definition.
    }
}

/// C++ `resizeTrigger(trig)` (file-local in `Config.cpp`): drop unbound
/// combos (agglomerate), pop from the back while `> maxBinds`, pad with
/// unbound combos while `< maxBinds`.
pub fn resize_trigger(trig: &mut Trigger) {
    let combos = trig.get_combos_mut();
    // Remove empty slots to agglomerate all binds close to each other.
    combos.retain(|c| !c.is_unbound());
    // if the config has more binds than are supported
    while combos.len() > MAX_BINDS as usize {
        combos.pop();
    }
    // if the config has less binds fill the spots with unbound combos
    while combos.len() < MAX_BINDS as usize {
        combos.push(Combo::cleared());
    }
}

/// C++ `rebindTrigger(trig, key, btn, index)` — `key` is the `KKey` int
/// (-1 for "not a key"), `btn` the `MBtn` int.
pub fn rebind_trigger(trig: &mut Trigger, key: i32, btn: i32, mut index: i32) {
    // if both slots are taken replace the first one
    if index >= MAX_BINDS {
        index = 0;
        trig.get_combos_mut()[0].clear_bind();
    }

    if key > -1 {
        trig.get_combos_mut()[index as usize].add_key(Key(key));
    } else {
        trig.get_combos_mut()[index as usize].add_btn(Button(btn));
    }
}

/// C++ `clearTriggerBind(trig, index)`.
pub fn clear_trigger_bind(trig: &mut Trigger, index: i32) {
    trig.get_combos_mut()[index as usize].clear_bind();
}

/// C++ `bindToHumanReadableName(s)`.
#[must_use]
pub fn bind_to_human_readable_name(s: &str) -> String {
    if let Some(rest) = s.strip_prefix('k') {
        rest.to_string()
    } else if s == "bLeft" {
        "LMB".to_string()
    } else if s == "bRight" {
        "RMB".to_string()
    } else if s == "bMiddle" {
        "MMB".to_string()
    } else {
        s.to_string()
    }
}

impl Config {
    /// C++ `getKeyboardBindNames(bindID)`: first set key then first set
    /// button per active combo (both loops always run — a combo with a key
    /// AND a button names both), names passed through
    /// `bindToHumanReadableName`, joined by `", "`, upper-cased at the end
    /// (C++ `std::toupper` per char = ASCII uppercasify).
    #[must_use]
    pub fn get_keyboard_bind_names(&self, bind_id: Tid) -> String {
        let mut bind_names = String::new();

        for c in self.trigger_ref(bind_id).get_combos() {
            if c.is_unbound() {
                break;
            }

            let key_binds = c.get_keys();
            let mut j = 0;
            while j <= Key::KEY_COUNT {
                if key_binds[j as usize] {
                    if !bind_names.is_empty() {
                        bind_names.push_str(", ");
                    }
                    // names are shifted compared to the Key enum
                    bind_names.push_str(&bind_to_human_readable_name(
                        crate::triggers::get_kkey_name(Key(j - 1)),
                    ));
                    break;
                }
                j += 1;
            }

            let btn_binds = c.get_btns();
            let mut j = 0;
            while j <= Button::BUTTON_COUNT {
                if btn_binds[j as usize] {
                    if !bind_names.is_empty() {
                        bind_names.push_str(", ");
                    }
                    // same as with keys
                    bind_names.push_str(&bind_to_human_readable_name(
                        crate::triggers::get_mbtn_name(Button(j - 1)),
                    ));
                    break;
                }
                j += 1;
            }
        }

        bind_names.chars().map(|c| c.to_ascii_uppercase()).collect()
    }
}
