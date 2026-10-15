//! Port of `extlibs/SSVStart/include/SSVStart/Utils/Vector2.hpp`.
//!
//! `Vec2` mirrors `sf::Vector2f` usage in the game: a plain pair of `f32`
//! components named `x` / `y`.

/// `sf::Vector2f` analogue: two `f32` components.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

impl Vec2 {
    /// `sf::Vector2f(x, y)` ctor.
    #[must_use]
    pub fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}
