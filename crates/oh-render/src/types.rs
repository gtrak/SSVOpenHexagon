//! Small shared value types: `sf::FloatRect` and `sf::VideoMode`.

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
