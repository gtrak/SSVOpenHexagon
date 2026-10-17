//! The emscripten (web) boot skeleton (issue 26c W0, D-029) — called from
//! `main` AFTER `__wasm_call_ctors()` (the 26c.2 trap fix): the MINIMAL web
//! platform boot — get-or-create the page's `<canvas id="c">`, create a
//! WebGL2 context through the `emscripten_webgl_*` C API (init attributes
//! with `major_version = 2` → create → make current), clear it a colour, and
//! log progress to the browser console. `oh-render`'s emscripten backend (the
//! real GL backend over `glow`) is W1; the real engine composition is W2.

use std::sync::atomic::{AtomicBool, Ordering};

use wasm_bindgen::prelude::wasm_bindgen;
use wasm_bindgen::JsCast;
use wasm_bindgen::JsValue;
use web_sys::console;

/// The GL `COLOR_BUFFER_BIT` (the `clear(mask)` bitfield — web-sys 0.3.106
/// exposes `clear(mask: u32)`, no `ClearColorBits` type).
const COLOR_BUFFER_BIT: u32 = 0x0000_0400;

/// The single-init guard (the 26c.1/26c.2 "main ran twice" artifact):
/// `true` once the boot has run — a re-entered `main` skips the second init
/// (one canvas / one WebGL2 context).
static BOOTED: AtomicBool = AtomicBool::new(false);
/// `true` once the WebGL2 context was created + made current.
static GL_OK: AtomicBool = AtomicBool::new(false);

/// The `#[wasm_bindgen]` probe (the 26c.1/26c.2 `status()` shape): the boot
/// state + whether the WebGL2 context came up — the supervisor's browser
/// check calls it after the module loads.
#[wasm_bindgen]
pub fn status() -> String {
    format!(
        "booted={};webgl2={}",
        BOOTED.load(Ordering::SeqCst),
        GL_OK.load(Ordering::SeqCst)
    )
}

/// The browser console (the web build's log sink — the 26c.1 evidence shape:
/// progress lines as `console.log`).
fn log(msg: &str) {
    console::log_1(&JsValue::from_str(msg));
}

/// The emscripten C API (emscripten.h) behind the web GL bootstrap (26c.1):
/// the WebGL2 context is created + made current through it BEFORE any GL
/// call. (The `emscripten_webgl_get_proc_address` glow-loader route is W1 —
/// this W0 skeleton only needs the context + a clear.)
mod emscripten_gl {
    /// `EMSCRIPTEN_WEBGL_CONTEXT_ATTRIBUTES` (emscripten.h) — the 15-field
    /// WebGL context-attribute struct; `init_context_attributes` fills it
    /// with the defaults, the bootstrap then sets `major_version = 2`.
    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct ContextAttributes {
        pub major_version: i32,
        pub minor_version: i32,
        pub alpha: i32,
        pub depth: i32,
        pub stencil: i32,
        pub antialias: i32,
        pub premultiplied_alpha: i32,
        pub preserve_drawing_buffer: i32,
        pub power_preference: i32,
        pub fail_if_major_performance_caveat: i32,
        pub render_to_texture: i32,
        pub depth_and_stencil: i32,
        pub stencil_back: i32,
        pub depth_back: i32,
        pub restore: i32,
    }

    extern "C" {
        pub fn emscripten_webgl_init_context_attributes(attrs: *mut ContextAttributes);
        /// `canvas` is a CSS SELECTOR (26c.1 gotcha: `"#c"`, not a bare id —
        /// it does `querySelector`).
        pub fn emscripten_webgl_create_context(
            canvas: *const std::ffi::c_char,
            attrs: *const ContextAttributes,
        ) -> i32;
        pub fn emscripten_webgl_make_context_current(ctx: i32) -> i32;
    }
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

    // Get-or-create the page's <canvas id="c"> (the 26c.1 selector contract).
    let window = web_sys::window().expect("oh-web: no window (not running in a browser)");
    let document = window.document().expect("oh-web: no document");
    let canvas: web_sys::HtmlCanvasElement = match document.get_element_by_id("c") {
        Some(el) => el
            .dyn_into::<web_sys::HtmlCanvasElement>()
            .expect("oh-web: <canvas id=\"c\"> is not a canvas element"),
        None => {
            let el = document
                .create_element("canvas")
                .expect("oh-web: create_element(\"canvas\") failed");
            el.set_id("c");
            let canvas: web_sys::HtmlCanvasElement = el
                .dyn_into::<web_sys::HtmlCanvasElement>()
                .expect("oh-web: created element is not a canvas");
            document
                .body()
                .expect("oh-web: no <body>")
                .append_child(&canvas)
                .expect("oh-web: append_child(<canvas>) failed");
            canvas
        }
    };
    canvas.set_width(640);
    canvas.set_height(480);
    log("oh-web: <canvas id=\"c\"> ready (640x480)");

    // Create the WebGL2 context via the emscripten_webgl_* C API (26c.1
    // recipe: the -sMAX_WEBGL_VERSION=2 link flag + the major_version = 2
    // request — without it emscripten logs "WebGL 2 requested but only
    // WebGL 1 is supported" and create returns 0).
    let mut attrs = emscripten_gl::ContextAttributes {
        major_version: 0,
        minor_version: 0,
        alpha: 0,
        depth: 0,
        stencil: 0,
        antialias: 0,
        premultiplied_alpha: 0,
        preserve_drawing_buffer: 0,
        power_preference: 0,
        fail_if_major_performance_caveat: 0,
        render_to_texture: 0,
        depth_and_stencil: 0,
        stencil_back: 0,
        depth_back: 0,
        restore: 0,
    };
    unsafe { emscripten_gl::emscripten_webgl_init_context_attributes(&mut attrs) };
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
            "oh-web: emscripten_webgl_create_context(\"#c\") failed \
             (WebGL2 unavailable - a <canvas id=\"c\"> + the \
             -sMAX_WEBGL_VERSION=2 link flag are required)"
        );
        return;
    }
    if unsafe { emscripten_gl::emscripten_webgl_make_context_current(ctx) } != 0 {
        eprintln!("oh-web: emscripten_webgl_make_context_current failed");
        return;
    }
    GL_OK.store(true, Ordering::SeqCst);
    log("oh-web: WebGL2 context created + current (via emscripten_webgl_*)");

    // Clear the canvas a colour (the W0 "a clear" acceptance: prove the
    // context is drawable through the web-sys WebGL2 binding). web-sys
    // 0.3.106 has no get_context_as_webgl2 convenience — get the JS context
    // object and cast it to the WebGL2 binding.
    let gl_obj = canvas
        .get_context("webgl2")
        .expect("oh-web: get_context(\"webgl2\") threw")
        .expect("oh-web: get_context(\"webgl2\") returned null (WebGL2 unavailable)");
    let gl: web_sys::WebGl2RenderingContext = gl_obj.unchecked_into();
    gl.clear_color(0.2, 0.5, 0.9, 1.0);
    gl.clear(COLOR_BUFFER_BIT);
    log("oh-web: cleared the canvas (a colour) - W0 boot skeleton complete");
}
