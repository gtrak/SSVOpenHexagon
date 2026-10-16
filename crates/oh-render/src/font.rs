//! `sf::Font` + `sf::Text` — TTF metrics, glyph rasterization, and text layout.
//!
//! Backed by `fontdue` 0.9 (pure-Rust TTF; glyph bitmaps become the GL
//! glyph atlas on a live context — `crate::glyphs`'s layout +
//! `crate::gl_passes`'s upload, issue-025c SG3). The layout math (character
//! size, origin, scale, rotation, fill/outline colour + thickness, and the
//! `getLocalBounds` / `getGlobalBounds` distinction the `drawText_*` code
//! relies on) lives here and is headless-testable.
//!
//! See mapping/rendering-menu.md §3.9–§4.3 (font sizing: `getScaledCharacterSize
//! (size) = unsigned(size / zoomFactor * textScaling)`; `getLocalBounds` is the
//! untransformed glyph-bounds rect; `getGlobalBounds` = local bounds through
//! position + origin + scale + rotation).

use std::path::Path;
use std::rc::Rc;

use oh_math::ssvs::vector2::Vec2;

use crate::error::RenderError;
use crate::types::FloatRect;

/// `sf::Font` — a loaded TrueType font (`fontdue` internally; the `Rc` lets
/// many `Text`s share one font the way the C++ shares `font` / `fontBold`).
#[derive(Clone, Debug)]
pub struct Font {
    inner: Rc<fontdue::Font>,
}

impl Font {
    /// `sf::Font::loadFromFile(path)`.
    ///
    /// # Errors
    /// [`RenderError::Io`] if the file cannot be read; [`RenderError::
    /// InvalidData`] if it is not a valid TTF.
    pub fn load(path: &Path) -> Result<Self, RenderError> {
        let bytes = std::fs::read(path)?;
        Self::from_bytes(&bytes)
    }

    /// `sf::Font::loadFromMemory(data, size)`.
    ///
    /// # Errors
    /// [`RenderError::InvalidData`] if the bytes are not a valid TTF.
    pub fn from_bytes(data: &[u8]) -> Result<Self, RenderError> {
        let inner = fontdue::Font::from_bytes(data, fontdue::FontSettings::default())
            .map_err(|_| RenderError::InvalidData)?;
        Ok(Self {
            inner: Rc::new(inner),
        })
    }

    /// The underlying `fontdue` font (metrics / rasterization entry points).
    #[must_use]
    pub fn inner(&self) -> &fontdue::Font {
        &self.inner
    }

    /// A stable per-instance identity (the `Rc` allocation address) — the
    /// `gl_text` glyph cache's font key. The game loads its fonts once at
    /// startup, so the identity is stable for the cache's life.
    #[must_use]
    pub fn id(&self) -> usize {
        Rc::as_ptr(&self.inner) as usize
    }

    /// `fontdue::horizontal_line_metrics(px)` — ascent / descent / line gap.
    #[must_use]
    pub fn line_metrics(&self, px: f32) -> Option<fontdue::LineMetrics> {
        self.inner.horizontal_line_metrics(px)
    }

    /// `fontdue::metrics(char, px)` — per-glyph metrics.
    #[must_use]
    pub fn glyph_metrics(&self, character: char, px: f32) -> fontdue::Metrics {
        self.inner.metrics(character, px)
    }

    /// `fontdue::rasterize(char, px)` — the glyph's alpha bitmap.
    pub fn rasterize(&self, character: char, px: f32) -> (fontdue::Metrics, Vec<u8>) {
        self.inner.rasterize(character, px)
    }
}

/// `sf::Text` — a string rendered with a font + transform + colours.
///
/// The GL text pass (issue-025c SG3, `crate::gl_passes`) consumes this
/// through the `get_*` surface + [`Self::transform_point`] (the C++
/// `sf::Text::draw`'s local-glyph geometry, `crate::glyphs`'s layout pin).
#[derive(Clone, Debug)]
pub struct Text {
    font: Font,
    string: String,
    character_size: u32,
    position: Vec2,
    origin: Vec2,
    scale: Vec2,
    rotation: f32,
    fill_color: super::Color,
    outline_color: super::Color,
    outline_thickness: f32,
}

impl Text {
    /// `sf::Text(string, font, characterSize)` (SFML's default char size is 30).
    #[must_use]
    pub fn new(font: Font, string: impl Into<String>, character_size: u32) -> Self {
        Self {
            font,
            string: string.into(),
            character_size,
            position: Vec2::new(0.0, 0.0),
            origin: Vec2::new(0.0, 0.0),
            scale: Vec2::new(1.0, 1.0),
            rotation: 0.0,
            fill_color: super::Color {
                r: 255,
                g: 255,
                b: 255,
                a: 255,
            },
            outline_color: super::Color {
                r: 0,
                g: 0,
                b: 0,
                a: 255,
            },
            outline_thickness: 0.0,
        }
    }

    /// `setFont`.
    pub fn set_font(&mut self, font: Font) {
        self.font = font;
    }

    /// `getFont()`.
    #[must_use]
    pub fn get_font(&self) -> &Font {
        &self.font
    }

    /// `setString`.
    pub fn set_string(&mut self, s: impl Into<String>) {
        self.string = s.into();
    }

    /// `getString()`.
    #[must_use]
    pub fn get_string(&self) -> &str {
        &self.string
    }

    /// `string().isEmpty()` (the `drawTextMessagePBImpl` guard).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.string.is_empty()
    }

    /// `setCharacterSize`.
    pub fn set_character_size(&mut self, size: u32) {
        self.character_size = size;
    }

    /// `getCharacterSize()`.
    #[must_use]
    pub fn get_character_size(&self) -> u32 {
        self.character_size
    }

    pub fn set_position(&mut self, position: Vec2) {
        self.position = position;
    }
    #[must_use]
    pub fn get_position(&self) -> Vec2 {
        self.position
    }

    pub fn set_origin(&mut self, origin: Vec2) {
        self.origin = origin;
    }
    #[must_use]
    pub fn get_origin(&self) -> Vec2 {
        self.origin
    }

    pub fn set_scale(&mut self, scale: Vec2) {
        self.scale = scale;
    }
    #[must_use]
    pub fn get_scale(&self) -> Vec2 {
        self.scale
    }

    /// `setRotation(sf::degrees(deg))`.
    pub fn set_rotation(&mut self, deg: f32) {
        self.rotation = deg;
    }
    #[must_use]
    pub fn get_rotation(&self) -> f32 {
        self.rotation
    }

    /// `setFillColor`.
    pub fn set_fill_color(&mut self, color: super::Color) {
        self.fill_color = color;
    }
    #[must_use]
    pub fn get_fill_color(&self) -> super::Color {
        self.fill_color
    }

    /// `setOutlineColor`.
    pub fn set_outline_color(&mut self, color: super::Color) {
        self.outline_color = color;
    }
    #[must_use]
    pub fn get_outline_color(&self) -> super::Color {
        self.outline_color
    }

    /// `setOutlineThickness`.
    pub fn set_outline_thickness(&mut self, thickness: f32) {
        self.outline_thickness = thickness;
    }
    #[must_use]
    pub fn get_outline_thickness(&self) -> f32 {
        self.outline_thickness
    }

    /// Measure the string at the current character size: `(total_width,
    /// total_height, line_count)`. Width is the max per-line advance; height is
    /// `line_count * line_height`.
    #[must_use]
    pub fn measure(&self) -> (f32, f32, u32) {
        let px = self.character_size as f32;
        let line_height = self
            .font
            .line_metrics(px)
            .map(|lm| lm.ascent - lm.descent)
            .unwrap_or(px);
        let mut width = 0.0f32;
        let mut lines = 0u32;
        for line in self.string.split('\n') {
            lines += 1;
            let mut w = 0.0f32;
            for c in line.chars() {
                w += self.font.glyph_metrics(c, px).advance_width;
            }
            width = width.max(w);
        }
        (width, line_height * lines as f32, lines)
    }

    /// `getLocalBounds()` — the untransformed glyph-bounds rect (left/top at
    /// `0`, width/height from `measure`). Ignores position/origin/scale.
    #[must_use]
    pub fn get_local_bounds(&self) -> FloatRect {
        let (w, h, _) = self.measure();
        FloatRect::new(0.0, 0.0, w, h)
    }

    /// `getGlobalBounds()` — the local bounds through
    /// `translate(position) * rotate(rotation) * scale(scale) *
    /// translate(-origin)`.
    #[must_use]
    pub fn get_global_bounds(&self) -> FloatRect {
        let lb = self.get_local_bounds();
        let corners = [
            Vec2::new(0.0, 0.0),
            Vec2::new(lb.get_width(), 0.0),
            Vec2::new(0.0, lb.get_height()),
            Vec2::new(lb.get_width(), lb.get_height()),
        ];
        let mut min_x = f32::INFINITY;
        let mut min_y = f32::INFINITY;
        let mut max_x = f32::NEG_INFINITY;
        let mut max_y = f32::NEG_INFINITY;
        for c in corners {
            let p = Self::apply_transform(c, self.position, self.origin, self.scale, self.rotation);
            min_x = min_x.min(p.x);
            min_y = min_y.min(p.y);
            max_x = max_x.max(p.x);
            max_y = max_y.max(p.y);
        }
        FloatRect::new(min_x, min_y, max_x - min_x, max_y - min_y)
    }

    /// The `Transformable` transform applied to a local point (shared with
    /// `Sprite`).
    fn apply_transform(p: Vec2, position: Vec2, origin: Vec2, scale: Vec2, rotation: f32) -> Vec2 {
        let translated = Vec2::new(p.x - origin.x, p.y - origin.y);
        let scaled = Vec2::new(translated.x * scale.x, translated.y * scale.y);
        let rad = oh_math::ssvu::math::to_rad(rotation);
        let (sin, cos) = rad.sin_cos();
        let rotated = Vec2::new(
            scaled.x * cos - scaled.y * sin,
            scaled.x * sin + scaled.y * cos,
        );
        Vec2::new(position.x + rotated.x, position.y + rotated.y)
    }

    /// `getTransform().transformPoint(p)` — the C++ `sf::Transformable`
    /// transform applied to a local point (the GL text pass's quad
    /// corners, issue-025c SG3; the `Sprite::transform_point` analogue).
    #[must_use]
    pub fn transform_point(&self, p: Vec2) -> Vec2 {
        Self::apply_transform(p, self.position, self.origin, self.scale, self.rotation)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn font() -> Font {
        // _RELEASE/Assets/OpenSquare-Regular.ttf (repo root relative to this crate).
        let root = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../_RELEASE/Assets/OpenSquare-Regular.ttf"
        );
        Font::load(Path::new(root)).expect("OpenSquare-Regular.ttf must load")
    }

    #[test]
    fn font_loads_and_has_line_metrics() {
        let f = font();
        let lm = f.line_metrics(32.0).expect("line metrics for a loaded TTF");
        assert!(lm.ascent > 0.0);
        assert!(lm.descent < 0.0);
        // a real glyph has a positive advance width.
        let m = f.glyph_metrics('H', 32.0);
        assert!(m.advance_width > 0.0);
    }

    #[test]
    fn text_layout_is_non_degenerate_and_stable() {
        let t = Text::new(font(), "HELLO", 64);
        let (w, h, lines) = t.measure();
        assert_eq!(lines, 1);
        // non-degenerate: a 5-char string at 64px must be >0 wide/tall.
        assert!(w > 0.0);
        assert!(h > 0.0);
        // stable: two independent measurements of the same text are equal.
        let t2 = Text::new(font(), "HELLO", 64);
        assert_eq!(t.measure(), t2.measure());
        // changing the string changes the width deterministically (H+E+L+L+O
        // is wider than a single char).
        let one = Text::new(font(), "H", 64).measure().0;
        assert!(w > one);

        // character size scales the width monotonically.
        let small = Text::new(font(), "HELLO", 8).measure().0;
        assert!(w > small);
    }

    #[test]
    fn text_global_bounds_apply_origin_and_scale() {
        let f = font();
        let (w, h, _) = Text::new(f.clone(), "AB", 32).measure();
        let mut t = Text::new(f, "AB", 32);
        let lb = t.get_local_bounds();
        assert_eq!((lb.get_width(), lb.get_height()), (w, h));
        assert!((lb.get_left()).abs() < 1e-6);

        // centre origin -> symmetric bounds around the position.
        t.set_origin(Vec2::new(w / 2.0, h / 2.0));
        t.set_position(Vec2::new(0.0, 0.0));
        let gb = t.get_global_bounds();
        assert!((gb.get_left() - (-w / 2.0)).abs() < 1e-3);
        assert!((gb.get_right() - (w / 2.0)).abs() < 1e-3);

        // scale 2x doubles the width.
        t.set_scale(Vec2::new(2.0, 2.0));
        let gb2 = t.get_global_bounds();
        assert!((gb2.get_width() - w * 2.0).abs() < 1e-2);
    }

    #[test]
    fn text_colour_and_outline_state() {
        let f = font();
        let mut t = Text::new(f, "x", 16);
        t.set_fill_color(super::super::Color {
            r: 10,
            g: 20,
            b: 30,
            a: 255,
        });
        t.set_outline_color(super::super::Color {
            r: 1,
            g: 2,
            b: 3,
            a: 255,
        });
        t.set_outline_thickness(2.0);
        assert_eq!(t.get_fill_color().r, 10);
        assert_eq!(t.get_outline_color().b, 3);
        assert_eq!(t.get_outline_thickness(), 2.0);
        assert!(Text::new(font(), "", 16).is_empty());
    }
}
