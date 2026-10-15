//! Port of `include/SSVOpenHexagon/Components/SpeedData.hpp` — `hg::SpeedData`.
//!
//! All members are `float` (`f32`); formulas copied verbatim (D-000).

use oh_math::FT;

/// `hg::SpeedData` — a speed that accelerates towards a bound and optionally
/// ping-pongs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpeedData {
    /// `_speed` (public in C++; read as `speed.speed` by the game).
    pub speed: f32,
    /// `_accel` (public in C++).
    pub accel: f32,
    min: f32,
    max: f32,
    ping_pong: f32,
}

impl SpeedData {
    /// C++ `explicit SpeedData(float speed = 0, float accel = 0.f,
    /// float min = 0.f, float max = 0.f, bool pingPong = false)`.
    ///
    /// `_pingPong` stores `-1.f` when `ping_pong` is true, else `1.f`.
    #[must_use]
    pub fn new(speed: f32, accel: f32, min: f32, max: f32, ping_pong: bool) -> Self {
        Self {
            speed,
            accel,
            min,
            max,
            ping_pong: if ping_pong { -1.0f32 } else { 1.0f32 },
        }
    }

    /// Default-constructed `SpeedData{}` (all bounds/speed zero, no ping-pong).
    #[must_use]
    pub const fn default() -> Self {
        Self {
            speed: 0.0,
            accel: 0.0,
            min: 0.0,
            max: 0.0,
            ping_pong: 1.0,
        }
    }

    /// C++ `update(const ssvu::FT ft)`.
    ///
    /// Order of checks: `_speed > _max` first, then `else if _speed < _min`.
    /// When `accel == 0` the update is a no-op.
    pub fn update(&mut self, ft: FT) {
        if self.accel == 0.0f32 {
            return;
        }

        self.speed += self.accel * ft;

        if self.speed > self.max {
            self.speed = self.max;
            self.accel *= self.ping_pong;
        } else if self.speed < self.min {
            self.speed = self.min;
            self.accel *= self.ping_pong;
        }
    }
}
