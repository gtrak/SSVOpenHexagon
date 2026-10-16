//! The real-GL backend (`glutin` 0.32 + `glow` 0.18), isolated from the rest
//! of the crate so everything else stays headless.
//!
//! `GlBackend::try_create` bootstraps a winit window + a glutin OpenGL
//! context + a `glow` API on a live display. When no display / GPU is
//! available (headless CI, this environment) it returns `None` — callers MUST
//! handle that (see the GPU smoke test, which skips with a printed reason).
//!
//! The whole bootstrap is wrapped so a display-panic degrades to `None`
//! instead of aborting the process (`catch_unwind`). This is a **hard
//! requirement**: pack content ships GLSL fragment shaders that must run on a
//! real GL context (DECISIONS.md D-004 SFML entry), so a CPU rasterizer is not
//! an option.
//!
//! Split into `GlBackend` (owns the winit `EventLoop`, consumed by the game
//! loop) and `GlParts` (the objects that keep the GL context valid — held by
//! `Window.gl`).
//!
//! `glow` 0.18 is a raw-GL API: its entry points are `unsafe` and take raw
//! `u32` constants (`glow::COLOR_BUFFER_BIT`, `glow::RGBA`, …), so every call
//! is wrapped in `unsafe`. The full vertex/triangle pipeline is built in
//! issue 11 (draw/graphics); here the smoke surface is `version_string`
//! (proves a live context) + a `glClear` → `glReadPixels` offscreen readback.
//!
//! The real `save_screenshot` readback (glReadPixels, rows BOTTOM-UP, row
//! flip, PNG encode — the C++ `GameWindow::saveScreenshot`, D-004 SFML
//! entry) lands with the issue-015c-tail screenshot work.

use std::num::NonZero;
use std::panic::AssertUnwindSafe;

use raw_window_handle::{HasDisplayHandle, HasWindowHandle};

use glow::HasContext;
use glutin::config::ConfigTemplateBuilder;
use glutin::context::{ContextAttributesBuilder, NotCurrentGlContext, PossiblyCurrentContext};
use glutin::display::{Display, DisplayApiPreference, GlDisplay};
use glutin::surface::{Surface, SurfaceAttributesBuilder, WindowSurface};
use winit::event_loop::EventLoop;
use winit::window::{Window as WinitWindow, WindowAttributes};

/// The keep-alive GL pieces (held by `Window::gl`).
pub struct GlParts {
    pub gl: glow::Context,
    pub width: u32,
    pub height: u32,
    /// Keep-alives (drop order: glow context, glutin context, surface, window,
    /// display last — so the GL context drops before its surface/window).
    _context: PossiblyCurrentContext,
    _surface: Surface<WindowSurface>,
    _window: WinitWindow,
    _display: Display,
}

/// `GlBackend` = a winit `EventLoop` + the `GlParts` that keep its context live.
pub struct GlBackend {
    pub event_loop: EventLoop<()>,
    pub parts: GlParts,
}

impl GlBackend {
    /// Bootstrap a real GL context of `width`×`height`. Returns `None`
    /// headless (no display / GPU).
    pub fn try_create(width: u32, height: u32, title: &str) -> Option<Self> {
        std::panic::catch_unwind(AssertUnwindSafe(|| Self::create(width, height, title)))
            .ok()
            .and_then(|o| o)
    }

    /// Bootstrap a real GL context of `width`×`height`. Returns `None`
    /// headless (no display / GPU).
    #[allow(deprecated)] // pre-`run_app` window creation uses `EventLoop::create_window` + `WindowAttributes::new`, both superseded by `ActiveEventLoop` in winit 0.30
    fn create(width: u32, height: u32, title: &str) -> Option<Self> {
        let event_loop = EventLoop::new().ok()?;
        let width = NonZero::new(width).unwrap_or(NonZero::new(1).unwrap());
        let height = NonZero::new(height).unwrap_or(NonZero::new(1).unwrap());
        let attrs = WindowAttributes::new()
            .with_title(title)
            .with_inner_size(winit::dpi::PhysicalSize::new(width.get(), height.get()));

        #[cfg(windows)]
        let (display, window, window_handle) = {
            // WGL needs the window's handle at display-creation time.
            let window = event_loop.create_window(attrs).ok()?;
            let window_handle = window.window_handle().ok()?.as_raw();
            let preference = DisplayApiPreference::Wgl(Some(window_handle));
            let display =
                unsafe { Display::new(event_loop.display_handle().ok()?.as_raw(), preference) }
                    .ok()?;
            (display, window, window_handle)
        };

        #[cfg(not(windows))]
        let (display, window, window_handle) = {
            let preference = {
                #[cfg(target_os = "macos")]
                {
                    DisplayApiPreference::Cgl
                }
                #[cfg(not(target_os = "macos"))]
                {
                    DisplayApiPreference::Egl
                }
            };
            let display =
                unsafe { Display::new(event_loop.display_handle().ok()?.as_raw(), preference) }
                    .ok()?;
            let window = event_loop.create_window(attrs).ok()?;
            let window_handle = window.window_handle().ok()?.as_raw();
            (display, window, window_handle)
        };

        let config = unsafe { display.find_configs(ConfigTemplateBuilder::new().build()) }
            .ok()?
            .next()?;
        let surface_attrs =
            SurfaceAttributesBuilder::<WindowSurface>::new().build(window_handle, width, height);
        let surface = unsafe { display.create_window_surface(&config, &surface_attrs) }.ok()?;
        let ctx_attrs = ContextAttributesBuilder::new().build(None);
        let not_current = unsafe { display.create_context(&config, &ctx_attrs) }.ok()?;
        let context = not_current.make_current(&surface).ok()?;
        let gl = unsafe {
            glow::Context::from_loader_function_cstr(|name| display.get_proc_address(name))
        };

        Some(Self {
            event_loop,
            parts: GlParts {
                gl,
                width: width.get(),
                height: height.get(),
                _context: context,
                _surface: surface,
                _window: window,
                _display: display,
            },
        })
    }
}

impl GlParts {
    /// The winit window this GL context lives on (issue 014b: the egui
    /// console's `egui-winit` state + `egui_glow` painter need the winit
    /// window — sizes / clipboard display handle / the event hook's
    /// argument — without taking the `glow 0.18` `Context` out of the
    /// render layer).
    #[must_use]
    pub fn winit_window(&self) -> &WinitWindow {
        &self._window
    }

    /// The GL entry-point address for `name` from this context's display
    /// loader (issue 014b / DECISIONS.md D-022: `egui_glow 0.37` pulls
    /// `glow 0.17` while this crate uses `glow 0.18` — different Rust
    /// types over the SAME GL context; the console builds its own
    /// `glow 0.17` context with this loader, exactly as [`Self::gl`]'s
    /// `glow 0.18` context was built at `create`).
    #[must_use]
    pub fn proc_address(&self, name: &std::ffi::CStr) -> *const () {
        self._display.get_proc_address(name) as *const ()
    }

    /// The live `GL_VERSION` string (proves a real context is current).
    #[must_use]
    pub fn version_string(&self) -> Option<String> {
        let v = self.gl.version();
        Some(format!("GL {}.{}", v.major, v.minor))
    }

    /// Clear the default framebuffer to `color` (RGBA `f32`) and read back the
    /// centre pixel — a genuine offscreen `glClear` → `glReadPixels` check that
    /// a live GL context renders and is readable. Returns the read-back RGBA.
    #[must_use]
    pub fn clear_and_read_center(&self, color: [f32; 4]) -> Option<[u8; 4]> {
        self.clear(color);
        let gl = &self.gl;
        let w = self.width as i32;
        let h = self.height as i32;
        let mut pixel = [0u8; 4];
        unsafe {
            gl.read_pixels(
                w / 2,
                h / 2,
                1,
                1,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelPackData::Slice(Some(&mut pixel)),
            );
        }
        Some(pixel)
    }

    /// The whole default framebuffer (RGBA8, row-major) — the C++
    /// `saveScreenshot`'s back-buffer read (`t.update(renderWindow)`,
    /// GameWindow.hpp:171). The GL origin is bottom-left, so the rows come
    /// back BOTTOM-UP (row 0 = the framebuffer's BOTTOM row): the C++'s SFML
    /// captures that as a flipped texture (`m_pixelsFlipped = true`, SFML
    /// fork `Texture.cpp:628-656`) and reverses the rows in `copyToImage`
    /// (`Texture.cpp:413-424`); the port exposes the raw readback and flips
    /// at the consumer (`save_screenshot` below, via [`flip_rows`]).
    #[must_use]
    pub fn read_framebuffer(&self) -> Vec<u8> {
        let gl = &self.gl;
        let mut data = vec![0u8; (self.width as usize) * (self.height as usize) * 4];
        unsafe {
            gl.read_pixels(
                0,
                0,
                self.width as i32,
                self.height as i32,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelPackData::Slice(Some(&mut data)),
            );
        }
        data
    }

    /// C++ `GameWindow::saveScreenshot(path)` (GameWindow.hpp:167-174): read
    /// the default framebuffer (rows bottom-up — `t.update(renderWindow)`,
    /// the port's direct `glReadPixels` equivalent), flip to top-down (the
    /// C++'s `copyToImage` row reversal), PNG-encode + write to `path` (the
    /// C++'s `img.saveToFile` — stb_image_write `stbi_write_png` in the SFML
    /// fork's `ImageLoader.cpp:262`; the port's `png` crate, D-004 SFML
    /// entry). Returns `true` on success; the C++ discards the result (the
    /// port's `DrawTarget` consumer mirrors that).
    ///
    /// **MANUAL verify** (no display in CI — D-011 skip-with-reason; the
    /// `gpu_smoke` half runs this on a live context when one exists):
    /// `cargo run` (the windowed boot) + the screenshot key (default F12,
    /// `Tid::Screenshot`) → `screenshot.png` appears in the CWD, the
    /// frame's content, top-down.
    pub fn save_screenshot(&self, path: &str) -> bool {
        let mut data = self.read_framebuffer();
        flip_rows(&mut data, self.width, self.height);
        let Ok(bytes) = encode_png(self.width, self.height, &data) else {
            return false;
        };
        std::fs::write(path, bytes).is_ok()
    }

    /// C++ `renderWindow.setIcon(...)` (main.cpp:322-323) — the winit window
    /// icon (the C++'s `sf::RenderWindow::setIcon` uploads the RGBA8 pixels
    /// to the platform window; the port's winit analogue is
    /// `Window::set_window_icon`).
    pub fn set_icon(&self, icon: winit::window::Icon) {
        self._window.set_window_icon(Some(icon));
    }

    /// C++ `renderWindow.clear(mColor)` (GameWindow.hpp:157-160) — the
    /// per-frame pre-update clear `GameWindow::run` performs (`this->clear()`
    /// at GameWindow.hpp:126, default `sf::Color::Transparent`), and the
    /// `window->clear(sf::Color::Black)` the game's `draw()` performs
    /// (HGGraphics.cpp:83) go through the same GL call.
    pub fn clear(&self, color: [f32; 4]) {
        let gl = &self.gl;
        unsafe {
            gl.clear_color(color[0], color[1], color[2], color[3]);
            gl.clear(glow::COLOR_BUFFER_BIT);
        }
    }
}

// ---------------------------------------------------------------------------
// The screenshot's pure half (headless-testable — the GL readback itself is
// the `gpu_smoke` skip-with-reason half, D-011).
// ---------------------------------------------------------------------------

/// Flip the row order of an RGBA8 row-major buffer in place — BOTTOM-UP
/// (the GL `glReadPixels` origin, bottom-left) to TOP-DOWN (the PNG
/// layout). `width_px * height_px` pixels, 4 bytes each, row-major.
///
/// The C++ equivalent: SFML's `Texture::update(Window)` stores the
/// back-buffer copy bottom-up-flipped (`m_pixelsFlipped = true`, SFML fork
/// `Texture.cpp:628-656`) and `copyToImage` reverses the rows on the way
/// back to the top-down `Image` that `saveToFile` PNG-writes
/// (`Texture.cpp:413-424`, `ImageLoader.cpp:262`).
pub fn flip_rows(data: &mut [u8], width_px: u32, height_px: u32) {
    let row = (width_px as usize) * 4;
    assert_eq!(
        data.len(),
        row * height_px as usize,
        "flip_rows: buffer is not width*height*4 bytes"
    );
    let height = height_px as usize;
    for i in 0..height / 2 {
        // data = [0..a) ++ row_a ++ mid ++ row_b ++ tail, with a = i*row,
        // b = (height-1-i)*row: swap row_a and row_b.
        let a = i * row;
        let b = (height - 1 - i) * row;
        // data = [0..a) ++ row_a ++ mid ++ row_b ++ tail
        let (pre, _tail) = data.split_at_mut(b + row);
        let (pre_b, row_b) = pre.split_at_mut(b);
        let (_pre_a, pre_mid) = pre_b.split_at_mut(a);
        let (row_a, _mid) = pre_mid.split_at_mut(row);
        row_a.swap_with_slice(row_b);
    }
}

/// PNG-encode a TOP-DOWN RGBA8 row-major buffer (the `png` crate — the
/// sound Rust analogue of the C++'s stb_image_write PNG path, D-004 SFML
/// entry; the C++ writes through `stbi_write_png`, SFML fork
/// `ImageLoader.cpp:262`, stb_image_write v1.16).
pub fn encode_png(width_px: u32, height_px: u32, rgba_top_down: &[u8]) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, width_px, height_px);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder
            .write_header()
            .map_err(|e| format!("png header: {e}"))?;
        writer
            .write_image_data(rgba_top_down)
            .map_err(|e| format!("png write: {e}"))?;
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `flip_rows` on a hand-built buffer: the GL bottom-up layout (row 0 =
    /// the BOTTOM row) becomes the PNG top-down layout. A 2×3 buffer, each
    /// row tagged `R[width*4..]`-style with distinct pixels.
    #[test]
    fn flip_rows_reverses_the_row_order() {
        // 2 wide × 3 high; row i's pixels are (i*2, i*2+1) in RGBA (R only,
        // the other channels 0).
        let mut buf: Vec<u8> = Vec::new();
        for y in 0..3u32 {
            for x in 0..2u32 {
                buf.extend_from_slice(&[(y * 2 + x) as u8, 0, 0, 255]);
            }
        }
        // As built this is TOP-DOWN (row 0 first); simulate the GL readback
        // (bottom-up) by pre-flipping, then flip back to top-down.
        flip_rows(&mut buf, 2, 3); // now bottom-up: row 0 = the bottom row
        flip_rows(&mut buf, 2, 3); // back to top-down (the screenshot's flip)

        // Top-down: the first row's first pixel is (0, 0, 0, 255), the LAST
        // row's first pixel is (4, 0, 0, 255).
        assert_eq!(&buf[0..4], &[0, 0, 0, 255]);
        assert_eq!(&buf[buf.len() - 8..buf.len() - 4], &[4, 0, 0, 255]);
    }

    /// The flip is one-shot and idempotent-pair: a SINGLE flip of a
    /// bottom-up buffer yields the top-down layout (the screenshot path's
    /// exact use).
    #[test]
    fn flip_rows_single_flip_of_bottom_up_is_top_down() {
        // Bottom-up 1×4: the first row in memory is the BOTTOM row (y=3),
        // the last is the TOP row (y=0).
        let mut buf = vec![
            30, 0, 0, 255, // y=3 (bottom)
            20, 0, 0, 255, // y=2
            10, 0, 0, 255, // y=1
            0, 0, 0, 255, // y=0 (top)
        ];
        flip_rows(&mut buf, 1, 4);
        assert_eq!(
            buf.iter().step_by(4).copied().collect::<Vec<u8>>(),
            vec![0, 10, 20, 30],
            "after the flip, memory order is top (y=0) to bottom (y=3)"
        );
    }

    /// `encode_png` + decode round trip on a hand-built buffer: the encoded
    /// PNG decodes to the SAME top-down RGBA layout (size + pixel order
    /// pinned — the flip above is what puts the readback in this layout).
    #[test]
    fn encode_png_round_trips_top_down_rgba() {
        // 2×2, top-down: the top-left pixel red, the bottom-right pixel
        // green (if the row order were flipped, these two corners would
        // swap and this test would fail).
        let top_down = vec![
            255, 0, 0, 255, // top-left: red
            0, 0, 255, 255, // top-right: blue
            0, 255, 0, 255, // bottom-left: green
            0, 0, 0, 255, // bottom-right: black
        ];
        let png = encode_png(2, 2, &top_down).expect("png encode");

        // Decode with the same crate (the C++'s `loadFromFile` half is the
        // icon lane's `Image::load_png`; here the raw reader).
        let mut decoder = png::Decoder::new(std::io::Cursor::new(&png[..]));
        decoder.set_transformations(png::Transformations::normalize_to_color8());
        let mut reader = decoder.read_info().expect("png read_info");
        let mut buf = vec![0u8; reader.output_buffer_size().expect("output size")];
        let out = reader.next_frame(&mut buf).expect("png frame");
        assert_eq!((out.width, out.height), (2, 2));
        assert_eq!(out.color_type, png::ColorType::Rgba);
        let rgba = &buf[..out.buffer_size()];
        assert_eq!(
            rgba,
            top_down.as_slice(),
            "the PNG is top-down RGBA8, row order preserved"
        );
    }

    /// `encode_png` rejects a wrong-length buffer (the `png` crate's
    /// image-size check) — the screenshot path then reports failure (the
    /// C++'s `saveToFile` bool would be `false` likewise).
    #[test]
    fn encode_png_rejects_a_mis_sized_buffer() {
        assert!(encode_png(2, 2, &[0u8; 7]).is_err());
    }
}
