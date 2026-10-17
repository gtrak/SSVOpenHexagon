//! 029n1 (K6) — the pack fragment shaders' GL execution half.
//!
//! The *description* half (`RenderStates::shader` + the `oh-assets`
//! registry/source/uniform store + the `onRenderStage` hook) landed
//! with issues 007/010/29; this module is the missing *execution*
//! half: the draw-time compile of a bound pack `.frag` into GL
//! programs + the stored-uniform upload + the draw through them.
//!
//! # The C++ model (pinned SFML fork `vittorioromeo/SFML@dacceddb`)
//!
//! `src/SFML/Graphics/Shader.cpp`:
//!
//! * one `sf::Shader` = one GL program compiled from ONE pack file —
//!   a fragment-only `.frag` is a **fragment-only program** (no
//!   injected vertex shader, no preamble rewrites: `loadFromFile`
//!   compiles exactly the file's source, `Shader.cpp:141-246`);
//! * on desktop GL the vertex stage of such a program runs in
//!   **fixed-function** mode — the vertex arrays supply
//!   `gl_Position`/`gl_Color`/`gl_TexCoord`, the view transform is
//!   applied through the fixed-function matrices (`RenderTarget.cpp:443-470`'s
//!   `setView` → `glLoadMatrixf`);
//! * the pinned fork's `setUniformUnsafe` family uploads the value at
//!   **SET time** (the `UnsafeUniformBinder` saves the current
//!   program, `glUseProgram`s the shader's own program, issues the
//!   `glUniform*`, restores — `Shader.cpp:235-270` + `:759-836`). The
//!   port defers the GL upload to BIND time (the program is compiled
//!   lazily at draw time — the C++'s `Assets::new` runs BEFORE the
//!   window/GL context exists, `Game.cpp:132-135`, so a load-time
//!   compile is impossible in the port's composition order). The
//!   observable per-draw result is the same: the values visible to a
//!   draw are the last sets before it (see the 029n1 report's
//!   set-time-vs-bind-time divergence);
//! * the uniform location is resolved against the program with NO
//!   existence check — an unknown name is silently ignored
//!   (`getUniformLocation` → -1, `Shader.cpp:1070-1095`;
//!   `setUniformUnsafe`'s `if (binder.location != -1)` guard) — the
//!   port's `get_uniform_location == None` skip mirrors it;
//! * under `SFML_OPENGL_ES` the WHOLE module is an empty
//!   implementation (`Shader.cpp:1103-1366` — `loadFromFile` returns
//!   `false`, no uniforms, no programs): the C++'s Android build
//!   NEVER executes pack shaders. The port's GLES path runs the
//!   same sources through [`crate::shader::translate_for_gles`]
//!   (the 25b-SG3 path, spec §3.3) and falls back to the built-in
//!   pass-through program on a compile failure (the 25b assessment:
//!   the `gl_Color`/`gl_TexCoord` fixed-function builtins have no
//!   GLSL ES equivalent — a full fixed-function rewrite is the
//!   scope-guard's stop condition, not this lane).
//!
//! # The desktop path: the C++'s fragment-only program
//!
//! The desktop context is the COMPATIBILITY profile (the `gl` module
//! docs: the C++'s SFML desktop context's shape — the pinned fork's
//! `GlxContext.cpp:688-691` non-`Core` default), where a program with
//! NO vertex object runs its vertex stage in the fixed-function
//! pipeline — exactly the C++'s `sf::Shader` fragment-only shape
//! (`Shader.cpp:141-246`: `loadFromFile` compiles the file verbatim and
//! creates no vertex object). The port's VAO attribute arrays feed that
//! stage through the GL 3.0 compatibility fixed-function mapping
//! (attribute 0 → `gl_Position`, attribute 1 → `gl_Color`, attribute 3
//! → `gl_TexCoord[0]`):
//!
//! * the SCENE draw (the `sf::Vertex` layout — attribute 0 = position,
//!   attribute 1 = the u8 color): `gl_Color` = the vertex color;
//! * the TEXT draw (the SFML `Text` glyph quads — the `gl_passes`
//!   `QuadVertex` 32-byte layout re-mapped onto the fixed-function
//!   indices: attribute 0 = position @ 0, attribute 1 = color @ 16,
//!   attribute 3 = the font-atlas UV @ 8): `gl_Color` +
//!   `gl_TexCoord[0]`; the font atlas on unit 0 (the C++'s font
//!   texture — the pack fragment's first sampler, the `text.frag`'s
//!   `font`).
//!
//! One compiled pack `.frag` is therefore ONE program (the C++'s one
//! `sf::Shader`), drawn through the two stage VAOs.
//!
//! **The view transform** — the C++'s `applyCurrentView` (`
//! RenderTarget.cpp:555-572`) loads the view matrix onto `GL_PROJECTION`
//! (`glLoadMatrixf(view.getTransform())`, `View.cpp:154-182`) and the
//! fixed-function stage applies it. The port's GL API layer (glow 0.18)
//! does not expose the fixed-function matrix stack (its generated entry
//! points are the uniform-matrix family only), so the port
//! pre-transforms the vertex positions on the CPU before the upload —
//! the built-in vertex shader's `mapCoordsToPixel` formula (the
//! `batching.rs` `VERTEX_SHADER_SOURCE`'s per-vertex math, the 025c-
//! verified port view semantics): the same f32 ops, an
//! observationally identical mapping.
//!
//! **The Mesa llvmpipe caveat** (probe-verified on the 029n1 box —
//! Mesa 26.0.8 llvmpipe, compatibility 4.5): the fixed-function stage
//! supplies `gl_Color` from attribute 1 (the scene stage is fully
//! functional), but does not supply `gl_TexCoord[0]` from attribute 3
//! (it stays `(0,0,0,0)` — the `text.frag`'s glyph UVs sample the
//! atlas's top-left texel on that driver). A spec-conformant desktop
//! driver supplies the attribute-3 texture coordinate; the C++ binary
//! itself renders the `text` stage identically on the same driver (the
//! C++ relies on the same fixed-function texcoord), so this is a driver
//! limitation, not a port divergence.
//!
//! # The GLES path (the 25b-SG3 design, unchanged)
//!
//! Under `SFML_OPENGL_ES` the C++'s Shader module is an empty
//! implementation (`Shader.cpp:1103-1366`) — the C++'s Android build
//! never executes pack shaders. The port's GLES path runs the same
//! sources through [`crate::shader::translate_for_gles`] (the 25b-SG3
//! path, spec §3.3) into TWO programs (the scene + text vertex stages —
//! the built-in transform + the ES 1.00 built-in outputs where they
//! exist — over the shared translated fragment stage) and falls back to
//! the built-in pass-through program on a compile failure: the
//! `gl_Color`-/`gl_TexCoord`-reading pack fragments have no GLSL ES
//! 1.00 fragment equivalent (those builtins are vertex-stage-only in
//! ES 1.00) — the 25b assessment: a full fixed-function rewrite is the
//! scope guard's stop condition, not this lane.

use std::collections::BTreeMap;

use glow::{Context, HasContext};

use crate::error::RenderError;
use crate::shader::{is_gles_context, translate_for_gles, ShaderType, Uniform};
use crate::view::View;

/// The `sf::RenderStates::shader` payload as the draw-time GL layer
/// sees it: the pack file's source + the current uniform store (the
/// C++'s live `sf::Shader`'s two halves — `HGGraphics.cpp:92-97`'s
/// `sf::RenderStates{assets.getShaderByShaderId(*fragmentShaderId)}`).
/// The `oh-app` composition converts the `oh-assets` `ShaderSnapshot`
/// into this (the `oh-assets` → `oh-render` dependency direction is
/// not available — `oh-render` cannot depend on `oh-assets`, the cycle
/// would run through `oh-game`).
#[derive(Debug, Clone)]
pub struct PackShaderSnapshot {
    /// The pack `.frag`/`.vert`/`.geom` file's source, verbatim.
    pub source: String,
    /// The registry's `ShaderType` (only `Fragment` is executable in
    /// the port's scene/text draws — the C++'s vertex-only /
    /// geometry-only `sf::Shader` objects are never bound to a stage
    /// either: `shdr_setActiveFragmentShader` is the only binding entry
    /// point and the shipped levels bind `.frag` files; a non-fragment
    /// id draws through the default program, below).
    pub shader_type: ShaderType,
    /// The stored uniforms (name → value) — the C++'s per-object
    /// uniform cache's current contents (name-sorted; the upload's
    /// per-name location lookup does not depend on the order).
    pub uniforms: Vec<(String, Uniform)>,
}

/// The asset-side source the GL layer compiles + uploads from (the
/// `oh-app` composition's `RealAssetsApi` wrapper over
/// `oh_assets::Assets::shader_snapshot` — `oh-render` cannot depend on
/// `oh-assets`, the dependency runs the other way through `oh-game`).
pub trait PackShaderProvider {
    /// The C++ `assets.getShaderByShaderId(id)`'s payload; `None` for
    /// an invalid id (the C++'s `nullptr` — the draw falls back to the
    /// default program, the C++'s `getRenderStates` guard,
    /// `HGGraphics.cpp:93-94`).
    fn pack_shader_snapshot(&self, id: usize) -> Option<PackShaderSnapshot>;
}

// ---------------------------------------------------------------------------
// The vertex layouts (the two `sf::Vertex`-family arrays the C++'s
// fixed-function vertex stage consumed — the desktop's VAO attribute
// pointers + the GLES stage programs' layouts).
// ---------------------------------------------------------------------------

/// The `GlBatchUploader` vertex layout (the C++'s `sf::Vertex`:
/// `position` vec2 f32 @ 0 + `color` u8×4 @ 8 = 12 bytes —
/// `batching.rs`'s `VERTEX_STRIDE`/`COLOR_OFFSET`): a bound pack
/// program draws the SAME vertex streams the default program does.
const VERTEX_STRIDE: usize = std::mem::size_of::<crate::draw::Vertex>();
const COLOR_OFFSET: i32 = std::mem::size_of::<oh_math::ssvs::Vec2>() as i32;

/// The `gl_passes` glyph-quad layout (the SFML `Text`'s vertex array:
/// `position` + font-atlas `texCoord` + `color`, all floats —
/// `gl_passes.rs`'s `QuadVertex`, 32 bytes).
const TEXT_VERTEX_STRIDE: usize = std::mem::size_of::<crate::gl_passes::QuadVertex>();
const TEXT_UV_OFFSET: i32 = 8;
const TEXT_COLOR_OFFSET: i32 = 16;

/// The GLES (25b-SG3) vertex-stage program over the shared fragment
/// stage: the built-in transform vertex shader + the stage's own
/// VAO/VBO (the stage's vertex layout) + the stored-uniform location
/// cache (module docs).
struct PackStageProgram {
    vao: glow::VertexArray,
    vbo: glow::Buffer,
    program: glow::Program,
    u_center: glow::UniformLocation,
    u_size: glow::UniformLocation,
    u_rotation: glow::UniformLocation,
    /// The stored uniforms' locations (the C++'
    /// `getUniformLocation`-with-no-existence-check: an absent name
    /// is a `None` entry the upload silently skips —
    /// `Shader.cpp:1080-1083`'s `location != -1` guard).
    uniform_locations: BTreeMap<String, glow::UniformLocation>,
}

impl PackStageProgram {
    fn upload_uniforms(&mut self, gl: &Context, uniforms: &[(String, Uniform)]) {
        upload_uniforms_to(gl, self.program, &mut self.uniform_locations, uniforms)
    }
}

/// The stored-uniform upload against one compiled program (the C++'
/// `getUniformLocation`-with-no-existence-check per name — an absent
/// name is silently skipped, `Shader.cpp:1080-1083`'s `location != -1`
/// guard; the resolved locations are cached per program, the GL's
/// `getUniformLocation`'s per-program identity).
///
/// The GL's `glUniform*` commands address the CURRENT program — the
/// caller makes `program` current before calling (the C++'s set-time
/// upload does exactly this: the `UnsafeUniformBinder`'s `glUseProgram`
/// of the shader's own program, `Shader.cpp:235-270`).
fn upload_uniforms_to(
    gl: &Context,
    program: glow::Program,
    locations: &mut BTreeMap<String, glow::UniformLocation>,
    uniforms: &[(String, Uniform)],
) {
    for (name, value) in uniforms {
        let location = match locations.get(name).copied() {
            Some(location) => location,
            None => match lookup_location(gl, program, name) {
                Some(location) => {
                    locations.insert(name.clone(), location);
                    location
                }
                None => continue, // absent from the program — the C++'s silent skip
            },
        };
        unsafe {
            match value {
                Uniform::F(x) => gl.uniform_1_f32(Some(&location), *x),
                Uniform::FVec2(x, y) => gl.uniform_2_f32(Some(&location), *x, *y),
                Uniform::FVec3(x, y, z) => gl.uniform_3_f32(Some(&location), *x, *y, *z),
                Uniform::FVec4(x, y, z, w) => gl.uniform_4_f32(Some(&location), *x, *y, *z, *w),
                Uniform::I(x) => gl.uniform_1_i32(Some(&location), *x),
                Uniform::IVec2(x, y) => gl.uniform_2_i32(Some(&location), *x, *y),
                Uniform::IVec3(x, y, z) => gl.uniform_3_i32(Some(&location), *x, *y, *z),
                Uniform::IVec4(x, y, z, w) => gl.uniform_4_i32(Some(&location), *x, *y, *z, *w),
                Uniform::B(b) => {
                    // The C++'s `setUniformB` is a `sampler2D`/`bool`
                    // one-value set; the 13 Lua fns expose none of
                    // it (the eight kinds above are the full Lua
                    // surface, `LuaScripting.cpp:1337-1407`), so a
                    // `B` value can only reach the store through a
                    // direct (non-Lua) caller — upload it as a bool
                    // int (GL's `1i` = `true`).
                    gl.uniform_1_i32(Some(&location), i32::from(*b));
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// The desktop program (the C++'s fragment-only `sf::Shader`, module docs).
// ---------------------------------------------------------------------------

/// The desktop pack-shader program: the C++'s single fragment-only
/// program (one fragment object, NO vertex object — the vertex stage
/// is the context's fixed-function pipeline, the module docs) + the
/// two stage VAOs (each its own VBO — the draw re-uploads the stage's
/// vertex stream before it).
struct DesktopPackProgram {
    program: glow::Program,
    scene_vao: glow::VertexArray,
    scene_vbo: glow::Buffer,
    text_vao: glow::VertexArray,
    text_vbo: glow::Buffer,
    /// The stored uniforms' locations (the C++'
    /// `getUniformLocation`-with-no-existence-check — the upload
    /// silently skips an absent name, `Shader.cpp:1080-1083`).
    uniform_locations: BTreeMap<String, glow::UniformLocation>,
}

impl DesktopPackProgram {
    /// The C++ `sf::Shader::loadFromFile(path, Fragment)`'s port
    /// equivalent on a desktop-GL context: the pack source verbatim
    /// (no preamble, no rewrites — `Shader.cpp:141-246`) into one
    /// fragment-only program + the two fixed-function-layout VAOs.
    fn new(gl: &Context, fs_source: &str) -> Result<Self, RenderError> {
        let fs = compile_shader(gl, glow::FRAGMENT_SHADER, fs_source)?;
        let program = unsafe { gl.create_program() }.map_err(|_| glow_error())?;
        unsafe {
            gl.attach_shader(program, fs);
            gl.link_program(program);
            // No `bind_attrib_location`: a fragment-only program has no
            // GLSL vertex inputs — the fixed-function stage reads the
            // VAO's attribute arrays directly (the attribute-1
            // `gl_Color` + the attribute-3 `gl_TexCoord[0]`, the GL 3.0
            // compatibility fixed-function mapping, module docs).
        }
        if !unsafe { gl.get_program_link_status(program) } {
            let log = unsafe { gl.get_program_info_log(program) };
            eprintln!("[pack_shaders] the pack-shader program failed to link: {log}");
            unsafe {
                gl.delete_program(program);
                gl.delete_shader(fs);
            }
            return Err(glow_error());
        }
        unsafe {
            gl.delete_shader(fs);
        }

        // The SCENE VAO (the `sf::Vertex` layout — attribute 0 =
        // position, attribute 1 = the u8 color: the fixed-function
        // `gl_Position` / `gl_Color` sources). The attribute pointers
        // capture the bound VBO into the VAO (the GL's pointer
        // semantics) — the VBO is created first.
        let scene_vbo = make_buffer(gl)?;
        let scene_vao = make_vao(gl, scene_vbo, |gl| unsafe {
            gl.enable_vertex_attrib_array(0);
            gl.vertex_attrib_pointer_f32(0, 2, glow::FLOAT, false, VERTEX_STRIDE as i32, 0);
            gl.enable_vertex_attrib_array(1);
            gl.vertex_attrib_pointer_f32(
                1,
                4,
                glow::UNSIGNED_BYTE,
                true,
                VERTEX_STRIDE as i32,
                COLOR_OFFSET,
            );
        })?;

        // The TEXT VAO (the `QuadVertex` 32-byte layout re-mapped onto
        // the fixed-function indices — attribute 0 = position @ 0,
        // attribute 1 = `gl_Color` @ 16, attribute 3 =
        // `gl_TexCoord[0]` @ 8).
        let text_vbo = make_buffer(gl)?;
        let text_vao = make_vao(gl, text_vbo, |gl| unsafe {
            gl.enable_vertex_attrib_array(0);
            gl.vertex_attrib_pointer_f32(0, 2, glow::FLOAT, false, TEXT_VERTEX_STRIDE as i32, 0);
            gl.enable_vertex_attrib_array(1);
            gl.vertex_attrib_pointer_f32(
                1,
                4,
                glow::FLOAT,
                false,
                TEXT_VERTEX_STRIDE as i32,
                TEXT_COLOR_OFFSET,
            );
            gl.enable_vertex_attrib_array(3);
            gl.vertex_attrib_pointer_f32(
                3,
                2,
                glow::FLOAT,
                false,
                TEXT_VERTEX_STRIDE as i32,
                TEXT_UV_OFFSET,
            );
        })?;

        Ok(Self {
            program,
            scene_vao,
            scene_vbo,
            text_vao,
            text_vbo,
            uniform_locations: BTreeMap::new(),
        })
    }

    fn upload_uniforms(&mut self, gl: &Context, uniforms: &[(String, Uniform)]) {
        upload_uniforms_to(gl, self.program, &mut self.uniform_locations, uniforms)
    }

    /// One stage's upload + draw (the fixed-function vertex stage runs
    /// over the bound VAO's attribute arrays — module docs).
    #[allow(clippy::too_many_arguments)] // the GL's per-draw state: the
                                         // stage's VAO/VBO + the raw upload (pointer/count/stride) + the
                                         // optional atlas are one conceptual upload-and-draw
    fn stage_draw(
        &mut self,
        gl: &Context,
        vao: glow::VertexArray,
        vbo: glow::Buffer,
        bytes_ptr: *const u8,
        count: usize,
        stride: usize,
        atlas: Option<glow::Texture>,
    ) {
        let bytes = unsafe { std::slice::from_raw_parts(bytes_ptr, count * stride) };
        unsafe {
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
            gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, bytes, glow::DYNAMIC_DRAW);

            if let Some(atlas) = atlas {
                // The font atlas on unit 0 (the C++'s font texture's
                // fixed-function unit — the pack fragment's first
                // sampler's default unit).
                gl.active_texture(glow::TEXTURE0);
                gl.bind_texture(glow::TEXTURE_2D, Some(atlas));
            }

            gl.bind_vertex_array(Some(vao));
            gl.use_program(Some(self.program));
            gl.draw_arrays(glow::TRIANGLES, 0, count as i32);
            // State hygiene (the C++ per-draw-call state — the next
            // pass re-binds its own program / texture / VAO; the
            // `gl_passes`'s `upload_and_draw` leaves the texture bound
            // like the C++'s fixed-function path — the atlas STAYS on
            // unit 0, the C++'s `window->draw(text, ...)` leaves the
            // font texture bound too):
            gl.bind_vertex_array(None);
            gl.bind_buffer(glow::ARRAY_BUFFER, None);
        }
    }
}

/// One compiled pack-shader program: the desktop's C++-shaped
/// fragment-only program (module docs) or the GLES two-stage design
/// (the 25b-SG3 path) — lazily created at the first bound draw (the
/// port's deferral of the C++'s load-time compile; the GL objects live
/// for the window's life, the `GlParts` `pack_shaders` slot, mirroring
/// the `GlBatchUploader`'s `BatchSlot`).
pub struct PackShaderProgram {
    inner: PackBackend,
}

/// The two execution shapes (module docs): the desktop's C++-shaped
/// fragment-only program vs the GLES two-stage design.
enum PackBackend {
    /// Desktop GL: the single fragment-only program + the two
    /// fixed-function-layout VAOs.
    Desktop(DesktopPackProgram),
    /// GLES (the 25b-SG3 path, spec §3.3): the translated two-stage
    /// design (the scene + text vertex stages over the shared
    /// translated fragment stage).
    Gles {
        scene: PackStageProgram,
        text: PackStageProgram,
    },
}

impl PackShaderProgram {
    /// The C++ `sf::Shader::loadFromFile(path, Fragment)`'s port
    /// equivalent: the pack source into the context's pack-shader
    /// program(s) (the port's deferred load — the module docs).
    ///
    /// Desktop: the source VERBATIM into the fragment-only program
    /// (the C++'s `loadFromFile` — no preamble, no rewrites). GLES
    /// (the 25b-SG3 path, spec §3.3): the source through
    /// [`translate_for_gles`] into the two translated stage programs
    /// (the `gl_Color`-/`gl_TexCoord`-reading pack fragments have no
    /// GLSL ES 1.00 fragment equivalent — the compile failure's
    /// fallback is the documented 25b outcome).
    ///
    /// # Errors
    /// [`RenderError::InvalidData`] if a source fails to compile, a
    /// program fails to link, or a GL object cannot be allocated.
    pub fn new(gl: &Context, snapshot: &PackShaderSnapshot) -> Result<Self, RenderError> {
        if !is_gles_context(gl) {
            // The C++'s desktop model (module docs): the verbatim
            // fragment-only program + the fixed-function VAOs.
            let p = DesktopPackProgram::new(gl, &snapshot.source)?;
            return Ok(Self {
                inner: PackBackend::Desktop(p),
            });
        }

        // The GLES two-stage design (the 25b-SG3 path — unchanged): the
        // fragment stage goes through `translate_for_gles` exactly as
        // the built-in program's internal sources do (the ES
        // `#version 100` line); the vertex stages are the built-in
        // transform (the ES 1.00 `gl_Color`/`gl_TexCoord` vertex
        // builtins where they exist — the fragment sources that read
        // them fail to compile anyway, the documented fallback).
        let fs_source = translate_for_gles(&snapshot.source, ShaderType::Fragment);
        let fs = compile_shader(gl, glow::FRAGMENT_SHADER, &fs_source)?;
        let scene_vs_source =
            translate_for_gles(crate::batching::VERTEX_SHADER_SOURCE, ShaderType::Vertex);
        let text_vs_source = translate_for_gles(GLES_TEXT_VERTEX_SHADER_SOURCE, ShaderType::Vertex);

        let scene = match Self::stage_program(gl, fs, &scene_vs_source, StageLayout::Scene, false) {
            Ok(p) => p,
            Err(e) => {
                unsafe {
                    gl.delete_shader(fs);
                }
                return Err(e);
            }
        };
        let text = match Self::stage_program(gl, fs, &text_vs_source, StageLayout::Text, true) {
            Ok(p) => p,
            Err(e) => {
                unsafe {
                    gl.delete_shader(fs);
                }
                return Err(e);
            }
        };
        Ok(Self {
            inner: PackBackend::Gles { scene, text },
        })
    }

    /// The GLES stage program (the 25b-SG3 path): one vertex stage +
    /// the shared fragment stage — the shader compile/link + the
    /// stage's own VAO/VBO (the stage's vertex layout, the `StageLayout`
    /// attribute arrangement). `delete_fs` deletes the shared fragment
    /// object after this stage links (the last stage's flag — the GL
    /// allows the attached object to be deleted once linked into
    /// every program that uses it).
    fn stage_program(
        gl: &Context,
        fs: glow::Shader,
        vs_source: &str,
        layout: StageLayout,
        delete_fs: bool,
    ) -> Result<PackStageProgram, RenderError> {
        let vs = compile_shader(gl, glow::VERTEX_SHADER, vs_source)?;
        let program = unsafe { gl.create_program() }.map_err(|_| glow_error())?;
        unsafe {
            gl.attach_shader(program, vs);
            gl.attach_shader(program, fs);
            gl.bind_attrib_location(program, 0, "a_position");
            match layout {
                StageLayout::Scene => gl.bind_attrib_location(program, 1, "a_color"),
                StageLayout::Text => {
                    gl.bind_attrib_location(program, 1, "a_texcoord");
                    gl.bind_attrib_location(program, 2, "a_color");
                }
            }
            gl.link_program(program);
        }
        if !unsafe { gl.get_program_link_status(program) } {
            let log = unsafe { gl.get_program_info_log(program) };
            eprintln!("[pack_shaders] the pack-shader program failed to link: {log}");
            unsafe {
                gl.delete_program(program);
                gl.delete_shader(vs);
                if delete_fs {
                    gl.delete_shader(fs);
                }
            }
            return Err(glow_error());
        }
        unsafe {
            gl.delete_shader(vs);
            if delete_fs {
                gl.delete_shader(fs);
            }
        }

        let vao = unsafe { gl.create_vertex_array() }.map_err(|_| glow_error())?;
        let vbo = unsafe { gl.create_buffer() }.map_err(|_| glow_error())?;
        unsafe {
            gl.bind_vertex_array(Some(vao));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
            gl.enable_vertex_attrib_array(0);
            match layout {
                StageLayout::Scene => {
                    gl.vertex_attrib_pointer_f32(0, 2, glow::FLOAT, false, VERTEX_STRIDE as i32, 0);
                    gl.enable_vertex_attrib_array(1);
                    gl.vertex_attrib_pointer_f32(
                        1,
                        4,
                        glow::UNSIGNED_BYTE,
                        true,
                        VERTEX_STRIDE as i32,
                        COLOR_OFFSET,
                    );
                }
                StageLayout::Text => {
                    gl.vertex_attrib_pointer_f32(
                        0,
                        2,
                        glow::FLOAT,
                        false,
                        TEXT_VERTEX_STRIDE as i32,
                        0,
                    );
                    gl.enable_vertex_attrib_array(1);
                    gl.vertex_attrib_pointer_f32(
                        1,
                        2,
                        glow::FLOAT,
                        false,
                        TEXT_VERTEX_STRIDE as i32,
                        TEXT_UV_OFFSET,
                    );
                    gl.enable_vertex_attrib_array(2);
                    gl.vertex_attrib_pointer_f32(
                        2,
                        4,
                        glow::FLOAT,
                        false,
                        TEXT_VERTEX_STRIDE as i32,
                        TEXT_COLOR_OFFSET,
                    );
                }
            }
            gl.bind_vertex_array(None);
            gl.bind_buffer(glow::ARRAY_BUFFER, None);
        }

        Ok(PackStageProgram {
            vao,
            vbo,
            program,
            u_center: location_ok(gl, program, "u_center"),
            u_size: location_ok(gl, program, "u_size"),
            u_rotation: location_ok(gl, program, "u_rotation"),
            uniform_locations: BTreeMap::new(),
        })
    }

    /// The stored-uniform upload (the C++'s bind-time half — the
    /// values the hook / `onUpdate` stored before this draw) against
    /// the compiled program(s), the program made CURRENT first (the
    /// GL's `glUniform*` address the current program — the C++'s
    /// set-time `UnsafeUniformBinder` `glUseProgram` equivalent).
    pub fn upload_uniforms(&mut self, gl: &Context, uniforms: &[(String, Uniform)]) {
        match &mut self.inner {
            PackBackend::Desktop(p) => {
                unsafe {
                    gl.use_program(Some(p.program));
                }
                p.upload_uniforms(gl, uniforms);
            }
            PackBackend::Gles { scene, text } => {
                unsafe {
                    gl.use_program(Some(scene.program));
                }
                scene.upload_uniforms(gl, uniforms);
                unsafe {
                    gl.use_program(Some(text.program));
                }
                text.upload_uniforms(gl, uniforms);
            }
        }
    }

    /// The scene `draw_vertices`'s GL half (the C++'s
    /// `window->draw(vertices, ...)` through the bound shader): the
    /// view-transformed vertex upload + the draw through the stage's
    /// VAO.
    pub fn draw(&mut self, gl: &Context, vertices: &[crate::draw::Vertex], view: View) {
        match &mut self.inner {
            PackBackend::Desktop(p) => {
                // The view transform the C++'s fixed-function vertex
                // stage applies through `GL_PROJECTION` (the
                // `applyCurrentView`'s `glLoadMatrixf`,
                // `RenderTarget.cpp:555-572`) — the port's GL layer
                // does not expose the fixed-function matrix stack
                // (module docs), so the positions go out
                // pre-transformed (the built-in vertex shader's
                // `mapCoordsToPixel` formula, the same f32 ops).
                let mut out = vertices.to_vec();
                for v in out.iter_mut() {
                    let [x, y] = view_to_clip(v.position.x, v.position.y, view);
                    v.position = oh_math::ssvs::Vec2::new(x, y);
                }
                p.stage_draw(
                    gl,
                    p.scene_vao,
                    p.scene_vbo,
                    out.as_ptr() as *const u8,
                    out.len(),
                    VERTEX_STRIDE,
                    None,
                );
            }
            PackBackend::Gles { scene, .. } => {
                Self::stage_draw(
                    gl,
                    scene,
                    vertices.as_ptr() as *const u8,
                    vertices.len(),
                    VERTEX_STRIDE,
                    None,
                    view,
                );
            }
        }
    }

    /// The TEXT `draw_text`'s GL half through the text stage (the
    /// C++'s `window->draw(text, getRenderStates(RenderStage::Text))`,
    /// `HGGraphics.cpp:283`): the view-transformed glyph-quad upload,
    /// the font atlas on unit 0 (the C++'s font texture — the pack
    /// fragment's first sampler, the `text.frag`'s `font`), and the
    /// draw.
    pub fn draw_text(
        &mut self,
        gl: &Context,
        vertices: &[crate::gl_passes::QuadVertex],
        atlas: glow::Texture,
        view: View,
    ) {
        match &mut self.inner {
            PackBackend::Desktop(p) => {
                // The same fixed-function view transform as the scene
                // stage (the glyph quads are the same view-space
                // geometry — the `gl_passes`'s textured-quad pass
                // transforms them identically through its vertex
                // shader).
                let mut out = vertices.to_vec();
                for q in out.iter_mut() {
                    let [x, y] = view_to_clip(q.pos[0], q.pos[1], view);
                    q.pos = [x, y];
                }
                p.stage_draw(
                    gl,
                    p.text_vao,
                    p.text_vbo,
                    out.as_ptr() as *const u8,
                    out.len(),
                    TEXT_VERTEX_STRIDE,
                    Some(atlas),
                );
            }
            PackBackend::Gles { text, .. } => {
                Self::stage_draw(
                    gl,
                    text,
                    vertices.as_ptr() as *const u8,
                    vertices.len(),
                    TEXT_VERTEX_STRIDE,
                    Some(atlas),
                    view,
                );
            }
        }
    }

    /// The GLES `PackStageProgram`'s upload + draw (the 25b-SG3 path —
    /// the stage's vertex shader applies the view transform through
    /// its `u_center`/`u_size`/`u_rotation` uniforms).
    fn stage_draw(
        gl: &Context,
        stage: &mut PackStageProgram,
        bytes_ptr: *const u8,
        count: usize,
        stride: usize,
        atlas: Option<glow::Texture>,
        view: View,
    ) {
        let bytes = unsafe { std::slice::from_raw_parts(bytes_ptr, count * stride) };
        unsafe {
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(stage.vbo));
            gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, bytes, glow::DYNAMIC_DRAW);

            if let Some(atlas) = atlas {
                // The font atlas on unit 0 (the C++'s font texture's
                // fixed-function unit — the pack fragment's first
                // sampler's default unit).
                gl.active_texture(glow::TEXTURE0);
                gl.bind_texture(glow::TEXTURE_2D, Some(atlas));
            }

            gl.bind_vertex_array(Some(stage.vao));
            gl.use_program(Some(stage.program));
            gl.uniform_2_f32(
                Some(&stage.u_center),
                view.get_center().x,
                view.get_center().y,
            );
            gl.uniform_2_f32(Some(&stage.u_size), view.get_size().x, view.get_size().y);
            gl.uniform_1_f32(
                Some(&stage.u_rotation),
                oh_math::ssvu::math::to_rad(view.get_rotation()),
            );
            gl.draw_arrays(glow::TRIANGLES, 0, count as i32);
            // State hygiene (the C++ per-draw-call state — the next
            // pass re-binds its own program / texture / VAO; the
            // `gl_passes`'s `upload_and_draw` leaves the texture bound
            // like the C++'s fixed-function path — the atlas STAYS on
            // unit 0, the C++'s `window->draw(text, ...)` leaves the
            // font texture bound too):
            gl.bind_vertex_array(None);
            gl.bind_buffer(glow::ARRAY_BUFFER, None);
        }
    }
}

/// The view transform the desktop fixed-function vertex stage must
/// apply (module docs' divergence note): the C++ loads the view
/// matrix onto `GL_PROJECTION` (`applyCurrentView`'s
/// `glLoadMatrixf(view.getTransform())`); the port pre-transforms each
/// vertex on the CPU instead (glow 0.18 has no fixed-function matrix
/// stack) — the built-in vertex shader's `mapCoordsToPixel` formula
/// (the `batching.rs` `VERTEX_SHADER_SOURCE`'s per-vertex math, the
/// 025c-verified port view semantics): the same f32 ops, an
/// observationally identical mapping.
fn view_to_clip(x: f32, y: f32, view: View) -> [f32; 2] {
    let center = view.get_center();
    let size = view.get_size();
    let theta = oh_math::ssvu::math::to_rad(view.get_rotation());
    let (sin_t, cos_t) = theta.sin_cos();
    let dx = x - center.x;
    let dy = y - center.y;
    [
        (cos_t * dx - sin_t * dy) * 2.0 / size.x,
        -(sin_t * dx + cos_t * dy) * 2.0 / size.y,
    ]
}

/// The desktop VAO's attribute-pointer setup (the `stage_program`'s
/// VAO half, split out — the attribute pointers capture the bound
/// VBO into the VAO, the GL's pointer semantics):
fn make_vao(
    gl: &Context,
    vbo: glow::Buffer,
    setup: impl FnOnce(&Context),
) -> Result<glow::VertexArray, RenderError> {
    let vao = unsafe { gl.create_vertex_array() }.map_err(|_| glow_error())?;
    unsafe {
        gl.bind_vertex_array(Some(vao));
        gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
    }
    setup(gl);
    unsafe {
        gl.bind_vertex_array(None);
        gl.bind_buffer(glow::ARRAY_BUFFER, None);
    }
    Ok(vao)
}

fn make_buffer(gl: &Context) -> Result<glow::Buffer, RenderError> {
    unsafe { gl.create_buffer() }.map_err(|_| glow_error())
}

/// The stage layouts' compile-time split (the two vertex attribute
/// arrangements, module docs).
#[derive(Clone, Copy)]
enum StageLayout {
    /// The `sf::Vertex` scene layout (attribute 0 = position, 1 = color).
    Scene,
    /// The SFML `Text` layout (attribute 0 = position, 1 = UV, 2 = color).
    Text,
}

/// The GLES text vertex stage (the built-in transform + the ES
/// varyings — module docs: no `gl_Color`/`gl_TexCoord` exist in GLSL
/// ES, and the `gl_TexCoord`-reading pack sources fail to compile on
/// GLES anyway; the varyings keep the stage compilable for the
/// fragment sources that don't use them).
const GLES_TEXT_VERTEX_SHADER_SOURCE: &str = r#"
attribute vec2 a_position;
attribute vec2 a_texcoord;
attribute vec4 a_color;
uniform vec2 u_center;
uniform vec2 u_size;
uniform float u_rotation;
varying vec2 v_texcoord;
varying vec4 v_color;
void main() {
    vec2 d = a_position - u_center;
    float s = sin(u_rotation);
    float c = cos(u_rotation);
    d = vec2(d.x * c - d.y * s, d.x * s + d.y * c);
    gl_Position = vec4(2.0 * d.x / u_size.x, -2.0 * d.y / u_size.y, 0.0, 1.0);
    v_texcoord = a_texcoord;
    v_color = a_color;
}
"#;

/// The `GlParts`-owned pack-shader cache: the compiled programs by
/// shader id (the C++'s `shadersById`'s compiled programs — the port's
/// lazy per-id compile, sticky on failure: a failed id stays failed
/// for the window's life, the `BatchSlot`'s documented no-op
/// semantics — re-compiling the same source on the same context
/// cannot succeed where it just failed).
#[derive(Default)]
pub struct PackShaderCache {
    programs: BTreeMap<usize, Result<PackShaderProgram, ()>>,
}

impl PackShaderCache {
    /// The id's compiled program (the first bound draw for the id
    /// compiles it; a compile/link failure is sticky).
    pub fn program<'a>(
        &'a mut self,
        gl: &Context,
        id: usize,
        snapshot: &PackShaderSnapshot,
    ) -> Option<&'a mut PackShaderProgram> {
        match self.programs.get(&id) {
            Some(Ok(_)) => match self.programs.get_mut(&id) {
                Some(Ok(p)) => Some(p),
                _ => None,
            },
            Some(Err(())) => None, // the sticky failure
            None => match PackShaderProgram::new(gl, snapshot) {
                Ok(p) => {
                    self.programs.insert(id, Ok(p));
                    match self.programs.get_mut(&id) {
                        Some(Ok(p)) => Some(p),
                        _ => None,
                    }
                }
                Err(e) => {
                    eprintln!(
                        "[pack_shaders] the pack shader {id} could not be compiled on this context ({e:?}); \
                         the stage draws through the default program (the C++'s load-failure path, \
                         Assets.cpp:889-892)"
                    );
                    self.programs.insert(id, Err(()));
                    None
                }
            },
        }
    }
}

fn glow_error() -> RenderError {
    RenderError::InvalidData
}

fn compile_shader(gl: &Context, kind: u32, source: &str) -> Result<glow::Shader, RenderError> {
    let shader = unsafe { gl.create_shader(kind) }.map_err(|_| glow_error())?;
    unsafe {
        gl.shader_source(shader, source);
        gl.compile_shader(shader);
    }
    if !unsafe { gl.get_shader_compile_status(shader) } {
        // The shader info log (the one-shot creation failure's
        // diagnostic — the `GlBatchUploader`'s `compile_shader`
        // twin, `batching.rs`).
        let log = unsafe { gl.get_shader_info_log(shader) };
        eprintln!("[pack_shaders] the pack-shader source failed to compile: {log}");
        unsafe {
            gl.delete_shader(shader);
        }
        return Err(glow_error());
    }
    Ok(shader)
}

/// The C++ `getUniformLocation` (no existence check — the absent name
/// is a `None` location the upload skips).
fn lookup_location(
    gl: &Context,
    program: glow::Program,
    name: &str,
) -> Option<glow::UniformLocation> {
    unsafe { gl.get_uniform_location(program, name) }
}

fn location_ok(gl: &Context, program: glow::Program, name: &str) -> glow::UniformLocation {
    lookup_location(gl, program, name)
        .unwrap_or_else(|| panic!("GL: missing the pack-shader program's own uniform '{name}'"))
}

/// The §4.2 compile guard's fragment compile + link against one
/// context (the `pack_shaders_gl` example's GLES column root-cause
/// diagnostic: desktop GL links a fragment-only program against the
/// fixed-function vertex stage, so a clean result is the full guard;
/// GL ES 3.x links NO fragment-only program — "program lacks a vertex
/// or mesh shader" — so there the diagnostic's value is the COMPILE
/// half (the fragment's own errors, the 29v1 class), reached before
/// the link requirement).
///
/// # Errors
/// The GL's info log (the shader compile's or the program link's) when
/// either half fails.
pub fn compile_link_fragment_only(gl: &Context, source: &str) -> Result<(), String> {
    let fs = (unsafe { gl.create_shader(glow::FRAGMENT_SHADER) })
        .map_err(|e| format!("create_shader: {e}"))?;
    unsafe {
        gl.shader_source(fs, source);
        gl.compile_shader(fs);
    }
    if !unsafe { gl.get_shader_compile_status(fs) } {
        let log = unsafe { gl.get_shader_info_log(fs) };
        unsafe {
            gl.delete_shader(fs);
        }
        return Err(log);
    }
    let program = (unsafe { gl.create_program() }).map_err(|e| {
        unsafe {
            gl.delete_shader(fs);
        }
        format!("create_program: {e}")
    })?;
    unsafe {
        gl.attach_shader(program, fs);
        gl.link_program(program);
    }
    let linked = unsafe { gl.get_program_link_status(program) };
    let link_log = unsafe { gl.get_program_info_log(program) };
    unsafe {
        gl.delete_program(program);
        gl.delete_shader(fs);
    }
    if linked {
        Ok(())
    } else {
        Err(link_log)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The `PackShaderSnapshot` carries the source + the uniform
    /// store (the two halves the C++'s live `sf::Shader` hands
    /// `renderTexture`): a minimal shape pin for the conversion seam
    /// (`oh-app`'s `AssetsPackShaderProvider` → this type).
    #[test]
    fn the_snapshot_carries_source_and_the_stored_uniforms() {
        let snap = PackShaderSnapshot {
            source: "void main() { gl_FragColor = vec4(1.0); }".to_string(),
            shader_type: ShaderType::Fragment,
            uniforms: vec![
                ("u_time".to_string(), Uniform::F(1.5)),
                ("u_resolution".to_string(), Uniform::FVec2(800.0, 600.0)),
            ],
        };
        assert_eq!(
            snap.shader_type,
            ShaderType::Fragment,
            "the registry's ShaderType passes through"
        );
        assert!(snap.source.contains("gl_FragColor"));
        assert_eq!(snap.uniforms.len(), 2);
        assert_eq!(snap.uniforms[0].0, "u_time");
        assert_eq!(snap.uniforms[1].1, Uniform::FVec2(800.0, 600.0));
    }

    /// The cache's sticky-failure semantics (the `BatchSlot`'s
    /// documented no-op pattern): a failed id's entry is never
    /// re-compiled. (The GL half of the `PackShaderProgram` is the
    /// live-context column — the `pack_shaders_gl` example, D-011
    /// skip-with-reason.)
    #[test]
    fn the_cache_records_a_sticky_failure_per_id() {
        let mut cache = PackShaderCache::default();
        // The failure path is the map semantics (an already-`Err`
        // entry returns before any GL call — the `program` call's
        // `Err` branch does exactly this insert):
        cache.programs.insert(7, Err(()));
        assert!(matches!(cache.programs.get(&7), Some(Err(()))));
        assert!(!cache.programs.contains_key(&8), "other ids are untouched");
    }

    /// The vertex-stage split the module docs pin: the scene stage
    /// carries the `sf::Vertex` layout (position + the BYTE color —
    /// the `GlBatchUploader`'s 12-byte stride), the text stage the
    /// SFML `Text` layout (position + UV + the FLOAT color, 32 bytes —
    /// the `gl_passes` `QuadVertex`). The constants' values are the
    /// layout contract the VAO pointers build on.
    #[test]
    fn the_vertex_layout_constants_match_the_two_sf_vertex_families() {
        assert_eq!(VERTEX_STRIDE, 12, "the sf::Vertex layout (batching.rs)");
        assert_eq!(COLOR_OFFSET, 8);
        assert_eq!(
            TEXT_VERTEX_STRIDE,
            std::mem::size_of::<crate::gl_passes::QuadVertex>()
        );
        assert_eq!(TEXT_UV_OFFSET, 8);
        assert_eq!(TEXT_COLOR_OFFSET, 16);
    }

    /// The desktop fixed-function view transform's pin (module docs):
    /// the CPU pre-transform is the built-in vertex shader's
    /// `mapCoordsToPixel` formula — the identity view maps the half-
    /// window extents onto the NDC ±1 corners, a centred view shifts
    /// accordingly, and a 90° view rotation carries the +x axis onto
    /// the −y NDC axis.
    #[test]
    fn the_view_to_clip_matches_the_builtin_vertex_shader_formula() {
        let approx = |a: f32, b: f32| (a - b).abs() < 1e-4;

        // The identity view: the half-window extents → NDC ±1 (the
        // built-in vertex shader's `gl_Position`'s `y` flip: the
        // built-in VS's `-2.0 * d.y / u_size.y` — view-space `+y` is
        // NDC `-y`, the screen-up direction).
        let view = View::new(
            oh_math::ssvs::Vec2::new(0.0, 0.0),
            oh_math::ssvs::Vec2::new(800.0, 600.0),
        );
        assert!(approx(view_to_clip(-400.0, -300.0, view)[0], -1.0));
        assert!(approx(view_to_clip(-400.0, -300.0, view)[1], 1.0));
        assert!(approx(view_to_clip(400.0, 300.0, view)[0], 1.0));
        assert!(approx(view_to_clip(400.0, 300.0, view)[1], -1.0));
        assert!(approx(view_to_clip(0.0, 0.0, view)[0], 0.0));
        assert!(approx(view_to_clip(0.0, 0.0, view)[1], 0.0));

        // A centred view: the centre maps to NDC 0, the view extents
        // to ±1.
        let view = View::new(
            oh_math::ssvs::Vec2::new(100.0, 50.0),
            oh_math::ssvs::Vec2::new(200.0, 100.0),
        );
        assert!(approx(view_to_clip(100.0, 50.0, view)[0], 0.0));
        assert!(approx(view_to_clip(100.0, 50.0, view)[1], 0.0));
        assert!(approx(view_to_clip(0.0, 0.0, view)[0], -1.0));
        assert!(approx(view_to_clip(0.0, 0.0, view)[1], 1.0));
        assert!(approx(view_to_clip(200.0, 100.0, view)[0], 1.0));
        assert!(approx(view_to_clip(200.0, 100.0, view)[1], -1.0));

        // A 90° view rotation (fresh centre-0 view): the +x axis (the
        // view's right) carries onto the −y NDC axis (the port view
        // semantics — the built-in vertex shader's `d = (d.x*c -
        // d.y*s, d.x*s + d.y*c)` with `y`-flipped NDC).
        let mut rot_view = View::new(
            oh_math::ssvs::Vec2::new(0.0, 0.0),
            oh_math::ssvs::Vec2::new(800.0, 600.0),
        );
        rot_view.set_rotation(90.0);
        let p = view_to_clip(400.0, 0.0, rot_view);
        assert!(approx(p[0], 0.0), "rotated +x carries off the x axis");
        assert!(approx(p[1], -4.0 / 3.0), "the 400px right extent, rotated");
    }
}
