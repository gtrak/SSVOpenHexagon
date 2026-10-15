//! The vertex-batch and render-state surface — the SFML
//! `sf::Vertex` / `sf::PrimitiveType` / `sf::RenderStates` analogue and the
//! `hg::Utils::FastVertexVector` (→ `FastVertexVectorTris`) batch that
//! `oh-game`'s draw code feeds.
//!
//! Sources (mapping/rendering-menu.md §3.13, §6.1):
//! - `sf::Vertex` is `{ sf::Vector2f position; sf::Color color; }` — **position
//!   first, color second**. `batch_unsafe_emplace_back(color, positions...)`
//!   builds `sf::Vertex{pos, color}`.
//! - Every quad is emitted as 6 vertices in the fixed order
//!   `(nw, sw, se, nw, se, ne)` (two triangles). **Vertex emission order
//!   matters for parity** — the C++ `CWall::draw` / `CPlayer::draw` emit into
//!   these buffers in the orders transcribed in mapping/gameplay.md.
//! - `FastVertexVector::draw` calls
//!   `mRenderTarget.draw(vertices, size, TPrimitive, renderStates)`.

use oh_math::hg::color::Color;
use oh_math::ssvs::vector2::Vec2;

/// `sf::PrimitiveType`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrimitiveType {
    /// `sf::PrimitiveType::Points`.
    Points,
    /// `sf::PrimitiveType::Lines`.
    Lines,
    /// `sf::PrimitiveType::LineStrip`.
    LineStrip,
    /// `sf::PrimitiveType::Triangles`.
    Triangles,
    /// `sf::PrimitiveType::TriangleStrip`.
    TriangleStrip,
    /// `sf::PrimitiveType::TriangleFan`.
    TriangleFan,
}

impl PrimitiveType {
    /// `sf::PrimitiveType::Triangles` — the primitive every `oh-game` vertex
    /// batch uses (`FastVertexVectorTris`).
    #[must_use]
    pub const fn triangles() -> Self {
        Self::Triangles
    }
}

/// `sf::Vertex` — `{ position, color }`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vertex {
    /// `sf::Vertex::position`.
    pub position: Vec2,
    /// `sf::Vertex::color`.
    pub color: Color,
}

impl Vertex {
    /// `sf::Vertex{position, color}`.
    #[must_use]
    pub const fn new(position: Vec2, color: Color) -> Self {
        Self { position, color }
    }
}

/// `sf::BlendMode` (subset the game uses — the default alpha blend).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlendMode {
    /// `sf::BlendMode::Alpha` (the default for `RenderStates::Default`).
    Alpha,
    /// `sf::BlendMode::Add`.
    Add,
    /// `sf::BlendMode::None` (opaque, no blending).
    None,
}

/// `sf::RenderStates`.
///
/// Handles are **pool indices** (not pointers): the texture / shader are looked
/// up in the crate's texture pool / shader registry by index, mirroring how
/// the C++ stores `sf::Texture*` / `sf::Shader*` in vectors
/// (`Assets::shadersById`, `getShaderByShaderId`). `None` means "use the
/// target default" — for the game's `getRenderStates(RenderStage)` path the
/// only field ever set is `shader`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RenderStates {
    /// `sf::RenderStates::texture` (pool index, `None` = default/none).
    pub texture: Option<usize>,
    /// `sf::RenderStates::shader` (shader-registry index, `None` = none).
    pub shader: Option<usize>,
    /// `sf::RenderStates::blendMode`.
    pub blend_mode: BlendMode,
}

impl Default for RenderStates {
    /// `sf::RenderStates::Default` — Alpha blend, no texture, no shader.
    fn default() -> Self {
        Self::new()
    }
}

impl RenderStates {
    /// `sf::RenderStates::Default`.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            texture: None,
            shader: None,
            blend_mode: BlendMode::Alpha,
        }
    }

    /// A render state bound to a fragment/vertex shader by registry index —
    /// the `sf::RenderStates{ assets.getShaderByShaderId(id) }` form.
    #[must_use]
    pub const fn with_shader(shader: usize) -> Self {
        Self {
            texture: None,
            shader: Some(shader),
            blend_mode: BlendMode::Alpha,
        }
    }
}

/// `hg::Utils::FastVertexVector<TPrimitive>` — a growable `sf::Vertex` buffer
/// with a fixed primitive type. (The C++ uses a `union`-trick zero-construct
/// array; the Rust analogue is a plain `Vec<Vertex>` with the same append
/// order/semantics — the observable surface is the vertex sequence.)
#[derive(Clone, Debug)]
pub struct VertexBatch {
    primitive: PrimitiveType,
    verts: Vec<Vertex>,
}

impl VertexBatch {
    /// `FastVertexVector<TPrimitive>()`.
    #[must_use]
    pub fn new(primitive: PrimitiveType) -> Self {
        Self {
            primitive,
            verts: Vec::new(),
        }
    }

    /// A triangles batch (`FastVertexVectorTris`).
    #[must_use]
    pub fn new_tris() -> Self {
        Self::new(PrimitiveType::triangles())
    }

    /// C++ `size()`.
    #[must_use]
    pub fn len(&self) -> usize {
        self.verts.len()
    }

    /// C++ `empty()`-equivalent.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.verts.is_empty()
    }

    /// C++ `getPrimitiveType()`-equivalent.
    #[must_use]
    pub fn primitive(&self) -> PrimitiveType {
        self.primitive
    }

    /// The vertex sequence in emission order (the draw entry consumes this).
    #[must_use]
    pub fn as_slice(&self) -> &[Vertex] {
        &self.verts
    }

    /// C++ `clear()`.
    pub fn clear(&mut self) {
        self.verts.clear();
    }

    /// C++ `reserve(n)` (absolute capacity).
    pub fn reserve(&mut self, n: usize) {
        self.verts.reserve_exact(n.saturating_sub(self.verts.len()));
    }

    /// C++ `reserve_more(n)` → `reserve(size * 2 + n)`.
    pub fn reserve_more(&mut self, n: usize) {
        self.reserve(self.verts.len() * 2 + n);
    }

    /// C++ `reserve_more_quad(n)` → `reserve_more(n * 6)`.
    pub fn reserve_more_quad(&mut self, n: usize) {
        self.reserve_more(n * 6);
    }

    /// C++ `reserve_quad(n)` → `reserve(n * 6)`.
    pub fn reserve_quad(&mut self, n: usize) {
        self.reserve(n * 6);
    }

    /// C++ `unsafe_emplace_back(pos, color)` — append one vertex.
    pub fn push(&mut self, vertex: Vertex) {
        self.verts.push(vertex);
    }

    /// C++ `unsafe_emplace_back(position, color)`.
    pub fn push_pos_color(&mut self, position: Vec2, color: Color) {
        self.verts.push(Vertex::new(position, color));
    }

    /// C++ `batch_unsafe_emplace_back(color, positions...)` — one color for
    /// every position (in order).
    pub fn push_color_positions(&mut self, color: Color, positions: &[Vec2]) {
        for &p in positions {
            self.verts.push(Vertex::new(p, color));
        }
    }

    /// C++ `FastVertexVectorTris::batch_unsafe_emplace_back_quad(color, nw,
    /// sw, se, ne)` — a uniform-color quad as 6 vertices `(nw,sw,se,nw,se,ne)`.
    pub fn push_quad(&mut self, color: Color, nw: Vec2, sw: Vec2, se: Vec2, ne: Vec2) {
        self.push_color_positions(color, &[nw, sw, se, nw, se, ne]);
    }

    /// C++ `FastVertexVectorTris::unsafe_emplace_back_quad(nw, cNW, sw, cSW,
    /// se, cSE, ne, cNE)` — a per-vertex-colored quad (same 6-vertex split).
    pub fn push_quad_per_vertex(
        &mut self,
        nw: Vec2,
        c_nw: Color,
        sw: Vec2,
        c_sw: Color,
        se: Vec2,
        c_se: Color,
        ne: Vec2,
        c_ne: Color,
    ) {
        self.push_pos_color(nw, c_nw);
        self.push_pos_color(sw, c_sw);
        self.push_pos_color(se, c_se);
        self.push_pos_color(nw, c_nw);
        self.push_pos_color(se, c_se);
        self.push_pos_color(ne, c_ne);
    }

    /// C++ `unsafe_emplace_other(rhs)` — append `rhs`'s vertices after ours.
    ///
    /// # Panics
    /// If the source batch has a different primitive type (the C++ asserts
    /// only on capacity; the primitive is a compile-time template arg, so a
    /// mismatch is a bug — we check it explicitly).
    pub fn append(&mut self, other: &VertexBatch) {
        assert!(
            self.primitive == other.primitive,
            "VertexBatch::append: primitive type mismatch"
        );
        self.verts.extend_from_slice(&other.verts);
    }

    /// The `FastVertexVector::draw` entry: emit this batch to a render target
    /// with the given primitive + render states. The concrete GL upload is
    /// performed by the `oh-render` window/target (see `window.rs`); this is
    /// the pure, headless-testable description of the draw call.
    #[must_use]
    pub fn draw_call(&self, states: RenderStates) -> DrawCall {
        DrawCall {
            primitive: self.primitive,
            count: self.verts.len(),
            states,
        }
    }
}

/// The resolved, headless-observable description of a vertex draw call
/// (`mRenderTarget.draw(vertices, size, primitive, states)`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DrawCall {
    pub primitive: PrimitiveType,
    pub count: usize,
    pub states: RenderStates,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quad_emits_six_vertices_in_sfml_order() {
        let mut batch = VertexBatch::new_tris();
        let c = Color::default();
        batch.push_quad(
            c,
            Vec2::new(0.0, 0.0),
            Vec2::new(0.0, 1.0),
            Vec2::new(1.0, 1.0),
            Vec2::new(1.0, 0.0),
        );
        assert_eq!(batch.len(), 6);
        let v = batch.as_slice();
        // nw, sw, se, nw, se, ne
        assert_eq!(v[0].position, Vec2::new(0.0, 0.0));
        assert_eq!(v[1].position, Vec2::new(0.0, 1.0));
        assert_eq!(v[2].position, Vec2::new(1.0, 1.0));
        assert_eq!(v[3].position, Vec2::new(0.0, 0.0));
        assert_eq!(v[4].position, Vec2::new(1.0, 1.0));
        assert_eq!(v[5].position, Vec2::new(1.0, 0.0));
        assert!(v.iter().all(|x| x.color == c));
    }

    #[test]
    fn per_vertex_quad_preserves_colors() {
        let mut batch = VertexBatch::new_tris();
        let red = Color {
            r: 255,
            g: 0,
            b: 0,
            a: 255,
        };
        let blue = Color {
            r: 0,
            g: 0,
            b: 255,
            a: 255,
        };
        batch.push_quad_per_vertex(
            Vec2::new(0.0, 0.0),
            red,
            Vec2::new(0.0, 1.0),
            red,
            Vec2::new(1.0, 1.0),
            blue,
            Vec2::new(1.0, 0.0),
            blue,
        );
        let v = batch.as_slice();
        assert_eq!(v[2].color, blue);
        assert_eq!(v[3].color, red); // nw repeated
        assert_eq!(v[4].color, blue); // se repeated
        assert_eq!(batch.len(), 6);
    }

    #[test]
    fn append_matches_unsafe_emplace_other() {
        let mut a = VertexBatch::new_tris();
        let mut b = VertexBatch::new_tris();
        a.push_pos_color(Vec2::new(0.0, 0.0), Color::default());
        b.push_quad(
            Color::default(),
            Vec2::new(0.0, 0.0),
            Vec2::new(0.0, 1.0),
            Vec2::new(1.0, 1.0),
            Vec2::new(1.0, 0.0),
        );
        a.append(&b);
        assert_eq!(a.len(), 7);
        assert_eq!(a.as_slice()[1].position, Vec2::new(0.0, 0.0)); // b's nw
    }

    #[test]
    fn reserve_more_quad_does_not_shrink_or_panic() {
        let mut batch = VertexBatch::new_tris();
        batch.reserve_more_quad(10); // reserve(0 + 60) internally
        assert_eq!(batch.len(), 0);
        // after reserving room for 10 quads we can emit 10 without growing
        for _ in 0..10 {
            batch.push_quad(
                Color::default(),
                Vec2::new(0.0, 0.0),
                Vec2::new(0.0, 1.0),
                Vec2::new(1.0, 1.0),
                Vec2::new(1.0, 0.0),
            );
        }
        assert_eq!(batch.len(), 60);
    }

    #[test]
    fn draw_call_describes_primitive_count_states() {
        let mut batch = VertexBatch::new_tris();
        batch.push_quad(
            Color::default(),
            Vec2::new(0.0, 0.0),
            Vec2::new(0.0, 1.0),
            Vec2::new(1.0, 1.0),
            Vec2::new(1.0, 0.0),
        );
        let call = batch.draw_call(RenderStates::with_shader(3));
        assert_eq!(call.primitive, PrimitiveType::Triangles);
        assert_eq!(call.count, 6);
        assert_eq!(call.states.shader, Some(3));
    }
}
