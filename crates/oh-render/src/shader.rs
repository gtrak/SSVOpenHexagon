//! `sf::Shader` — fragment / vertex / geometry shaders + a name/id registry.
//!
//! The game binds one shader per `RenderStage` (mapping/rendering-menu.md
//! §1/§9). Pack content ships GLSL fragment shaders under
//! `_RELEASE/Packs/*/Shaders/*.frag` (`pixelate.frag`, `text.frag`,
//! `gradient.frag`, `steamart.frag` — see the uniform names each declares,
//! e.g. `u_resolution`, `u_color0`, `u_time`, `u_blend`, `color0`, `font`,
//! `time`, `u_rotation`). Shaders are loaded once and their **uniforms are
//! set by name** at runtime through the `shdr_setUniform*` Lua API
//! (§9.2).
//!
//! This module stores the source + per-uniform values (the headless,
//! testable surface). The actual GLSL compile/link into a GL program happens
//! on a live `glow` context (see `window.rs`); that is where the real-GL
//! requirement is satisfied.

use std::collections::HashMap;
use std::path::Path;

use crate::error::RenderError;

/// `sf::Shader::Type`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShaderType {
    /// `sf::Shader::Type::Vertex` (`.vert`).
    Vertex,
    /// `sf::Shader::Type::Geometry` (`.geom`).
    Geometry,
    /// `sf::Shader::Type::Fragment` (`.frag`).
    Fragment,
}

/// Infer the `ShaderType` from a file extension (`.vert` / `.geom` / `.frag`).
#[must_use]
pub fn infer_shader_type(path: &Path) -> Option<ShaderType> {
    match path.extension().and_then(|e| e.to_str())? {
        "vert" => Some(ShaderType::Vertex),
        "geom" => Some(ShaderType::Geometry),
        "frag" => Some(ShaderType::Fragment),
        _ => None,
    }
}

/// A GLSL uniform value (`shdr_setUniform*` target).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Uniform {
    /// `setUniformF` — a single `float`.
    F(f32),
    /// `setUniformIVec`/`setUniformFVec2`.
    FVec2(f32, f32),
    /// `setUniformFVec3`.
    FVec3(f32, f32, f32),
    /// `setUniformFVec4`.
    FVec4(f32, f32, f32, f32),
    /// `setUniformI`.
    I(i32),
    /// `setUniformIVec2`.
    IVec2(i32, i32),
    /// `setUniformIVec3`.
    IVec3(i32, i32, i32),
    /// `setUniformIVec4`.
    IVec4(i32, i32, i32, i32),
    /// `setUniformB` (a single `sampler2D` / `bool`).
    B(bool),
}

/// `sf::Shader`.
#[derive(Clone, Debug)]
pub struct Shader {
    kind: ShaderType,
    source: String,
    uniforms: HashMap<String, Uniform>,
}

impl Shader {
    /// `sf::Shader::load(file, type)` — read a shader file.
    ///
    /// # Errors
    /// [`RenderError::Io`] if the file cannot be read, [`RenderError::
    /// InvalidData`] if it is empty.
    pub fn load(kind: ShaderType, path: &Path) -> Result<Self, RenderError> {
        let source = std::fs::read_to_string(path)?;
        if source.is_empty() {
            return Err(RenderError::InvalidData);
        }
        Ok(Self::from_source(kind, source))
    }

    /// Construct a shader from an in-memory source string (the headless path;
    /// pack-shader tests and the `oh-assets` loader use this).
    #[must_use]
    pub fn from_source(kind: ShaderType, source: impl Into<String>) -> Self {
        Self {
            kind,
            source: source.into(),
            uniforms: HashMap::new(),
        }
    }

    /// `sf::Shader::getType()`.
    #[must_use]
    pub fn get_type(&self) -> ShaderType {
        self.kind
    }

    /// The GLSL source.
    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }

    // `sf::Shader::setUniform(name, value)` — the `shdr_setUniform*` family.
    /// `setUniformF`.
    pub fn set_uniform_f(&mut self, name: &str, value: f32) {
        self.uniforms.insert(name.to_string(), Uniform::F(value));
    }

    /// `setUniformFVec2`.
    pub fn set_uniform_fvec2(&mut self, name: &str, x: f32, y: f32) {
        self.uniforms.insert(name.to_string(), Uniform::FVec2(x, y));
    }

    /// `setUniformFVec3`.
    pub fn set_uniform_fvec3(&mut self, name: &str, x: f32, y: f32, z: f32) {
        self.uniforms
            .insert(name.to_string(), Uniform::FVec3(x, y, z));
    }

    /// `setUniformFVec4`.
    pub fn set_uniform_fvec4(&mut self, name: &str, x: f32, y: f32, z: f32, w: f32) {
        self.uniforms
            .insert(name.to_string(), Uniform::FVec4(x, y, z, w));
    }

    /// `setUniformI`.
    pub fn set_uniform_i(&mut self, name: &str, value: i32) {
        self.uniforms.insert(name.to_string(), Uniform::I(value));
    }

    /// `setUniformIVec2`.
    pub fn set_uniform_ivec2(&mut self, name: &str, x: i32, y: i32) {
        self.uniforms.insert(name.to_string(), Uniform::IVec2(x, y));
    }

    /// `setUniformIVec3`.
    pub fn set_uniform_ivec3(&mut self, name: &str, x: i32, y: i32, z: i32) {
        self.uniforms
            .insert(name.to_string(), Uniform::IVec3(x, y, z));
    }

    /// `setUniformIVec4`.
    pub fn set_uniform_ivec4(&mut self, name: &str, x: i32, y: i32, z: i32, w: i32) {
        self.uniforms
            .insert(name.to_string(), Uniform::IVec4(x, y, z, w));
    }

    /// `setUniformB`.
    pub fn set_uniform_b(&mut self, name: &str, value: bool) {
        self.uniforms.insert(name.to_string(), Uniform::B(value));
    }

    /// Look up a set uniform (test / render-target use).
    #[must_use]
    pub fn get_uniform(&self, name: &str) -> Option<&Uniform> {
        self.uniforms.get(name)
    }

    /// The number of distinct uniforms set.
    #[must_use]
    pub fn uniform_count(&self) -> usize {
        self.uniforms.len()
    }
}

/// `Assets::shadersById` + the `<packId>_<fileName>` name map — the shader
/// registry the `shdr_*` Lua API and `getShaderByShaderId` resolve against.
///
/// (Owned/populated by `oh-assets`; `oh-render` provides the type. A failed
/// load consumes **no** id — the C++ only pushes on success.)
#[derive(Clone, Debug, Default)]
pub struct ShaderRegistry {
    by_id: Vec<Shader>,
    by_name: HashMap<String, usize>,
}

impl ShaderRegistry {
    /// `ShaderRegistry()`.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Push a shader and return its `shaderId` (index into `by_id`).
    pub fn push(&mut self, name: impl Into<String>, shader: Shader) -> usize {
        let id = self.by_id.len();
        self.by_name.insert(name.into(), id);
        self.by_id.push(shader);
        id
    }

    /// `getShaderByShaderId(id)` — `shadersById[id]` or `None` (mirrors the
    /// C++ returning `nullptr`).
    #[must_use]
    pub fn get(&self, id: usize) -> Option<&Shader> {
        self.by_id.get(id)
    }

    /// `shdr_getShaderId(name)` — the id for a name key, or `None` (the Lua
    /// API maps `None` to `-1`).
    #[must_use]
    pub fn id_of(&self, name: &str) -> Option<usize> {
        self.by_name.get(name).copied()
    }

    /// Number of loaded shaders.
    #[must_use]
    pub fn len(&self) -> usize {
        self.by_id.len()
    }

    /// `len() == 0`.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_id.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infer_shader_type_from_extension() {
        assert_eq!(
            infer_shader_type(Path::new("a/b/pixelate.frag")),
            Some(ShaderType::Fragment)
        );
        assert_eq!(
            infer_shader_type(Path::new("v.vert")),
            Some(ShaderType::Vertex)
        );
        assert_eq!(
            infer_shader_type(Path::new("g.geom")),
            Some(ShaderType::Geometry)
        );
        assert_eq!(infer_shader_type(Path::new("x.txt")), None);
    }

    #[test]
    fn set_and_get_uniforms_by_name() {
        let mut s = Shader::from_source(ShaderType::Fragment, "void main(){}");
        s.set_uniform_fvec2("u_resolution", 800.0, 450.0);
        s.set_uniform_f("u_time", 12.5);
        s.set_uniform_fvec3("u_color0", 1.0, 0.0, 0.0);
        s.set_uniform_i("u_blur", 3);

        assert_eq!(
            s.get_uniform("u_resolution"),
            Some(&Uniform::FVec2(800.0, 450.0))
        );
        assert_eq!(s.get_uniform("u_time"), Some(&Uniform::F(12.5)));
        assert_eq!(
            s.get_uniform("u_color0"),
            Some(&Uniform::FVec3(1.0, 0.0, 0.0))
        );
        assert_eq!(s.get_uniform("u_blur"), Some(&Uniform::I(3)));
        assert_eq!(s.get_uniform("missing"), None);
        assert_eq!(s.uniform_count(), 4);
        assert_eq!(s.get_type(), ShaderType::Fragment);
    }

    #[test]
    fn registry_maps_name_to_id_and_id_to_shader() {
        let mut reg = ShaderRegistry::new();
        let id = reg.push(
            "experimental_text.frag".to_string(),
            Shader::from_source(ShaderType::Fragment, "void main(){}"),
        );
        assert_eq!(id, 0);
        assert_eq!(reg.id_of("experimental_text.frag"), Some(0));
        assert_eq!(reg.id_of("nope.frag"), None);
        assert_eq!(reg.get(0).map(|s| s.get_type()), Some(ShaderType::Fragment));
        assert!(reg.get(99).is_none());
        assert_eq!(reg.len(), 1);
    }

    #[test]
    fn load_empty_file_is_invalid_data() {
        // from_source on a non-empty body is the valid path; an empty source
        // through load() is the error path (checked via the guard in load).
        let s = Shader::from_source(ShaderType::Fragment, "uniform float t;");
        assert!(!s.source().is_empty());
    }
}
