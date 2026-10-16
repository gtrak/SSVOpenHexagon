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
//!
//! # Android (issue 25b SG2)
//!
//! `create_android_parts` bootstraps the GL backend over the activity's
//! window on the first `resumed` (the `ANativeWindow` only exists between
//! `InitWindow` and `TerminateWindow` — `Window::ensure_android_gl` calls
//! it from `RunHandler::resumed`; the desktop `create` keeps its own
//! pre-`run` bootstrap, unchanged). The differences, exactly:
//!
//! * **API** — `DisplayApiPreference::Egl` over the android display handle
//!   (glutin's `EGL_PLATFORM_ANDROID` display — `EGL_KHR_platform_android`);
//! * **context** — `ContextApi::Gles(3.0)` explicitly requested (the D-026
//!   target). The minSdk is 28 (the cargo-apk metadata, issue 25b SG6), and
//!   every API-28+ device guarantees OpenGL ES 3.0, so the request cannot
//!   fail on a supported device. **GLES 2.0 fallback assessment**: a
//!   `Gles(2.0)` retry would additionally need VAOs through the
//!   `OES_vertex_array_object` extension (ES 2.0 has no core VAOs — the
//!   [`crate::batching::GlBatchUploader`] uses them unconditionally) and
//!   the SG3 `#version 100` translation (already ES-1.00-targeted) —
//!   feasible but never exercised at minSdk 28, so the retry is NOT
//!   implemented (a `create_context` failure degrades to the headless
//!   frames, `Window::ensure_android_gl`'s documented path);
//! * **glow** — the SAME `glow 0.18` `Context::from_loader_function_cstr`
//!   loader as desktop: it loads the GL-4.6 desktop function table by name;
//!   on a GLES context the desktop-only / fixed-function names (e.g.
//!   `glBegin`, `glMatrixMode`) resolve to NULL and stay null. The port
//!   calls ONLY functions present in ES 3.0 core (`glClear*`,
//!   `glReadPixels`, the program/shader/VAO/VBO/uniform/draw calls of
//!   `GlParts` + `GlBatchUploader`, `glGetString(GL_VERSION)`,
//!   `glGetParameter(NUM_EXTENSIONS/EXTENSIONS/MAX_LABEL_LENGTH)`), so the
//!   nulls are never dereferenced — the ES 3.0 core ⊇ every call made.
//!   (The pack's `gl_Color`/`gl_TexCoord` fixed-function builtins are a
//!   SHADER-language gap, not a loader gap — the SG3 assessment.)
//! * **surface size** — the activity window is screen-sized (winit's
//!   android `create_window` ignores the attributes — its documented
//!   FIXME); `GlParts.width/height` come from the native window, so the
//!   framebuffer readbacks + the `GlDrawTarget`'s view see the real size.
//! * **egui console** — desktop-only (D-026 plan): its `glow 0.17` +
//!   `#version 330`-era shaders are the desktop GL surface; on the GLES
//!   context `IlcConsole::new` (oh-app) fails gracefully and the overlay
//!   stays off (the C++ Android build has no ImGui — `#ifndef
//!   SSVOH_ANDROID` in Imgui.cpp).

use std::cell::{RefCell, RefMut};
use std::num::NonZero;
use std::panic::AssertUnwindSafe;

use raw_window_handle::{HasDisplayHandle, HasWindowHandle};

use glow::HasContext;
use glutin::config::ConfigTemplateBuilder;
#[cfg(target_os = "android")]
use glutin::context::ContextApi;
use glutin::context::PossiblyCurrentContext;
#[cfg(target_os = "android")]
use glutin::context::Version;
use glutin::context::{ContextAttributes, ContextAttributesBuilder, NotCurrentGlContext};
use glutin::display::{Display, DisplayApiPreference, GlDisplay};
use glutin::surface::{GlSurface, Surface, SurfaceAttributesBuilder, WindowSurface};
#[cfg(target_os = "android")]
use winit::event_loop::ActiveEventLoop;
use winit::event_loop::EventLoop;
#[cfg(target_os = "android")]
use winit::platform::android::ActiveEventLoopExtAndroid;
use winit::window::{Window as WinitWindow, WindowAttributes};

use crate::font::Text;
use crate::view::View;

/// The keep-alive GL pieces (held by `Window::gl`).
pub struct GlParts {
    pub gl: glow::Context,
    pub width: u32,
    pub height: u32,
    /// The issue-025c draw passes (the texture cache, the textured-quad
    /// program, the font atlas) — shared across frames (the per-frame
    /// `GlDrawTarget` borrows it via [`Self::passes`]; the single-threaded
    /// GL — the `GlParts` contract — needs no interior synchronization).
    passes: RefCell<crate::gl_passes::GlPasses>,
    /// The shared batch-uploader slot (the `GlBatchUploader`'s
    /// program / VAO / VBO live for the window's life — the pre-025c
    /// per-frame `GlDrawTarget` re-created them EVERY frame and never
    /// deleted them: the GL object-name leak the issue-025c SG5 audit
    /// found, the `GlDrawTarget` borrows it via [`Self::batch`]).
    batch: RefCell<BatchSlot>,

    /// Keep-alives (drop order: glow context, glutin context, surface, window,
    /// display last — so the GL context drops before its surface/window).
    _context: PossiblyCurrentContext,
    _surface: Surface<WindowSurface>,
    _window: WinitWindow,
    _display: Display,
}

/// The `GlParts`-owned `GlBatchUploader` slot: lazily created on the first
/// `draw_vertices` / `draw_rectangle`, re-created never (a creation failure
/// is sticky for the window's life — the documented no-op batch).
#[derive(Default)]
pub struct BatchSlot {
    pub uploader: Option<crate::batching::GlBatchUploader>,
    pub failed: bool,
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
            // The context attributes need the window handle too (the
            // windows-testing fix; EGL/CGl use it for the context-surface
            // pairing the same way WGL does).
            let window_handle = window.window_handle().ok()?.as_raw();
            (display, window, window_handle)
        };

        // The desktop context: no explicit API request (`ContextAttributes`
        // `api == None` — the EGL/CGl config's API decides: desktop GL on
        // desktop EGL, the Cgl context on macOS, WGL on Windows). The
        // shared tail is `finish_gl` (the android path, below, passes the
        // GLES 3.0 request through the same tail). The context attributes
        // carry Some(window_handle) (windows-testing fix 1: build(None) made
        // WGL fall back to the screen DC and wglCreateContextAttribsARB fail
        // silently — the window never appeared).
        let ctx_attrs = ContextAttributesBuilder::new().build(Some(window_handle));
        let parts = finish_gl(display, window, width.get(), height.get(), &ctx_attrs)?;

        Some(Self { event_loop, parts })
    }
}

/// The shared GL bootstrap tail (desktop `create` + the android
/// `create_android_parts`, issue 25b SG2): pick the first config, build the
/// window surface on the winit window's raw handle, create + make current
/// the context with the given attributes, load the `glow` API, and wrap
/// everything in the keep-alive `GlParts` (drop order: glow context →
/// glutin context → surface → window → display last). `window` MOVES into
/// the `GlParts` (winit's `Window` is not `Clone` — the handle is read
/// first).
///
/// `width`/`height` are the SURFACE (framebuffer) size: the desktop's
/// requested physical window size (clamped to 1, the `create`'s `NonZero`
/// semantics), the android native window's screen size.
fn finish_gl(
    display: Display,
    window: WinitWindow,
    width: u32,
    height: u32,
    ctx_attrs: &ContextAttributes,
) -> Option<GlParts> {
    let window_handle = window.window_handle().ok()?.as_raw();
    let width = NonZero::new(width).unwrap_or(NonZero::new(1).unwrap());
    let height = NonZero::new(height).unwrap_or(NonZero::new(1).unwrap());
    let config = unsafe { display.find_configs(ConfigTemplateBuilder::new().build()) }
        .ok()?
        .next()?;
    let surface_attrs =
        SurfaceAttributesBuilder::<WindowSurface>::new().build(window_handle, width, height);
    let surface = unsafe { display.create_window_surface(&config, &surface_attrs) }.ok()?;
    let not_current = unsafe { display.create_context(&config, ctx_attrs) }.ok()?;
    let context = not_current.make_current(&surface).ok()?;
    let gl =
        unsafe { glow::Context::from_loader_function_cstr(|name| display.get_proc_address(name)) };
    // 025i PROBE (temporary, one-shot): the actual GL renderer/vendor — a
    // software (SwiftShader) context would explain the ~10ms per-frame
    // gl.clear (a CPU clear); a hardware GLES GPU should clear in <0.5ms.
    if cfg!(target_os = "android") {
        if let (Ok(r), Ok(v)) = (
            gl.get_string(glow::RENDERER),
            gl.get_string(glow::VENDOR),
        ) {
            eprintln!("[025i-gl] RENDERER={r} VENDOR={v}");
        }
    }
    Some(GlParts {
        gl,
        width: width.get(),
        height: height.get(),
        passes: RefCell::new(crate::gl_passes::GlPasses::new()),
        batch: RefCell::new(BatchSlot::default()),
        _context: context,
        _surface: surface,
        _window: window,
        _display: display,
    })
}

/// Android (issue 25b SG2): the GL backend over the activity's window —
/// the module-docs' Android section is the full assessment (the EGL
/// `EGL_PLATFORM_ANDROID` display, the explicit `Gles(3.0)` context
/// request, the glow-on-GLES loader safety, the screen-sized surface).
///
/// `el` is the loop's `ActiveEventLoop` at the FIRST `resumed` (the
/// `ANativeWindow` exists from `InitWindow` — winit's android
/// `create_window` wraps it; calling earlier is the winit-documented
/// null-window panic). Returns `None` on any failure (the `Window::
/// ensure_android_gl` degradation: headless frames, once).
#[cfg(target_os = "android")]
#[allow(deprecated)] // `WindowAttributes::new` (winit 0.30's superseded ctor) — the android platform ignores the attributes anyway (screen-sized activity window)
pub fn create_android_parts(el: &ActiveEventLoop) -> Option<GlParts> {
    // The screen size (the C++ render window's physical size — the
    // `GameWindow::run`'s framebuffer; `GlParts.width/height` drive the
    // `GlDrawTarget`'s view + the `read_framebuffer`/screenshot readback).
    let (width, height) = el.android_app().native_window().map_or((1, 1), |nw| {
        (nw.width().max(1) as u32, nw.height().max(1) as u32)
    });
    let attrs =
        WindowAttributes::new().with_inner_size(winit::dpi::PhysicalSize::new(width, height));
    // The activity's window (winit's android `create_window` — attributes
    // ignored, the platform window is the `ANativeWindow`'s).
    let window = el.create_window(attrs).ok()?;
    // The EGL display (glutin's `RawDisplayHandle::Android` →
    // `eglGetPlatformDisplay(EGL_PLATFORM_ANDROID_KHR, …)` — the
    // `EGL_KHR_platform_android` extension, module docs).
    let display = unsafe {
        Display::new(
            el.display_handle().ok()?.as_raw(),
            DisplayApiPreference::Egl,
        )
    }
    .ok()?;
    // The D-026 GLES 3.0 request (module docs: the minSdk-28 guarantee +
    // the GLES 2.0 fallback assessment).
    let ctx_attrs = ContextAttributesBuilder::new()
        .with_context_api(ContextApi::Gles(Some(Version::new(3, 0))))
        .build(None);
    finish_gl(display, window, width, height, &ctx_attrs)
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

    /// The issue-025c draw passes (module docs) — the per-frame
    /// `GlDrawTarget` borrows this for the texture cache (the sprite /
    /// text / rectangle passes' uploads, the later 025c sub-goals); the
    /// `RefMut` is the `RefCell`'s interior-mutability hand-off — the
    /// single-threaded GL makes the borrow sound.
    #[must_use]
    pub fn passes(&self) -> RefMut<'_, crate::gl_passes::GlPasses> {
        self.passes.borrow_mut()
    }

    /// The shared batch-uploader slot (the `GlParts` contract — the
    /// single-threaded GL makes the `RefMut` borrow sound; the
    /// `GlDrawTarget`'s vertex pass lazily creates the uploader here so
    /// it survives across frames, the SG5 audit's leak fix).
    #[must_use]
    pub fn batch(&self) -> RefMut<'_, BatchSlot> {
        self.batch.borrow_mut()
    }

    /// The SFML per-draw-call blend application (issue-025c SG2 — the C++
    /// `RenderWindow` applies the render states' blend BEFORE every draw):
    /// `BlendMode::Alpha` = `SRC_ALPHA, ONE_MINUS_SRC_ALPHA` (the C++'s
    /// `sf::BlendFactor` pair for `sf::BlendMode::Alpha`), `Add` = `ONE,
    /// ONE`, `None` = blending off. The `GlDrawTarget`'s draw calls all go
    /// through this (the pre-025c vertex pass left GL blending at the
    /// driver default (off), which the C++ never does — the correction is
    /// behaviour-neutral for the opaque scene batches and enables the
    /// C++'s alpha-blended flash / 3D layers on every platform).
    pub fn apply_blend(&self, blend: crate::draw::BlendMode) {
        let gl = &self.gl;
        unsafe {
            match blend {
                crate::draw::BlendMode::Alpha => {
                    gl.enable(glow::BLEND);
                    gl.blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);
                }
                crate::draw::BlendMode::Add => {
                    gl.enable(glow::BLEND);
                    gl.blend_func(glow::ONE, glow::ONE);
                }
                crate::draw::BlendMode::None => {
                    gl.disable(glow::BLEND);
                }
            }
        }
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

    /// C++ `renderWindow.display()` — the frame present:
    /// `Surface::swap_buffers` (the context is current on this thread —
    /// single-threaded GL, the `GlParts` contract; issue 25, on-device
    /// finding: the swap was never wired and no GL platform presented).
    pub fn present(&self) {
        if let Err(e) = self._surface.swap_buffers(&self._context) {
            // The android lifecycle: `BadSurface` at pause/lock (the
            // EGLSurface destroyed under the app). The C++ SFML window
            // survives focus loss; a panic here aborts the whole game on
            // the phone's first screen lock (the on-device finding: the
            // config-save clean exit at the lock). Log once per surface
            // state and continue — the next `resumed` recreates the
            // surface path; desktop GL surfaces don't hit this.
            eprintln!("[GlParts] swap_buffers failed ({e:?}) — frame not presented (android pause/lifecycle)");
        }
    }

    /// C++ `window->draw(text)` — the text pass, delegated to the
    /// issue-025c pipeline ([`Self::passes`]: the font-atlas glyphs through
    /// `gl_passes::GlPasses::draw_text`; the atlas/program live for the
    /// window's lifetime). The `DrawTarget` caller's `RenderStates` shader
    /// is ignored, as the other passes do. (Reconcile note, D-027: the
    /// windows-testing `gl_text::TextRenderer` pipeline was retired in
    /// favor of this one — on-device-verified on the arm64 phone, integrated
    /// with the sprite/rectangle passes and the texture-cache hardening.)
    pub fn draw_text(&self, text: &Text, view: View) {
        self.passes().draw_text(&self.gl, text, view);
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
