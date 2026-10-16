//! Textures, images, sprites, and render textures.
//!
//! These are the SFML `sf::Image` / `sf::Texture` / `sf::Sprite` /
//! `sf::RenderTexture` analogues. The pixel buffer and transform math live
//! here (headless-testable); the actual GL upload / framebuffer attach
//! happens on a live `glow` context (`crate::gl_passes` — the issue-025c
//! texture pool upload + the sprite/text/rectangle passes).
//!
//! File I/O (`Texture::loadFromFile`, `Image::saveToFile`) is deliberately
//! NOT here: pack/asset discovery and codec handling belong to `oh-assets`
//! (which already owns `_RELEASE/Packs/*/` scanning). The one exception is
//! the WINDOW LAYER's own PNG surface — in the C++ it lives inside SFML
//! (the window layer's dependency: `ImageLoader.cpp`'s stb_image decode /
//! encode, D-004 SFML entry) — so `Image::load_png` (the icon decode,
//! C++ main.cpp:312-328) and the screenshot PNG encode (`gl.rs`,
//! GameWindow.hpp:167-174) use the same `png` crate. Everything else
//! (the GPU-backed texture surface fed by a decoded RGBA buffer —
//! `Texture::from_rgba` / `from_image` — and `copy_to_image`) stays here.

use oh_math::ssvs::vector2::Vec2;

use crate::types::FloatRect;

/// `sf::Image` — a raw RGBA8 pixel buffer.
#[derive(Clone, Debug)]
pub struct Image {
    width: u32,
    height: u32,
    /// RGBA8, row-major, `width * height * 4` bytes.
    data: Vec<u8>,
}

impl Image {
    /// `sf::Image(width, height)` — zero-filled.
    #[must_use]
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            data: vec![0u8; (width * height * 4) as usize],
        }
    }

    /// Wrap an existing RGBA8 buffer.
    ///
    /// # Panics
    /// If `data.len() != width * height * 4`.
    #[must_use]
    pub fn from_rgba(width: u32, height: u32, data: Vec<u8>) -> Self {
        assert_eq!(data.len(), (width * height * 4) as usize, "bad RGBA buffer");
        Self {
            width,
            height,
            data,
        }
    }

    /// `getSize().x`.
    #[must_use]
    pub fn get_width(&self) -> u32 {
        self.width
    }

    /// `getSize().y`.
    #[must_use]
    pub fn get_height(&self) -> u32 {
        self.height
    }

    /// `getSize()`.
    #[must_use]
    pub fn get_size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// `getPixels()` — the raw RGBA8 buffer.
    #[must_use]
    pub fn get_pixels(&self) -> &[u8] {
        &self.data
    }

    /// Mutable pixel access (upload / readback use).
    pub fn get_pixels_mut(&mut self) -> &mut [u8] {
        &mut self.data
    }

    /// `getPixel(x, y)` → `sf::Color` (out-of-range reads return transparent,
    /// matching SFML's clamped behaviour for the read).
    #[must_use]
    pub fn get_pixel(&self, x: u32, y: u32) -> super::Color {
        if x >= self.width || y >= self.height {
            return super::Color {
                r: 0,
                g: 0,
                b: 0,
                a: 0,
            };
        }
        let i = ((y * self.width + x) as usize) * 4;
        super::Color {
            r: self.data[i],
            g: self.data[i + 1],
            b: self.data[i + 2],
            a: self.data[i + 3],
        }
    }

    /// `setPixel(x, y, color)`.
    pub fn set_pixel(&mut self, x: u32, y: u32, color: super::Color) {
        if x >= self.width || y >= self.height {
            return;
        }
        let i = ((y * self.width + x) as usize) * 4;
        self.data[i] = color.r;
        self.data[i + 1] = color.g;
        self.data[i + 2] = color.b;
        self.data[i + 3] = color.a;
    }

    /// Decode a PNG file into an RGBA8 [`Image`] — the port's
    /// `sf::Image::loadFromFile` half of the C++'s window icon load
    /// (main.cpp:315-316: `icon.loadFromFile("Assets/icon.png")`). The C++
    /// decodes through SFML's stb_image (`ImageLoader.cpp`, D-004 SFML
    /// entry); the port uses the `png` crate (the screenshot encode's
    /// dependency). `sf::Image` is always RGBA8 — the decode normalizes to
    /// color-8 (`png::Transformations::normalize_to_color8` = the pre-0.17
    /// default: `EXPAND | STRIP_16`) and expands a 3-channel result to
    /// RGBA8 (opaque), exactly as the C++'s `sf::Image` does for any input
    /// PNG.
    pub fn load_png(path: &std::path::Path) -> Result<Self, String> {
        let file =
            std::fs::File::open(path).map_err(|e| format!("open {}: {e}", path.display()))?;
        let mut decoder = png::Decoder::new(std::io::BufReader::new(file));
        decoder.set_transformations(png::Transformations::normalize_to_color8());
        let mut reader = decoder
            .read_info()
            .map_err(|e| format!("png read {}: {e}", path.display()))?;
        let mut buf = vec![0u8; reader.output_buffer_size().expect("info read")];
        let out = reader
            .next_frame(&mut buf)
            .map_err(|e| format!("png frame {}: {e}", path.display()))?;
        let buf = buf[..out.buffer_size()].to_vec();
        // `sf::Image` is always RGBA8: the C++'s stb-based loader forces a
        // 4-channel output for ANY input PNG. `normalize_to_color8`
        // (EXPAND | STRIP_16) leaves the channels as the input had them
        // (Rgba / Rgb / Grayscale / GrayscaleAlpha, 8-bit each), so expand
        // to RGBA8 the way the C++'s `loadFromFile` does.
        let samples = out.color_type.samples();
        if samples > 4 {
            return Err(format!(
                "png {} decoded to an unsupported colour type {out:?} (expected ≤ 4 channels)",
                path.display()
            ));
        }
        let mut rgba = Vec::with_capacity(buf.len() / samples * 4);
        for px in buf.chunks_exact(samples) {
            match out.color_type {
                png::ColorType::Rgba => {
                    rgba.extend_from_slice(px);
                }
                png::ColorType::Rgb => {
                    rgba.extend_from_slice(px);
                    rgba.push(255);
                }
                png::ColorType::Grayscale => {
                    let v = px[0];
                    rgba.extend_from_slice(&[v, v, v, 255]);
                }
                png::ColorType::GrayscaleAlpha => {
                    let v = px[0];
                    rgba.extend_from_slice(&[v, v, v, px[1]]);
                }
                _ => {
                    unreachable!("samples() ≤ 4 covers the Rgb/Rgba/Grayscale/GrayscaleAlpha arms")
                }
            }
        }
        Ok(Self::from_rgba(out.width, out.height, rgba))
    }
}

/// `sf::Texture` — a (possibly GL-backed) 2D texture.
///
/// The `gl_id` is the raw `GLuint` set by the `glow` upload path on a live
/// context; it is `None` until uploaded (e.g. in a headless build). The CPU
/// `Image` is retained so `copy_to_image` (screenshot readback) and GL
/// re-uploads work without a display.
#[derive(Clone, Debug)]
pub struct Texture {
    image: Image,
    smooth: bool,
    gl_id: Option<u32>,
}

impl Default for Texture {
    /// `sf::Texture()` — the 1×1 white fallback texture.
    fn default() -> Self {
        Self::new()
    }
}

impl Texture {
    /// `sf::Texture()` — the default 1×1 white texture SFML uses as a valid
    /// fallback (also what `getTextureOrNullTexture` falls back to).
    #[must_use]
    pub fn new() -> Self {
        Self {
            image: Image::from_rgba(1, 1, vec![255, 255, 255, 255]),
            smooth: false,
            gl_id: None,
        }
    }

    /// `texture.create({w, h})` — a blank (transparent) texture.
    #[must_use]
    pub fn create(width: u32, height: u32) -> Self {
        Self {
            image: Image::new(width, height),
            smooth: false,
            gl_id: None,
        }
    }

    /// Build from a decoded RGBA buffer (the `oh-assets`-fed path).
    #[must_use]
    pub fn from_rgba(width: u32, height: u32, data: Vec<u8>) -> Self {
        Self {
            image: Image::from_rgba(width, height, data),
            smooth: false,
            gl_id: None,
        }
    }

    /// Build from an `Image`.
    #[must_use]
    pub fn from_image(image: Image) -> Self {
        Self {
            image,
            smooth: false,
            gl_id: None,
        }
    }

    /// `getSize()`.
    #[must_use]
    pub fn get_size(&self) -> (u32, u32) {
        self.image.get_size()
    }

    /// `getPitch()` (bytes per row) — `width * 4`.
    #[must_use]
    pub fn get_pitch(&self) -> u32 {
        self.image.get_width() * 4
    }

    /// The pixel buffer (GL upload source).
    #[must_use]
    pub fn pixels(&self) -> &Image {
        &self.image
    }

    /// `setSmooth(bool)` / `isSmooth()`.
    pub fn set_smooth(&mut self, smooth: bool) {
        self.smooth = smooth;
    }

    #[must_use]
    pub fn is_smooth(&self) -> bool {
        self.smooth
    }

    /// The raw GL texture id (set by the upload path; `None` headless).
    #[must_use]
    pub fn gl_id(&self) -> Option<u32> {
        self.gl_id
    }

    /// The upload path records the `GLuint` here once a context exists.
    pub fn set_gl_id(&mut self, id: u32) {
        self.gl_id = Some(id);
    }

    /// `sf::Texture::copyToImage()` — read the texture back into an `Image`
    /// (used by `GameWindow::saveScreenshot`).
    #[must_use]
    pub fn copy_to_image(&self) -> Image {
        self.image.clone()
    }
}

/// `sf::Transformable` transform + `sf::Sprite` (texture + transform + color).
///
/// Particles, the key icons, and the replay icon are all sprites in the game
/// (mapping/rendering-menu.md §3.6/§3.8/§4.2).
#[derive(Clone, Debug)]
pub struct Sprite {
    texture: Texture,
    rect: FloatRect,
    position: Vec2,
    origin: Vec2,
    scale: Vec2,
    rotation: f32,
    color: super::Color,
}

impl Default for Sprite {
    fn default() -> Self {
        Self {
            texture: Texture::new(),
            rect: FloatRect::new(0.0, 0.0, 1.0, 1.0),
            position: Vec2::new(0.0, 0.0),
            origin: Vec2::new(0.0, 0.0),
            scale: Vec2::new(1.0, 1.0),
            rotation: 0.0,
            color: super::Color {
                r: 255,
                g: 255,
                b: 255,
                a: 255,
            },
        }
    }
}

impl Sprite {
    /// `sf::Sprite(texture)` — full-texture rect, white.
    #[must_use]
    pub fn new(texture: Texture) -> Self {
        let (w, h) = texture.get_size();
        Self {
            texture,
            rect: FloatRect::new(0.0, 0.0, w as f32, h as f32),
            ..Self::default()
        }
    }

    /// `setTexture`.
    pub fn set_texture(&mut self, texture: Texture) {
        let (w, h) = texture.get_size();
        self.texture = texture;
        self.rect = FloatRect::new(0.0, 0.0, w as f32, h as f32);
    }

    /// `getTexture()`.
    #[must_use]
    pub fn get_texture(&self) -> &Texture {
        &self.texture
    }

    /// `getTextureRect()` — the texture sub-rect the sprite's quad samples
    /// (the full texture after `setTexture`, issue-025c SG2 surface).
    #[must_use]
    pub fn get_texture_rect(&self) -> FloatRect {
        self.rect
    }

    /// `setTextureRect()` — the C++ `sf::Sprite::setTextureRect` (the
    /// sub-rect the quad's UV maps; the port's issue-025c GL sprite pass
    /// reads it — `sprite_quad`'s UV pin).
    pub fn set_texture_rect(&mut self, rect: FloatRect) {
        self.rect = rect;
    }

    // transform
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

    /// `setColor`.
    pub fn set_color(&mut self, color: super::Color) {
        self.color = color;
    }
    #[must_use]
    pub fn get_color(&self) -> super::Color {
        self.color
    }

    /// `getLocalBounds()` — the untransformed rect (usually negative left/top
    /// because of the origin): `{-origin.x, -origin.y, w, h}`.
    #[must_use]
    pub fn get_local_bounds(&self) -> FloatRect {
        FloatRect::new(
            -self.origin.x,
            -self.origin.y,
            self.rect.get_width(),
            self.rect.get_height(),
        )
    }

    /// `getGlobalBounds()` — the local rect through
    /// `translate(position) * rotate(rotation) * scale(scale) *
    /// translate(-origin)`, as a bounding box.
    #[must_use]
    pub fn get_global_bounds(&self) -> FloatRect {
        let w = self.rect.get_width();
        let h = self.rect.get_height();
        let corners = [
            Vec2::new(0.0, 0.0),
            Vec2::new(w, 0.0),
            Vec2::new(0.0, h),
            Vec2::new(w, h),
        ];
        let mut min_x = f32::INFINITY;
        let mut min_y = f32::INFINITY;
        let mut max_x = f32::NEG_INFINITY;
        let mut max_y = f32::NEG_INFINITY;
        for c in corners {
            let p = self.transform_point(c);
            min_x = min_x.min(p.x);
            min_y = min_y.min(p.y);
            max_x = max_x.max(p.x);
            max_y = max_y.max(p.y);
        }
        FloatRect::new(min_x, min_y, max_x - min_x, max_y - min_y)
    }

    /// `getTransform().transformPoint(p)` for a local point.
    #[must_use]
    pub fn transform_point(&self, p: Vec2) -> Vec2 {
        let translated = Vec2::new(p.x - self.origin.x, p.y - self.origin.y);
        let scaled = Vec2::new(translated.x * self.scale.x, translated.y * self.scale.y);
        let rad = oh_math::ssvu::math::to_rad(self.rotation);
        let (sin, cos) = rad.sin_cos();
        let rotated = Vec2::new(
            scaled.x * cos - scaled.y * sin,
            scaled.x * sin + scaled.y * cos,
        );
        Vec2::new(self.position.x + rotated.x, self.position.y + rotated.y)
    }
}

/// `sf::RenderTexture` — an offscreen render target (for screenshots/shaders).
///
/// Holds the target size + view + the CPU readback buffer. The GL framebuffer
/// object (`gl_id`) and the bound color texture are created on a live
/// `glow` context (see `window.rs`); headless, these stay `None` and the CPU
/// image is the source of truth for `copy_to_image`.
#[derive(Clone, Debug, Default)]
pub struct RenderTexture {
    width: u32,
    height: u32,
    view: Option<crate::view::View>,
    smooth: bool,
    gl_id: Option<u32>,
    image: Option<Image>,
}

impl RenderTexture {
    /// `sf::RenderTexture(width, height)`.
    #[must_use]
    pub fn create(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            view: None,
            smooth: false,
            gl_id: None,
            image: Some(Image::new(width, height)),
        }
    }

    /// `getSize()`.
    #[must_use]
    pub fn get_size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// `create(w, h)` (blank, transparent).
    pub fn reset(&mut self, width: u32, height: u32) {
        self.width = width;
        self.height = height;
        self.image = Some(Image::new(width, height));
    }

    /// `setView` / `getView`.
    pub fn set_view(&mut self, view: crate::view::View) {
        self.view = Some(view);
    }
    #[must_use]
    pub fn get_view(&self) -> Option<crate::view::View> {
        self.view
    }

    /// `setSmooth`.
    pub fn set_smooth(&mut self, smooth: bool) {
        self.smooth = smooth;
    }
    #[must_use]
    pub fn is_smooth(&self) -> bool {
        self.smooth
    }

    /// The offscreen colour readback buffer.
    #[must_use]
    pub fn image(&self) -> Option<&Image> {
        self.image.as_ref()
    }

    /// `copyToImage()`.
    #[must_use]
    pub fn copy_to_image(&self) -> Option<Image> {
        self.image.clone()
    }

    /// Raw GL FBO id (set on a live context; `None` headless).
    #[must_use]
    pub fn gl_id(&self) -> Option<u32> {
        self.gl_id
    }

    /// The upload path records the FBO `GLuint` here once a context exists.
    pub fn set_gl_id(&mut self, id: u32) {
        self.gl_id = Some(id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_pixel_round_trip() {
        let mut img = Image::new(2, 2);
        img.set_pixel(
            0,
            0,
            super::super::Color {
                r: 1,
                g: 2,
                b: 3,
                a: 4,
            },
        );
        img.set_pixel(
            1,
            1,
            super::super::Color {
                r: 255,
                g: 0,
                b: 0,
                a: 255,
            },
        );
        assert_eq!(
            img.get_pixel(0, 0),
            super::super::Color {
                r: 1,
                g: 2,
                b: 3,
                a: 4
            }
        );
        assert_eq!(img.get_size(), (2, 2));
        // out-of-range reads are transparent (SFML clamped-read behaviour).
        assert_eq!(img.get_pixel(5, 5).a, 0);
    }

    #[test]
    fn texture_from_rgba_and_copy_to_image() {
        let mut tex = Texture::from_rgba(2, 1, vec![255, 0, 0, 255, 0, 255, 0, 255]);
        assert_eq!(tex.get_size(), (2, 1));
        assert_eq!(tex.get_pitch(), 8);
        let img = tex.copy_to_image();
        assert_eq!(
            img.get_pixel(1, 0),
            super::super::Color {
                r: 0,
                g: 255,
                b: 0,
                a: 255
            }
        );
        assert!(tex.gl_id().is_none());
        tex.set_gl_id(42);
        assert_eq!(tex.gl_id(), Some(42));
    }

    #[test]
    fn default_texture_is_one_by_one() {
        assert_eq!(Texture::new().get_size(), (1, 1));
    }

    #[test]
    fn sprite_local_and_global_bounds() {
        // 64x64 texture, origin = centre (32,32), scale 1, no rotation.
        let tex = Texture::create(64, 64);
        let mut s = Sprite::new(tex);
        s.set_origin(Vec2::new(32.0, 32.0));
        let lb = s.get_local_bounds();
        assert_eq!(lb, FloatRect::new(-32.0, -32.0, 64.0, 64.0));

        s.set_position(Vec2::new(100.0, 100.0));
        let gb = s.get_global_bounds();
        assert!((gb.get_left() - 68.0).abs() < 1e-4);
        assert!((gb.get_top() - 68.0).abs() < 1e-4);
        assert!((gb.get_width() - 64.0).abs() < 1e-4);
        let c = gb.get_center();
        assert!((c.x - 100.0).abs() < 1e-4);
        assert!((c.y - 100.0).abs() < 1e-4);
    }

    #[test]
    fn sprite_scale_grows_global_bounds() {
        let tex = Texture::create(10, 10);
        let mut s = Sprite::new(tex);
        s.set_origin(Vec2::new(5.0, 5.0));
        s.set_position(Vec2::new(0.0, 0.0));
        s.set_scale(Vec2::new(2.0, 2.0));
        let gb = s.get_global_bounds();
        // scaled 10x10 texture, centred: -10..10 on each axis.
        assert!((gb.get_left() - (-10.0)).abs() < 1e-4);
        assert!((gb.get_right() - 10.0).abs() < 1e-4);
    }

    #[test]
    fn render_texture_copy_to_image() {
        let mut rt = RenderTexture::create(4, 3);
        assert_eq!(rt.get_size(), (4, 3));
        let img = rt.copy_to_image().unwrap();
        assert_eq!(img.get_size(), (4, 3));
        rt.set_view(crate::view::View::new(
            Vec2::new(2.0, 1.5),
            Vec2::new(4.0, 3.0),
        ));
        assert!(rt.get_view().is_some());
        assert!(rt.gl_id().is_none());
    }
}
