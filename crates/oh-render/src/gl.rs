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
