//! Port of the shader registry inside `HGAssetsImpl` (`Assets.cpp`):
//!
//! ```cpp
//! struct LoadedShader { sf::Shader; std::string path; sf::Shader::Type shaderType; std::size_t id; };
//! std::unordered_map<std::string, LoadedShader> shaders;      // key "packId_fileName.ext"
//! std::unordered_map<std::string, std::size_t> shadersPathToId; // key: full shader path
//! std::vector<sf::Shader*> shadersById;                       // index == shader id
//! ```
//!
//! **GLSL compilation is issue 10's job; its execution half is 029n1**
//! (K6): the registry keeps the file's SOURCE (the C++ hands it to
//! `sf::Shader`'s compile at load time; the port hands it to the GL
//! compiler lazily at draw time — the C++'s `Assets::new` runs BEFORE
//! the window/GL context exists, `Game.cpp:132-135`) and the uniform
//! store (the C++'s per-`sf::Shader` uniform cache, uploaded by the GL
//! layer at bind time, `Shader.cpp:294-303`). This module keeps the
//! registry + id allocation + the lookup/getter semantics (mapping §4.4,
//! §4.9) and the fail-soft returns (missing → `None`).
//!
//! The C++ `shaders`/`shadersPathToId` maps are `unordered_map`; their
//! iteration order is never observable (only `find`), so `BTreeMap` is a
//! faithful, deterministic stand-in. `shadersById` is a `vector` indexed by
//! id — reproduced exactly as `by_id` (index == shader id).

/// C++ `sf::Shader::Type` (only the values the loaders use).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShaderType {
    Vertex,
    Geometry,
    Fragment,
}

/// C++ `HGAssetsImpl::LoadedShader`, minus the `sf::Shader` handle
/// (issue 10's compiled program; 029n1's draw-time compile takes
/// `source` instead). `id` equals the index of this entry in
/// [`ShaderRegistry::by_id`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedShader {
    /// Full (normalized) shader path, e.g. `"Packs/artwork/Shaders/gradient.frag"`.
    pub path: String,
    pub shader_type: ShaderType,
    pub id: usize,
    /// The `.vert`/`.geom`/`.frag` file's source, verbatim (the C++
    /// `sf::Shader::loadFromFile` reads it + compiles it into the
    /// program; the port keeps the text for the draw-time GL compile).
    pub source: String,
}

/// A stored uniform value — the C++ `sf::Shader`'s per-object uniform
/// cache (`sf::Shader::setUniform` stores the value per shader object,
/// `Shader.cpp:102-196`; the GL upload happens at bind time,
/// `Shader.cpp:294-303`). The eight kinds map 1:1 onto the C++
/// `setUniform(F|FVec2|FVec3|FVec4|I|IVec2|IVec3|IVec4)` overloads
/// (`LuaScripting.cpp:1337-1407`).
///
/// Kept in `oh-assets` (NOT `oh-render`'s `Uniform`) so the
/// asset→render dependency stays one-way: `oh-render` cannot depend on
/// `oh-assets` (the cycle would run through `oh-game`); the GL layer
/// converts at the seam (the `oh-app` provider).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum UniformValue {
    F(f32),
    FVec2(f32, f32),
    FVec3(f32, f32, f32),
    FVec4(f32, f32, f32, f32),
    I(i32),
    IVec2(i32, i32),
    IVec3(i32, i32, i32),
    IVec4(i32, i32, i32, i32),
}

/// The K6 draw-time snapshot the GL layer compiles + uploads from (the
/// C++'s `getShader(id)` hands `renderTexture` the live `sf::Shader` —
/// program + uniform cache — for the `setShader`;
/// `HGGraphics.cpp:92-97`). The port snapshots the same two halves:
/// the source to compile (lazily, on the live context) and the uniform
/// store to upload at bind time.
#[derive(Debug, Clone)]
pub struct ShaderSnapshot {
    pub path: String,
    pub shader_type: ShaderType,
    pub source: String,
    /// Name-sorted (the C++'s `unordered_map` iteration order is
    /// unobservable — the GL upload's per-name `getUniformLocation`
    /// lookup does not depend on it).
    pub uniforms: std::collections::BTreeMap<String, UniformValue>,
}

/// The shader registry (C++ `shaders` + `shadersPathToId` + `shadersById`).
#[derive(Debug, Default, Clone)]
pub struct ShaderRegistry {
    /// C++ `shadersById`: one entry per successfully loaded shader, indexed
    /// by shader id.
    by_id: Vec<LoadedShader>,
    /// C++ `shaders`: name key (`"packId_fileName.ext"`) → shader id.
    by_name: std::collections::BTreeMap<String, usize>,
    /// C++ `shadersPathToId`: full path → shader id.
    path_to_id: std::collections::BTreeMap<String, usize>,
}

impl ShaderRegistry {
    #[must_use = "mirrors the C++ default-constructed members"]
    pub fn new() -> Self {
        Self::default()
    }

    /// Allocate a new shader id (C++ end of `loadShadersOfType`):
    /// `shadersById.push_back`, `id = size-1`, `shaders.emplace(nameKey, ...)`,
    /// `shadersPathToId.emplace(path, id)`. Returns the id.
    pub fn add(
        &mut self,
        name_key: &str,
        path: &str,
        source: &str,
        shader_type: ShaderType,
    ) -> usize {
        let id = self.by_id.len();
        self.by_name.insert(name_key.to_string(), id);
        self.path_to_id.insert(path.to_string(), id);
        self.by_id.push(LoadedShader {
            path: path.to_string(),
            shader_type,
            id,
            source: source.to_string(),
        });
        id
    }

    /// C++ `getShader(packId, id)` → the `LoadedShader` (or `None` + log).
    pub fn get(&self, name_key: &str) -> Option<&LoadedShader> {
        self.by_name
            .get(name_key)
            .and_then(|&id| self.by_id.get(id))
    }

    /// C++ `getShaderId(packId, id)` → the id, or `None` (nullopt).
    pub fn get_id_by_name(&self, name_key: &str) -> Option<usize> {
        self.by_name.get(name_key).copied()
    }

    /// C++ `getShaderIdByPath(shaderPath)` → the id, or `None` (nullopt).
    pub fn get_id_by_path(&self, path: &str) -> Option<usize> {
        self.path_to_id.get(path).copied()
    }

    /// C++ `getShaderByShaderId(shaderId)` → the shader, or `None` when the
    /// id is invalid.
    pub fn get_by_id(&self, id: usize) -> Option<&LoadedShader> {
        if self.is_valid_id(id) {
            self.by_id.get(id)
        } else {
            None
        }
    }

    /// C++ `isValidShaderId(shaderId)` = `shaderId < shadersById.size()`.
    pub fn is_valid_id(&self, id: usize) -> bool {
        id < self.by_id.len()
    }

    /// `shadersById.size()` (total successfully loaded shaders).
    #[must_use = "mirrors the C++ [[nodiscard]] size"]
    pub fn len(&self) -> usize {
        self.by_id.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_id.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_allocated_in_insertion_order() {
        let mut r = ShaderRegistry::new();
        let a = r.add(
            "cube_a.frag",
            "Packs/cube/Shaders/a.frag",
            "void main() { gl_FragColor = vec4(1.0); }",
            ShaderType::Fragment,
        );
        let b = r.add(
            "cube_b.vert",
            "Packs/cube/Shaders/b.vert",
            "void main() { gl_Position = ftransform(); }",
            ShaderType::Vertex,
        );
        assert_eq!((a, b), (0, 1));
        assert_eq!(r.len(), 2);

        assert!(r.is_valid_id(0));
        assert!(r.is_valid_id(1));
        assert!(!r.is_valid_id(2));

        assert_eq!(r.get_id_by_name("cube_a.frag"), Some(0));
        assert_eq!(r.get_id_by_path("Packs/cube/Shaders/b.vert"), Some(1));
        assert_eq!(
            r.get("cube_b.vert").unwrap().shader_type,
            ShaderType::Vertex
        );

        assert_eq!(r.get_by_id(1).unwrap().path, "Packs/cube/Shaders/b.vert");
        assert_eq!(
            r.get_by_id(1).unwrap().source,
            "void main() { gl_Position = ftransform(); }",
            "the source is kept verbatim (the draw-time GL compile's input)"
        );
        assert_eq!(r.get_by_id(2), None); // invalid id → None
        assert_eq!(r.get_id_by_name("nope"), None);
    }
}
