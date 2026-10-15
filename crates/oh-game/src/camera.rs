//! Minimal `ssvs::Camera` port — the exact surface the game *update* loop
//! uses (plan 001 issue 09). The full camera (deferred `apply()`/`computedView`,
//! mouse mapping) belongs to `oh-render` (issue 10).
//!
//! Ported from `extlibs/SSVStart/include/SSVStart/Camera/Camera.hpp`
//! (AFL-3.0), reading the pinned copy in-tree:
//!
//! * `turn`/`pan` are DEFERRED: they accumulate into `nextRotation` /
//!   `nextPan` and only take effect in `update(ft)`.
//! * `set_view` / `set_center` / `set_rotation` / `set_skew` apply
//!   IMMEDIATELY.
//! * `update(ft)`: `center += nextPan`; `view.zoom(pow(nextZoomFactor, ft))`
//!   (SFML `View::zoom` DIVIDES the view size by the factor);
//!   `view.rotate(degrees(nextRotation * ft))`; then the deltas reset.
//!
//! Rotation is stored directly in **degrees** (`f32`): the C++ round-trips
//! through SFML's radian `Angle` (`setRotation(degrees(d))` /
//! `getRotation().asDegrees()`), while this port keeps the degree value the
//! game loop actually consumes (`refreshPulse` preserves the rotation
//! across a view reset; `updateRotation` turns by degrees/FT). The
//! camera only ever runs on the windowed path, which no test of this issue
//! exercises — replay state (the RNG, the player, the walls) never reads
//! the camera.

use oh_math::ssvs::Vec2;

/// `ssvs::Camera` (update-loop surface).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Camera {
    center: Vec2,
    size: Vec2,
    rotation_deg: f32,
    skew: Vec2,
    next_pan: Vec2,
    next_zoom_factor: f32,
    next_rotation_deg: f32,
    invalid: bool,
}

impl Camera {
    /// `Camera(const sf::View& mView)` — view with the given center/size.
    #[must_use]
    pub fn new(center: Vec2, size: Vec2) -> Self {
        Self {
            center,
            size,
            rotation_deg: 0.0,
            skew: Vec2::new(1.0, 1.0),
            next_pan: Vec2::new(0.0, 0.0),
            next_zoom_factor: 1.0,
            next_rotation_deg: 0.0,
            invalid: false,
        }
    }

    /// `turn(mDeg)` — changes the view ON NEXT UPDATE.
    pub fn turn(&mut self, m_deg: f32) {
        self.next_rotation_deg += m_deg;
        self.invalid = true;
    }

    /// `pan(mVec)` — changes the view ON NEXT UPDATE.
    pub fn pan(&mut self, m_vec: Vec2) {
        self.next_pan += m_vec;
        self.invalid = true;
    }

    /// `update(mFT)` — applies the deferred deltas.
    pub fn update(&mut self, m_ft: f32) {
        if !self.invalid {
            return;
        }
        self.center += self.next_pan;
        // C++: `view.zoom(std::pow(nextZoomFactor, mFT))`; SFML
        // `View::zoom(factor)` divides the size by the factor (guarding
        // `factor != 0`).
        let factor = self.next_zoom_factor.powf(m_ft);
        if factor != 0.0 {
            self.size.x /= factor;
            self.size.y /= factor;
        }
        // C++: `view.rotate(sf::degrees(nextRotation * mFT))`.
        self.rotation_deg += self.next_rotation_deg * m_ft;

        self.next_pan = Vec2::new(0.0, 0.0);
        self.next_zoom_factor = 1.0;
        self.next_rotation_deg = 0.0;
        self.invalid = false;
    }

    /// `setView(mView)` — IMMEDIATE.
    pub fn set_view(&mut self, center: Vec2, size: Vec2) {
        self.center = center;
        self.size = size;
    }

    /// `setRotation(mDeg)` — IMMEDIATE (degrees).
    pub fn set_rotation(&mut self, m_deg: f32) {
        self.rotation_deg = m_deg;
    }

    /// `getRotation().asDegrees()`.
    #[must_use]
    pub fn get_rotation(&self) -> f32 {
        self.rotation_deg
    }

    /// `setCenter(mPosition)` — IMMEDIATE.
    pub fn set_center(&mut self, m_position: Vec2) {
        self.center = m_position;
    }

    /// `getCenter()`.
    #[must_use]
    pub fn get_center(&self) -> Vec2 {
        self.center
    }

    /// `getCenter()` copy-alias used by the flash-of-centre reset.
    #[must_use]
    pub fn get_size(&self) -> Vec2 {
        self.size
    }

    /// `setSkew(mSkew)` — IMMEDIATE (`{1, 1}` = no skew).
    pub fn set_skew(&mut self, m_skew: Vec2) {
        self.skew = m_skew;
    }

    /// `getSkew()`.
    #[must_use]
    pub fn get_skew(&self) -> Vec2 {
        self.skew
    }
}
