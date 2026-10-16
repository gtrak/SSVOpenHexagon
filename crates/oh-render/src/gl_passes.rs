//! The GL draw passes (issue-025c SG1–SG4) — the texture upload + sampler
//! (SG1), the sprite pass (SG2), the font-atlas text pass (SG3) and the
//! rectangle pass's vertex build (SG4), on a live `glow` context.
//!
//! The CPU-side layout math (the glyph atlas, the text→quad, the
//! sprite→quad, the rectangle→vertex) lives in [`crate::glyphs`]
//! (host-testable, no GL); this module owns the GL objects (the textured
//! quad program + VAO/VBO, the texture objects) and the per-draw upload +
//! `glDrawArrays`. The GL state it touches is re-applied on EVERY draw
//! call (the C++ `RenderStates` discipline — the `GlDrawTarget` sets the
//! blend before each pass; this module sets the program / texture / VAO /
//! uniforms for its own draws).
//!
//! # The premultiplied-alpha decision (the spec's SG1)
//!
//! **Straight alpha, matching SFML**: the RGBA8 pixels (the PNG-decoded
//! texture bytes) are uploaded to `gl.tex_image_2d` AS-IS — no
//! premultiplication anywhere. The blend function is SFML's
//! `BlendMode::Alpha` (`SRC_ALPHA, ONE_MINUS_SRC_ALPHA`, the
//! `sf::RenderStates::Default` the C++'s `window->draw` calls all use),
//! and the fragment output is the plain `texture × vertexColor` product.
//! A premultiplied pipeline (converting the uploads + the blend factors)
//! would change the blended edges of EVERY sprite/text against the C++ —
//! the port matches SFML, not "modern GL style".
//!
//! # The texture convention (shared with `crate::glyphs`)
//!
//! Uploads are UNFLIPPED (the image row 0 = the GL texture's row 0) and
//! the geometry addresses the texture with a TOP-DOWN UV (`v = 0` at the
//! texture's top edge) — SFML's own Image→Texture→geometry convention
//! (`Texture::loadFromFile` uploads the image rows as-is; `Text::draw` /
//! `Sprite`'s geometry address the texture top-down). No row flip anywhere
//! in the
//! pipeline (the screenshot readback's flip, `gl::flip_rows`, is a
//! separate concern — the framebuffer READ, not the uploads).
//!
//! # The texture cache (SG1's "pool")
//!
//! On-demand upload at FIRST draw: the `Texture` pool's semantics are the
//! port's `oh_render::texture::Texture` objects (long-lived, owned by the
//! draw states — the menu's bars, the game's key icons / particle
//! textures, `menu_draw.rs` / `bridge.rs`); [`GlPasses::textures`] maps a
//! `(size, pixel-pointer)` key to the `glow::Texture` and re-uploads
//! nothing while the pool object lives. **Eviction** (the pool semantics'
//! bound): the pool objects live until their owner drops them — but the
//! game's PARTICLE sprites clone their (placeholder) texture every frame
//! (the `bridge.rs` `sprite_from_state` — each clone is a fresh pixel
//! allocation, a fresh key): a pointer-keyed entry per clone would grow
//! the cache without bound. The cap (`TEX_CACHE_CAP`, below): on overflow
//! every cached GL texture is DELETED (the object freed, not just the
//! entry dropped) and the next draw re-uploads — the re-upload is the
//! rare overflow path, and the stable long-lived textures (the menu's
//! bars, the key icons) re-upload in the same frame they are evicted.
//! No LRU: the C++'s `sf::Texture` pool does not evict either (a texture
//! lives until its owner destroys it) — the cap is the port's
//! short-lived-clone bound, not an SFML mechanism.
//!
//! # Filtering
//!
//! `LINEAR` + `CLAMP_TO_EDGE` when the texture's `smooth` flag is set
//! (the SFML `setSmooth(true)` — the C++'s `initKeyIcons` sets it on the
//! key-icon textures), `NEAREST` + `CLAMP_TO_EDGE` otherwise (the flag's
//! off state — the menu's placeholder textures' default). The font atlas
//! is always `LINEAR` (SFML's font texture is smoothed; the 1-px zero
//! padding makes the bilinear edge sample land on zero alpha, `glyphs.rs`).

//!
//! # The view transform (the 025c orientation finding)
//!
//! The quad vertex shader applies the SFML top-left-origin view:
//! `ndc = (2·(R·(p − center)).x / size.x, −2·(R·(p − center)).y /
//! size.y)` — the negative `y` is the world's down-is-`+y` against GL's
//! up-is-`+y` NDC (`View::map_coords_to_pixel`'s exact transform).
//! `crate::batching`'s pass-through shader had the `y` sign flipped (the
//! scene drawn upside-down — undiscovered before 025c: only
//! vertically-symmetric content had been drawn); it carries the same
//! corrected transform now (the 025c fix, `batching.rs`).

use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use glow::{Context, HasContext};
use oh_math::ssvs::vector2::Vec2;

use crate::draw::Vertex;
use crate::glyphs::{self, GlyphStore, TextQuad};
use crate::texture::{Sprite, Texture};
use crate::types::RectangleShape;
use crate::view::View;

// ---------------------------------------------------------------------------
// The texture cache (SG1).
// ---------------------------------------------------------------------------

/// The pool key: `(size, pixel-buffer pointer)` — on-demand upload at
/// first draw, module docs. (`Texture`'s `Image` pixels are a stable
/// `Vec<u8>` for the texture's lifetime; the pointer keys the content.)
struct TexKey {
    w: u32,
    h: u32,
    ptr: *const u8,
}

impl Hash for TexKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.w.hash(state);
        self.h.hash(state);
        (self.ptr as usize).hash(state);
    }
}

impl PartialEq for TexKey {
    fn eq(&self, o: &Self) -> bool {
        self.w == o.w && self.h == o.h && self.ptr == o.ptr
    }
}

impl Eq for TexKey {}

/// The texture cache's cap (the module docs' eviction pin): the game's
/// particle sprites clone their placeholder texture per frame (a fresh
/// pixel allocation = a fresh key per clone) — the cap bounds the map +
/// the live GL texture count; on overflow the cache is deleted + cleared
/// (the next draw re-uploads).
const TEX_CACHE_CAP: usize = 256;

// ---------------------------------------------------------------------------
// The textured-quad program (SG2).
// ---------------------------------------------------------------------------

/// The quad vertex: `position (vec2) + uv (vec2) + color (vec4)` — 8 f32.
#[repr(C)]
#[derive(Clone, Copy)]
struct QuadVertex {
    pos: [f32; 2],
    uv: [f32; 2],
    color: [f32; 4],
}

const QUAD_VERTEX_STRIDE: usize = std::mem::size_of::<QuadVertex>();

const QUAD_VERTEX_SHADER_SOURCE: &str = r#"
attribute vec2 a_position;
attribute vec2 a_uv;
attribute vec4 a_color;
uniform vec2 u_center;
uniform vec2 u_size;
uniform float u_rotation;
varying vec2 v_uv;
varying vec4 v_color;
void main() {
    vec2 d = a_position - u_center;
    float s = sin(u_rotation);
    float c = cos(u_rotation);
    d = vec2(d.x * c - d.y * s, d.x * s + d.y * c);
    gl_Position = vec4(2.0 * d.x / u_size.x, -2.0 * d.y / u_size.y, 0.0, 1.0);
    v_uv = a_uv;
    v_color = a_color;
}
"#;

const QUAD_FRAGMENT_SHADER_SOURCE: &str = r#"
uniform sampler2D u_texture;
varying vec2 v_uv;
varying vec4 v_color;
void main() {
    gl_FragColor = texture2D(u_texture, v_uv) * v_color;
}
"#;

/// The live-GL draw passes, shared across frames (held by `GlParts` —
/// the per-frame `GlDrawTarget` borrows it; the GL state it owns —
/// programs / buffers / texture objects / the font atlas — must NOT be
/// re-created per frame, the C++'s target owns them for the window's
/// life). Single-threaded GL (the `GlParts` contract) — no interior
/// synchronization.
pub struct GlPasses {
    /// The texture upload cache (SG1).
    textures: HashMap<TexKey, glow::Texture>,
    /// The textured-quad program (SG2/SG3) + its uniforms — lazy; a
    /// compile/link failure documents a no-op (the `GlBatchUploader`'s
    /// `uploader_failed` pattern).
    program: Option<glow::Program>,
    u_center: Option<glow::UniformLocation>,
    u_size: Option<glow::UniformLocation>,
    u_rotation: Option<glow::UniformLocation>,
    u_texture: Option<glow::UniformLocation>,
    vao: Option<glow::VertexArray>,
    vbo: Option<glow::Buffer>,
    failed: bool,
    /// The font atlas (SG3) — the CPU store (raster cache + the growing
    /// RGBA8 buffer) + the GL texture object + the last-uploaded size
    /// (the growth re-upload's detector).
    store: GlyphStore,
    atlas_tex: Option<glow::Texture>,
    atlas_size: (u32, u32),
}

impl Default for GlPasses {
    fn default() -> Self {
        Self::new()
    }
}

impl GlPasses {
    #[must_use]
    pub fn new() -> Self {
        Self {
            textures: HashMap::new(),
            program: None,
            u_center: None,
            u_size: None,
            u_rotation: None,
            u_texture: None,
            vao: None,
            vbo: None,
            failed: false,
            store: GlyphStore::new(),
            atlas_tex: None,
            atlas_size: (0, 0),
        }
    }

    /// exactly as `GlBatchUploader::new` does.
    fn ensure_program(&mut self, gl: &Context) -> bool {
        if self.program.is_some() {
            return true;
        }
        let is_gles = crate::shader::is_gles_context(gl);
        let vs_source = if is_gles {
            crate::shader::translate_for_gles(
                QUAD_VERTEX_SHADER_SOURCE,
                crate::shader::ShaderType::Vertex,
            )
        } else {
            QUAD_VERTEX_SHADER_SOURCE.to_string()
        };
        let fs_source = if is_gles {
            crate::shader::translate_for_gles(
                QUAD_FRAGMENT_SHADER_SOURCE,
                crate::shader::ShaderType::Fragment,
            )
        } else {
            QUAD_FRAGMENT_SHADER_SOURCE.to_string()
        };
        let compile = |gl: &Context, kind: u32, source: &str| -> Option<glow::Shader> {
            let shader = unsafe { gl.create_shader(kind) }.ok()?;
            unsafe {
                gl.shader_source(shader, source);
                gl.compile_shader(shader);
            }
            if !unsafe { gl.get_shader_compile_status(shader) } {
                return None;
            }
            Some(shader)
        };
        let (vs, fs) = match (
            compile(gl, glow::VERTEX_SHADER, &vs_source),
            compile(gl, glow::FRAGMENT_SHADER, &fs_source),
        ) {
            (Some(vs), Some(fs)) => (vs, fs),
            (vs, fs) => {
                if let Some(s) = vs.or(fs) {
                    unsafe {
                        gl.delete_shader(s);
                    }
                }
                eprintln!("[GlPasses] the textured-quad program failed to compile");
                self.failed = true;
                return false;
            }
        };
        let program = match unsafe { gl.create_program() }.ok() {
            Some(p) => p,
            None => {
                unsafe {
                    gl.delete_shader(vs);
                    gl.delete_shader(fs);
                }
                eprintln!("[GlPasses] the textured-quad program could not be created");
                self.failed = true;
                return false;
            }
        };
        unsafe {
            gl.attach_shader(program, vs);
            gl.attach_shader(program, fs);
            gl.bind_attrib_location(program, 0, "a_position");
            gl.bind_attrib_location(program, 1, "a_uv");
            gl.bind_attrib_location(program, 2, "a_color");
            gl.link_program(program);
        }
        if !unsafe { gl.get_program_link_status(program) } {
            unsafe {
                gl.delete_program(program);
                gl.delete_shader(vs);
                gl.delete_shader(fs);
            }
            eprintln!("[GlPasses] the textured-quad program failed to link");
            self.failed = true;
            return false;
        }
        unsafe {
            gl.delete_shader(vs);
            gl.delete_shader(fs);
        }

        let vao = match unsafe { gl.create_vertex_array() }.ok() {
            Some(v) => v,
            None => {
                eprintln!("[GlPasses] the quad VAO could not be created");
                self.failed = true;
                return false;
            }
        };
        let vbo = match unsafe { gl.create_buffer() }.ok() {
            Some(b) => b,
            None => {
                eprintln!("[GlPasses] the quad VBO could not be created");
                self.failed = true;
                return false;
            }
        };
        // The per-VAO attribute state (the `GlBatchUploader`'s pattern):
        // 0 = position (vec2 @ 0), 1 = uv (vec2 @ 8), 2 = color
        // (vec4 @ 16).
        unsafe {
            gl.bind_vertex_array(Some(vao));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
            gl.enable_vertex_attrib_array(0);
            gl.vertex_attrib_pointer_f32(0, 2, glow::FLOAT, false, QUAD_VERTEX_STRIDE as i32, 0);
            gl.enable_vertex_attrib_array(1);
            gl.vertex_attrib_pointer_f32(1, 2, glow::FLOAT, false, QUAD_VERTEX_STRIDE as i32, 8);
            gl.enable_vertex_attrib_array(2);
            gl.vertex_attrib_pointer_f32(2, 4, glow::FLOAT, false, QUAD_VERTEX_STRIDE as i32, 16);
            gl.bind_vertex_array(None);
            gl.bind_buffer(glow::ARRAY_BUFFER, None);
        }

        self.program = Some(program);
        self.u_center = Some(location(gl, program, "u_center"));
        self.u_size = Some(location(gl, program, "u_size"));
        self.u_rotation = Some(location(gl, program, "u_rotation"));
        self.u_texture = Some(location(gl, program, "u_texture"));
        self.vao = Some(vao);
        self.vbo = Some(vbo);
        true
    }

    // ---- the texture cache (SG1) -----------------------------------------

    /// The pool object's `glow::Texture` (the on-demand upload at first
    /// draw — the module docs' cache pin; the cap's eviction, module
    /// docs).
    fn ensure_texture(&mut self, gl: &Context, tex: &Texture) -> Option<glow::Texture> {
        let img = tex.pixels();
        let (w, h) = img.get_size();
        if w == 0 || h == 0 {
            return None;
        }
        let key = TexKey {
            w,
            h,
            ptr: img.get_pixels().as_ptr(),
        };
        if let Some(t) = self.textures.get(&key) {
            return Some(*t);
        }
        // The cap's eviction (module docs): the per-frame particle
        // clones' keys would grow the cache without bound — delete every
        // cached GL texture (the object freed) and start over; the
        // stable long-lived textures re-upload in the same frame.
        if self.textures.len() >= TEX_CACHE_CAP {
            for t in self.textures.values() {
                unsafe {
                    gl.delete_texture(*t);
                }
            }
            self.textures.clear();
        }
        let created = unsafe { gl.create_texture() }.ok()?;
        unsafe {
            gl.bind_texture(glow::TEXTURE_2D, Some(created));
            // The RGBA8 PNG pixels AS-IS (the straight-alpha decision, the
            // module docs) — the top-down rows stay top-down (the
            // unflipped-upload convention).
            gl.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                glow::RGBA8 as i32,
                w as i32,
                h as i32,
                0,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelUnpackData::Slice(Some(img.get_pixels())),
            );
            // The `Texture::smooth` flag's SFML mapping (module docs):
            // LINEAR + clamp (the SFML smoothed default) or NEAREST +
            // clamp.
            let filter = if tex.is_smooth() {
                glow::LINEAR
            } else {
                glow::NEAREST
            };
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, filter as i32);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, filter as i32);
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
        }
        self.textures.insert(key, created);
        Some(created)
    }
    /// The font-atlas `glow::Texture` (the re-upload on atlas growth — the
    /// last-uploaded size is the detector, module docs' cache pin).
    fn ensure_atlas_texture(&mut self, gl: &Context) -> Option<glow::Texture> {
        let (w, h) = self.store.atlas().size();
        let tex = match self.atlas_tex {
            Some(t) => t,
            None => {
                let created = unsafe { gl.create_texture() }.ok()?;
                // The atlas is ALWAYS smoothed (the module docs' filtering pin).
                unsafe {
                    gl.bind_texture(glow::TEXTURE_2D, Some(created));
                    gl.tex_parameter_i32(
                        glow::TEXTURE_2D,
                        glow::TEXTURE_MIN_FILTER,
                        glow::LINEAR as i32,
                    );
                    gl.tex_parameter_i32(
                        glow::TEXTURE_2D,
                        glow::TEXTURE_MAG_FILTER,
                        glow::LINEAR as i32,
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
                }
                self.atlas_tex = Some(created);
                // `atlas_size` stays at its (0, 0) start here so the
                // (re-)upload check below fires on FIRST use (the
                // pre-audit code set the size at creation: the first
                // frame's glyphs drew into a zero texture, and the size
                // then "matched" forever — the text never uploaded).
                created
            }
        };
        // (Re-)upload on first use OR atlas growth (the buffer's top rows
        // are unchanged — the full re-upload is the rare growth path).
        if self.atlas_size != (w, h) {
            unsafe {
                gl.bind_texture(glow::TEXTURE_2D, Some(tex));
                gl.tex_image_2d(
                    glow::TEXTURE_2D,
                    0,
                    glow::RGBA8 as i32,
                    w as i32,
                    h as i32,
                    0,
                    glow::RGBA,
                    glow::UNSIGNED_BYTE,
                    glow::PixelUnpackData::Slice(Some(self.store.atlas().pixels())),
                );
                gl.bind_texture(glow::TEXTURE_2D, None);
            }
            self.atlas_size = (w, h);
        }
        Some(tex)
    }

    // ---- the sprite pass (SG2) --------------------------------------------

    /// The C++ `window->draw(sprite)` — the textured quad (the sprite's
    /// position/rotation/scale/origin + texture rect, `glyphs::sprite_
    /// quad`) with the SFML `Alpha` blend (the `GlDrawTarget` applies the
    /// blend before the call — the default render states).
    pub fn draw_sprite(&mut self, gl: &Context, sprite: &Sprite, view: View) {
        if self.failed || !self.ensure_program(gl) {
            return;
        }
        // The pool texture (SG1's on-demand upload):
        let Some(tex) = self.ensure_texture(gl, sprite.get_texture()) else {
            // A 0-area texture object / an allocation failure draws
            // nothing (the C++'s null-texture sprite — the documented
            // no-op).
            return;
        };
        let quad = glyphs::sprite_quad(sprite);
        // A degenerate rect (a 0-area texture rect) draws nothing (the
        // C++'s 0-size quad).
        if quad.uv.2 <= quad.uv.0 || quad.uv.3 <= quad.uv.1 {
            return;
        }
        let [nw, sw, se, ne] = quad.corners;
        let (u0, v_top, u1, v_bot) = quad.uv;
        let c = [
            quad.color.r as f32 / 255.0,
            quad.color.g as f32 / 255.0,
            quad.color.b as f32 / 255.0,
            quad.color.a as f32 / 255.0,
        ];
        // The port's quad order (nw, sw, se, nw, se, ne) — the C++
        // `sf::Sprite`'s quad split.
        let verts = [
            QuadVertex {
                pos: [nw.x, nw.y],
                uv: [u0, v_top],
                color: c,
            },
            QuadVertex {
                pos: [sw.x, sw.y],
                uv: [u0, v_bot],
                color: c,
            },
            QuadVertex {
                pos: [se.x, se.y],
                uv: [u1, v_bot],
                color: c,
            },
            QuadVertex {
                pos: [nw.x, nw.y],
                uv: [u0, v_top],
                color: c,
            },
            QuadVertex {
                pos: [se.x, se.y],
                uv: [u1, v_bot],
                color: c,
            },
            QuadVertex {
                pos: [ne.x, ne.y],
                uv: [u1, v_top],
                color: c,
            },
        ];
        self.upload_and_draw(gl, &verts, tex, view);
    }
    // ---- the text pass (SG3) ------------------------------------------------

    /// The C++ `window->draw(text, states)` — the font-atlas glyphs (the
    /// `glyphs::text_quads` layout: the per-glyph quads, the outline
    /// before the fill, the pen advance AFTER the glyph) through the
    /// textured quad with the atlas texture. The `Text`'s Transformable
    /// (position/origin/scale/rotation — SFML's `Transformable::
    /// getTransform`) is applied to the local quads here.
    pub fn draw_text(&mut self, gl: &Context, text: &crate::font::Text, view: View) {
        if self.failed {
            return;
        }
        let quads = glyphs::text_quads(text, &mut self.store);
        if quads.is_empty() {
            // An empty string / a zero character size draws nothing
            // (the C++'s `Text::draw` — the SFML semantics).
            return;
        }
        if !self.ensure_program(gl) {
            return;
        }
        let Some(atlas) = self.ensure_atlas_texture(gl) else {
            return;
        };
        // The atlas's FINAL size (the texture the upload just wrote): the
        // quads' cells are atlas-relative, the UV normalisation happens
        // HERE — a mid-string atlas growth (the `text_quads`'s first pass)
        // never left a quad's UV normalised by a stale size (the module
        // docs' font-cache pin, `crate::glyphs`).
        let (atlas_w, atlas_h) = self.store.atlas().size();
        let verts: Vec<QuadVertex> = quads
            .iter()
            .flat_map(|q: &TextQuad| {
                let c = if q.outline {
                    // The outline quad's colour (the text's outline
                    // colour — the fill colour for the non-outline quads;
                    // `text_quads` carries the kind, the colour follows).
                    let oc = text.get_outline_color();
                    [
                        oc.r as f32 / 255.0,
                        oc.g as f32 / 255.0,
                        oc.b as f32 / 255.0,
                        oc.a as f32 / 255.0,
                    ]
                } else {
                    let fc = text.get_fill_color();
                    [
                        fc.r as f32 / 255.0,
                        fc.g as f32 / 255.0,
                        fc.b as f32 / 255.0,
                        fc.a as f32 / 255.0,
                    ]
                };
                let nw = text.transform_point(q.nw);
                let sw = text.transform_point(q.sw);
                let se = text.transform_point(q.se);
                let ne = text.transform_point(q.ne);
                let (u0, v_top, u1, v_bot) =
                    glyphs::uv_rect(q.cell.x, q.cell.y, q.cell.w, q.cell.h, atlas_w, atlas_h);
                [
                    QuadVertex {
                        pos: [nw.x, nw.y],
                        uv: [u0, v_top],
                        color: c,
                    },
                    QuadVertex {
                        pos: [sw.x, sw.y],
                        uv: [u0, v_bot],
                        color: c,
                    },
                    QuadVertex {
                        pos: [se.x, se.y],
                        uv: [u1, v_bot],
                        color: c,
                    },
                    QuadVertex {
                        pos: [nw.x, nw.y],
                        uv: [u0, v_top],
                        color: c,
                    },
                    QuadVertex {
                        pos: [se.x, se.y],
                        uv: [u1, v_bot],
                        color: c,
                    },
                    QuadVertex {
                        pos: [ne.x, ne.y],
                        uv: [u1, v_top],
                        color: c,
                    },
                ]
            })
            .collect();
        self.upload_and_draw(gl, &verts, atlas, view);
    }

    // ---- the shared upload + draw ------------------------------------------

    fn upload_and_draw(
        &mut self,
        gl: &Context,
        verts: &[QuadVertex],
        tex: glow::Texture,
        view: View,
    ) {
        let (Some(vbo), Some(vao), Some(program)) = (self.vbo, self.vao, self.program) else {
            return;
        };
        let u_center = self.u_center.unwrap();
        let u_size = self.u_size.unwrap();
        let u_rotation = self.u_rotation.unwrap();
        let u_texture = self.u_texture.unwrap();
        let bytes = unsafe {
            std::slice::from_raw_parts(
                verts.as_ptr() as *const u8,
                verts.len() * QUAD_VERTEX_STRIDE,
            )
        };
        unsafe {
            // The dynamic upload (the `GlBatchUploader`'s pattern):
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
            gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, bytes, glow::DYNAMIC_DRAW);
            gl.bind_vertex_array(Some(vao));
            gl.use_program(Some(program));
            gl.uniform_2_f32(Some(&u_center), view.get_center().x, view.get_center().y);
            gl.uniform_2_f32(Some(&u_size), view.get_size().x, view.get_size().y);
            gl.uniform_1_f32(
                Some(&u_rotation),
                oh_math::ssvu::math::to_rad(view.get_rotation()),
            );
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, Some(tex));
            gl.uniform_1_i32(Some(&u_texture), 0);
            gl.draw_arrays(glow::TRIANGLES, 0, verts.len() as i32);
            // State hygiene (the C++ per-draw-call state — the next pass
            // re-binds its own program / texture / VAO):
            gl.bind_vertex_array(None);
            gl.bind_buffer(glow::ARRAY_BUFFER, None);
            gl.bind_texture(glow::TEXTURE_2D, None);
        }
    }
}

fn location(gl: &Context, program: glow::Program, name: &str) -> glow::UniformLocation {
    unsafe { gl.get_uniform_location(program, name) }
        .unwrap_or_else(|| panic!("GlPasses: missing uniform '{name}'"))
}

// ---------------------------------------------------------------------------
// The rectangle pass's vertex build (SG4) — the pure half.
// ---------------------------------------------------------------------------

/// The C++ `sf::RectangleShape` geometry → the vertex-colour stream the
/// `GlDrawTarget`'s batch uploader submits: the FILL quad (the shape's
/// `[0, w] × [0, h]` local geometry through the Transformable) followed
/// by the OUTLINE's four quads when a thickness > 0 (the ring of width
/// `t` around the fill — the four side strips; the corner coverage is the
/// top/bottom strips' overlap, invisible for the C++'s opaque outline
/// colours, the SFML outline ring's visual equivalent). Emission order:
/// the C++ `Shape::draw`'s fill-then-outline.
#[must_use]
pub fn rectangle_vertices(rect: &RectangleShape) -> Vec<Vertex> {
    let w = rect.get_size().x;
    let h = rect.get_size().y;
    let t = rect.get_outline_thickness();
    let fill = rect.get_fill_color();
    let outline = rect.get_outline_color();

    // The shape's local point → world (the C++ Transformable:
    // `translate(position) · rotate · scale · translate(-origin)` — the
    // same math as `RectangleShape::get_global_bounds`' corner loop).
    let to_world = |p: Vec2| -> Vec2 {
        let o = rect.get_origin();
        let s = rect.get_scale();
        let shifted = Vec2::new(p.x - o.x, p.y - o.y);
        let scaled = Vec2::new(shifted.x * s.x, shifted.y * s.y);
        let rad = oh_math::ssvu::math::to_rad(rect.get_rotation());
        let (sin, cos) = rad.sin_cos();
        let pos = rect.get_position();
        Vec2::new(
            pos.x + scaled.x * cos - scaled.y * sin,
            pos.y + scaled.x * sin + scaled.y * cos,
        )
    };

    let mut out: Vec<Vertex> = Vec::with_capacity(6 + 24);
    // The fill (the port's (nw, sw, se, ne) quad order):
    let nw = to_world(Vec2::new(0.0, 0.0));
    let sw = to_world(Vec2::new(0.0, h));
    let se = to_world(Vec2::new(w, h));
    let ne = to_world(Vec2::new(w, 0.0));
    out.push(Vertex::new(nw, fill));
    out.push(Vertex::new(sw, fill));
    out.push(Vertex::new(se, fill));
    out.push(Vertex::new(nw, fill));
    out.push(Vertex::new(se, fill));
    out.push(Vertex::new(ne, fill));
    if t > 0.0 {
        // The four side strips of the outline ring (the local geometry,
        // the C++ shape's outline expanded by `t` on all sides):
        for strip in [
            // top: (−t, −t) .. (w+t, 0)
            ((-t, -t), (-t, 0.0), (w + t, 0.0), (w + t, -t)),
            // bottom: (−t, h) .. (w+t, h+t)
            ((-t, h), (-t, h + t), (w + t, h + t), (w + t, h)),
            // left: (−t, 0) .. (0, h)
            ((-t, 0.0), (-t, h), (0.0, h), (0.0, 0.0)),
            // right: (w, 0) .. (w+t, h)
            ((w, 0.0), (w, h), (w + t, h), (w + t, 0.0)),
        ] {
            let [a, b, c, d] = [
                to_world(Vec2::new(strip.0 .0, strip.0 .1)),
                to_world(Vec2::new(strip.1 .0, strip.1 .1)),
                to_world(Vec2::new(strip.2 .0, strip.2 .1)),
                to_world(Vec2::new(strip.3 .0, strip.3 .1)),
            ];
            out.push(Vertex::new(a, outline));
            out.push(Vertex::new(b, outline));
            out.push(Vertex::new(c, outline));
            out.push(Vertex::new(a, outline));
            out.push(Vertex::new(c, outline));
            out.push(Vertex::new(d, outline));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Color;

    #[test]
    fn rectangle_fill_is_the_transformed_shape() {
        let mut r = RectangleShape::default();
        r.set_size(Vec2::new(100.0, 50.0));
        r.set_position(Vec2::new(20.0, 30.0));
        r.set_fill_color(Color::RED);
        r.set_outline_thickness(0.0);
        let v = rectangle_vertices(&r);
        assert_eq!(v.len(), 6, "no outline: the fill quad only");
        // No origin / no rotation / no scale: the local corners + position.
        assert_eq!(v[0].position, Vec2::new(20.0, 30.0)); // nw
        assert_eq!(v[1].position, Vec2::new(20.0, 80.0)); // sw
        assert_eq!(v[2].position, Vec2::new(120.0, 80.0)); // se
        assert_eq!(v[3].position, v[0].position);
        assert_eq!(v[4].position, v[2].position);
        assert_eq!(v[5].position, Vec2::new(120.0, 30.0)); // ne
        assert!(v.iter().all(|x| x.color == Color::RED));
    }

    #[test]
    fn rectangle_outline_ring_follows_the_transform() {
        let mut r = RectangleShape::default();
        r.set_size(Vec2::new(10.0, 4.0));
        r.set_position(Vec2::new(100.0, 100.0));
        r.set_outline_thickness(3.0);
        r.set_fill_color(Color::WHITE);
        r.set_outline_color(Color::BLACK);
        let v = rectangle_vertices(&r);
        assert_eq!(v.len(), 6 + 24, "the fill + four 6-vertex outline quads");
        // The fill vertices are WHITE, the outline's BLACK:
        assert!(v[..6].iter().all(|x| x.color == Color::WHITE));
        assert!(v[6..].iter().all(|x| x.color == Color::BLACK));
        // The outer ring's extreme points (the ring = the fill box grown
        // by 3 on all sides): the top edge's top-left at (100 − 3, 100 −
        // 3), the bottom edge's bottom-right at (100 + 10 + 3, 100 + 4 +
        // 3):
        let mut xs: [f32; 24] = v[6..]
            .iter()
            .map(|x| x.position.x)
            .collect::<Vec<f32>>()
            .try_into()
            .unwrap();
        let mut ys: [f32; 24] = v[6..]
            .iter()
            .map(|x| x.position.y)
            .collect::<Vec<f32>>()
            .try_into()
            .unwrap();
        xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
        ys.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert!((xs.first().unwrap() - 97.0).abs() < 1e-4);
        assert!((xs.last().unwrap() - 113.0).abs() < 1e-4);
        assert!((ys.first().unwrap() - 97.0).abs() < 1e-4);
        assert!((ys.last().unwrap() - 107.0).abs() < 1e-4);
    }

    #[test]
    fn rectangle_zero_thickness_has_no_outline() {
        let mut r = RectangleShape::default();
        r.set_outline_thickness(0.0);
        assert_eq!(rectangle_vertices(&r).len(), 6);
    }

    #[test]
    fn rectangle_rotated_and_scaled_follows_the_transformable() {
        // The `to_world` closure's full Transformable (the C++'s
        // `translate(position) · rotate · scale · translate(-origin)`):
        // a 4×2 box, origin (2, 1) (the box centre), rotated 90°, scale
        // 2, at (100, 100). The local corner (0, 0) → ((−2, −1)·2) rotated
        // 90° (y-down: +x edge points down) = (−2·cos90 − (−2)·sin90, ...):
        // d = p − origin = (−2, −1); scaled (−4, −2); rotated 90° →
        // (d.x·c − d.y·s, d.x·s + d.y·c) with c = 0, s = 1 → (2, −4);
        // + position → (102, 96).
        let mut r = RectangleShape::default();
        r.set_size(Vec2::new(4.0, 2.0));
        r.set_origin(Vec2::new(2.0, 1.0));
        r.set_position(Vec2::new(100.0, 100.0));
        r.set_rotation(90.0);
        r.set_scale(Vec2::new(2.0, 2.0));
        r.set_outline_thickness(0.0);
        let v = rectangle_vertices(&r);
        assert_eq!(v.len(), 6, "no outline: the fill quad only");
        // The four fill corners (the port's (nw, sw, se, ne) order) through
        // the same corner math the shape's `get_global_bounds` uses — the
        // pin is the transform's application:
        let corners = [
            (Vec2::new(0.0, 0.0), 0usize), // nw → fill vertex 0
            (Vec2::new(0.0, 2.0), 1),      // sw → fill vertex 1
            (Vec2::new(4.0, 2.0), 2),      // se → fill vertex 2
            (Vec2::new(4.0, 0.0), 5),      // ne → fill vertex 5 (the quad's last)
        ];
        for (c, vi) in corners {
            let o = r.get_origin();
            let s = r.get_scale();
            let shifted = Vec2::new(c.x - o.x, c.y - o.y);
            let scaled = Vec2::new(shifted.x * s.x, shifted.y * s.y);
            let rad = oh_math::ssvu::math::to_rad(r.get_rotation());
            let (sin, cos) = rad.sin_cos();
            let pos = r.get_position();
            let expect = Vec2::new(
                pos.x + scaled.x * cos - scaled.y * sin,
                pos.y + scaled.x * sin + scaled.y * cos,
            );
            assert_eq!(
                v[vi].position, expect,
                "fill corner at local {c:?} through the Transformable"
            );
        }
        // The nw corner's hand-computed value (the 90° y-down check):
        assert_eq!(v[0].position, Vec2::new(102.0, 96.0));
    }
}
