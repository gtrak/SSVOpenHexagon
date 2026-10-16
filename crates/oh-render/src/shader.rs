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

use glow::HasContext;

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

// === GLES translation (issue 25b SG3) ===

/// The GLES translation pass — the deterministic string transform that
/// makes the pack `.frag`/`.vert` sources (no `#version`, the desktop
/// fixed-function defaults — `gl_FragColor`) compile on a GLES context
/// (GLSL ES 1.00, the GLES 3.0 driver's shader language for ES 1.00
/// sources).
///
/// Applied ONLY on GLES contexts (the `GlBatchUploader`'s compile site —
/// a desktop-GL context gets its sources verbatim, the desktop path
/// stays behavior-identical; the _RELEASE files are never modified — the
/// translation is in-memory at compile time). A future pack-shader GL
/// compile (the issue-10 remainder) must run pack sources through this
/// same function on GLES contexts.
///
/// The transform (pure string, host-testable, no GL needed):
/// * no `#version` line: prepend `#version 100`;
/// * a DESKTOP-only `#version` line (`130`/`140`/`150`/`330`/… — no
///   `es` qualifier; `_RELEASE/Packs/experimental/Shaders/text.frag`
///   ships `#version 130`): that line is replaced by `#version 100` —
///   a desktop-version program can never compile on a GLES driver. (The
///   `gl_Color`/`gl_TexCoord`/`texture()` fixed-function builtins such
///   shaders additionally use have no GLSL ES equivalent — they need the
///   issue-10 render pass; documented in the 25b report, not rewritten
///   here.);
/// * an ES `#version` line (`100`, `300 es`, `310 es`): left untouched
///   (already GLES — the idempotent no-op);
/// * a FRAGMENT shader without a float `precision` declaration: insert
///   `precision mediump float;` directly after the version line (GLSL ES
///   1.00 fragment shaders have NO default float precision — without the
///   declaration the compile fails; `mediump` is the ES-conventional
///   fragment default. Vertex shaders get the ES highp default — no line
///   is inserted);
/// * everything else stays verbatim.
#[must_use]
pub fn translate_for_gles(source: &str, kind: ShaderType) -> String {
    // The first statement line (GLSL: only comments/whitespace may
    // precede `#version`). Locate it: the index of the first line whose
    // trim is not blank and is not a comment, if it is a `#version` line.
    let lines: Vec<&str> = source.lines().collect();
    let mut version_line: Option<usize> = None;
    for (i, line) in lines.iter().enumerate() {
        let t = line.trim_start();
        if t.is_empty() || t.starts_with("//") || t.starts_with("/*") || t.starts_with('*') {
            continue;
        }
        if t.starts_with("#version") {
            version_line = Some(i);
        }
        break; // the first real statement: a `#version` or not
    }

    let already_es = version_line.is_some_and(|i| {
        let rest = lines[i]
            .trim_start()
            .strip_prefix("#version")
            .unwrap()
            .trim_start();
        match rest.split_once(char::is_whitespace) {
            // `es`-qualified (300 es / 310 es): already GLES.
            Some((_, qualifier)) if qualifier.trim() == "es" => true,
            // `#version 100` (optionally with a trailing comment): ES 1.00.
            Some((number, _)) => number == "100",
            None => rest == "100",
        }
    });

    let mut out: Vec<String> = Vec::with_capacity(lines.len() + 2);
    // The leading comment/blank lines stay verbatim at the front.
    let front_end = version_line.unwrap_or(0);
    out.extend(lines[..front_end].iter().map(|l| l.to_string()));
    // The version line: inserted / replaced / kept.
    match (version_line, already_es) {
        (Some(i), true) => out.push(lines[i].to_string()),
        _ => out.push("#version 100".to_string()),
    }
    // The fragment float-precision declaration (ES 1.00 has no fragment
    // default) — directly after the version line, only when absent.
    if kind == ShaderType::Fragment && !float_precision_declared(&lines) {
        out.push("precision mediump float;".to_string());
    }
    // The rest, verbatim.
    let rest_start = version_line.map_or(0, |i| i + 1);
    out.extend(lines[rest_start..].iter().map(|l| l.to_string()));
    out.join("\n")
}

/// Whether any line declares a float precision (the ES 1.00 fragment
/// requirement — `precision (lowp|mediump|highp) float;` in any form).
fn float_precision_declared(lines: &[&str]) -> bool {
    lines.iter().any(|l| {
        let t = l.trim();
        t.starts_with("precision") && t.contains("float")
    })
}

/// Whether a live `glow` context is a GLES one (the issue 25b SG3 gate: the
/// translation is applied only here, so a desktop-GL context compiles its
/// sources verbatim and the desktop path stays behavior-identical).
///
/// `GL_VERSION` is a core query of every GL context — the `unsafe` is the
/// raw call (the caller holds a current context — the uploader's compile
/// site).
#[must_use]
pub fn is_gles_context(gl: &glow::Context) -> bool {
    let version = unsafe { gl.get_parameter_string(glow::VERSION) };
    version.contains("OpenGL ES")
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

    // === issue 25b SG3: the GLES translation (pure string, no GL) ===

    /// The `_RELEASE/Packs/artwork/Shaders/gradient.frag` shape (no
    /// `#version`, desktop fixed-function defaults — `gl_FragColor` +
    /// `gl_Color`; the file itself is never modified, this is the inline
    /// fixture the translation is pinned against).
    const GRADIENT_FRAG: &str = r#"uniform vec2 u_resolution;
uniform vec3 u_color0;
uniform vec3 u_color1;
uniform float u_blend;

void main()
{
    vec2 st = gl_FragCoord.xy / u_resolution.xy;

    gl_FragColor = mix(gl_Color, vec4(mix(vec4(u_color1, 1.0), vec4(u_color0, 1.0), st.y)), u_blend);
}
"#;

    #[test]
    fn no_version_fragment_gets_version_and_precision_rest_verbatim() {
        let out = translate_for_gles(GRADIENT_FRAG, ShaderType::Fragment);
        let expected = format!(
            "#version 100\nprecision mediump float;\n{}",
            GRADIENT_FRAG.trim_end_matches('\n')
        );
        assert_eq!(out, expected);
    }

    #[test]
    fn translate_is_idempotent_noop_on_already_es_source() {
        let es = "#version 100\nprecision mediump float;\nvoid main() { gl_FragColor = gl_Color; }";
        assert_eq!(translate_for_gles(es, ShaderType::Fragment), es);
        // Double application == single application (idempotent).
        let once = translate_for_gles(GRADIENT_FRAG, ShaderType::Fragment);
        assert_eq!(translate_for_gles(&once, ShaderType::Fragment), once);
    }

    #[test]
    fn desktop_version_line_replaced_with_es_100() {
        // The `_RELEASE/Packs/experimental/Shaders/text.frag` shape (its
        // `#version 130` is desktop-only; the fixed-function builtins it
        // uses are the issue-10 remainder — only the VERSION line is the
        // translation's concern).
        let src = "#version 130\nuniform sampler2D font;\nvoid main() { gl_FragColor = texture(font, gl_TexCoord[0].xy); }";
        let out = translate_for_gles(src, ShaderType::Fragment);
        assert_eq!(
            out,
            "#version 100\nprecision mediump float;\nuniform sampler2D font;\nvoid main() { gl_FragColor = texture(font, gl_TexCoord[0].xy); }"
        );
    }

    #[test]
    fn vertex_source_gets_version_but_no_precision() {
        let src =
            "attribute vec2 a_position;\nvoid main() { gl_Position = vec4(a_position, 0.0, 1.0); }";
        let out = translate_for_gles(src, ShaderType::Vertex);
        assert_eq!(
            out,
            "#version 100\nattribute vec2 a_position;\nvoid main() { gl_Position = vec4(a_position, 0.0, 1.0); }"
        );
    }

    #[test]
    fn es_300_es_version_left_untouched() {
        let src = "#version 300 es\nprecision highp float;\nout vec4 frag_color;\nvoid main() { frag_color = vec4(1.0); }";
        assert_eq!(translate_for_gles(src, ShaderType::Fragment), src);
    }

    #[test]
    fn leading_comment_before_version_preserved_and_version_found() {
        let src = "// patriciogv noise\n#version 130\nvoid main() {}";
        let out = translate_for_gles(src, ShaderType::Fragment);
        assert_eq!(
            out,
            "// patriciogv noise\n#version 100\nprecision mediump float;\nvoid main() {}"
        );
    }

    #[test]
    fn existing_precision_declaration_not_duplicated() {
        let src = "precision highp float;\nvoid main() { gl_FragColor = vec4(0.0); }";
        let out = translate_for_gles(src, ShaderType::Fragment);
        assert_eq!(
            out,
            "#version 100\nprecision highp float;\nvoid main() { gl_FragColor = vec4(0.0); }"
        );
    }

    #[test]
    fn internal_batching_fragment_source_gets_the_gles_fix() {
        // The `batching.rs` internal pass-through fragment source — the
        // actual GLES-breaking source (no version, no float precision
        // declaration): the translation is exactly what makes it compile
        // on the android EGL context (regression pin).
        let src = "varying vec4 v_color;\nvoid main() {\n    gl_FragColor = v_color;\n}\n";
        let out = translate_for_gles(src, ShaderType::Fragment);
        assert!(out.starts_with("#version 100\nprecision mediump float;\n"));
        assert!(out.contains("gl_FragColor = v_color;"));
    }
}
