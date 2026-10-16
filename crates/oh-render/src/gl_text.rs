//! GL text rendering — `Text` → textured glyph quads (the GL text pass).
//!
//! [`TextRenderer`] is the persistent, interior-mutable GL text pipeline held
//! by [`crate::gl::GlParts`] (it outlives every frame, so the glyph textures
//! and the program are NOT rebuilt per frame):
//!
//! - a textured program — the SAME view mapping as [`crate::batching`]
//!   (`u_center` / `u_size` / `u_rotation`,
//!   `gl_Position = (2·R(u_rotation)·(p − u_center).x / u_size.x,
//!   −2·R(u_rotation)·(p − u_center).y / u_size.y)` — the y-negated NDC form,
//!   see [`crate::view::View::map_coords_to_pixel`]), plus a `u_tex`
//!   sampler; attributes: `a_position` (vec2), `a_color` (vec4, normalized
//!   `u8`), `a_uv` (vec2);
//! - a VAO + a dynamic VBO (per-glyph quad uploads);
//! - a glyph cache `HashMap<(font id, px, char), GlyphRef>` — each glyph is
//!   its own R8 texture (the `fontdue` alpha bitmap, uploaded with
//!   `UNPACK_ALIGNMENT = 1`) + the placement metrics. The font id is the
//!   `Rc` allocation identity ([`crate::font::Font::id`]) — the game loads
//!   its fonts once at startup, so the identity is stable for the cache's
//!   life.
//!
//! Layout (headless-testable, see [`layout_text`]): each `\n`-separated line
//! is laid out with the `fontdue` line / glyph metrics — the baseline starts
//! at the line ascent, a glyph's quad top = `baseline − (ymin + height)`,
//! left = `pen_x + xmin`, `pen_x += advance_width`; lines advance by the
//! `new_line_size`. The `Text` transform (`T(position)·R(rotation°)·
//! S(scale)·T(-origin)`, the same math as `crate::font::Text`) is applied to
//! every vertex CPU-side; the shader then applies the view mapping, so the
//! text aligns with the batched quads.
//!
//! Colour: the fragment multiplies the vertex colour's alpha by the glyph
//! coverage (`texture2D(u_tex, v_uv).r`) and the draw uses
//! `SRC_ALPHA, ONE_MINUS_SRC_ALPHA` blending (SFML's default blend state —
//! left enabled afterwards; the opaque batch pass is unaffected by it).
//! `outline_thickness > 0` draws an expanded copy of every quad behind in
//! `outline_color` (a constant local-space inset — the simple outline, not a
//! stroked one).
//!
//! Live GL only — the headless tests cover the layout / transform math
//! (D-011: the GPU half is verified by the live-context screenshot / smoke
//! checks).

use std::collections::HashMap;

use glow::{Context, HasContext};
use oh_math::ssvs::vector2::Vec2;
use oh_math::ssvu::math::to_rad;

use crate::error::RenderError;
use crate::font::{Font, Text};
use crate::view::View;

const VERTEX_SHADER_SOURCE: &str = r#"
attribute vec2 a_position;
attribute vec4 a_color;
attribute vec2 a_uv;
uniform vec2 u_center;
uniform vec2 u_size;
uniform float u_rotation;
varying vec4 v_color;
varying vec2 v_uv;
void main() {
    vec2 d = a_position - u_center;
    float s = sin(u_rotation);
    float c = cos(u_rotation);
    d = vec2(d.x * c - d.y * s, d.x * s + d.y * c);
    gl_Position = vec4(2.0 * d.x / u_size.x, -2.0 * d.y / u_size.y, 0.0, 1.0);
    v_color = a_color;
    v_uv = a_uv;
}
"#;

const FRAGMENT_SHADER_SOURCE: &str = r#"
uniform sampler2D u_tex;
varying vec4 v_color;
varying vec2 v_uv;
void main() {
    gl_FragColor = v_color * vec4(1.0, 1.0, 1.0, texture2D(u_tex, v_uv).r);
}
"#;

/// The persistent GL text pipeline (textured program + VAO/VBO + glyph
/// cache). Held behind a `RefCell` in [`crate::gl::GlParts`] (main-thread
/// only).
pub struct TextRenderer {
    vao: glow::VertexArray,
    vbo: glow::Buffer,
    program: glow::Program,
    u_center: glow::UniformLocation,
    u_size: glow::UniformLocation,
    u_rotation: glow::UniformLocation,
    u_tex: glow::UniformLocation,
    /// `(font id, px, char)` → the cached glyph.
    glyphs: HashMap<(usize, u32, char), GlyphRef>,
}

/// A cached glyph: the R8 alpha texture (`None` for empty / whitespace
/// bitmaps) + the `fontdue` metrics needed to place the quad and advance the
/// pen.
#[derive(Clone, Copy)]
struct GlyphRef {
    texture: Option<glow::Texture>,
    width: u32,
    height: u32,
    xmin: i32,
    ymin: i32,
    advance_width: f32,
}

/// One text vertex: `position` (2 f32) + `color` (4 normalized u8) + `uv`
/// (2 f32) — 20 bytes.
#[repr(C)]
#[derive(Clone, Copy)]
struct GlyphVertex {
    pos: [f32; 2],
    color: [u8; 4],
    uv: [f32; 2],
}

/// `sf::Vertex`-style stride: 2 f32 + 4 u8 + 2 f32 = 20 bytes.
const VERTEX_STRIDE: usize = std::mem::size_of::<GlyphVertex>();

/// A laid-out glyph quad in text-local space (origin at the text's top-left,
/// y DOWN — the SFML convention).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GlyphQuad {
    /// The glyph's character.
    pub character: char,
    /// The quad's left (text-local x).
    pub x: f32,
    /// The quad's top (text-local y).
    pub y: f32,
    /// The quad's width (the glyph bitmap's width).
    pub width: f32,
    /// The quad's height (the glyph bitmap's height).
    pub height: f32,
    /// `true` for whitespace / zero-bitmap glyphs (the draw skips them).
    pub empty: bool,
}

/// A laid-out line.
#[derive(Debug, Clone, PartialEq)]
pub struct LineLayout {
    /// The line's glyph quads (in string order).
    pub quads: Vec<GlyphQuad>,
    /// The line's total pen advance.
    pub width: f32,
}

/// Lay out `string` at `px` with `font`: the per-`\n`-line glyph quads —
/// the baseline starts at the line ascent, a glyph's quad top =
/// `baseline − (ymin + height)`, left = `pen_x + xmin`,
/// `pen_x += advance_width` — and the per-line pen advance. Lines advance by
/// the `fontdue` `new_line_size`.
#[must_use]
pub fn layout_text(font: &Font, string: &str, px: u32) -> Vec<LineLayout> {
    let px_f = px as f32;
    let (ascent, line_advance) = match font.line_metrics(px_f) {
        Some(lm) => (lm.ascent, lm.new_line_size),
        None => (px_f, px_f),
    };
    let mut lines = Vec::new();
    let mut pen_y = 0.0f32;
    for line in string.split('\n') {
        let baseline = pen_y + ascent;
        let mut quads = Vec::new();
        let mut pen_x = 0.0f32;
        for c in line.chars() {
            let m = font.glyph_metrics(c, px_f);
            let empty = m.width == 0 || m.height == 0;
            quads.push(GlyphQuad {
                character: c,
                x: pen_x + m.xmin as f32,
                y: baseline - (m.ymin as f32 + m.height as f32),
                width: m.width as f32,
                height: m.height as f32,
                empty,
            });
            pen_x += m.advance_width;
        }
        lines.push(LineLayout {
            quads,
            width: pen_x,
        });
        pen_y += line_advance;
    }
    lines
}

/// The `Text` transform on a text-local point: `T(position)·R(rotation°)·
/// S(scale)·T(-origin)` (the same math as `crate::font::Text`).
fn transform(p: Vec2, position: Vec2, origin: Vec2, scale: Vec2, rotation: f32) -> Vec2 {
    let t = Vec2::new(p.x - origin.x, p.y - origin.y);
    let s = Vec2::new(t.x * scale.x, t.y * scale.y);
    let rad = to_rad(rotation);
    let (sin, cos) = rad.sin_cos();
    Vec2::new(
        position.x + s.x * cos - s.y * sin,
        position.y + s.x * sin + s.y * cos,
    )
}

fn vertex(pos: Vec2, color: [u8; 4], u: f32, v: f32) -> GlyphVertex {
    GlyphVertex {
        pos: [pos.x, pos.y],
        color,
        uv: [u, v],
    }
}

impl TextRenderer {
    /// Build the textured program + VAO/VBO.
    ///
    /// # Errors
    /// [`RenderError::InvalidData`] if a shader fails to compile/link on
    /// this context (or a GL object cannot be allocated).
    pub fn new(gl: &Context) -> Result<Self, RenderError> {
        let vs = compile_shader(gl, glow::VERTEX_SHADER, VERTEX_SHADER_SOURCE)?;
        let fs = compile_shader(gl, glow::FRAGMENT_SHADER, FRAGMENT_SHADER_SOURCE)?;

        let program = unsafe { gl.create_program() }.map_err(|_| RenderError::InvalidData)?;
        unsafe {
            gl.attach_shader(program, vs);
            gl.attach_shader(program, fs);
            gl.bind_attrib_location(program, 0, "a_position");
            gl.bind_attrib_location(program, 1, "a_color");
            gl.bind_attrib_location(program, 2, "a_uv");
            gl.link_program(program);
        }
        if !unsafe { gl.get_program_link_status(program) } {
            return Err(RenderError::InvalidData);
        }
        unsafe {
            gl.delete_shader(vs);
            gl.delete_shader(fs);
        }

        let vao = unsafe { gl.create_vertex_array() }.map_err(|_| RenderError::InvalidData)?;
        let vbo = unsafe { gl.create_buffer() }.map_err(|_| RenderError::InvalidData)?;
        unsafe {
            gl.bind_vertex_array(Some(vao));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
            gl.enable_vertex_attrib_array(0);
            gl.vertex_attrib_pointer_f32(0, 2, glow::FLOAT, false, VERTEX_STRIDE as i32, 0);
            gl.enable_vertex_attrib_array(1);
            gl.vertex_attrib_pointer_f32(
                1,
                4,
                glow::UNSIGNED_BYTE,
                true,
                VERTEX_STRIDE as i32,
                std::mem::size_of::<[f32; 2]>() as i32,
            );
            gl.enable_vertex_attrib_array(2);
            gl.vertex_attrib_pointer_f32(
                2,
                2,
                glow::FLOAT,
                false,
                VERTEX_STRIDE as i32,
                (std::mem::size_of::<[f32; 2]>() + std::mem::size_of::<[u8; 4]>()) as i32,
            );
            gl.bind_vertex_array(None);
            gl.bind_buffer(glow::ARRAY_BUFFER, None);
        }

        Ok(Self {
            vao,
            vbo,
            program,
            u_center: location_ok(gl, program, "u_center"),
            u_size: location_ok(gl, program, "u_size"),
            u_rotation: location_ok(gl, program, "u_rotation"),
            u_tex: location_ok(gl, program, "u_tex"),
            glyphs: HashMap::new(),
        })
    }

    /// Lay out + draw `text` under `view` (the C++ `window->draw(text)`).
    /// Whitespace / empty glyphs are skipped; `outline_thickness > 0` draws
    /// the outline pass (the expanded copy in `outline_color`) first, the
    /// fill pass second.
    pub fn draw(&mut self, gl: &Context, text: &Text, view: View) {
        if text.is_empty() {
            return;
        }
        let font = text.get_font();
        let px = text.get_character_size();
        let px_f = px as f32;
        let (ascent, line_advance) = match font.line_metrics(px_f) {
            Some(lm) => (lm.ascent, lm.new_line_size),
            None => (px_f, px_f),
        };
        // Walk the string with the CACHED glyph metrics (the same layout
        // math as [`layout_text`]): the non-empty glyph quads (text-local
        // space) with their cached glyph (texture + metrics).
        let mut quads: Vec<(GlyphRef, f32, f32, f32, f32)> = Vec::new();
        let mut pen_y = 0.0f32;
        for line in text.get_string().split('\n') {
            let baseline = pen_y + ascent;
            let mut pen_x = 0.0f32;
            for c in line.chars() {
                let g = self.glyph(gl, font, px, c);
                if g.width > 0 && g.height > 0 {
                    let left = pen_x + g.xmin as f32;
                    let top = baseline - (g.ymin as f32 + g.height as f32);
                    quads.push((
                        g,
                        left,
                        top,
                        left + g.width as f32,
                        top + g.height as f32,
                    ));
                }
                pen_x += g.advance_width;
            }
            pen_y += line_advance;
        }
        if quads.is_empty() {
            return;
        }

        let position = text.get_position();
        let origin = text.get_origin();
        let scale = text.get_scale();
        let rotation = text.get_rotation();
        let fill = {
            let c = text.get_fill_color();
            [c.r, c.g, c.b, c.a]
        };
        let outline = {
            let c = text.get_outline_color();
            [c.r, c.g, c.b, c.a]
        };
        let outline_t = text.get_outline_thickness();

        unsafe {
            gl.use_program(Some(self.program));
            gl.uniform_2_f32(
                Some(&self.u_center),
                view.get_center().x,
                view.get_center().y,
            );
            gl.uniform_2_f32(Some(&self.u_size), view.get_size().x, view.get_size().y);
            gl.uniform_1_f32(Some(&self.u_rotation), to_rad(view.get_rotation()));
            gl.active_texture(glow::TEXTURE0);
            gl.uniform_1_i32(Some(&self.u_tex), 0);
            gl.bind_vertex_array(Some(self.vao));
            // SFML's default blend state — left enabled afterwards (the
            // opaque batch pass is unaffected by SRC_ALPHA blending).
            gl.enable(glow::BLEND);
            gl.blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);

            // Pass 1: the outline (the expanded copy behind).
            if outline_t > 0.0 {
                for (g, x0, y0, x1, y1) in &quads {
                    self.draw_quad(
                        gl,
                        *g,
                        x0 - outline_t,
                        y0 - outline_t,
                        x1 + outline_t,
                        y1 + outline_t,
                        outline,
                        position,
                        origin,
                        scale,
                        rotation,
                    );
                }
            }
            // Pass 2: the fill.
            for (g, x0, y0, x1, y1) in &quads {
                self.draw_quad(gl, *g, *x0, *y0, *x1, *y1, fill, position, origin, scale, rotation);
            }

            gl.bind_vertex_array(None);
            gl.bind_texture(glow::TEXTURE_2D, None);
            gl.use_program(None);
        }
    }

    /// Upload + draw one glyph quad (its texture on unit 0).
    #[allow(clippy::too_many_arguments)]
    fn draw_quad(
        &self,
        gl: &Context,
        glyph: GlyphRef,
        x0: f32,
        y0: f32,
        x1: f32,
        y1: f32,
        color: [u8; 4],
        position: Vec2,
        origin: Vec2,
        scale: Vec2,
        rotation: f32,
    ) {
        let Some(tex) = glyph.texture else {
            return;
        };
        // The 4 corners (text-local, y down) → the `Text` transform
        // (CPU-side) → the shader's view mapping.
        let tl = transform(Vec2::new(x0, y0), position, origin, scale, rotation);
        let tr = transform(Vec2::new(x1, y0), position, origin, scale, rotation);
        let bl = transform(Vec2::new(x0, y1), position, origin, scale, rotation);
        let br = transform(Vec2::new(x1, y1), position, origin, scale, rotation);
        // The texture was uploaded top-row-first (the `fontdue` bitmap), so
        // the glyph's TOP maps to v = 0 (the GL texture's first row).
        let verts = [
            vertex(tl, color, 0.0, 0.0),
            vertex(tr, color, 1.0, 0.0),
            vertex(bl, color, 0.0, 1.0),
            vertex(tr, color, 1.0, 0.0),
            vertex(br, color, 1.0, 1.0),
            vertex(bl, color, 0.0, 1.0),
        ];
        let bytes = unsafe {
            std::slice::from_raw_parts(verts.as_ptr() as *const u8, verts.len() * VERTEX_STRIDE)
        };
        unsafe {
            gl.bind_texture(glow::TEXTURE_2D, Some(tex));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.vbo));
            gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, bytes, glow::DYNAMIC_DRAW);
            gl.draw_arrays(glow::TRIANGLES, 0, verts.len() as i32);
        }
    }

    /// The cached glyph for `(font, px, c)` — rasterized + uploaded on a
    /// miss (an empty / whitespace bitmap is cached with `texture = None`).
    fn glyph(&mut self, gl: &Context, font: &Font, px: u32, c: char) -> GlyphRef {
        let key = (font.id(), px, c);
        if let Some(g) = self.glyphs.get(&key) {
            return *g;
        }
        let (m, bitmap) = font.rasterize(c, px as f32);
        let mut texture = None;
        if m.width > 0 && m.height > 0 {
            unsafe {
                // The `fontdue` bitmaps are tightly packed (row stride =
                // `width` bytes) — alignment 1, not the default 4.
                gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);
                if let Ok(tex) = gl.create_texture() {
                    gl.bind_texture(glow::TEXTURE_2D, Some(tex));
                    gl.tex_image_2d(
                        glow::TEXTURE_2D,
                        0,
                        glow::R8 as i32,
                        m.width as i32,
                        m.height as i32,
                        0,
                        glow::RED,
                        glow::UNSIGNED_BYTE,
                        glow::PixelUnpackData::Slice(Some(&bitmap)),
                    );
                    gl.tex_parameter_i32(
                        glow::TEXTURE_2D,
                        glow::TEXTURE_MIN_FILTER,
                        glow::NEAREST as i32,
                    );
                    gl.tex_parameter_i32(
                        glow::TEXTURE_2D,
                        glow::TEXTURE_MAG_FILTER,
                        glow::NEAREST as i32,
                    );
                    gl.tex_parameter_i32(
                        glow::TEXTURE_2D,
                        glow::TEXTURE_WRAP_S,
                        glow::CLAMP_TO_EDGE as i32,
                    );
                    gl.tex_parameter_i32(
                        glow::TEXTURE_2D,
                        glow::TEXTURE_WRAP_T,
                        glow::CLAMP_TO_EDGE as i32,
                    );
                    gl.bind_texture(glow::TEXTURE_2D, None);
                    texture = Some(tex);
                }
            }
        }
        let g = GlyphRef {
            texture,
            width: m.width as u32,
            height: m.height as u32,
            xmin: m.xmin,
            ymin: m.ymin,
            advance_width: m.advance_width,
        };
        self.glyphs.insert(key, g);
        g
    }
}

fn compile_shader(gl: &Context, kind: u32, source: &str) -> Result<glow::Shader, RenderError> {
    let shader = unsafe { gl.create_shader(kind) }.map_err(|_| RenderError::InvalidData)?;
    unsafe {
        gl.shader_source(shader, source);
        gl.compile_shader(shader);
    }
    if !unsafe { gl.get_shader_compile_status(shader) } {
        return Err(RenderError::InvalidData);
    }
    Ok(shader)
}

fn location_ok(gl: &Context, program: glow::Program, name: &str) -> glow::UniformLocation {
    let loc = unsafe { gl.get_uniform_location(program, name) };
    loc.unwrap_or_else(|| panic!("GL: missing uniform '{name}'"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn font() -> Font {
        // _RELEASE/Assets/OpenSquare-Regular.ttf (repo root relative to this
        // crate — the same fixture as `font.rs`).
        let root = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../_RELEASE/Assets/OpenSquare-Regular.ttf"
        );
        Font::load(Path::new(root)).expect("OpenSquare-Regular.ttf must load")
    }

    #[test]
    fn layout_places_glyphs_on_the_baseline_and_advances_the_pen() {
        let f = font();
        let lines = layout_text(&f, "AB", 32);
        assert_eq!(lines.len(), 1);
        let lm = f.line_metrics(32.0).expect("line metrics");
        let quads = &lines[0].quads;
        assert_eq!(quads.len(), 2);
        for q in quads {
            assert!(!q.empty);
            assert!(q.width > 0.0);
            assert!(q.height > 0.0);
            // The quad top sits at baseline − (ymin + height).
            let m = f.glyph_metrics(q.character, 32.0);
            let want_top = lm.ascent - (m.ymin as f32 + m.height as f32);
            assert!((q.y - want_top).abs() < 1e-3);
        }
        // The first glyph's left = its xmin (the pen starts at 0); the second
        // sits one advance_width further.
        let m_a = f.glyph_metrics('A', 32.0);
        let m_b = f.glyph_metrics('B', 32.0);
        assert!((quads[0].x - m_a.xmin as f32).abs() < 1e-6);
        assert!((quads[1].x - (m_a.advance_width + m_b.xmin as f32)).abs() < 1e-3);
        // The line width is the total advance.
        assert!((lines[0].width - (m_a.advance_width + m_b.advance_width)).abs() < 1e-3);
    }

    #[test]
    fn layout_skips_whitespace_and_stacks_lines() {
        let f = font();
        let lines = layout_text(&f, "A B\nC", 32);
        assert_eq!(lines.len(), 2);
        // The space's quad is empty (no bitmap) but still advances the pen.
        let space = &lines[0].quads[1];
        assert_eq!(space.character, ' ');
        assert!(space.empty);
        assert!(space.width == 0.0 || space.height == 0.0);
        // The second line's baseline sits one new_line_size below.
        let lm = f.line_metrics(32.0).expect("line metrics");
        let c = f.glyph_metrics('C', 32.0);
        let want = lm.ascent + lm.new_line_size - (c.ymin as f32 + c.height as f32);
        assert!((lines[1].quads[0].y - want).abs() < 1e-3);
    }

    #[test]
    fn transform_matches_the_font_text_math() {
        // Identity: no position / origin / scale / rotation → unchanged.
        let p = Vec2::new(3.0, 4.0);
        let t = transform(p, Vec2::new(0.0, 0.0), Vec2::new(0.0, 0.0), Vec2::new(1.0, 1.0), 0.0);
        assert!((t.x - 3.0).abs() < 1e-6 && (t.y - 4.0).abs() < 1e-6);
        // Scale 2x: (3,4) → (6,8).
        let t = transform(p, Vec2::new(0.0, 0.0), Vec2::new(0.0, 0.0), Vec2::new(2.0, 2.0), 0.0);
        assert!((t.x - 6.0).abs() < 1e-6 && (t.y - 8.0).abs() < 1e-6);
        // 90° rotation (SFML degrees): (1,0) → (0,1).
        let t = transform(
            Vec2::new(1.0, 0.0),
            Vec2::new(0.0, 0.0),
            Vec2::new(0.0, 0.0),
            Vec2::new(1.0, 1.0),
            90.0,
        );
        assert!(t.x.abs() < 1e-6 && (t.y - 1.0).abs() < 1e-6);
        // Origin subtracts before the scale: (3,4), origin (1,2), scale 2x →
        // (4,4).
        let t = transform(
            p,
            Vec2::new(0.0, 0.0),
            Vec2::new(1.0, 2.0),
            Vec2::new(2.0, 2.0),
            0.0,
        );
        assert!((t.x - 4.0).abs() < 1e-6 && (t.y - 4.0).abs() < 1e-6);
    }
}
