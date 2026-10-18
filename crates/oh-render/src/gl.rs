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
#[cfg(not(target_os = "emscripten"))]
use std::num::NonZero;
#[cfg(not(target_os = "emscripten"))]
use std::panic::AssertUnwindSafe;

#[cfg(not(target_os = "emscripten"))]
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};

use glow::HasContext;
#[cfg(not(target_os = "emscripten"))]
use glutin::config::ConfigTemplateBuilder;
#[cfg(not(target_os = "emscripten"))]
use glutin::config::GlConfig;
#[cfg(not(target_os = "emscripten"))]
use glutin::context::ContextApi;
#[cfg(not(target_os = "emscripten"))]
use glutin::context::PossiblyCurrentContext;
#[cfg(not(target_os = "emscripten"))]
use glutin::context::Version;
#[cfg(not(target_os = "emscripten"))]
use glutin::context::{
    ContextAttributes, ContextAttributesBuilder, GlProfile, NotCurrentGlContext,
};
#[cfg(not(target_os = "emscripten"))]
use glutin::display::{Display, DisplayApiPreference, GlDisplay};
#[cfg(not(target_os = "emscripten"))]
use glutin::surface::{GlSurface, Surface, SurfaceAttributesBuilder, WindowSurface};
#[cfg(target_os = "emscripten")]
use wasm_bindgen::JsCast;
#[cfg(target_os = "android")]
use winit::event_loop::ActiveEventLoop;
#[cfg(not(target_os = "emscripten"))]
use winit::event_loop::EventLoop;
#[cfg(target_os = "android")]
use winit::platform::android::ActiveEventLoopExtAndroid;
#[cfg(not(target_os = "emscripten"))]
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

    /// The 029n1 (K6) pack-shader cache (the bound stages' compiled
    /// pack `.frag` programs — the lazy per-id compile of the C++'s
    /// load-time `sf::Shader::loadFromFile`; the `GlDrawTarget`'s
    /// shader-bound `draw_vertices` borrows it via
    /// [`Self::pack_shaders`]; the GL objects live for the window's
    /// life, the `BatchSlot` pattern).
    pack_shaders: RefCell<crate::pack_shaders::PackShaderCache>,

    /// Keep-alives (drop order: glow context, glutin context, surface, window,
    /// display last — so the GL context drops before its surface/window).
    /// Emscripten (W1): the canvas element (keeps the page's
    /// `<canvas id="c">` alive for the window's life) + the emscripten
    /// context handle instead.
    #[cfg(not(target_os = "emscripten"))]
    _context: PossiblyCurrentContext,
    #[cfg(not(target_os = "emscripten"))]
    _surface: Surface<WindowSurface>,
    #[cfg(not(target_os = "emscripten"))]
    _window: WinitWindow,
    #[cfg(not(target_os = "emscripten"))]
    _display: Display,
    #[cfg(target_os = "emscripten")]
    _canvas: web_sys::HtmlCanvasElement,
    #[cfg(target_os = "emscripten")]
    _context_handle: i32,
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
#[cfg(not(target_os = "emscripten"))]
pub struct GlBackend {
    pub event_loop: EventLoop<()>,
    pub parts: GlParts,
}

#[cfg(not(target_os = "emscripten"))]
impl GlBackend {
    /// Bootstrap a real GL context of `width`×`height`.
    /// `antialiasing_level` is the config's level — the C++'s
    /// `ContextSettings::antialiasing` sample count (`main.cpp:307` →
    /// `GameWindow.hpp:91`); 0 = no MSAA request. Returns `None`
    /// headless (no display / GPU).
    pub fn try_create(
        width: u32,
        height: u32,
        title: &str,
        antialiasing_level: u32,
    ) -> Option<Self> {
        std::panic::catch_unwind(AssertUnwindSafe(|| {
            Self::create(width, height, title, antialiasing_level)
        }))
        .ok()
        .and_then(|o| o)
    }

    /// Bootstrap a real GL context of `width`×`height`.
    /// `antialiasing_level` is the context's MSAA sample count request
    /// (0 = off); the config pick + the downgrade warning are
    /// `pick_msaa_config` (the 050 shape, module docs above). Returns
    /// `None` headless (no display / GPU).
    #[allow(deprecated)] // pre-`run_app` window creation uses `EventLoop::create_window` + `WindowAttributes::new`, both superseded by `ActiveEventLoop` in winit 0.30
    fn create(width: u32, height: u32, title: &str, antialiasing_level: u32) -> Option<Self> {
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

        // The desktop context: EXPLICIT desktop GL + the COMPATIBILITY
        // profile. The C++'s SFML fork's desktop context (`sfml-src`
        // `Window/Unix/GlxContext.cpp`): the C++ passes no
        // `ContextSettings` (`majorVersion == 0`, no `Core` flag) —
        // the legacy `glXCreateContext` path = the compatibility profile
        // at the implementation's default version (the explicit-profile
        // arm, `:688-691`, picks `GLX_CONTEXT_COMPATIBILITY_PROFILE_BIT_ARB`
        // unless `ContextSettings::Core`). That profile is load-bearing
        // for the 029n1 pack shaders: a fragment-only pack program's
        // vertex stage is the fixed-function pipeline (the
        // `pack_shaders` module docs), which only exists in a
        // compatibility context — in a core context `gl_Color` is not a
        // writable vertex output, the port's fixed-function-stand-in
        // vertex stages fail to compile, and the bound stages fall back
        // to the default program (the C++'s fixed-function model is
        // unrepresentable there). glutin's own default is Core 3.3
        // (`glutin::context::pick_profile`'s `(None, None)` arm) — so the
        // desktop-GL + compatibility request is explicit here. The
        // built-in programs' GLSL-1.20 sources (`gl_FragColor` /
        // `texture2D` + the varyings) are the compatibility profile's
        // native language. The shared tail is `finish_gl` (the android
        // path, below, passes the GLES 3.0 request through the same tail
        // — unaffected: the `GlProfile` request is a desktop-GL concept,
        // the android builder never sets it). The context attributes
        // carry Some(window_handle) (windows-testing fix 1: build(None)
        // made WGL fall back to the screen DC and wglCreateContextAttribsARB
        // fail silently — the window never appeared).
        let ctx_attrs = ContextAttributesBuilder::new()
            .with_context_api(ContextApi::OpenGl(None))
            .with_profile(GlProfile::Compatibility)
            .build(Some(window_handle));
        let parts = finish_gl(
            display,
            window,
            width.get(),
            height.get(),
            &ctx_attrs,
            antialiasing_level,
        )?;

        Some(Self { event_loop, parts })
    }

    /// A GLES-3.0-requested backend over the EGL display (the 029n1
    /// §4.2 compile guard's translated-source column's context — the
    /// `translate_for_gles` output is GLSL ES 1.00, the 25b-SG3
    /// target; a desktop-GL context cannot be its assertion home).
    /// Returns `None` headless / without a GLES-capable EGL stack —
    /// A GLES-3.0-requested backend over the EGL display (the 029n1
    /// §4.2 compile guard's translated-source column's context — the
    /// `translate_for_gles` output is GLSL ES 1.00, the 25b-SG3
    /// target; a desktop-GL context cannot be its assertion home).
    /// winit 0.30 allows ONE `EventLoop` per process (`RecreationAttempt`
    /// on the second), so a caller that already bootstrapped a desktop
    /// backend cannot reach this one — the `pack_shaders_gl` example's
    /// `gles` child mode (a fresh process) is the entry point.
    /// Returns `None` headless / without a GLES-capable EGL stack —
    /// the caller's skip-with-reason (D-011).
    // Desktop/Android only: it bootstraps a winit loop plus an EGL/glutin GLES
    // context, neither of which exists for emscripten (`glutin` is not even a
    // dependency there). 026's gating rule, restored after the K6 landing left
    // this ungated - the host gate cannot see it because the emscripten cfg
    // path is never compiled there.
    #[cfg(not(target_os = "emscripten"))]
    #[allow(deprecated)] // the desktop `create`'s same pre-`run_app` bootstrap
    pub fn try_create_gles(width: u32, height: u32, title: &str) -> Option<Self> {
        use std::num::NonZero;

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let event_loop = winit::event_loop::EventLoop::new().ok()?;
            let width = NonZero::new(width).unwrap_or(NonZero::new(1).unwrap());
            let height = NonZero::new(height).unwrap_or(NonZero::new(1).unwrap());
            let attrs = winit::window::WindowAttributes::new()
                .with_title(title)
                .with_inner_size(winit::dpi::PhysicalSize::new(width.get(), height.get()));
            let window = event_loop.create_window(attrs).ok()?;
            let display = unsafe {
                Display::new(
                    event_loop.display_handle().ok()?.as_raw(),
                    DisplayApiPreference::Egl,
                )
            }
            .ok()?;
            // The GLES-capable config (the shared tail's first-config
            // pick is the desktop-GL one — a GLES context request
            // against a non-ES config fails the EGL context creation):
            let template = glutin::config::ConfigTemplateBuilder::new()
                .with_api(glutin::config::Api::GLES3)
                .build();
            let config = unsafe { display.find_configs(template) }
                .ok()?
                .into_iter()
                .find(|c| c.api().contains(glutin::config::Api::GLES3))?;
            let ctx_attrs = ContextAttributesBuilder::new()
                .with_context_api(ContextApi::Gles(Some(Version::new(3, 0))))
                .build(None);
            let parts = finish_gl_on_config(
                display,
                window,
                width.get(),
                height.get(),
                &config,
                &ctx_attrs,
            )?;
            Some(Self { event_loop, parts })
        }));
        result.ok().and_then(|o| o)
    }
}

/// The shared GL bootstrap tail (desktop `create` + the android
/// `create_android_parts`, issue 25b SG2): pick the config for the
/// requested MSAA level ([`pick_msaa_config`] — the 050 shape), then
/// `finish_gl_on_config` (build the window surface on the winit
/// window's raw handle, create + make current the context with the
/// given attributes, load the `glow` API, and wrap everything in the
/// keep-alive `GlParts` — drop order: glow context → glutin context →
/// surface → window → display last). `window` MOVES into the
/// `GlParts` (winit's `Window` is not `Clone` — the handle is read
/// first).
///
/// `width`/`height` are the SURFACE (framebuffer) size: the desktop's
/// requested physical window size (clamped to 1, the `create`'s `NonZero`
/// semantics), the android native window's screen size. `msaa_level` is
/// the config's `antialiasing_level` (0 = no MSAA request, the C++'s
/// level-0 arm — SFML's `ContextSettings::antialiasing = 0`).
#[cfg(not(target_os = "emscripten"))]
fn finish_gl(
    display: Display,
    window: WinitWindow,
    width: u32,
    height: u32,
    ctx_attrs: &ContextAttributes,
    msaa_level: u32,
) -> Option<GlParts> {
    let width = NonZero::new(width).unwrap_or(NonZero::new(1).unwrap());
    let height = NonZero::new(height).unwrap_or(NonZero::new(1).unwrap());
    let config = pick_msaa_config(&display, msaa_level)?;
    finish_gl_on_config(
        display,
        window,
        width.get(),
        height.get(),
        &config,
        ctx_attrs,
    )
}

/// The 050 config pick: the `antialiasing_level` → context MSAA wiring
/// (the C++'s own model — `main.cpp:307` stores the level, the first
/// `GameWindow::recreateWindow` reads it into
/// `sf::ContextSettings{0, 0, antialiasingLevel, 0, 0}` (GameWindow.hpp:
/// 91) BEFORE the render window exists; the scene renders to the DEFAULT
/// framebuffer — `GlDrawTarget` binds no FBO anywhere in `oh-render` — so
/// a multisampled context IS the whole fix, exactly SFML's shape).
///
/// SFML's own selection (pinned fork `dacceddbd`) enumerates ALL the
/// display's configs and SCORES them: `evaluateFormat`'s
/// `antialiasingDiff` (GlContext.cpp:733-749) weights a SHORTFALL (fewer
/// samples than requested) ×100000 — so the smallest-samples-≥-request
/// config wins when one exists, a short config is only picked when nothing
/// better is available, and the post-creation check warns (GlContext.cpp:
/// 974-988) instead of failing. This mirrors that shape over glutin's
/// backend-abstracted `GlConfig::num_samples` (the `EGL_SAMPLES` /
/// `GLX_SAMPLES` / `WGL_SAMPLES_ARB` read-back, glutin 0.32.3's
/// per-backend config impls):
///
/// * **level 0** → the FIRST config (the pre-050 port behaviour — a plain
///   template's first pick; the C++'s level 0 requests no MSAA);
/// * **level > 0, a config ≥ level exists** → the
///   smallest-samples-≥-level config (NOT the first config: a plain
///   template's first pick is commonly the 0-sample one — the pick the
///   pre-050 port made, which is why the level was inert);
/// * **level > 0, no config ≥ level** → the config with the LARGEST
///   sample count BELOW the level (SFML's scoring's own answer: the
///   shortfall penalty is monotonic in the shortfall, so the closest
///   available config wins — level 8 over a `{0, 4}` device picks the
///   4-sample config, not 0; the `GlContext.cpp:974-988` warning is the
///   "report which happened" half). Only when NO config carries ANY
///   samples at all does the pick fall to the first config (the pre-050
///   behaviour — all configs are equally short, the other scoring terms
///   decide, and the port's documented prior behaviour is the tie-break).
///
/// Deliberately NOT a hard `ConfigTemplateBuilder::with_multisampling(n)`
/// filter: that pushes `EGL_SAMPLES = n` into `eglChooseConfig` (glutin
/// 0.32.3 `api/egl/config.rs:122-124`) and returns ZERO configs when the
/// driver offers only HIGHER sample counts (the C++'s SFML would take
/// those — the scoring's `antialiasingDiff < 0` arm, ×1 — and a hard
/// filter would silently lose MSAA entirely); it also `debug_assert`s a
/// power of two (glutin 0.32.3 `config.rs:143-147`), which the C++
/// menu's reachable level 1 (the `0 → 1 → 2 → 4 → 8 → 16` cycle,
/// MenuGame.cpp:1450-1464) is not (drivers carry no 1-sample configs —
/// the C++'s level 1 resolves to the smallest ≥1, the ×100000-shortfall
/// scoring's own answer).
#[cfg(not(target_os = "emscripten"))]
fn pick_msaa_config(display: &Display, msaa_level: u32) -> Option<glutin::config::Config> {
    // The PLAIN template — the same config universe the pre-050 tail
    // queried; the MSAA filter is applied HERE, over each config's
    // `num_samples` (the SFML enumerate-and-score shape, module above).
    let configs: Vec<glutin::config::Config> =
        unsafe { display.find_configs(ConfigTemplateBuilder::new().build()) }
            .ok()?
            .collect();
    let first = configs.first()?;
    if msaa_level == 0 {
        return Some(first.clone());
    }
    // The smallest-samples-≥-level config (among the non-shortfall configs
    // the `evaluateFormat` AA term rewards the SMALLEST excess: samples 4
    // beats 8 for a level-4 request).
    let mut best: Option<&glutin::config::Config> = None;
    for c in &configs {
        let samples = c.num_samples() as u32;
        if samples < msaa_level {
            continue;
        }
        match best {
            Some(b) if b.num_samples() <= c.num_samples() => {}
            _ => best = Some(c),
        }
    }
    match best {
        Some(c) => {
            eprintln!(
                "[oh-render] context MSAA: requested level {msaa_level} — \
                 config with {} samples",
                c.num_samples()
            );
            Some(c.clone())
        }
        None => {
            // No config carries ≥ `msaa_level` samples: SFML's scoring
            // (GlContext.cpp:733-749) still picks the CLOSEST available
            // config — the shortfall penalty is monotonic in the
            // shortfall, so the LARGEST sample count under the request
            // wins (level 16 over `{0, 4, 8}`: the 8-sample config; a
            // level-8 request on a `{0, 4}` Android device: the 4-sample
            // config, not 0). No config with ANY samples at all → the
            // first config (all equally short — the pre-050 pick).
            let pick = configs
                .iter()
                .filter(|c| c.num_samples() > 0)
                .max_by_key(|c| c.num_samples())
                .unwrap_or(first);
            let available: Vec<u8> = configs.iter().map(|c| c.num_samples()).collect();
            eprintln!(
                "[oh-render] context MSAA: requested level {msaa_level} but no \
                 config carries that many samples (available: {available:?}) — \
                 falling back to the closest config ({} samples); the scene \
                 runs with fewer samples than requested (the SFML downgrade \
                 warning's analogue, pinned GlContext.cpp:974-988)",
                pick.num_samples()
            );
            Some(pick.clone())
        }
    }
}

/// The tail's config-explicit half: the surface/context/glow bootstrap
/// over a caller-picked config (the desktop/android `finish_gl`'s
/// MSAA-aware pick + the `try_create_gles` probe's GLES3-filtered
/// pick).
#[cfg(not(target_os = "emscripten"))]
fn finish_gl_on_config(
    display: Display,
    window: WinitWindow,
    width: u32,
    height: u32,
    config: &glutin::config::Config,
    ctx_attrs: &ContextAttributes,
) -> Option<GlParts> {
    let window_handle = window.window_handle().ok()?.as_raw();
    let width = NonZero::new(width).unwrap_or(NonZero::new(1).unwrap());
    let height = NonZero::new(height).unwrap_or(NonZero::new(1).unwrap());
    let surface_attrs =
        SurfaceAttributesBuilder::<WindowSurface>::new().build(window_handle, width, height);
    let surface = unsafe { display.create_window_surface(config, &surface_attrs) }.ok()?;
    let not_current = unsafe { display.create_context(config, ctx_attrs) }.ok()?;
    let context = not_current.make_current(&surface).ok()?;
    let gl =
        unsafe { glow::Context::from_loader_function_cstr(|name| display.get_proc_address(name)) };
    // The 050 acceptance observable: the context's ACTUAL sample count
    // (`glGetIntegerv(GL_SAMPLES)`) — the driver's answer to the request,
    // not the request (a downgrade reports here; the pick's warning above
    // says which config was available).
    eprintln!("[oh-render] context GL_SAMPLES = {}", unsafe {
        gl.get_parameter_i32(glow::SAMPLES)
    });
    Some(GlParts {
        gl,
        width: width.get(),
        height: height.get(),
        passes: RefCell::new(crate::gl_passes::GlPasses::new()),
        batch: RefCell::new(BatchSlot::default()),
        pack_shaders: RefCell::new(crate::pack_shaders::PackShaderCache::default()),
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
pub fn create_android_parts(el: &ActiveEventLoop, antialiasing_level: u32) -> Option<GlParts> {
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
    // The 050 level (the C++ main.cpp:307 → the C++ `recreateWindow`'s
    // `ContextSettings` sample count, GameWindow.hpp:91): the Android
    // EGL config carries the request — `pick_msaa_config`'s enumerate +
    // the real risk it handles (no Android EGL config may carry the
    // requested samples: the fallback + the warning, not a failure).
    finish_gl(
        display,
        window,
        width,
        height,
        &ctx_attrs,
        antialiasing_level,
    )
}

// ---------------------------------------------------------------------------
// Emscripten (W1) — the WebGL2 canvas backend (module docs)
// ---------------------------------------------------------------------------

/// The emscripten C API (emscripten.h) behind the web GL bootstrap: the
/// WebGL2 context is created + made current through it BEFORE the `glow`
/// loader resolves (W1 — `emscripten_webgl_get_proc_address` dispatches
/// off emscripten's current `GLctx`).
#[cfg(target_os = "emscripten")]
mod emscripten_gl {
    /// `EmscriptenWebGLContextAttributes` (emscripten's
    /// `html5_webgl.h`) — the WebGL context-attribute struct, MIRRORED
    /// FIELD-FOR-FIELD (the C layout: six `bool`s, the `int
    /// powerPreference` at offset 8, the `bool failIfMajorPerformanceCaveat`
    /// at 12, `int majorVersion`/`int minorVersion` at 16/20, the two `bool`s
    /// at 24/25, the `int proxyContextToMainThread` at 28, the two `bool`s at
    /// 32/33 — size 36; the offsets are pinned by emscripten's generated
    /// JS, e.g. `majorVersion: HEAP32[attr32 + (16>>2)]`). `init_context_
    /// attributes` fills it with the defaults, the bootstrap then sets
    /// `major_version = 2`. A layout drift here is SILENT until the browser
    /// (the fields land in the wrong offsets — e.g. `major_version` writes
    /// into the C's `alpha`, and emscripten reads its own `majorVersion`
    /// default of 1 → "WebGL 1 requested but only WebGL 2 is supported"
    /// (MIN_WEBGL_VERSION is 2) + a WebGL1/failed context → the engine's
    /// first frame aborts (the wasm build's first real browser run caught
    /// exactly this, issue 026 integration)).
    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct ContextAttributes {
        pub alpha: bool,
        pub depth: bool,
        pub stencil: bool,
        pub antialias: bool,
        pub premultiplied_alpha: bool,
        pub preserve_drawing_buffer: bool,
        /// `EM_WEBGL_POWER_PREFERENCE` (`int` — the C typedef).
        pub power_preference: i32,
        pub fail_if_major_performance_caveat: bool,
        pub major_version: i32,
        pub minor_version: i32,
        pub enable_extensions_by_default: bool,
        pub explicit_swap_control: bool,
        /// `EMSCRIPTEN_WEBGL_CONTEXT_PROXY_MODE` (`int` — the C typedef).
        pub proxy_context_to_main_thread: i32,
        pub render_via_offscreen_back_buffer: bool,
        pub desynchronized: bool,
    }

    extern "C" {
        pub fn emscripten_webgl_init_context_attributes(attrs: *mut ContextAttributes);
        /// `canvas` is a CSS SELECTOR (W1 gotcha: `"#c"`, not a bare id —
        /// it does `querySelector`).
        pub fn emscripten_webgl_create_context(
            canvas: *const std::ffi::c_char,
            attrs: *const ContextAttributes,
        ) -> i32;
        pub fn emscripten_webgl_make_context_current(ctx: i32) -> i32;
        /// Returns `void *` in the C header — `*const c_void` (on this
        /// target `c_void` is NOT `()` — cast at the `*const ()` seams).
        pub fn emscripten_webgl_get_proc_address(
            name: *const std::ffi::c_char,
        ) -> *const std::ffi::c_void;
    }
}

/// `GlBackend` (web) = the `GlParts` that keep the canvas's WebGL2 context
/// live — no winit `EventLoop` on the web path (the loop is a plain
/// `request_animation_frame`, `window.rs`).
#[cfg(target_os = "emscripten")]
pub struct GlBackend {
    pub parts: GlParts,
}

#[cfg(target_os = "emscripten")]
impl GlBackend {
    /// Bootstrap the WebGL2 context on the page's `<canvas id="c">` (the
    /// W1 selector — the web page MUST provide it; the `title` is the
    /// page's `<title>`, a no-op here). `antialiasing_level` maps to the
    /// WebGL context's `antialias` BOOLEAN (the 050 honest mapping, below).
    /// Returns `None` when the canvas is
    /// missing or WebGL2 is unavailable (the `Window::create` headless
    /// degradation — the desktop no-display arm).
    #[must_use]
    pub fn try_create(
        width: u32,
        height: u32,
        _title: &str,
        antialiasing_level: u32,
    ) -> Option<Self> {
        let canvas: web_sys::HtmlCanvasElement = web_sys::window()?
            .document()?
            .get_element_by_id("c")?
            .unchecked_into();
        let width = width.max(1);
        let height = height.max(1);
        canvas.set_width(width);
        canvas.set_height(height);
        let mut attrs = emscripten_gl::ContextAttributes {
            alpha: false,
            depth: false,
            stencil: false,
            // The 050 honest mapping: WebGL2's `antialias` is a BOOLEAN —
            // the browser picks the sample count (typically 4), so the
            // config's LEVEL cannot be honoured exactly on the web target
            // (unlike the desktop/android contexts, where the level IS the
            // requested sample count). 0 = off, nonzero = on; the
            // `GL_SAMPLES` read-back below is the browser's actual answer.
            antialias: antialiasing_level > 0,
            premultiplied_alpha: false,
            preserve_drawing_buffer: false,
            power_preference: 0, // EM_WEBGL_POWER_PREFERENCE_DEFAULT
            fail_if_major_performance_caveat: false,
            major_version: 0,
            minor_version: 0,
            enable_extensions_by_default: false,
            explicit_swap_control: false,
            proxy_context_to_main_thread: 0, // ..._PROXY_DISALLOW
            render_via_offscreen_back_buffer: false,
            desynchronized: false,
        };
        unsafe { emscripten_gl::emscripten_webgl_init_context_attributes(&mut attrs) };
        // WebGL2 (the W1 recipe: `-sMAX_WEBGL_VERSION=2` link flag + the
        // `major_version = 2` request — without it emscripten logs "WebGL 2
        // requested but only WebGL 1 is supported" and the create returns 0).
        attrs.major_version = 2;
        let selector = b"#c\0";
        let ctx = unsafe {
            emscripten_gl::emscripten_webgl_create_context(
                selector.as_ptr() as *const std::ffi::c_char,
                &attrs,
            )
        };
        if ctx == 0 {
            eprintln!(
                "[oh-render] emscripten_webgl_create_context(\"#c\") failed \
                  (WebGL2 unavailable — a <canvas id=\"c\"> + the \
                  -sMAX_WEBGL_VERSION=2 link flag are required)"
            );
            return None;
        }
        if unsafe { emscripten_gl::emscripten_webgl_make_context_current(ctx) } != 0 {
            eprintln!("[oh-render] emscripten_webgl_make_context_current failed");
            return None;
        }
        // The W1 route: glow's NATIVE loader over the emscripten proc
        // table (the context is current — `get_proc_address` dispatches off
        // it). The GLES 3.0 context engages the `translate_for_gles` shader
        // translation automatically (`is_gles_context`).
        let gl = unsafe {
            glow::Context::from_loader_function_cstr(|name| {
                emscripten_gl::emscripten_webgl_get_proc_address(name.as_ptr())
            })
        };
        // The 050 acceptance observable: the browser's ACTUAL sample count
        // (`glGetIntegerv(GL_SAMPLES)`) — the boolean mapping's answer
        // (the level's non-honourable-exactly caveat, the task's wasm
        // constraint, stated at the create site).
        eprintln!(
            "[oh-render] WebGL2 antialias = {} (antialiasing_level \
             {antialiasing_level} → boolean; the browser picks the sample \
             count) — GL_SAMPLES = {}",
            attrs.antialias,
            unsafe { gl.get_parameter_i32(glow::SAMPLES) }
        );
        Some(Self {
            parts: GlParts {
                gl,
                width,
                height,
                passes: RefCell::new(crate::gl_passes::GlPasses::new()),
                batch: RefCell::new(BatchSlot::default()),
                pack_shaders: RefCell::new(crate::pack_shaders::PackShaderCache::default()),
                _canvas: canvas,
                _context_handle: ctx,
            },
        })
    }
}

impl GlParts {
    /// The winit window this GL context lives on (issue 014b: the egui
    /// console's `egui-winit` state + `egui_glow` painter need the winit
    /// window — sizes / clipboard display handle / the event hook's
    /// argument — without taking the `glow 0.18` `Context` out of the
    /// render layer). Desktop/android only (the web path has no winit
    /// window — the browser tab's favicon is the page's, not the
    /// canvas's; `Window::set_icon` records the set there).
    #[cfg(not(target_os = "emscripten"))]
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
        #[cfg(not(target_os = "emscripten"))]
        {
            self._display.get_proc_address(name) as *const ()
        }
        #[cfg(target_os = "emscripten")]
        {
            // The web loader table (the same `emscripten_webgl_get_proc_address`
            // the `glow` context was built with — the egui console seam has
            // no web consumer yet, the API stays honest).
            unsafe { emscripten_gl::emscripten_webgl_get_proc_address(name.as_ptr()) as *const () }
        }
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

    /// The 029n1 (K6) pack-shader cache (the bound stages' compiled
    /// pack `.frag` programs — the module-docs' lazy per-id compile;
    /// the `GlDrawTarget`'s shader-bound `draw_vertices` lazily
    /// compiles the id's program here so it survives across frames,
    /// the `BatchSlot` pattern).
    #[must_use]
    pub fn pack_shaders(&self) -> RefMut<'_, crate::pack_shaders::PackShaderCache> {
        self.pack_shaders.borrow_mut()
    }

    /// One framebuffer pixel at (`x`, `y`) — GL origin BOTTOM-LEFT
    /// (`glReadPixels`'s coordinates), the §4.3 GL column's + the
    /// `pack_shaders_gl` example's pixel assertion (the D-022 caveat:
    /// the `oh-app`'s `glow 0.17` console type cannot call this
    /// crate's `glow 0.18` `Context` — the example reads pixels
    /// through this helper instead of a direct `read_pixels` call).
    #[must_use]
    pub fn read_pixel(&self, x: i32, y: i32) -> [u8; 4] {
        let mut pixel = [0u8; 4];
        let gl = &self.gl;
        unsafe {
            gl.read_pixels(
                x,
                y,
                1,
                1,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelPackData::Slice(Some(&mut pixel)),
            );
        }
        pixel
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

    /// `glGetIntegerv(GL_SAMPLES)` — the live context's ACTUAL sample count
    /// (0 = the default framebuffer is not multisampled). The 050
    /// acceptance observable on all three targets: the config's
    /// `antialiasing_level` is the context REQUEST (the C++'s
    /// `ContextSettings::antialiasing`, GameWindow.hpp:91); this read-back
    /// is the driver/browser's ANSWER (a downgrade reports here, not in
    /// the request — the C++'s own post-creation check is the analogue,
    /// pinned SFML fork `GlContext.cpp:974-988`).
    #[must_use]
    pub fn gl_samples(&self) -> u32 {
        let gl = &self.gl;
        unsafe { gl.get_parameter_i32(glow::SAMPLES) }.max(0) as u32
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
    /// `Window::set_window_icon`). Desktop/android only (the web path has no
    /// winit window — the browser tab's favicon is the page's, not the
    /// canvas's; `Window::set_icon` records the set there).
    #[cfg(not(target_os = "emscripten"))]
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

    /// Emscripten (issue 32 §2.3): the web window's RESIZE — the C++
    /// `recreateWindow`'s render-window half, applied to the canvas (the
    /// desktop port's creation-frozen GL surface is the documented
    /// deviation; the web canvas resizes cheaply, so the web window
    /// tracks the real size 1:1 — spec §2.3: the canvas CSS size + the
    /// `devicePixelRatio` → the real window size, and a wrong mapping
    /// silently breaks every hit test):
    ///
    /// * the backing store (`canvas.width/height` — the browser resizes
    ///   the drawing buffer; the WebGL context SURVIVES the canvas
    ///   resize, no context loss);
    /// * the GL viewport — a canvas resize does NOT re-set the WebGL
    ///   viewport (the context's default viewport stays at the creation
    ///   size; the port sets no viewport anywhere else, so without this
    ///   call the rendering would stay clipped / stretched to the old
    ///   size);
    /// * the size bookkeeping (`width/height` — `GlDrawTarget::new`'s
    ///   initial view reads them, the `Window`'s `set_size` half moves
    ///   in `window.rs`'s `Resized` fold alongside this call).
    ///
    /// A no-op at the current size (the per-frame cost of the `Resized`
    /// arm is then a compare).
    #[cfg(target_os = "emscripten")]
    pub fn resize(&mut self, width: u32, height: u32) {
        let w = width.max(1);
        let h = height.max(1);
        if w == self.width && h == self.height {
            return;
        }
        self._canvas.set_width(w);
        self._canvas.set_height(h);
        self.width = w;
        self.height = h;
        let gl = &self.gl;
        unsafe {
            gl.viewport(0, 0, w as i32, h as i32);
        }
    }

    /// C++ `renderWindow.display()` — the frame present:
    /// `Surface::swap_buffers` (the context is current on this thread —
    /// single-threaded GL, the `GlParts` contract; issue 25, on-device
    /// finding: the swap was never wired and no GL platform presented).
    /// Emscripten (W1): a documented no-op — the canvas presents implicitly
    /// (the browser composites the WebGL back buffer after the
    /// `request_animation_frame` callback returns; there is no swap call on
    /// the emscripten C API).
    pub fn present(&self) {
        #[cfg(not(target_os = "emscripten"))]
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

    /// C++ `renderWindow.setVerticalSyncEnabled(vsync)` (GameWindow.hpp:94,
    /// inside `recreateWindow` — re-applied on every recreation; the
    /// `setVsync` setter :192-195 marks `mustRecreate`) → the pinned SFML fork's
    /// `Window::setVerticalSyncEnabled` (SFML `2f11710`'s
    /// `src/SFML/Window/Window.cpp:274-280` — applied at runtime when the
    /// context is active) → on Android the window's context IS the
    /// `EglContext` (the `Window.cpp:113` selection, the activity-owned
    /// display), whose `setVerticalSyncEnabled` (`src/SFML/Window/
    /// EglContext.cpp:197-200`) is `eglSwapInterval(display, enabled ? 1 : 0)`
    /// — the display-level request, applied at runtime. glutin 0.32's
    /// `GlSurface::set_swap_interval` issues exactly that EGL call on the
    /// EGL backend (glutin 0.32.3 `src/api/egl/surface.rs:369-378`);
    /// on the other backends it is the platform's equivalent (GLX
    /// `glXSwapInterval*` — pinned `Unix/GlxContext.cpp:276`; WGL
    /// `wglSwapIntervalEXT` — pinned `Win32/WglContext.cpp:245`; CGL
    /// `NSOpenGLCPSwapInterval` — pinned `OSX/SFContext.mm:157`;
    /// SFML's `GlContext::setVerticalSyncEnabled` backend family).
    pub fn set_swap_interval(&self, enabled: bool) {
        #[cfg(not(target_os = "emscripten"))]
        {
            use glutin::surface::SwapInterval;
            let interval = match enabled {
                true => SwapInterval::Wait(NonZero::new(1).expect("const 1 is non-zero")),
                false => SwapInterval::DontWait,
            };
            if let Err(e) = self._surface.set_swap_interval(&self._context, interval) {
                // The C++'s `eglCheck` logs and carries on (the request is
                // re-applied on every recreation — GameWindow.hpp:94 — and the
                // next `set_vsync` re-applies it here): log and continue.
                eprintln!("[GlParts] set_swap_interval failed ({e:?})");
            }
        }
        // Emscripten (W1): a documented no-op — the WebGL swap interval is
        // browser-controlled (the `request_animation_frame` loop paces the
        // frames at the display refresh; there is no `eglSwapInterval`-style
        // call on the emscripten C API).
        #[cfg(target_os = "emscripten")]
        {
            let _ = enabled;
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
