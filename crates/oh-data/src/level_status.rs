//! Port of `include/SSVOpenHexagon/Data/LevelStatus.hpp` +
//! `src/SSVOpenHexagon/Data/LevelStatus.cpp`.
//!
//! Runtime per-level state (NOT loaded from JSON — mutated by Lua).
//! Defaults below are the C++ in-class defaults; field order matches the
//! C++ declaration order.

/// C++ `hg::LevelStatus`.
#[derive(Debug, Clone, PartialEq)]
pub struct LevelStatus {
    // `std::unordered_map<std::string, std::string>` — ported as
    // `BTreeMap` (unordered order is unspecified anyway).
    pub tracked_variables: std::collections::BTreeMap<String, String>,

    // Allows alternative scoring to be possible
    pub score_overridden: bool,
    pub score_override: String,

    // Music and sound related attributes
    pub sync_music_to_dm: bool,
    pub music_pitch: f32,
    pub beep_sound: String,
    pub level_up_sound: String,
    pub swap_sound: String,
    pub death_sound: String,

    pub speed_mult: f32,
    pub player_speed_mult: f32,
    pub speed_inc: f32,
    pub speed_max: f32,
    pub rotation_speed: f32,
    pub rotation_speed_inc: f32,
    pub rotation_speed_max: f32,
    pub delay_mult: f32,
    pub delay_inc: f32,
    pub delay_min: f32,
    pub delay_max: f32,
    pub fast_spin: f32,
    pub inc_time: f32,
    pub pulse_min: f32,
    pub pulse_max: f32,
    pub pulse_speed: f32,
    pub pulse_speed_r: f32,
    pub pulse_delay_max: f32,
    pub pulse_initial_delay: f32,
    pub swap_cooldown_mult: f32,

    // ------------------------------------------------------------------------
    // A "beat pulse" controls the size of the center polygon. It is supposed
    // to match the beat of the music.
    pub beat_pulse_initial_delay: f32, // Initial delay of the beat pulse.
    pub beat_pulse_max: f32,           // Max size increment of the polygon.
    pub beat_pulse_delay_max: f32,     // Delay between beat pulses.
    pub beat_pulse_speed_mult: f32,    // How fast the pulse "moves" back.

    pub radius_min: f32,
    pub wall_skew_left: f32,
    pub wall_skew_right: f32,
    pub wall_angle_left: f32,
    pub wall_angle_right: f32,
    pub wall_spawn_distance: f32,

    pub camera_shake: f32,

    pub sides: u32,
    pub sides_max: u32,
    pub sides_min: u32,

    pub swap_enabled: bool,
    pub tutorial_mode: bool,
    pub _3d_required: bool,
    pub shaders_required: bool,
    pub inc_enabled: bool,
    pub rnd_side_changes_enabled: bool,
    pub darken_uneven_background_chunk: bool,

    pub manual_pulse_control: bool,
    pub manual_beat_pulse_control: bool,

    // C++ `unsigned long` = 64-bit on the target.
    pub current_increments: u64,
}

impl Default for LevelStatus {
    /// C++ default member initializers.
    fn default() -> Self {
        Self {
            tracked_variables: std::collections::BTreeMap::new(),
            score_overridden: false,
            score_override: String::new(),
            sync_music_to_dm: true,
            music_pitch: 1.0,
            beep_sound: "beep.ogg".into(),
            level_up_sound: "increment.ogg".into(),
            swap_sound: "swap.ogg".into(),
            death_sound: "death.ogg".into(),
            speed_mult: 1.0,
            player_speed_mult: 1.0,
            speed_inc: 0.0,
            speed_max: 0.0,
            rotation_speed: 0.0,
            rotation_speed_inc: 0.0,
            rotation_speed_max: 0.0,
            delay_mult: 1.0,
            delay_inc: 0.0,
            delay_min: 0.0,
            delay_max: 0.0,
            fast_spin: 0.0,
            inc_time: 15.0,
            pulse_min: 75.0,
            pulse_max: 80.0,
            pulse_speed: 0.0,
            pulse_speed_r: 0.0,
            pulse_delay_max: 0.0,
            pulse_initial_delay: 0.0,
            swap_cooldown_mult: 1.0,
            beat_pulse_initial_delay: 0.0,
            beat_pulse_max: 0.0,
            beat_pulse_delay_max: 0.0,
            beat_pulse_speed_mult: 1.0,
            radius_min: 72.0,
            wall_skew_left: 0.0,
            wall_skew_right: 0.0,
            wall_angle_left: 0.0,
            wall_angle_right: 0.0,
            wall_spawn_distance: 1600.0,
            camera_shake: 0.0,
            sides: 6,
            sides_max: 6,
            sides_min: 6,
            swap_enabled: false,
            tutorial_mode: false,
            _3d_required: false,
            shaders_required: false,
            inc_enabled: true,
            rnd_side_changes_enabled: true,
            darken_uneven_background_chunk: true,
            manual_pulse_control: false,
            manual_beat_pulse_control: false,
            current_increments: 0,
        }
    }
}

impl LevelStatus {
    /// C++ `LevelStatus(mSyncMusicToDM, mWallSpawnDistance)` — overrides
    /// only those two fields.
    #[must_use = "mirrors the C++ [[nodiscard]]"]
    pub fn new(m_sync_music_to_dm: bool, m_wall_spawn_distance: f32) -> Self {
        LevelStatus {
            sync_music_to_dm: m_sync_music_to_dm,
            wall_spawn_distance: m_wall_spawn_distance,
            ..Default::default()
        }
    }

    /// C++ `hasSpeedMaxLimit()` = `speedMax > 0.f`.
    #[must_use = "mirrors the C++ [[nodiscard]]"]
    pub fn has_speed_max_limit(&self) -> bool {
        self.speed_max > 0.0
    }

    /// C++ `hasDelayMaxLimit()` = `delayMax > 0.f`.
    #[must_use = "mirrors the C++ [[nodiscard]]"]
    pub fn has_delay_max_limit(&self) -> bool {
        self.delay_max > 0.0
    }
}
