//! Golden-frame guard for the `oh-render` drawing layer (issue 11).
//!
//! A COARSE regression guard only (mapping verification item: "a
//! golden-frame test (offscreen render of a fixed level/seed/frame) compared
//! to a reference image generated once"):
//! - it uploads a fixed vertex scene through the REAL GL pipeline
//!   (`GlBatchUploader` → dynamic VBO → `glDrawArrays`) on a live context
//!   and reads back a few pixels;
//! - it is sensitive to driver / AA / colour-management differences, so the
//!   tolerance is coarse (dominant colour, not per-pixel law) — it must
//!   never be the sole gate (the vertex-order/sequence assertions in
//!   `graphics.rs` are the fine evidence);
//! - per D-011 there is no fake rasterizer: when no display / GPU is
//!   available the test prints a reason and skips (the gate stays green in
//!   headless CI).
//!
//! `TODO(later issue)`: once the sprite-quad / glyph-atlas `DrawTarget`
//! lands, extend this to a full `draw_frame` offscreen frame (the
//! fixed-function fragment-shader model of the pack shaders also needs to
//! be emulated before pixel-law comparisons are meaningful).

use glow::HasContext;

use oh_render::batching::GlBatchUploader;
use oh_render::draw::{RenderStates, VertexBatch};
use oh_render::gl::GlBackend;
use oh_render::graphics::{draw_background, RenderStage};
use oh_render::view::View;
use oh_render::Color;
use oh_render::Vec2;

/// Fixed scene: 6 background sectors (red/green/blue palette) + one white
/// quad in the centre — the same inputs the `graphics.rs` unit tests use.
fn fixed_scene() -> (VertexBatch, View, Vec2) {
    let mut batch = VertexBatch::new_tris();
    let colors = [
        Color::new(220, 30, 30, 255),
        Color::new(30, 220, 30, 255),
        Color::new(30, 30, 220, 255),
    ];
    // Background sectors (the BackgroundTris stage content).
    draw_background(
        &mut batch,
        Vec2::new(0.0, 0.0),
        6,
        false,
        false,
        &colors,
        10000.0,
        0.0,
    );
    // A 2×2 white quad centred at the origin (WallQuads-stage-like content).
    let white = Color::WHITE;
    batch.push_quad(
        white,
        Vec2::new(-1.0, -1.0),
        Vec2::new(-1.0, 1.0),
        Vec2::new(1.0, 1.0),
        Vec2::new(1.0, -1.0),
    );
    // The background-camera view for an 800×450 window at zoom 1 (the
    // scene covers the window; the white quad is 4px wide in world units
    // at this view scale → sub-pixel, so the centre pixel stays the
    // background colour: the coarse guard checks the dominant colour).
    let view = View::new(Vec2::new(0.0, 0.0), Vec2::new(800.0, 450.0));
    (batch, view, Vec2::new(400.0, 225.0))
}

fn pixel_at(gl: &glow::Context, x: u32, y: u32) -> [u8; 4] {
    let mut pixel = [0u8; 4];
    unsafe {
        gl.read_pixels(
            x as i32,
            y as i32,
            1,
            1,
            glow::RGBA,
            glow::UNSIGNED_BYTE,
            glow::PixelPackData::Slice(Some(&mut pixel)),
        );
    }
    pixel
}

/// `None` = skip (headless); `Some(Ok(()))` = guard passed; `Some(Err(e))`
/// = real failure on a live context.
fn run_guard() -> Option<Result<(), String>> {
    // 0 = no MSAA request: the guard's context is a probe, not the game's
    // window (the 050 level wiring — `pick_msaa_config`'s level-0 arm is
    // the pre-050 first-config pick, unchanged).
    let backend = GlBackend::try_create(800, 450, "oh-render golden frame", 0);
    let Some(backend) = backend else {
        eprintln!(
            "SKIP golden_frames: no display/GPU available (headless) — \
             the coarse GPU guard cannot run; the fine vertex-order \
             assertions live in the oh-render unit tests (D-011)."
        );
        return None;
    };

    let gl = &backend.parts.gl;
    let size = (backend.parts.width, backend.parts.height);

    // Offscreen target: RGBA8 renderbuffer-backed texture at the window
    // size (glClear → draw → glReadPixels from the FBO).
    let rbo = match unsafe { gl.create_renderbuffer() } {
        Ok(r) => r,
        Err(e) => return Some(Err(format!("renderbuffer: {e}"))),
    };
    unsafe {
        gl.bind_renderbuffer(glow::RENDERBUFFER, Some(rbo));
        gl.renderbuffer_storage(
            glow::RENDERBUFFER,
            glow::RGBA8,
            size.0 as i32,
            size.1 as i32,
        );
    }
    let fbo = match unsafe { gl.create_framebuffer() } {
        Ok(f) => f,
        Err(e) => return Some(Err(format!("fbo: {e}"))),
    };
    unsafe {
        gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
        gl.framebuffer_renderbuffer(
            glow::FRAMEBUFFER,
            glow::COLOR_ATTACHMENT0,
            glow::RENDERBUFFER,
            Some(rbo),
        );
        gl.viewport(0, 0, size.0 as i32, size.1 as i32);
        gl.clear_color(0.0, 0.0, 0.0, 1.0);
        gl.clear(glow::COLOR_BUFFER_BIT);
    }
    if !unsafe { gl.check_framebuffer_status(glow::FRAMEBUFFER) == glow::FRAMEBUFFER_COMPLETE } {
        return Some(Err("FBO not complete".into()));
    }

    let mut uploader = match GlBatchUploader::new(gl) {
        Ok(u) => u,
        Err(e) => return Some(Err(format!("uploader: {}", e.message()))),
    };

    let (batch, view, center) = fixed_scene();
    uploader.draw(gl, batch.as_slice(), view);

    // ---- coarse checks ---------------------------------------------------
    // 1. The centre pixel is a palette colour (the background covers the
    //    whole window; with the 1.0001 sector overlap the boundary pixel
    //    at the origin is one of the three palette colours — NOT black).
    let centre = pixel_at(gl, center.x as u32, center.y as u32);
    let palette: [[u8; 4]; 3] = [[220, 30, 30, 255], [30, 220, 30, 255], [30, 30, 220, 255]];
    // Allow ±16 per channel (driver/dithering tolerance).
    let in_palette = |p: [u8; 4]| {
        palette
            .iter()
            .any(|c| (0..4).all(|i| (p[i] as i32 - c[i] as i32).abs() <= 16))
    };
    if !in_palette(centre) {
        return Some(Err(format!(
            "centre pixel {centre:?} not within ±16 of any palette colour"
        )));
    }

    // 2. A far corner is a palette colour too (the sectors extend to
    //    radius 10000).
    let corner = pixel_at(gl, size.0 - 1, size.1 - 1);
    if !in_palette(corner) {
        return Some(Err(format!(
            "corner pixel {corner:?} not within ±16 of any palette colour"
        )));
    }

    // 3. The render-stage surface this scene was built for is intact
    //    (catches accidental enum drift that would change the scene
    //    layout's stage binding).
    assert_eq!(RenderStage::BackgroundTris as usize, 0);
    assert_eq!(RenderStage::WallQuads as usize, 4);
    assert_eq!(RenderStates::with_shader(1).shader, Some(1));

    Some(Ok(()))
}

#[test]
fn golden_frame_coarse_guard() {
    match run_guard() {
        None => {} // skipped with a printed reason (headless)
        Some(Ok(())) => {}
        Some(Err(e)) => panic!("golden frame guard failed on a live context: {e}"),
    }
}
