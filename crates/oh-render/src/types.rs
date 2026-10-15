//! Small shared value types: `sf::FloatRect`, `sf::RectangleShape` and
//! `sf::VideoMode`.

use oh_math::hg::color::Color;
use oh_math::ssvs::vector2::Vec2;

/// `sf::FloatRect` — an axis-aligned rectangle in world/pixel units.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct FloatRect {
    pub left: f32,
    pub top: f32,
    pub width: f32,
    pub height: f32,
}

impl FloatRect {
    /// `sf::FloatRect(left, top, width, height)`.
    #[must_use]
    pub const fn new(left: f32, top: f32, width: f32, height: f32) -> Self {
        Self {
            left,
            top,
            width,
            height,
        }
    }

    /// `getLeft()`.
    #[must_use]
    pub const fn get_left(&self) -> f32 {
        self.left
    }

    /// `getTop()`.
    #[must_use]
    pub const fn get_top(&self) -> f32 {
        self.top
    }

    /// `getWidth()`.
    #[must_use]
    pub const fn get_width(&self) -> f32 {
        self.width
    }

    /// `getHeight()`.
    #[must_use]
    pub const fn get_height(&self) -> f32 {
        self.height
    }

    /// `getRight()` = `left + width`.
    #[must_use]
    pub const fn get_right(&self) -> f32 {
        self.left + self.width
    }

    /// `getBottom()` = `top + height`.
    #[must_use]
    pub const fn get_bottom(&self) -> f32 {
        self.top + self.height
    }

    /// `getCenter()`.
    #[must_use]
    pub fn get_center(&self) -> oh_math::ssvs::vector2::Vec2 {
        oh_math::ssvs::vector2::Vec2::new(
            self.left + self.width * 0.5,
            self.top + self.height * 0.5,
        )
    }

    /// `contains(point)`.
    #[must_use]
    pub fn contains(&self, p: oh_math::ssvs::vector2::Vec2) -> bool {
        p.x >= self.left && p.x <= self.get_right() && p.y >= self.top && p.y <= self.get_bottom()
    }

    // ---- corner / edge-centre accessors (SSVStart `Utils/SFML.hpp`,
    // mapping/rendering-menu.md §10.1) --------------------------------------

    /// `getGlobalNW` / `getLocalNW` — `(left, top)`.
    #[must_use]
    pub const fn get_nw(&self) -> Vec2 {
        Vec2::new(self.left, self.top)
    }

    /// `getGlobalNE` / `getLocalNE` — `(left + width, top)`.
    #[must_use]
    pub const fn get_ne(&self) -> Vec2 {
        Vec2::new(self.get_right(), self.top)
    }

    /// `getGlobalSW` / `getLocalSW` — `(left, top + height)`.
    #[must_use]
    pub const fn get_sw(&self) -> Vec2 {
        Vec2::new(self.left, self.get_bottom())
    }

    /// `getGlobalSE` / `getLocalSE` — `(left + width, top + height)`.
    #[must_use]
    pub const fn get_se(&self) -> Vec2 {
        Vec2::new(self.get_right(), self.get_bottom())
    }

    /// `getGlobalCenterN` / `getLocalCenterN` — `(cx, top)`.
    #[must_use]
    pub fn get_center_n(&self) -> Vec2 {
        Vec2::new(self.get_center().x, self.top)
    }

    /// `getGlobalCenterE` / `getLocalCenterE` — `(left + width, cy)`.
    #[must_use]
    pub fn get_center_e(&self) -> Vec2 {
        Vec2::new(self.get_right(), self.get_center().y)
    }

    /// `getGlobalCenterS` / `getLocalCenterS` — `(cx, top + height)`.
    #[must_use]
    pub fn get_center_s(&self) -> Vec2 {
        Vec2::new(self.get_center().x, self.get_bottom())
    }

    /// `getGlobalCenterW` / `getLocalCenterW` — `(left, cy)`.
    #[must_use]
    pub fn get_center_w(&self) -> Vec2 {
        Vec2::new(self.left, self.get_center().y)
    }
}

/// `sf::RectangleShape` — the transform + colour surface the game's draw code
/// uses for `levelInfoRectangle` (`HexagonGame::updateLevelInfo`, mapping
/// §4.3).
///
/// SFML `Shape` semantics: `getLocalBounds()` includes the outline thickness
/// on all sides (`{-origin.x - t, -origin.y - t, w + 2t, h + 2t}`) and
/// `getGlobalBounds()` applies `translate(position) * rotate * scale *`
/// `translate(-origin - t)`. (Level-info text positions from
/// `getGlobalNW/SE/SW` therefore depend on the outline thickness, not just
/// the size.)
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RectangleShape {
    size: Vec2,
    position: Vec2,
    origin: Vec2,
    scale: Vec2,
    rotation: f32,
    fill_color: Color,
    outline_color: Color,
    outline_thickness: f32,
}

impl Default for RectangleShape {
    /// C++ `sf::RectangleShape()` — size (10, 10), white, outline thickness 1.
    fn default() -> Self {
        Self {
            size: Vec2::new(10.0, 10.0),
            position: Vec2::new(0.0, 0.0),
            origin: Vec2::new(0.0, 0.0),
            scale: Vec2::new(1.0, 1.0),
            rotation: 0.0,
            fill_color: Color::WHITE,
            outline_color: Color::WHITE,
            outline_thickness: 1.0,
        }
    }
}

impl RectangleShape {
    /// `getSize()`.
    #[must_use]
    pub fn get_size(&self) -> Vec2 {
        self.size
    }

    /// `setSize`.
    pub fn set_size(&mut self, size: Vec2) {
        self.size = size;
    }

    /// `getPosition()`.
    #[must_use]
    pub fn get_position(&self) -> Vec2 {
        self.position
    }

    /// `setPosition`.
    pub fn set_position(&mut self, position: Vec2) {
        self.position = position;
    }

    /// `getOrigin()`.
    #[must_use]
    pub fn get_origin(&self) -> Vec2 {
        self.origin
    }

    /// `setOrigin`.
    pub fn set_origin(&mut self, origin: Vec2) {
        self.origin = origin;
    }

    /// `getScale()`.
    #[must_use]
    pub fn get_scale(&self) -> Vec2 {
        self.scale
    }

    /// `setScale`.
    pub fn set_scale(&mut self, scale: Vec2) {
        self.scale = scale;
    }

    /// `getFillColor()`.
    #[must_use]
    pub fn get_fill_color(&self) -> Color {
        self.fill_color
    }

    /// `setFillColor`.
    pub fn set_fill_color(&mut self, color: Color) {
        self.fill_color = color;
    }

    /// `getOutlineColor()`.
    #[must_use]
    pub fn get_outline_color(&self) -> Color {
        self.outline_color
    }

    /// `setOutlineColor`.
    pub fn set_outline_color(&mut self, color: Color) {
        self.outline_color = color;
    }

    /// `getOutlineThickness()`.
    #[must_use]
    pub fn get_outline_thickness(&self) -> f32 {
        self.outline_thickness
    }

    /// `setOutlineThickness`.
    pub fn set_outline_thickness(&mut self, thickness: f32) {
        self.outline_thickness = thickness;
    }

    /// `getLocalBounds()` — includes the outline thickness on all sides.
    #[must_use]
    pub fn get_local_bounds(&self) -> FloatRect {
        let t = self.outline_thickness;
        FloatRect::new(
            -self.origin.x - t,
            -self.origin.y - t,
            self.size.x + 2.0 * t,
            self.size.y + 2.0 * t,
        )
    }

    /// `getGlobalBounds()` — SFML `Shape` transforms the **geometry** rect
    /// expanded by the outline (`[-t, w+t] x [-t, h+t]`) through
    /// `translate(position) * rotate(rotation) * scale(scale) *`
    /// `translate(-origin - outlineThickness)`, as a bounding box.
    /// (Note: this is NOT `getLocalBounds()` through the transform — the
    /// local-bounds rect already carries the origin offset once.)
    #[must_use]
    pub fn get_global_bounds(&self) -> FloatRect {
        let t = self.outline_thickness;
        let corners = [
            Vec2::new(-t, -t),
            Vec2::new(self.size.x + t, -t),
            Vec2::new(-t, self.size.y + t),
            Vec2::new(self.size.x + t, self.size.y + t),
        ];
        let mut min_x = f32::INFINITY;
        let mut min_y = f32::INFINITY;
        let mut max_x = f32::NEG_INFINITY;
        let mut max_y = f32::NEG_INFINITY;
        for c in corners {
            let shifted = c - Vec2::new(
                self.origin.x + self.outline_thickness,
                self.origin.y + self.outline_thickness,
            );
            let scaled = Vec2::new(shifted.x * self.scale.x, shifted.y * self.scale.y);
            let rad = oh_math::ssvu::math::to_rad(self.rotation);
            let (sin, cos) = rad.sin_cos();
            let rotated = Vec2::new(
                scaled.x * cos - scaled.y * sin,
                scaled.x * sin + scaled.y * cos,
            );
            let p = Vec2::new(self.position.x + rotated.x, self.position.y + rotated.y);
            min_x = min_x.min(p.x);
            min_y = min_y.min(p.y);
            max_x = max_x.max(p.x);
            max_y = max_y.max(p.y);
        }
        FloatRect::new(min_x, min_y, max_x - min_x, max_y - min_y)
    }
}

/// `sf::VideoMode` — a pixel resolution (+ optional refresh rate).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VideoMode {
    pub width: u32,
    pub height: u32,
    pub refresh: u32,
}

impl VideoMode {
    /// `sf::VideoMode(width, height)` (60 Hz default refresh, matching SFML).
    #[must_use]
    pub const fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            refresh: 60,
        }
    }

    /// The C++ `ssvs::GameWindow` default (`width{640}, height{480}`).
    #[must_use]
    pub const fn default_window() -> Self {
        Self {
            width: 640,
            height: 480,
            refresh: 60,
        }
    }
}
