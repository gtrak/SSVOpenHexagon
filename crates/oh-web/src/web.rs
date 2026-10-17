//! The emscripten (web) boot (issue 26c sub-goal 4b, D-029) — called from
//! `main` AFTER `__wasm_call_ctors()` (the 26c.2 trap fix):
//!
//! 1. the page's `<canvas id="c">` (the `oh-render` emscripten backend's
//!    selector contract — the 26c.1 gotcha: `"#c"`, not a bare id),
//! 2. the `oh-render` emscripten window — the WebGL2 context (the
//!    `emscripten_webgl_*` C API + the `glow` loader, `oh-render`'s
//!    `GlBackend`) + the DOM event listeners,
//! 3. the `request_animation_frame` loop (`oh-render`'s web
//!    `Window::run` — the winit event loop's web replacement): an animated
//!    clear per frame (the boot skeleton's stand-in for the engine's draw
//!    pass — the real content / Lua / menu composition is sub-goal 5/6).
//!
//! Progress is logged to the browser console, and `status()` (the
//! `#[wasm_bindgen]` export) reports the boot state + the frame count.

use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};

use wasm_bindgen::prelude::wasm_bindgen;
use wasm_bindgen::JsCast;
use wasm_bindgen::JsValue;
use web_sys::console;

/// The page's canvas id (the `oh-render` emscripten backend looks up
/// `<canvas id="c">` — the 26c.1 selector contract).
const CANVAS_ID: &str = "c";
/// The canvas size (the 26c.1 spike's 640×480).
const WIDTH: u32 = 640;
const HEIGHT: u32 = 480;

/// The single-init guard (the 26c.1/26c.2 "main ran twice" artifact):
/// `true` once the boot has run — a re-entered `main` skips the second
/// init (only one canvas / one WebGL2 context / one rAF loop).
static BOOTED: AtomicBool = AtomicBool::new(false);
/// A live WebGL2 context was created (the 26c.1/26c.2 `glow-ok` flag).
static GL_OK: AtomicBool = AtomicBool::new(false);
/// The `request_animation_frame` frames rendered so far.
static FRAMES: AtomicI32 = AtomicI32::new(0);

/// The `#[wasm_bindgen]` probe (the 26c.1/26c.2 `status()` shape): the
/// boot state + the frame count — the supervisor's browser check calls it
/// after the module loads.
#[wasm_bindgen]
pub fn status() -> String {
    format!(
        "booted={};glow={};frames={}",
        BOOTED.load(Ordering::SeqCst),
        GL_OK.load(Ordering::SeqCst),
        FRAMES.load(Ordering::SeqCst)
    )
}

/// The browser console (the web build's log sink — the 26c.1 evidence
/// shape: progress lines as `console.log`).
fn log(msg: &str) {
    console::log_1(&JsValue::from_str(msg));
}

/// The boot (called from `main` AFTER `__wasm_call_ctors()`).
pub fn boot() {
    // The single-init guard (the 26c.1/26c.2 "main ran twice" artifact):
    // only the FIRST boot does the work — a re-entered `main` no-ops.
    if BOOTED.swap(true, Ordering::SeqCst) {
        log("oh-web: main re-entered - second init skipped (single-init guard)");
        return;
    }
    log("oh-web: boot started (after __wasm_call_ctors)");

    // 1. The canvas (the page MUST provide `<canvas id="c">`).
    let canvas = match web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id(CANVAS_ID))
        .map(|el| el.unchecked_into::<web_sys::HtmlCanvasElement>())
    {
        Some(canvas) => canvas,
        None => {
            log("oh-web: FATAL - no <canvas id=\"c\"> in the page");
            return;
        }
    };
    log(&format!(
        "oh-web: canvas found ({}x{})",
        canvas.width(),
        canvas.height()
    ));

    // 2. The `oh-render` emscripten window: the WebGL2 context (the
    //    `emscripten_webgl_*` C API — init attributes (major_version = 2)
    //    -> create ("#c") -> make_context_current — then `glow` via
    //    `from_loader_function_cstr`) + the DOM event listeners. Degrades
    //    to the headless frames when the canvas / WebGL2 is unavailable.
    let window = oh_render::Window::create(WIDTH, HEIGHT, "SSVOpenHexagon (web)");
    match window.gl() {
        Some(gl) => {
            GL_OK.store(true, Ordering::SeqCst);
            log(&format!(
                "oh-web: WebGL2 context live - {} (glow-ok)",
                gl.version_string().unwrap_or_default()
            ));
        }
        None => {
            log("oh-web: WARNING - no WebGL2 context (headless frames; check WebGL2 support)");
        }
    }

    // 3. The `request_animation_frame` loop (`oh-render`'s emscripten
    //    `Window::run` — RETURNS after scheduling the first frame; the
    //    browser owns the loop, the frames run until `Event::Closed`). An
    //    animated clear per frame: a slow orange <-> slate sweep (f32 —
    //    the boot skeleton's stand-in for the engine's draw pass;
    //    sub-goal 5/6 replaces it with the real content / menu /
    //    gameplay).
    log("oh-web: render loop starting (request_animation_frame)");
    window.run(|w| {
        let frame = FRAMES.fetch_add(1, Ordering::Relaxed);
        if frame == 0 {
            log("oh-web: first frame rendered");
        }
        // The sweep: `t` in -1..1 (one full sweep every ~314 frames).
        let t = (frame as f32 * 0.02).sin();
        let r = 0.55 + 0.30 * t;
        let g = 0.40 + 0.10 * t;
        let b = 0.15 - 0.15 * t;
        if let Some(gl) = w.gl() {
            gl.clear([r, g, b, 1.0]);
            gl.present();
        }
    });
}
