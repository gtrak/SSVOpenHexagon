//! Batched vertex submission — `FastVertexVectorTris` → GPU buffer upload
//! (issue 11 file map: `batching.rs`).
//!
//! [`GlBatchUploader`] owns a dynamic vertex buffer + VAO + a pass-through
//! program on a live `glow::Context` and performs the real GL submission that
//! a `VertexBatch::draw_call` describes: upload the vertex stream, set the
//! view uniforms, `glDrawArrays(TRIANGLES, 0, count)`. The view transform is
//! the SFML view (see [`crate::view::View::map_coords_to_pixel`]):
//! `ndc = 2 · R(rotation) · (world − center) / viewSize`, drawn into a
//! target whose pixel origin is top-left (SFML `sf::RenderWindow`
//! convention).
//!
//! Only exercised on a live GL context — headless tests cannot run it
//! (D-011; the `golden_frames` integration test skips with a printed reason
//! when no display/GPU is available). The fragment-shader-binding surface
//! (`RenderStates::shader`) stays at the description level: executing the
//! pack's fixed-function-style fragment shaders (SFML's shader model with
//! `gl_Color`/`gl_TexCoord`) is a later issue — BUT the two INTERNAL
//! pass-through sources of this uploader run through the issue-25b-SG3
//! GLES translation ([`crate::shader::translate_for_gles`], gated on the
//! context being GLES) so the program compiles on the android EGL context.
//!//!
//! `TODO(issue-009-wiring)`: the `Window`-backed `DrawTarget` (full frame
//! pipeline: sprites, texts, rectangle shapes) wires into this uploader once
//! the glyph-atlas / sprite-quad pipeline lands.

use glow::{Context, HasContext};
use oh_math::ssvu::math::to_rad;

use crate::draw::{PrimitiveType, Vertex, VertexBatch};
use crate::error::RenderError;
use crate::view::View;

const VERTEX_SHADER_SOURCE: &str = r#"
attribute vec2 a_position;
attribute vec4 a_color;
uniform vec2 u_center;
uniform vec2 u_size;
uniform float u_rotation;
varying vec4 v_color;
void main() {
    vec2 d = a_position - u_center;
    float s = sin(u_rotation);
    float c = cos(u_rotation);
    d = vec2(d.x * c - d.y * s, d.x * s + d.y * c);
    gl_Position = vec4(2.0 * d.x / u_size.x, 2.0 * d.y / u_size.y, 0.0, 1.0);
    v_color = a_color;
}
"#;

const FRAGMENT_SHADER_SOURCE: &str = r#"
varying vec4 v_color;
void main() {
    gl_FragColor = v_color;
}
"#;

/// A live-GL triangle-batch uploader (the `FastVertexVector::draw` GL side).
pub struct GlBatchUploader {
    vao: glow::VertexArray,
    vbo: glow::Buffer,
    program: glow::Program,
    u_center: glow::UniformLocation,
    u_size: glow::UniformLocation,
    u_rotation: glow::UniformLocation,
}

/// `sf::Vertex` stride: `position` (vec2) + `color` (vec4) = 6 f32.
const VERTEX_STRIDE: usize = std::mem::size_of::<Vertex>();

fn glow_error() -> RenderError {
    RenderError::InvalidData
}

impl GlBatchUploader {
    /// Build the VAO / VBO / pass-through program.
    ///
    /// GLES (issue 25b SG3): when the context is a GLES one (queried via
    /// [`crate::shader::is_gles_context`] — the "when the created context
    /// is GLES" gate, so a desktop-GL context gets the sources verbatim
    /// and the desktop path stays behavior-identical), the two internal
    /// sources run through [`crate::shader::translate_for_gles`] — the
    /// pass-through vertex source is already ES 1.00-syntax (it gains the
    /// explicit `#version 100` line) and the fragment source gains the
    /// `precision mediump float;` declaration that GLSL ES 1.00 fragment
    /// shaders require (no default float precision — without it the
    /// compile fails on every GLES context).
    ///
    /// # Errors
    /// [`RenderError::InvalidData`] if a shader fails to compile/link on
    /// this context (or a GL object cannot be allocated).
    pub fn new(gl: &Context) -> Result<Self, RenderError> {
        let is_gles = crate::shader::is_gles_context(gl);
        let vs_source = if is_gles {
            crate::shader::translate_for_gles(
                VERTEX_SHADER_SOURCE,
                crate::shader::ShaderType::Vertex,
            )
        } else {
            VERTEX_SHADER_SOURCE.to_string()
        };
        let fs_source = if is_gles {
            crate::shader::translate_for_gles(
                FRAGMENT_SHADER_SOURCE,
                crate::shader::ShaderType::Fragment,
            )
        } else {
            FRAGMENT_SHADER_SOURCE.to_string()
        };
        let vs = compile_shader(gl, glow::VERTEX_SHADER, &vs_source)?;
        let fs = compile_shader(gl, glow::FRAGMENT_SHADER, &fs_source)?;

        let program = unsafe { gl.create_program() }.map_err(|_| glow_error())?;
        unsafe {
            gl.attach_shader(program, vs);
            gl.attach_shader(program, fs);
            gl.bind_attrib_location(program, 0, "a_position");
            gl.bind_attrib_location(program, 1, "a_color");
            gl.link_program(program);
        }
        if !unsafe { gl.get_program_link_status(program) } {
            return Err(glow_error());
        }
        unsafe {
            gl.delete_shader(vs);
            gl.delete_shader(fs);
        }

        let vao = unsafe { gl.create_vertex_array() }.map_err(|_| glow_error())?;
        let vbo = unsafe { gl.create_buffer() }.map_err(|_| glow_error())?;
        unsafe {
            gl.bind_vertex_array(Some(vao));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
            gl.enable_vertex_attrib_array(0);
            gl.vertex_attrib_pointer_f32(0, 2, glow::FLOAT, false, (VERTEX_STRIDE * 2) as i32, 0);
            gl.enable_vertex_attrib_array(1);
            gl.vertex_attrib_pointer_f32(
                1,
                4,
                glow::FLOAT,
                false,
                (VERTEX_STRIDE * 4) as i32,
                (VERTEX_STRIDE * 2) as i32,
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
        })
    }

    /// Upload + draw one batch (the `mRenderTarget.draw(vertices, size,
    /// Triangles, states)` of [`VertexBatch::draw_call`]).
    pub fn draw(&mut self, gl: &Context, vertices: &[Vertex], view: View) {
        let bytes = unsafe {
            std::slice::from_raw_parts(
                vertices.as_ptr() as *const u8,
                vertices.len() * VERTEX_STRIDE,
            )
        };
        unsafe {
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.vbo));
            gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, bytes, glow::DYNAMIC_DRAW);

            gl.bind_vertex_array(Some(self.vao));
            gl.use_program(Some(self.program));
            gl.uniform_2_f32(
                Some(&self.u_center),
                view.get_center().x,
                view.get_center().y,
            );
            gl.uniform_2_f32(Some(&self.u_size), view.get_size().x, view.get_size().y);
            gl.uniform_1_f32(Some(&self.u_rotation), to_rad(view.get_rotation()));

            gl.draw_arrays(glow::TRIANGLES, 0, vertices.len() as i32);
            gl.bind_vertex_array(None);
        }
    }

    /// Draw a whole batch (the common caller form).
    pub fn draw_batch(&mut self, gl: &Context, batch: &VertexBatch, view: View) {
        debug_assert_eq!(batch.primitive(), PrimitiveType::Triangles);
        self.draw(gl, batch.as_slice(), view);
    }
}

fn compile_shader(gl: &Context, kind: u32, source: &str) -> Result<glow::Shader, RenderError> {
    let shader = unsafe { gl.create_shader(kind) }.map_err(|_| glow_error())?;
    unsafe {
        gl.shader_source(shader, source);
        gl.compile_shader(shader);
    }
    if !unsafe { gl.get_shader_compile_status(shader) } {
        return Err(glow_error());
    }
    Ok(shader)
}

fn location_ok(gl: &Context, program: glow::Program, name: &str) -> glow::UniformLocation {
    let loc = unsafe { gl.get_uniform_location(program, name) };
    loc.unwrap_or_else(|| panic!("GL: missing uniform '{name}'"))
}
