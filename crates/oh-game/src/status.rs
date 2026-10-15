//! Port of `include/SSVOpenHexagon/Core/HGStatus.hpp` +
//! `src/SSVOpenHexagon/Core/HGStatus.cpp` — `hg::HexagonGameStatus`.
//!
//! Time accumulators and pause values are `double` (`f64`); the pulse/radius
//! visual fields are `float` (`f32`); `ft` is passed in as `double`. Conversion
//! factor is always `60.0` (60 FT-units per second).

use crate::clock::HRTimePoint;

/// `hg::StateChange`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateChange {
    None,
    MustRestart,
    MustReplay,
}

/// `hg::RenderStage` (used to size `fragmentShaderIds`; `Count == 9`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderStage {
    BackgroundTris = 0,
    WallQuads3D = 1,
    PivotQuads3D = 2,
    PlayerTris3D = 3,
    WallQuads = 4,
    CapTris = 5,
    PivotQuads = 6,
    PlayerTris = 7,
    Text = 8,
}

/// `hg::HexagonGameStatus::FlashColor` (`{ int r, g, b }`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FlashColor {
    pub r: i32,
    pub g: i32,
    pub b: i32,
}

/// `hg::HexagonGameStatus`.
#[derive(Debug, Clone, PartialEq)]
pub struct HexagonGameStatus {
    // Private accumulators / timers (all `double` / `f64`).
    total_frametime_accumulator: f64,
    played_frametime_accumulator: f64,
    paused_frametime_accumulator: f64,
    current_pause: f64,
    current_increment_time: f64,
    // `customScore` is a `float` (f32).
    custom_score: f32,

    // Public visual / control fields (floats).
    pub pulse: f32,
    pub pulse_direction: f32,
    pub pulse_delay: f32,
    pub beat_pulse: f32,
    pub beat_pulse_delay: f32,
    pub pulse_3d: f32,
    pub pulse_3d_direction: f32,
    pub flash_effect: f32,
    pub radius: f32,
    pub fast_spin: f32,
    pub camera_shake: f32,

    // Public flags / misc.
    pub has_died: bool,
    pub must_state_change: StateChange,
    pub score_invalid: bool,
    pub invalid_reason: String,
    pub started: bool,
    pub restart_input: String,
    pub replay_input: String,
    pub show_player_trail: bool,
    pub fragment_shader_ids: [Option<u64>; 9],
}

impl Default for HexagonGameStatus {
    /// C++ value-initialisation (all public fields take their documented
    /// defaults; `currentPause` is re-primed by `resetTime`, but the struct
    /// default is `0.1 * 60` too — see the field initializers).
    fn default() -> Self {
        Self {
            total_frametime_accumulator: 0.0,
            played_frametime_accumulator: 0.0,
            paused_frametime_accumulator: 0.0,
            current_pause: 0.1 * 60.0,
            current_increment_time: 0.0,
            custom_score: 0.0,

            pulse: 75.0,
            pulse_direction: 1.0,
            pulse_delay: 0.0,
            beat_pulse: 0.0,
            beat_pulse_delay: 0.0,
            pulse_3d: 1.0,
            pulse_3d_direction: 1.0,
            flash_effect: 0.0,
            radius: 75.0,
            fast_spin: 0.0,
            camera_shake: 0.0,

            has_died: false,
            must_state_change: StateChange::None,
            score_invalid: false,
            invalid_reason: String::new(),
            started: false,
            restart_input: String::new(),
            replay_input: String::new(),
            show_player_trail: true,
            fragment_shader_ids: [None; 9],
        }
    }
}

impl HexagonGameStatus {
    /// Fresh status (equivalent to the C++ `HexagonGameStatus{}`).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// `start()` — reset time + custom score, mark started.
    pub fn start(&mut self) {
        self.reset_time();
        self.custom_score = 0.0;
        self.started = true;
    }

    /// `getIncrementTimeSeconds()` — `currentIncrementTime / 60.0`.
    #[must_use]
    pub fn get_increment_time_seconds(&self) -> f64 {
        self.current_increment_time / 60.0
    }

    /// `getTimeSeconds()` — `getPlayedAccumulatedFrametimeInSeconds()`.
    #[must_use]
    pub fn get_time_seconds(&self) -> f64 {
        self.get_played_accumulated_frametime_in_seconds()
    }

    /// `getCurrentTP()` — time point at `ms((int64)(totalSeconds * 1000.0))`.
    #[must_use]
    pub fn get_current_tp(&self) -> HRTimePoint {
        let ms = (self.get_total_accumulated_frametime_in_seconds() * 1000.0) as i64;
        HRTimePoint::from_millis(ms as u64)
    }

    /// `getTimeTP()` — time point at `ms((int64)(playedSeconds * 1000.0))`.
    #[must_use]
    pub fn get_time_tp(&self) -> HRTimePoint {
        let ms = (self.get_played_accumulated_frametime_in_seconds() * 1000.0) as i64;
        HRTimePoint::from_millis(ms as u64)
    }

    /// `getLevelStartTP()` — the epoch.
    #[must_use]
    pub fn get_level_start_tp(&self) -> HRTimePoint {
        HRTimePoint::ZERO
    }

    /// `isTimePaused()` — `currentPause > 0.0`.
    #[must_use]
    pub fn is_time_paused(&self) -> bool {
        self.current_pause > 0.0
    }

    /// `pauseTime(seconds)` — `currentPause += seconds * 60.0`.
    pub fn pause_time(&mut self, seconds: f64) {
        self.current_pause += seconds * 60.0;
    }

    /// `resetIncrementTime()` — `currentIncrementTime = 0.0`.
    pub fn reset_increment_time(&mut self) {
        self.current_increment_time = 0.0;
    }

    /// `resetTime()` — zeroes the accumulators and re-primes `currentPause`
    /// to `0.1 * 60` (a 0.1 s initial pause).
    pub fn reset_time(&mut self) {
        self.total_frametime_accumulator = 0.0;
        self.played_frametime_accumulator = 0.0;
        self.paused_frametime_accumulator = 0.0;
        self.current_pause = 0.1 * 60.0;
        self.current_increment_time = 0.0;
    }

    /// `accumulateFrametime(ft)` — `ft` is a `double`.
    ///
    /// The whole frame counts toward the *total* accumulator; if time is
    /// paused the frame is *entirely* treated as paused (any over-run of
    /// `currentPause` is dropped — the remainder code is commented out in
    /// the C++). Otherwise it counts toward the played + increment times.
    pub fn accumulate_frametime(&mut self, ft: f64) {
        self.total_frametime_accumulator += ft;

        if self.current_pause > 0.0 {
            self.current_pause -= ft;
        } else {
            self.played_frametime_accumulator += ft;
            self.current_increment_time += ft;
        }
    }

    /// `updateCustomScore(score)` — `customScore = score`.
    pub fn update_custom_score(&mut self, score: f32) {
        self.custom_score = score;
    }

    /// `getTotalAccumulatedFrametime()`.
    #[must_use]
    pub fn get_total_accumulated_frametime(&self) -> f64 {
        self.total_frametime_accumulator
    }

    /// `getTotalAccumulatedFrametimeInSeconds()` — total / 60.0.
    #[must_use]
    pub fn get_total_accumulated_frametime_in_seconds(&self) -> f64 {
        self.get_total_accumulated_frametime() / 60.0
    }

    /// `getPlayedAccumulatedFrametime()`.
    #[must_use]
    pub fn get_played_accumulated_frametime(&self) -> f64 {
        self.played_frametime_accumulator
    }

    /// `getPlayedAccumulatedFrametimeInSeconds()` — played / 60.0.
    #[must_use]
    pub fn get_played_accumulated_frametime_in_seconds(&self) -> f64 {
        self.get_played_accumulated_frametime() / 60.0
    }

    /// `getPausedAccumulatedFrametime()` (dead accumulator: always 0 in the
    /// C++ — the getter is retained for API parity).
    #[must_use]
    pub fn get_paused_accumulated_frametime(&self) -> f64 {
        self.paused_frametime_accumulator
    }

    /// `getPausedAccumulatedFrametimeInSeconds()` — paused / 60.0.
    #[must_use]
    pub fn get_paused_accumulated_frametime_in_seconds(&self) -> f64 {
        self.get_paused_accumulated_frametime() / 60.0
    }

    /// `getCustomScore()`.
    #[must_use]
    pub fn get_custom_score(&self) -> f32 {
        self.custom_score
    }
}
